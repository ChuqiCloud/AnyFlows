use std::fmt;

use af_domain::{GroupId, PLAYGROUND_TOKEN_NAME, TokenId, UserId};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseTransaction, DbErr, EntityTrait, QueryFilter,
    QueryResult, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminTokenRecord, AdminTokenRepository, MAX_USER_TOKENS_PER_USER,
    admin_token::detail_query,
    entity::{TokenHash, TokenIpAllowlist, TokenModelAllowlist, groups, tokens},
    token_owner_guard::{TokenOwnerGuardError, lock_non_deleted_owner, non_deleted_token_count},
};

/// 管理员签发令牌时写入的密钥材料和完整配置。
pub struct AdminTokenCreateRecord {
    user_id: UserId,
    key_hash: String,
    key_prefix: String,
    fields: AdminTokenWriteRecord,
}

impl AdminTokenCreateRecord {
    /// 组装已经由应用层校验的令牌签发记录。
    #[must_use]
    pub fn new(
        user_id: UserId,
        key_hash: String,
        key_prefix: String,
        fields: AdminTokenWriteRecord,
    ) -> Self {
        Self {
            user_id,
            key_hash,
            key_prefix,
            fields,
        }
    }
}

impl fmt::Debug for AdminTokenCreateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminTokenCreateRecord(<redacted>)")
    }
}

/// 管理员创建或完整更新令牌时允许覆盖的配置字段。
pub struct AdminTokenWriteRecord {
    name: String,
    status: i16,
    group_id: Option<GroupId>,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    cross_group_retry: bool,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    max_requests: Option<i64>,
}

impl AdminTokenWriteRecord {
    /// 组装已经由应用层完成公开边界校验的令牌配置。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端令牌写入契约一一对应"
    )]
    #[must_use]
    pub fn new(
        name: String,
        status: i16,
        group_id: Option<GroupId>,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
        cross_group_retry: bool,
        rate_limit_5h: Option<i64>,
        rate_limit_1d: Option<i64>,
        rate_limit_7d: Option<i64>,
        max_requests: Option<i64>,
    ) -> Self {
        Self {
            name,
            status,
            group_id,
            remain_quota,
            unlimited_quota,
            expired_at,
            model_limits,
            allow_ips,
            cross_group_retry,
            rate_limit_5h,
            rate_limit_1d,
            rate_limit_7d,
            max_requests,
        }
    }
}

impl fmt::Debug for AdminTokenWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminTokenWriteRecord(<redacted>)")
    }
}

/// 令牌更新结果；不存在和已软删除统一视为未找到。
pub enum AdminTokenMutationOutcome {
    /// 令牌配置已经写入，并返回最新非敏感快照。
    Mutated(Box<AdminTokenRecord>),
    /// 令牌不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminTokenMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mutated(_) => {
                formatter.write_str("AdminTokenMutationOutcome::Mutated(<redacted>)")
            }
            Self::NotFound => formatter.write_str("AdminTokenMutationOutcome::NotFound"),
        }
    }
}

/// 令牌软删除结果；重复删除不会伪装成成功。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminTokenDeleteOutcome {
    /// 令牌已经写入软删除墓碑。
    Deleted,
    /// 令牌不存在或已经软删除。
    NotFound,
}

/// 管理令牌写入仓储错误；不携带密钥、摘要、白名单或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminTokenWriteRepositoryError {
    /// 写入记录不满足持久化类型或时间范围。
    #[error("管理令牌写入参数无效")]
    InvalidInput,
    /// 用户或分组引用不存在或已经软删除。
    #[error("管理令牌引用无效")]
    InvalidReference,
    /// 用户现有未软删除令牌已经达到统一容量上限。
    #[error("用户 API Key 数量已达上限")]
    LimitReached,
    /// 获取连接、事务或执行 SQL 失败。
    #[error("管理令牌数据库写入失败")]
    Query,
    /// 写入事务超过配置的硬截止时间。
    #[error("管理令牌数据库写入超时")]
    Timeout,
    /// 现有持久化状态违反不变量。
    #[error("管理令牌持久化状态损坏")]
    Invariant,
}

