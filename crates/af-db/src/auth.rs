use std::{fmt, time::Duration};

use af_domain::{GatewayPrincipal, GroupId, TokenId, UserId};
use sea_orm::{
    ConnectionTrait, DbErr, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, Func, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{TokenHash, groups, tokens, users},
};

/// 只携带规范 SHA-256 摘要的令牌鉴权查询值。
///
/// 调用方必须先在同步边界将明文 API Key 派生为摘要并释放明文，再进入异步数据库查询。
#[derive(Clone, Eq, PartialEq)]
pub struct TokenAuthLookup(TokenHash);

impl TokenAuthLookup {
    /// 校验并构造与 `tokens.key_hash` 一致的 64 位小写十六进制摘要。
    pub fn new(digest: &str) -> Result<Self, TokenAuthLookupError> {
        TokenHash::parse(digest)
            .map(Self)
            .map_err(|_| TokenAuthLookupError::InvalidDigest)
    }
}

impl fmt::Debug for TokenAuthLookup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenAuthLookup(<redacted>)")
    }
}

/// 令牌查询摘要格式错误；不保留原始输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenAuthLookupError {
    /// 摘要不是规范的小写 SHA-256 十六进制文本。
    #[error("令牌鉴权摘要必须是 64 位小写十六进制字符串")]
    InvalidDigest,
}

/// 完成持久化校验后的令牌鉴权结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenAuthLookupOutcome {
    /// 令牌、用户和有效分组均已通过校验。
    Authenticated(GatewayPrincipal),
    /// Key 不存在，或关联记录已禁用、软删除或过期。
    Rejected,
}

/// 令牌鉴权仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenAuthRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("令牌鉴权查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 令牌鉴权仓储内部错误；不暴露数据库诊断或查询值。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenAuthRepositoryError {
    /// 获取连接或执行查询失败。
    #[error("令牌鉴权数据库查询失败")]
    Query,
    /// 数据库查询超过配置的硬截止时间。
    #[error("令牌鉴权数据库查询超时")]
    Timeout,
    /// 持久化结果违反状态、关联或标识不变量。
    #[error("令牌鉴权持久化状态损坏")]
    Invariant,
}

/// 使用数据库持久化状态校验下游 API Key 的仓储。
#[derive(Clone)]
pub struct TokenAuthRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl TokenAuthRepository {
    /// 使用共享数据库连接池和单次完整查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, TokenAuthRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(TokenAuthRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按完整摘要查询并原子校验令牌、所属用户和有效分组。
    pub async fn lookup(
        &self,
        lookup: &TokenAuthLookup,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        match timeout(self.lookup_timeout, self.lookup_inner(lookup)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(TokenAuthRepositoryError::Timeout)),
        }
    }

    async fn lookup_inner(
        &self,
        lookup: &TokenAuthLookup,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        let connection = self.pool.connection();
        let statement = connection
            .get_database_backend()
            .build(&lookup_query(lookup));
        let mut results = connection
            .query_all(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
        let result = match results.len() {
            0 => return Ok(TokenAuthLookupOutcome::Rejected),
            1 => results
                .pop()
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            _ => {
                return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
            }
        };
        let row = TokenAuthRow::try_from_query_result(&result)
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        row.validate()
    }
}

impl fmt::Debug for TokenAuthRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TokenAuthRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

