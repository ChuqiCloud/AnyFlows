use af_domain::{GroupId, PLAYGROUND_TOKEN_NAME, TokenId, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    entity::{TokenHash, TokenIpAllowlist, TokenModelAllowlist, groups, tokens},
    token_owner_guard::{TokenOwnerGuardError, lock_non_deleted_owner, non_deleted_token_count},
};

use super::{
    MAX_USER_TOKENS_PER_USER, UserTokenCreateRecord, UserTokenDeleteOutcome,
    UserTokenMutationOutcome, UserTokenRecord, UserTokenRepository, UserTokenRepositoryError,
    UserTokenWriteRecord, read, record_internal_error,
};

struct StoredUserTokenFields {
    name: String,
    status: i16,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<TimeDateTimeWithTimeZone>,
    model_limits: Option<TokenModelAllowlist>,
    allow_ips: Option<TokenIpAllowlist>,
}

pub(super) async fn create(
    repository: &UserTokenRepository,
    record: UserTokenCreateRecord,
) -> Result<UserTokenRecord, UserTokenRepositoryError> {
    let fields = validate_fields(record.fields)?;
    let key_hash =
        TokenHash::parse(&record.key_hash).map_err(|_| UserTokenRepositoryError::InvalidInput)?;
    if !valid_key_prefix(&record.key_prefix) {
        return Err(UserTokenRepositoryError::InvalidInput);
    }
    let transaction = begin_transaction(repository).await?;
    let Some(owner) = lock_non_deleted_owner(&transaction, record.owner_user_id)
        .await
        .map_err(map_owner_guard_error)?
    else {
        rollback(transaction).await?;
        return Err(UserTokenRepositoryError::OwnerUnavailable);
    };
    if owner.status() != 1 {
        rollback(transaction).await?;
        return Err(UserTokenRepositoryError::OwnerUnavailable);
    }
    let default_group_id = owner.default_group_id();
    if !active_group_exists(&transaction, default_group_id).await? {
        rollback(transaction).await?;
        return Err(record_internal_error(UserTokenRepositoryError::Invariant));
    }
    if non_deleted_token_count(&transaction, record.owner_user_id)
        .await
        .map_err(map_owner_guard_error)?
        >= MAX_USER_TOKENS_PER_USER as u64
    {
        rollback(transaction).await?;
        return Err(UserTokenRepositoryError::LimitReached);
    }

    let now = TimeDateTimeWithTimeZone::now_utc();
    let inserted = tokens::ActiveModel {
        user_id: Set(record.owner_user_id.get()),
        key_hash: Set(key_hash),
        key_prefix: Set(record.key_prefix),
        name: Set(fields.name),
        status: Set(fields.status),
        group_id: Set(None),
        organization_id: Set(None),
        organization_membership_id: Set(None),
        organization_team_id: Set(None),
        remain_quota: Set(fields.remain_quota),
        unlimited_quota: Set(fields.unlimited_quota),
        expired_at: Set(fields.expired_at),
        model_limits: Set(fields.model_limits),
        allow_ips: Set(fields.allow_ips),
        cross_group_retry: Set(false),
        window_5h_start: Set(now),
        window_1d_start: Set(now),
        window_7d_start: Set(now),
        ..Default::default()
    }
    .insert(&transaction)
    .with_subscriber(NoSubscriber::default())
    .await
    .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))?;
    let token_id = TokenId::new(inserted.id)
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Invariant))?;
    let snapshot = fetch_snapshot(&transaction, record.owner_user_id, token_id).await?;
    commit(transaction).await?;
    Ok(snapshot)
}

pub(super) async fn update(
    repository: &UserTokenRepository,
    owner_user_id: UserId,
    token_id: TokenId,
    record: UserTokenWriteRecord,
) -> Result<UserTokenMutationOutcome, UserTokenRepositoryError> {
    let fields = validate_fields(record)?;
    let transaction = begin_transaction(repository).await?;
    if lock_non_deleted_owner(&transaction, owner_user_id)
        .await
        .map_err(map_owner_guard_error)?
        .is_none_or(|owner| owner.status() != 1)
    {
        rollback(transaction).await?;
        return Err(UserTokenRepositoryError::OwnerUnavailable);
    }
    // 只更新普通用户可控列，管理员绑定的分组、重试和窗口字段保持原值。
    let result = tokens::Entity::update_many()
        .filter(tokens::Column::Id.eq(token_id.get()))
        .filter(tokens::Column::UserId.eq(owner_user_id.get()))
        .filter(tokens::Column::OrganizationId.is_null())
        .filter(tokens::Column::Name.ne(PLAYGROUND_TOKEN_NAME))
        .filter(tokens::Column::DeletedAt.is_null())
        .col_expr(tokens::Column::Name, Expr::value(fields.name))
        .col_expr(tokens::Column::Status, Expr::value(fields.status))
        .col_expr(
            tokens::Column::RemainQuota,
            Expr::value(fields.remain_quota),
        )
        .col_expr(
            tokens::Column::UnlimitedQuota,
            Expr::value(fields.unlimited_quota),
        )
        .col_expr(tokens::Column::ExpiredAt, Expr::value(fields.expired_at))
        .col_expr(
            tokens::Column::ModelLimits,
            Expr::value(fields.model_limits),
        )
        .col_expr(tokens::Column::AllowIps, Expr::value(fields.allow_ips))
        .col_expr(
            tokens::Column::UpdatedAt,
            Expr::value(TimeDateTimeWithTimeZone::now_utc()),
        )
        .exec(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))?;
    match result.rows_affected {
        0 => {
            rollback(transaction).await?;
            Ok(UserTokenMutationOutcome::NotFound)
        }
        1 => {
            let snapshot = fetch_snapshot(&transaction, owner_user_id, token_id).await?;
            commit(transaction).await?;
            Ok(UserTokenMutationOutcome::Mutated(Box::new(snapshot)))
        }
        _ => {
            rollback(transaction).await?;
            Err(record_internal_error(UserTokenRepositoryError::Invariant))
        }
    }
}