struct StoredTokenWriteFields {
    name: String,
    status: i16,
    group_id: Option<GroupId>,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<TimeDateTimeWithTimeZone>,
    model_limits: Option<TokenModelAllowlist>,
    allow_ips: Option<TokenIpAllowlist>,
    cross_group_retry: bool,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    max_requests: Option<i64>,
}

impl AdminTokenRepository {
    /// 签发一个令牌并初始化额度、窗口和请求计数状态。
    pub async fn create_token(
        &self,
        record: AdminTokenCreateRecord,
    ) -> Result<AdminTokenRecord, AdminTokenWriteRepositoryError> {
        match timeout(self.lookup_timeout, self.create_token_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                AdminTokenWriteRepositoryError::Timeout,
            )),
        }
    }

    /// 完整更新未软删除令牌的配置，保留密钥、累计用量和窗口起点。
    pub async fn update_token(
        &self,
        token_id: TokenId,
        expected_user_id: UserId,
        record: AdminTokenWriteRecord,
    ) -> Result<AdminTokenMutationOutcome, AdminTokenWriteRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.update_token_inner(token_id, expected_user_id, record),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                AdminTokenWriteRepositoryError::Timeout,
            )),
        }
    }

    /// 软删除令牌；已有计费和用量记录仍可通过主键保留审计关联。
    pub async fn delete_token(
        &self,
        token_id: TokenId,
    ) -> Result<AdminTokenDeleteOutcome, AdminTokenWriteRepositoryError> {
        match timeout(self.lookup_timeout, self.delete_token_inner(token_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                AdminTokenWriteRepositoryError::Timeout,
            )),
        }
    }

    async fn create_token_inner(
        &self,
        record: AdminTokenCreateRecord,
    ) -> Result<AdminTokenRecord, AdminTokenWriteRepositoryError> {
        let fields = validate_storage_fields(record.fields)?;
        let key_hash = TokenHash::parse(&record.key_hash)
            .map_err(|_| AdminTokenWriteRepositoryError::InvalidInput)?;
        if !valid_key_prefix(&record.key_prefix) {
            return Err(AdminTokenWriteRepositoryError::InvalidInput);
        }

        let transaction = begin_transaction(self).await?;
        ensure_create_references_and_capacity(&transaction, record.user_id, fields.group_id)
            .await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let inserted = tokens::ActiveModel {
            user_id: Set(record.user_id.get()),
            key_hash: Set(key_hash),
            key_prefix: Set(record.key_prefix),
            name: Set(fields.name),
            status: Set(fields.status),
            group_id: Set(fields.group_id.map(GroupId::get)),
            organization_id: Set(None),
            organization_membership_id: Set(None),
            organization_team_id: Set(None),
            remain_quota: Set(fields.remain_quota),
            unlimited_quota: Set(fields.unlimited_quota),
            expired_at: Set(fields.expired_at),
            model_limits: Set(fields.model_limits),
            allow_ips: Set(fields.allow_ips),
            cross_group_retry: Set(fields.cross_group_retry),
            rate_limit_5h: Set(fields.rate_limit_5h),
            rate_limit_1d: Set(fields.rate_limit_1d),
            rate_limit_7d: Set(fields.rate_limit_7d),
            window_5h_start: Set(now),
            window_1d_start: Set(now),
            window_7d_start: Set(now),
            max_requests: Set(fields.max_requests),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_write_db_error)?;
        let token_id = TokenId::new(inserted.id)
            .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Invariant))?;
        let snapshot = fetch_token_snapshot(&transaction, token_id).await?;
        commit_transaction(transaction).await?;
        Ok(snapshot)
    }

    async fn update_token_inner(
        &self,
        token_id: TokenId,
        expected_user_id: UserId,
        record: AdminTokenWriteRecord,
    ) -> Result<AdminTokenMutationOutcome, AdminTokenWriteRepositoryError> {
        let fields = validate_storage_fields(record)?;
        let transaction = begin_transaction(self).await?;
        let Some(user_id) = active_token_owner(&transaction, token_id).await? else {
            return Ok(AdminTokenMutationOutcome::NotFound);
        };
        if user_id != expected_user_id {
            return Err(AdminTokenWriteRepositoryError::InvalidInput);
        }
        ensure_existing_owner_and_group(&transaction, user_id, fields.group_id).await?;

        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = tokens::Entity::update_many()
            .filter(Expr::col((tokens::Entity, tokens::Column::Id)).eq(token_id.get()))
            .filter(Expr::col((tokens::Entity, tokens::Column::OrganizationId)).is_null())
            .filter(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
            .filter(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
            .col_expr(tokens::Column::Name, Expr::value(fields.name))
            .col_expr(tokens::Column::Status, Expr::value(fields.status))
            .col_expr(
                tokens::Column::GroupId,
                Expr::value(fields.group_id.map(GroupId::get)),
            )
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
                tokens::Column::CrossGroupRetry,
                Expr::value(fields.cross_group_retry),
            )
            .col_expr(
                tokens::Column::RateLimit5h,
                Expr::value(fields.rate_limit_5h),
            )
            .col_expr(
                tokens::Column::RateLimit1d,
                Expr::value(fields.rate_limit_1d),
            )
            .col_expr(
                tokens::Column::RateLimit7d,
                Expr::value(fields.rate_limit_7d),
            )
            .col_expr(
                tokens::Column::MaxRequests,
                Expr::value(fields.max_requests),
            )
            .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        if result.rows_affected != 1 {
            return Err(record_internal_error(
                AdminTokenWriteRepositoryError::Invariant,
            ));
        }
        let snapshot = fetch_token_snapshot(&transaction, token_id).await?;
        commit_transaction(transaction).await?;
        Ok(AdminTokenMutationOutcome::Mutated(Box::new(snapshot)))
    }

    async fn delete_token_inner(
        &self,
        token_id: TokenId,
    ) -> Result<AdminTokenDeleteOutcome, AdminTokenWriteRepositoryError> {
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = tokens::Entity::update_many()
            .filter(Expr::col((tokens::Entity, tokens::Column::Id)).eq(token_id.get()))
            .filter(Expr::col((tokens::Entity, tokens::Column::OrganizationId)).is_null())
            .filter(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
            .filter(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
            .col_expr(tokens::Column::DeletedAt, Expr::value(now))
            .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_db_error)?;
        match result.rows_affected {
            0 => Ok(AdminTokenDeleteOutcome::NotFound),
            1 => Ok(AdminTokenDeleteOutcome::Deleted),
            _ => Err(record_internal_error(
                AdminTokenWriteRepositoryError::Invariant,
            )),
        }
    }
}

fn validate_storage_fields(
    record: AdminTokenWriteRecord,
) -> Result<StoredTokenWriteFields, AdminTokenWriteRepositoryError> {
    if !valid_text(&record.name, 128)
        || record.name == PLAYGROUND_TOKEN_NAME
        || !matches!(record.status, 1 | 2)
        || record.remain_quota < 0
        || [
            record.rate_limit_5h,
            record.rate_limit_1d,
            record.rate_limit_7d,
            record.max_requests,
        ]
        .into_iter()
        .flatten()
        .any(|value| value < 0)
    {
        return Err(AdminTokenWriteRepositoryError::InvalidInput);
    }
    let expired_at = record
        .expired_at
        .map(TimeDateTimeWithTimeZone::from_unix_timestamp)
        .transpose()
        .map_err(|_| AdminTokenWriteRepositoryError::InvalidInput)?;
    let model_limits = record
        .model_limits
        .map(|entries| TokenModelAllowlist::validate(serde_json::json!(entries)))
        .transpose()
        .map_err(|_| AdminTokenWriteRepositoryError::InvalidInput)?;
    let allow_ips = record
        .allow_ips
        .map(|entries| TokenIpAllowlist::validate(serde_json::json!(entries)))
        .transpose()
        .map_err(|_| AdminTokenWriteRepositoryError::InvalidInput)?;
    Ok(StoredTokenWriteFields {
        name: record.name,
        status: record.status,
        group_id: record.group_id,
        remain_quota: record.remain_quota,
        unlimited_quota: record.unlimited_quota,
        expired_at,
        model_limits,
        allow_ips,
        cross_group_retry: record.cross_group_retry,
        rate_limit_5h: record.rate_limit_5h,
        rate_limit_1d: record.rate_limit_1d,
        rate_limit_7d: record.rate_limit_7d,
        max_requests: record.max_requests,
    })
}

async fn begin_transaction(
    repository: &AdminTokenRepository,
) -> Result<DatabaseTransaction, AdminTokenWriteRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Query))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AdminTokenWriteRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Query))
}