/// 构造只读取鉴权所需字段的单语句查询，避免加载用户密码和 TOTP 等敏感列。
fn lookup_query(lookup: &TokenAuthLookup) -> SelectStatement {
    Query::select()
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Id)),
            Alias::new("token_id"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Status)),
            Alias::new("token_status"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::ExpiredAt)),
            Alias::new("token_expired_at"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::DeletedAt)),
            Alias::new("token_deleted_at"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Id)),
            Alias::new("user_id"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::Status)),
            Alias::new("user_status"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::DeletedAt)),
            Alias::new("user_deleted_at"),
        )
        .expr_as(
            Expr::col((groups::Entity, groups::Column::Id)),
            Alias::new("group_id"),
        )
        .expr_as(
            Expr::col((groups::Entity, groups::Column::DeletedAt)),
            Alias::new("group_deleted_at"),
        )
        .from(tokens::Entity)
        .left_join(
            users::Entity,
            Expr::col((tokens::Entity, tokens::Column::UserId))
                .equals((users::Entity, users::Column::Id)),
        )
        .left_join(
            groups::Entity,
            Expr::col((groups::Entity, groups::Column::Id)).eq(Func::coalesce([
                Expr::col((tokens::Entity, tokens::Column::GroupId)).into(),
                Expr::col((users::Entity, users::Column::DefaultGroupId)).into(),
            ])),
        )
        .and_where(Expr::col((tokens::Entity, tokens::Column::KeyHash)).eq(lookup.0.clone()))
        .limit(2)
        .to_owned()
}

struct TokenAuthRow {
    token_id: i64,
    token_status: i16,
    token_expired_at: Option<TimeDateTimeWithTimeZone>,
    token_deleted_at: Option<TimeDateTimeWithTimeZone>,
    user_id: Option<i64>,
    user_status: Option<i16>,
    user_deleted_at: Option<TimeDateTimeWithTimeZone>,
    group_id: Option<i64>,
    group_deleted_at: Option<TimeDateTimeWithTimeZone>,
}

impl TokenAuthRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            token_id: result.try_get("", "token_id")?,
            token_status: result.try_get("", "token_status")?,
            token_expired_at: result.try_get("", "token_expired_at")?,
            token_deleted_at: result.try_get("", "token_deleted_at")?,
            user_id: result.try_get("", "user_id")?,
            user_status: result.try_get("", "user_status")?,
            user_deleted_at: result.try_get("", "user_deleted_at")?,
            group_id: result.try_get("", "group_id")?,
            group_deleted_at: result.try_get("", "group_deleted_at")?,
        })
    }

    fn validate(self) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        let token_enabled = enabled_or_rejected(self.token_status)?;
        let Some(user_id) = self.user_id else {
            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
        };
        let Some(user_status) = self.user_status else {
            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
        };
        let user_enabled = enabled_or_rejected(user_status)?;
        let Some(group_id) = self.group_id else {
            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
        };
        let principal = GatewayPrincipal::new(
            TokenId::new(self.token_id)
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            UserId::new(user_id)
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            GroupId::new(group_id)
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
        );

        if !token_enabled
            || self.token_deleted_at.is_some()
            || self
                .token_expired_at
                .is_some_and(|expired_at| expired_at <= TimeDateTimeWithTimeZone::now_utc())
            || !user_enabled
            || self.user_deleted_at.is_some()
            || self.group_deleted_at.is_some()
        {
            return Ok(TokenAuthLookupOutcome::Rejected);
        }
        Ok(TokenAuthLookupOutcome::Authenticated(principal))
    }
}

/// 用户和令牌状态只有 1/2 属于已知持久化契约，其他值表示数据损坏。
fn enabled_or_rejected(status: i16) -> Result<bool, TokenAuthRepositoryError> {
    match status {
        1 => Ok(true),
        2 => Ok(false),
        _ => Err(record_internal_error(TokenAuthRepositoryError::Invariant)),
    }
}

/// 只记录闭合内部分类，禁止把查询值或底层数据库诊断写入日志。
fn record_internal_error(error: TokenAuthRepositoryError) -> TokenAuthRepositoryError {
    let error_kind = match error {
        TokenAuthRepositoryError::Query => "token_auth_query",
        TokenAuthRepositoryError::Timeout => "token_auth_timeout",
        TokenAuthRepositoryError::Invariant => "token_auth_invariant",
    };
    tracing::error!(
        target: "af_db::auth",
        error_kind,
        "令牌鉴权仓储发生内部错误"
    );
    error
}
