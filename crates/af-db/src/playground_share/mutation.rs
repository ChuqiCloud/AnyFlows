use std::time::Duration;

use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, DbBackend,
    DbErr, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};

use crate::entity::{SensitiveJson, TokenHash, playground_shares, users};

use super::{
    MAX_ACTIVE_PLAYGROUND_SHARES_PER_USER, PlaygroundShareCreatedRecord, PlaygroundShareRepository,
    PlaygroundShareRepositoryError, PlaygroundShareWrite, record_internal_error, validate_snapshot,
};

const MAX_PLAYGROUND_SHARE_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

pub(super) async fn create(
    repository: &PlaygroundShareRepository,
    write: PlaygroundShareWrite,
) -> Result<PlaygroundShareCreatedRecord, PlaygroundShareRepositoryError> {
    let token_hash = TokenHash::parse(&write.token_hash)
        .map_err(|_| PlaygroundShareRepositoryError::InvalidInput)?;
    validate_snapshot(&write.snapshot)?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let expires_at = parse_expiration(write.expires_at, now)?;
    let transaction = repository
        .pool
        .connection()
        .begin()
        .await
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))?;

    if !lock_active_owner(&transaction, write.owner_user_id).await? {
        rollback(transaction).await?;
        return Err(PlaygroundShareRepositoryError::OwnerUnavailable);
    }
    cleanup_inactive(&transaction, write.owner_user_id, now).await?;
    if active_count(&transaction, write.owner_user_id, now).await?
        >= MAX_ACTIVE_PLAYGROUND_SHARES_PER_USER as u64
    {
        rollback(transaction).await?;
        return Err(PlaygroundShareRepositoryError::LimitReached);
    }

    let inserted = playground_shares::ActiveModel {
        owner_user_id: Set(write.owner_user_id.get()),
        token_hash: Set(token_hash),
        snapshot: Set(SensitiveJson::from(write.snapshot)),
        created_at: Set(now),
        expires_at: Set(expires_at),
        revoked_at: Set(None),
        ..Default::default()
    }
    .insert(&transaction)
    .await;
    let inserted = match inserted {
        Ok(model) => model,
        Err(error) => {
            let mapped = if is_unique_violation(&error) {
                PlaygroundShareRepositoryError::TokenConflict
            } else {
                record_internal_error(PlaygroundShareRepositoryError::Query)
            };
            rollback(transaction).await?;
            return Err(mapped);
        }
    };
    transaction
        .commit()
        .await
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))?;
    Ok(PlaygroundShareCreatedRecord {
        created_at: inserted.created_at.unix_timestamp(),
        expires_at: inserted.expires_at.unix_timestamp(),
    })
}

fn parse_expiration(
    expires_at: i64,
    now: TimeDateTimeWithTimeZone,
) -> Result<TimeDateTimeWithTimeZone, PlaygroundShareRepositoryError> {
    let expires_at = TimeDateTimeWithTimeZone::from_unix_timestamp(expires_at)
        .map_err(|_| PlaygroundShareRepositoryError::InvalidInput)?;
    if expires_at <= now || expires_at > now + MAX_PLAYGROUND_SHARE_TTL {
        return Err(PlaygroundShareRepositoryError::InvalidInput);
    }
    Ok(expires_at)
}

async fn lock_active_owner(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
) -> Result<bool, PlaygroundShareRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，先用不改变业务值的写语句取得数据库写锁。
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(owner_user_id.get()))
            .col_expr(users::Column::Quota, Expr::col(users::Column::Quota).into())
            .exec(transaction)
            .await
            .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))?;
        if result.rows_affected != 1 {
            return Ok(false);
        }
    }
    let mut query = users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .filter(users::Column::Id.eq(owner_user_id.get()))
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .into_tuple::<i64>()
        .one(transaction)
        .await
        .map(|model| model.is_some())
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))
}

async fn cleanup_inactive(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), PlaygroundShareRepositoryError> {
    playground_shares::Entity::delete_many()
        .filter(playground_shares::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(
            Condition::any()
                .add(playground_shares::Column::RevokedAt.is_not_null())
                .add(playground_shares::Column::ExpiresAt.lte(now)),
        )
        .exec(transaction)
        .await
        .map(|_| ())
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))
}

async fn active_count(
    transaction: &DatabaseTransaction,
    owner_user_id: UserId,
    now: TimeDateTimeWithTimeZone,
) -> Result<u64, PlaygroundShareRepositoryError> {
    playground_shares::Entity::find()
        .filter(playground_shares::Column::OwnerUserId.eq(owner_user_id.get()))
        .filter(playground_shares::Column::RevokedAt.is_null())
        .filter(playground_shares::Column::ExpiresAt.gt(now))
        .count(transaction)
        .await
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), PlaygroundShareRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| record_internal_error(PlaygroundShareRepositoryError::Query))
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}