async fn fetch_token_snapshot(
    transaction: &DatabaseTransaction,
    token_id: TokenId,
) -> Result<AdminTokenRecord, AdminTokenWriteRepositoryError> {
    let statement = transaction
        .get_database_backend()
        .build(&detail_query(transaction.get_database_backend(), token_id));
    let mut results = transaction
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Query))?;
    match results.len() {
        1 => AdminTokenRecord::try_from_query_result(
            &results
                .pop()
                .ok_or_else(|| record_internal_error(AdminTokenWriteRepositoryError::Invariant))?,
        )
        .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Invariant)),
        _ => Err(record_internal_error(
            AdminTokenWriteRepositoryError::Invariant,
        )),
    }
}

async fn ensure_create_references_and_capacity(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    group_id: Option<GroupId>,
) -> Result<(), AdminTokenWriteRepositoryError> {
    let owner = lock_non_deleted_owner(transaction, user_id)
        .await?
        .ok_or(AdminTokenWriteRepositoryError::InvalidReference)?;
    if !matches!(owner.status(), 1 | 2) {
        return Err(record_internal_error(
            AdminTokenWriteRepositoryError::Invariant,
        ));
    }
    let effective_group_id = group_id.unwrap_or(owner.default_group_id());
    if !active_group_exists(transaction, effective_group_id).await? {
        return Err(AdminTokenWriteRepositoryError::InvalidReference);
    }
    if non_deleted_token_count(transaction, user_id).await? >= MAX_USER_TOKENS_PER_USER as u64 {
        return Err(AdminTokenWriteRepositoryError::LimitReached);
    }
    Ok(())
}

