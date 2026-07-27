use std::{fmt, time::Duration};

use af_domain::{GatewayPrincipal, GroupId, TokenId, TokenModelPolicy, TrustedClientIp, UserId};
use sea_orm::{
    ConnectionTrait, DbBackend, DbErr, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, ExprTrait, Func, Query, SelectStatement, SimpleExpr},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{
        MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES, MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES,
        TokenHash, TokenIpAllowlist, TokenModelAllowlist, groups, tokens, users,
    },
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenAuthLookupOutcome {
    /// 令牌、用户、有效分组和客户端 IP 均已通过校验，并携带本次请求的策略快照。
    Authenticated {
        /// 已验证且只携带稳定标识的网关主体。
        principal: GatewayPrincipal,
        /// 已完整解码的不可变令牌模型策略。
        model_policy: TokenModelPolicy,
    },
    /// Key 不存在，或状态、软删除、过期、客户端 IP 不允许继续访问。
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

/// 使用数据库持久化状态与可信客户端 IP 校验下游 API Key 的仓储。
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

    /// 按完整摘要查询并校验令牌、所属用户、有效分组和客户端 IP。
    pub async fn lookup(
        &self,
        lookup: &TokenAuthLookup,
        client_ip: TrustedClientIp,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        match timeout(self.lookup_timeout, self.lookup_inner(lookup, client_ip)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(TokenAuthRepositoryError::Timeout)),
        }
    }

    async fn lookup_inner(
        &self,
        lookup: &TokenAuthLookup,
        client_ip: TrustedClientIp,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        let connection = self.pool.connection();
        let database_backend = connection.get_database_backend();
        let statement = database_backend.build(&lookup_query(database_backend, lookup));
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
        row.validate(client_ip)
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
fn lookup_query(database_backend: DbBackend, lookup: &TokenAuthLookup) -> SelectStatement {
    let model_limits_length =
        token_json_serialized_length(database_backend, tokens::Column::ModelLimits);
    let allow_ips_length = token_json_serialized_length(database_backend, tokens::Column::AllowIps);
    let model_limits_column = || Expr::col((tokens::Entity, tokens::Column::ModelLimits));
    let allow_ips_column = || Expr::col((tokens::Entity, tokens::Column::AllowIps));

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
            Expr::case(
                model_limits_length
                    .clone()
                    .lte(MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES),
                model_limits_column(),
            )
            .finally(Expr::value(Option::<sea_orm::JsonValue>::None)),
            Alias::new("token_model_limits"),
        )
        .expr_as(
            Expr::case(
                model_limits_column()
                    .is_not_null()
                    .and(model_limits_length.gt(MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES)),
                true,
            )
            .finally(false),
            Alias::new("token_model_limits_oversized"),
        )
        .expr_as(
            Expr::case(
                allow_ips_length
                    .clone()
                    .lte(MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES),
                allow_ips_column(),
            )
            .finally(Expr::value(Option::<sea_orm::JsonValue>::None)),
            Alias::new("token_allow_ips"),
        )
        .expr_as(
            Expr::case(
                allow_ips_column()
                    .is_not_null()
                    .and(allow_ips_length.gt(MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES)),
                true,
            )
            .finally(false),
            Alias::new("token_allow_ips_oversized"),
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

/// 按数据库方言计算 JSON 序列化后的字节长度，避免把超大白名单传回应用层。
pub(crate) fn token_json_serialized_length(
    database_backend: DbBackend,
    column: tokens::Column,
) -> SimpleExpr {
    let value = Expr::col((tokens::Entity, column));
    match database_backend {
        DbBackend::Postgres => Func::cust(Alias::new("OCTET_LENGTH"))
            .arg(value.cast_as(Alias::new("TEXT")))
            .into(),
        DbBackend::MySql => Func::cust(Alias::new("OCTET_LENGTH"))
            .arg(value.cast_as(Alias::new("CHAR")))
            .into(),
        DbBackend::Sqlite => {
            // SQLite 保留 JSON 原始文本，额外空白与转义同样计入持久化字节上限。
            Func::char_length(value.cast_as(Alias::new("BLOB"))).into()
        }
    }
}

struct TokenAuthRow {
    token_id: i64,
    token_status: i16,
    token_expired_at: Option<TimeDateTimeWithTimeZone>,
    token_deleted_at: Option<TimeDateTimeWithTimeZone>,
    token_model_limits: Option<TokenModelAllowlist>,
    token_model_limits_oversized: bool,
    token_allow_ips: Option<TokenIpAllowlist>,
    token_allow_ips_oversized: bool,
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
            token_model_limits: result.try_get("", "token_model_limits")?,
            token_model_limits_oversized: result.try_get("", "token_model_limits_oversized")?,
            token_allow_ips: result.try_get("", "token_allow_ips")?,
            token_allow_ips_oversized: result.try_get("", "token_allow_ips_oversized")?,
            user_id: result.try_get("", "user_id")?,
            user_status: result.try_get("", "user_status")?,
            user_deleted_at: result.try_get("", "user_deleted_at")?,
            group_id: result.try_get("", "group_id")?,
            group_deleted_at: result.try_get("", "group_deleted_at")?,
        })
    }

    fn validate(
        self,
        client_ip: TrustedClientIp,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        if self.token_model_limits_oversized || self.token_allow_ips_oversized {
            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
        }
        let model_policy = self.token_model_limits.map_or_else(
            TokenModelPolicy::unrestricted,
            TokenModelAllowlist::into_policy,
        );
        let token_enabled = enabled_or_rejected(self.token_status)?;
        let ip_allowed = self
            .token_allow_ips
            .as_ref()
            .is_none_or(|allowlist| allowlist.allows(client_ip));
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
            || !ip_allowed
        {
            return Ok(TokenAuthLookupOutcome::Rejected);
        }
        Ok(TokenAuthLookupOutcome::Authenticated {
            principal,
            model_policy,
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_size_guards_use_each_database_byte_length_dialect() {
        let lookup = TokenAuthLookup::new(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        )
        .expect("固定摘要必须有效");
        let cases = [
            (
                DbBackend::Postgres,
                "OCTET_LENGTH(CAST(\"tokens\".\"allow_ips\" AS TEXT))",
            ),
            (
                DbBackend::MySql,
                "OCTET_LENGTH(CAST(`tokens`.`allow_ips` AS CHAR))",
            ),
            (
                DbBackend::Sqlite,
                "LENGTH(CAST(\"tokens\".\"allow_ips\" AS BLOB))",
            ),
        ];

        for (database_backend, length_expression) in cases {
            let statement = database_backend.build(&lookup_query(database_backend, &lookup));
            let rendered = statement.to_string();
            assert_eq!(statement.sql.matches(length_expression).count(), 2);
            let model_length_expression = length_expression.replace("allow_ips", "model_limits");
            assert_eq!(statement.sql.matches(&model_length_expression).count(), 2);
            assert!(rendered.contains("<= 4352"), "{rendered}");
            assert!(rendered.contains("> 4352"), "{rendered}");
            assert!(rendered.contains("<= 69632"), "{rendered}");
            assert!(rendered.contains("> 69632"), "{rendered}");
            assert_eq!(rendered.matches("ELSE NULL END)").count(), 2);
            assert_eq!(rendered.matches("ELSE FALSE END)").count(), 2);
            assert!(statement.sql.contains("token_model_limits_oversized"));
            assert!(statement.sql.contains("token_allow_ips_oversized"));
        }
    }
}