pub(super) async fn delete(
    repository: &UserTokenRepository,
    owner_user_id: UserId,
    token_id: TokenId,
) -> Result<UserTokenDeleteOutcome, UserTokenRepositoryError> {
    let transaction = begin_transaction(repository).await?;
    if lock_non_deleted_owner(&transaction, owner_user_id)
        .await
        .map_err(map_owner_guard_error)?
        .is_none_or(|owner| owner.status() != 1)
    {
        rollback(transaction).await?;
        return Err(UserTokenRepositoryError::OwnerUnavailable);
    }
    let now = TimeDateTimeWithTimeZone::now_utc();
    let result = tokens::Entity::update_many()
        .filter(tokens::Column::Id.eq(token_id.get()))
        .filter(tokens::Column::UserId.eq(owner_user_id.get()))
        .filter(tokens::Column::OrganizationId.is_null())
        .filter(tokens::Column::Name.ne(PLAYGROUND_TOKEN_NAME))
        .filter(tokens::Column::DeletedAt.is_null())
        .col_expr(tokens::Column::DeletedAt, Expr::value(now))
        .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
        .exec(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))?;
    match result.rows_affected {
        0 => {
            rollback(transaction).await?;
            Ok(UserTokenDeleteOutcome::NotFound)
        }
        1 => {
            commit(transaction).await?;
            Ok(UserTokenDeleteOutcome::Deleted)
        }
        _ => {
            rollback(transaction).await?;
            Err(record_internal_error(UserTokenRepositoryError::Invariant))
        }
    }
}

fn validate_fields(
    record: UserTokenWriteRecord,
) -> Result<StoredUserTokenFields, UserTokenRepositoryError> {
    if !valid_text(&record.name, 128)
        || !matches!(record.status, 1 | 2)
        || record.remain_quota < 0
        || record.expired_at.is_some_and(|value| value < 0)
    {
        return Err(UserTokenRepositoryError::InvalidInput);
    }
    let expired_at = record
        .expired_at
        .map(TimeDateTimeWithTimeZone::from_unix_timestamp)
        .transpose()
        .map_err(|_| UserTokenRepositoryError::InvalidInput)?;
    let model_limits = record
        .model_limits
        .map(|entries| TokenModelAllowlist::validate(serde_json::json!(entries)))
        .transpose()
        .map_err(|_| UserTokenRepositoryError::InvalidInput)?;
    let allow_ips = record
        .allow_ips
        .map(|entries| TokenIpAllowlist::validate(serde_json::json!(entries)))
        .transpose()
        .map_err(|_| UserTokenRepositoryError::InvalidInput)?;
    Ok(StoredUserTokenFields {
        name: record.name,
        status: record.status,
        remain_quota: record.remain_quota,
        unlimited_quota: record.unlimited_quota,
        expired_at,
        model_limits,
        allow_ips,
    })
}

async fn active_group_exists(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
) -> Result<bool, UserTokenRepositoryError> {
    groups::Entity::find()
        .select_only()
        .column(groups::Column::Id)
        .filter(groups::Column::Id.eq(group_id.get()))
        .filter(groups::Column::DeletedAt.is_null())
        .into_tuple::<i64>()
        .one(transaction)
        .await
        .map(|row| row.is_some())
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))
}

async fn fetch_snapshot(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
    token_id: TokenId,
) -> Result<UserTokenRecord, UserTokenRepositoryError> {
    let query = read::detail_query(transaction.get_database_backend(), owner_user_id, token_id);
    let statement = transaction.get_database_backend().build(&query);
    let mut rows = transaction
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))?;
    match rows.len() {
        1 => read::record_from_row(
            &rows
                .pop()
                .ok_or_else(|| record_internal_error(UserTokenRepositoryError::Invariant))?,
            owner_user_id,
        ),
        _ => Err(record_internal_error(UserTokenRepositoryError::Invariant)),
    }
}

async fn begin_transaction(
    repository: &UserTokenRepository,
) -> Result<DatabaseTransaction, UserTokenRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))
}

async fn commit(transaction: DatabaseTransaction) -> Result<(), UserTokenRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), UserTokenRepositoryError> {
    transaction
        .rollback()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserTokenRepositoryError::Query))
}

fn valid_key_prefix(value: &str) -> bool {
    value.len() == 18
        && value.starts_with("sk-af-")
        && value[6..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn map_owner_guard_error(error: TokenOwnerGuardError) -> UserTokenRepositoryError {
    match error {
        TokenOwnerGuardError::Query => record_internal_error(UserTokenRepositoryError::Query),
        TokenOwnerGuardError::Invariant => {
            record_internal_error(UserTokenRepositoryError::Invariant)
        }
    }
}