async fn ensure_existing_owner_and_group(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    group_id: Option<GroupId>,
) -> Result<(), AdminTokenWriteRepositoryError> {
    let owner = lock_non_deleted_owner(transaction, user_id)
        .await?
        .ok_or_else(|| record_internal_error(AdminTokenWriteRepositoryError::Invariant))?;
    if !matches!(owner.status(), 1 | 2) {
        return Err(record_internal_error(
            AdminTokenWriteRepositoryError::Invariant,
        ));
    }
    let effective_group_id = group_id.unwrap_or(owner.default_group_id());
    if active_group_exists(transaction, effective_group_id).await? {
        Ok(())
    } else if group_id.is_some() {
        Err(AdminTokenWriteRepositoryError::InvalidReference)
    } else {
        Err(record_internal_error(
            AdminTokenWriteRepositoryError::Invariant,
        ))
    }
}

async fn active_token_owner(
    transaction: &DatabaseTransaction,
    token_id: TokenId,
) -> Result<Option<UserId>, AdminTokenWriteRepositoryError> {
    let query = Query::select()
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::UserId)),
            Alias::new("user_id"),
        )
        .from(tokens::Entity)
        .and_where(Expr::col((tokens::Entity, tokens::Column::Id)).eq(token_id.get()))
        .and_where(Expr::col((tokens::Entity, tokens::Column::OrganizationId)).is_null())
        .and_where(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
        .and_where(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned();
    let mut rows = query_all(transaction, query).await?;
    match rows.len() {
        0 => Ok(None),
        1 => {
            let value: i64 = rows
                .pop()
                .ok_or_else(|| record_internal_error(AdminTokenWriteRepositoryError::Invariant))?
                .try_get("", "user_id")
                .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Invariant))?;
            UserId::new(value)
                .map(Some)
                .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Invariant))
        }
        _ => Err(record_internal_error(
            AdminTokenWriteRepositoryError::Invariant,
        )),
    }
}

async fn active_group_exists(
    transaction: &DatabaseTransaction,
    group_id: GroupId,
) -> Result<bool, AdminTokenWriteRepositoryError> {
    let query = Query::select()
        .expr_as(
            Expr::col((groups::Entity, groups::Column::Id)),
            Alias::new("id"),
        )
        .from(groups::Entity)
        .and_where(Expr::col((groups::Entity, groups::Column::Id)).eq(group_id.get()))
        .and_where(Expr::col((groups::Entity, groups::Column::DeletedAt)).is_null())
        .limit(1)
        .to_owned();
    Ok(!query_all(transaction, query).await?.is_empty())
}

async fn query_all(
    transaction: &DatabaseTransaction,
    query: SelectStatement,
) -> Result<Vec<QueryResult>, AdminTokenWriteRepositoryError> {
    let statement = transaction.get_database_backend().build(&query);
    transaction
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(AdminTokenWriteRepositoryError::Query))
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

fn map_write_db_error(error: DbErr) -> AdminTokenWriteRepositoryError {
    let rendered = error.to_string();
    if rendered.contains("FOREIGN KEY") || rendered.contains("foreign key") {
        return AdminTokenWriteRepositoryError::InvalidReference;
    }
    record_internal_error(AdminTokenWriteRepositoryError::Query)
}

fn record_internal_error(error: AdminTokenWriteRepositoryError) -> AdminTokenWriteRepositoryError {
    let error_kind = match error {
        AdminTokenWriteRepositoryError::InvalidInput => "admin_token_write_invalid_input",
        AdminTokenWriteRepositoryError::InvalidReference => "admin_token_write_invalid_reference",
        AdminTokenWriteRepositoryError::LimitReached => return error,
        AdminTokenWriteRepositoryError::Query => "admin_token_write_query",
        AdminTokenWriteRepositoryError::Timeout => "admin_token_write_timeout",
        AdminTokenWriteRepositoryError::Invariant => "admin_token_write_invariant",
    };
    tracing::error!(
        target: "af_db::admin_token_write",
        error_kind,
        "管理令牌写入仓储发生内部错误"
    );
    error
}

impl From<TokenOwnerGuardError> for AdminTokenWriteRepositoryError {
    fn from(error: TokenOwnerGuardError) -> Self {
        match error {
            TokenOwnerGuardError::Query => {
                record_internal_error(AdminTokenWriteRepositoryError::Query)
            }
            TokenOwnerGuardError::Invariant => {
                record_internal_error(AdminTokenWriteRepositoryError::Invariant)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_records_redact_key_material_and_configuration() {
        let fields = AdminTokenWriteRecord::new(
            "private-token".to_owned(),
            1,
            None,
            100,
            false,
            None,
            Some(vec!["private-model".to_owned()]),
            Some(vec!["192.0.2.1".to_owned()]),
            false,
            None,
            None,
            None,
            None,
        );
        let create = AdminTokenCreateRecord::new(
            UserId::new(1).unwrap(),
            "11".repeat(32),
            "sk-af-public000001".to_owned(),
            fields,
        );
        assert_eq!(format!("{create:?}"), "AdminTokenCreateRecord(<redacted>)");
        assert!(!format!("{create:?}").contains("private-model"));
    }
}
