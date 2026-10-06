use std::{fmt, time::Duration};

use af_domain::{GroupId, PLAYGROUND_TOKEN_NAME, TokenId, UserId};
use sea_orm::{
    ConnectionTrait, DbBackend, DbErr, QueryResult,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, ExprTrait, Func, Order, Query, SelectStatement},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    auth::token_json_serialized_length,
    entity::{
        MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES, MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES,
        TokenIpAllowlist, TokenModelAllowlist, groups, tokens, users,
    },
};

/// 单页令牌查询允许返回的最大记录数。
pub const MAX_ADMIN_TOKEN_PAGE_SIZE: usize = 100;

/// 管理端可读取的非敏感令牌快照。
pub struct AdminTokenRecord {
    token_id: TokenId,
    user_id: UserId,
    key_prefix: String,
    name: String,
    status: i16,
    group_id: Option<GroupId>,
    remain_quota: i64,
    unlimited_quota: bool,
    used_quota: i64,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    cross_group_retry: bool,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    usage_5h: i64,
    usage_1d: i64,
    usage_7d: i64,
    window_5h_start: i64,
    window_1d_start: i64,
    window_7d_start: i64,
    max_requests: Option<i64>,
    used_requests: i64,
    created_at: i64,
    updated_at: i64,
}

impl AdminTokenRecord {
    /// 返回令牌主键。
    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.token_id
    }

    /// 返回令牌所属用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回不可用于鉴权的展示前缀。
    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    /// 返回令牌名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回数据库令牌状态码。
    #[must_use]
    pub const fn status(&self) -> i16 {
        self.status
    }

    /// 返回可选的强制绑定分组。
    #[must_use]
    pub const fn group_id(&self) -> Option<GroupId> {
        self.group_id
    }

    /// 返回有限令牌的剩余额度。
    #[must_use]
    pub const fn remain_quota(&self) -> i64 {
        self.remain_quota
    }

    /// 返回令牌是否不受令牌级额度限制。
    #[must_use]
    pub const fn unlimited_quota(&self) -> bool {
        self.unlimited_quota
    }

    /// 返回令牌累计已用额度。
    #[must_use]
    pub const fn used_quota(&self) -> i64 {
        self.used_quota
    }

    /// 返回可选过期时间的 Unix 秒数。
    #[must_use]
    pub const fn expired_at(&self) -> Option<i64> {
        self.expired_at
    }

    /// 返回可选模型白名单，保留持久化顺序和重复项。
    #[must_use]
    pub fn model_limits(&self) -> Option<&[String]> {
        self.model_limits.as_deref()
    }

    /// 返回可选 IP/CIDR 白名单，保留持久化顺序和重复项。
    #[must_use]
    pub fn allow_ips(&self) -> Option<&[String]> {
        self.allow_ips.as_deref()
    }

    /// 返回是否允许自动分组场景跨组重试。
    #[must_use]
    pub const fn cross_group_retry(&self) -> bool {
        self.cross_group_retry
    }

    /// 返回 5 小时窗口额度上限。
    #[must_use]
    pub const fn rate_limit_5h(&self) -> Option<i64> {
        self.rate_limit_5h
    }

    /// 返回 1 天窗口额度上限。
    #[must_use]
    pub const fn rate_limit_1d(&self) -> Option<i64> {
        self.rate_limit_1d
    }

    /// 返回 7 天窗口额度上限。
    #[must_use]
    pub const fn rate_limit_7d(&self) -> Option<i64> {
        self.rate_limit_7d
    }

    /// 返回 5 小时窗口已用额度。
    #[must_use]
    pub const fn usage_5h(&self) -> i64 {
        self.usage_5h
    }

    /// 返回 1 天窗口已用额度。
    #[must_use]
    pub const fn usage_1d(&self) -> i64 {
        self.usage_1d
    }

    /// 返回 7 天窗口已用额度。
    #[must_use]
    pub const fn usage_7d(&self) -> i64 {
        self.usage_7d
    }

    /// 返回 5 小时窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn window_5h_start(&self) -> i64 {
        self.window_5h_start
    }

    /// 返回 1 天窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn window_1d_start(&self) -> i64 {
        self.window_1d_start
    }

    /// 返回 7 天窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn window_7d_start(&self) -> i64 {
        self.window_7d_start
    }

    /// 返回可选请求数上限。
    #[must_use]
    pub const fn max_requests(&self) -> Option<i64> {
        self.max_requests
    }

    /// 返回当前已用请求数。
    #[must_use]
    pub const fn used_requests(&self) -> i64 {
        self.used_requests
    }

    /// 返回令牌创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回令牌最近更新时间的 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

impl fmt::Debug for AdminTokenRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminTokenRecord(<redacted>)")
    }
}

/// 一页有界令牌结果。
pub struct AdminTokenPageRecord {
    tokens: Vec<AdminTokenRecord>,
    next_cursor: Option<TokenId>,
}

impl AdminTokenPageRecord {
    /// 消费页面并返回令牌记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminTokenRecord>, Option<TokenId>) {
        (self.tokens, self.next_cursor)
    }
}

impl fmt::Debug for AdminTokenPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminTokenPageRecord(<redacted>)")
    }
}

/// 令牌详情查询结果。
pub enum AdminTokenLookupOutcome {
    /// 找到当前未软删除令牌。
    Found(Box<AdminTokenRecord>),
    /// 令牌不存在或已经软删除。
    NotFound,
}

/// 管理令牌仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminTokenRepositoryConfigError {
    /// 零超时无法形成有效的数据库查询截止时间。
    #[error("管理令牌查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 管理令牌仓储内部错误；不携带密钥、摘要、白名单或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminTokenRepositoryError {
    /// 获取连接或执行查询失败。
    #[error("管理令牌数据库查询失败")]
    Query,
    /// 查询超过配置的硬截止时间。
    #[error("管理令牌数据库查询超时")]
    Timeout,
    /// 查询输入、关联或持久化结果违反不变量。
    #[error("管理令牌持久化状态损坏")]
    Invariant,
}

/// 管理端令牌列表与详情共用的只读仓储。
#[derive(Clone)]
pub struct AdminTokenRepository {
    pub(super) pool: DatabasePool,
    pub(super) lookup_timeout: Duration,
}

impl AdminTokenRepository {
    /// 使用共享数据库连接池和单次查询截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, AdminTokenRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(AdminTokenRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按单调令牌 ID 游标读取一页未软删除令牌。
    pub async fn list(
        &self,
        after: Option<TokenId>,
        limit: usize,
    ) -> Result<AdminTokenPageRecord, AdminTokenRepositoryError> {
        if !(1..=MAX_ADMIN_TOKEN_PAGE_SIZE).contains(&limit) {
            return Err(record_internal_error(AdminTokenRepositoryError::Invariant));
        }
        let mut results = match timeout(
            self.lookup_timeout,
            self.query_all(list_query(self.database_backend(), after, limit)),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => return Err(record_internal_error(AdminTokenRepositoryError::Timeout)),
        };
        let has_more = results.len() > limit;
        if has_more {
            results.truncate(limit);
        }
        let tokens = results
            .iter()
            .map(AdminTokenRecord::try_from_query_result)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| tokens.last().map(AdminTokenRecord::token_id))
            .flatten();
        Ok(AdminTokenPageRecord {
            tokens,
            next_cursor,
        })
    }

    /// 按稳定令牌 ID 查询当前未软删除令牌。
    pub async fn get(
        &self,
        token_id: TokenId,
    ) -> Result<AdminTokenLookupOutcome, AdminTokenRepositoryError> {
        let mut results = match timeout(
            self.lookup_timeout,
            self.query_all(detail_query(self.database_backend(), token_id)),
        )
        .await
        {
            Ok(result) => result?,
            Err(_) => return Err(record_internal_error(AdminTokenRepositoryError::Timeout)),
        };
        match results.len() {
            0 => Ok(AdminTokenLookupOutcome::NotFound),
            1 => {
                Ok(AdminTokenLookupOutcome::Found(Box::new(
                    AdminTokenRecord::try_from_query_result(&results.pop().ok_or_else(|| {
                        record_internal_error(AdminTokenRepositoryError::Invariant)
                    })?)?,
                )))
            }
            _ => Err(record_internal_error(AdminTokenRepositoryError::Invariant)),
        }
    }

    fn database_backend(&self) -> DbBackend {
        self.pool.connection().get_database_backend()
    }

    async fn query_all(
        &self,
        query: SelectStatement,
    ) -> Result<Vec<QueryResult>, AdminTokenRepositoryError> {
        let connection = self.pool.connection();
        let statement = connection.get_database_backend().build(&query);
        connection
            .query_all(statement)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(AdminTokenRepositoryError::Query))
    }
}

impl fmt::Debug for AdminTokenRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminTokenRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

impl AdminTokenRecord {
    pub(crate) fn try_from_query_result(
        result: &QueryResult,
    ) -> Result<Self, AdminTokenRepositoryError> {
        let row = AdminTokenRow::try_from_query_result(result)
            .map_err(|_| record_internal_error(AdminTokenRepositoryError::Invariant))?;
        row.validate()
    }
}

struct AdminTokenRow {
    token_id: i64,
    user_id: Option<i64>,
    user_status: Option<i16>,
    user_deleted_at: Option<TimeDateTimeWithTimeZone>,
    effective_group_id: Option<i64>,
    effective_group_deleted_at: Option<TimeDateTimeWithTimeZone>,
    key_prefix: String,
    name: String,
    status: i16,
    group_id: Option<i64>,
    remain_quota: i64,
    unlimited_quota: bool,
    used_quota: i64,
    expired_at: Option<TimeDateTimeWithTimeZone>,
    model_limits: Option<TokenModelAllowlist>,
    model_limits_oversized: bool,
    allow_ips: Option<TokenIpAllowlist>,
    allow_ips_oversized: bool,
    cross_group_retry: bool,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    usage_5h: i64,
    usage_1d: i64,
    usage_7d: i64,
    window_5h_start: TimeDateTimeWithTimeZone,
    window_1d_start: TimeDateTimeWithTimeZone,
    window_7d_start: TimeDateTimeWithTimeZone,
    max_requests: Option<i64>,
    used_requests: i64,
    created_at: TimeDateTimeWithTimeZone,
    updated_at: TimeDateTimeWithTimeZone,
}

impl AdminTokenRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            token_id: result.try_get("", "token_id")?,
            user_id: result.try_get("", "user_id")?,
            user_status: result.try_get("", "user_status")?,
            user_deleted_at: result.try_get("", "user_deleted_at")?,
            effective_group_id: result.try_get("", "effective_group_id")?,
            effective_group_deleted_at: result.try_get("", "effective_group_deleted_at")?,
            key_prefix: result.try_get("", "key_prefix")?,
            name: result.try_get("", "name")?,
            status: result.try_get("", "status")?,
            group_id: result.try_get("", "group_id")?,
            remain_quota: result.try_get("", "remain_quota")?,
            unlimited_quota: result.try_get("", "unlimited_quota")?,
            used_quota: result.try_get("", "used_quota")?,
            expired_at: result.try_get("", "expired_at")?,
            model_limits: result.try_get("", "model_limits")?,
            model_limits_oversized: result.try_get("", "model_limits_oversized")?,
            allow_ips: result.try_get("", "allow_ips")?,
            allow_ips_oversized: result.try_get("", "allow_ips_oversized")?,
            cross_group_retry: result.try_get("", "cross_group_retry")?,
            rate_limit_5h: result.try_get("", "rate_limit_5h")?,
            rate_limit_1d: result.try_get("", "rate_limit_1d")?,
            rate_limit_7d: result.try_get("", "rate_limit_7d")?,
            usage_5h: result.try_get("", "usage_5h")?,
            usage_1d: result.try_get("", "usage_1d")?,
            usage_7d: result.try_get("", "usage_7d")?,
            window_5h_start: result.try_get("", "window_5h_start")?,
            window_1d_start: result.try_get("", "window_1d_start")?,
            window_7d_start: result.try_get("", "window_7d_start")?,
            max_requests: result.try_get("", "max_requests")?,
            used_requests: result.try_get("", "used_requests")?,
            created_at: result.try_get("", "created_at")?,
            updated_at: result.try_get("", "updated_at")?,
        })
    }

    fn validate(self) -> Result<AdminTokenRecord, AdminTokenRepositoryError> {
        let Some(user_id) = self.user_id else {
            return Err(record_internal_error(AdminTokenRepositoryError::Invariant));
        };
        let Some(user_status) = self.user_status else {
            return Err(record_internal_error(AdminTokenRepositoryError::Invariant));
        };
        let Some(effective_group_id) = self.effective_group_id else {
            return Err(record_internal_error(AdminTokenRepositoryError::Invariant));
        };
        if self.model_limits_oversized
            || self.allow_ips_oversized
            || self.user_deleted_at.is_some()
            || self.effective_group_deleted_at.is_some()
            || !matches!(user_status, 1 | 2)
            || !valid_key_prefix(&self.key_prefix)
            || !valid_text(&self.name, 128)
            || !matches!(self.status, 1 | 2)
            || [
                self.remain_quota,
                self.used_quota,
                self.usage_5h,
                self.usage_1d,
                self.usage_7d,
                self.used_requests,
            ]
            .into_iter()
            .any(|value| value < 0)
            || [
                self.rate_limit_5h,
                self.rate_limit_1d,
                self.rate_limit_7d,
                self.max_requests,
            ]
            .into_iter()
            .flatten()
            .any(|value| value < 0)
        {
            return Err(record_internal_error(AdminTokenRepositoryError::Invariant));
        }
        let user_id = UserId::new(user_id)
            .map_err(|_| record_internal_error(AdminTokenRepositoryError::Invariant))?;
        GroupId::new(effective_group_id)
            .map_err(|_| record_internal_error(AdminTokenRepositoryError::Invariant))?;
        let group_id = self
            .group_id
            .map(GroupId::new)
            .transpose()
            .map_err(|_| record_internal_error(AdminTokenRepositoryError::Invariant))?;
        let model_limits = self
            .model_limits
            .map(TokenModelAllowlist::into_raw)
            .map(validated_string_array)
            .transpose()?;
        let allow_ips = self
            .allow_ips
            .map(TokenIpAllowlist::into_raw)
            .map(validated_string_array)
            .transpose()?;
        Ok(AdminTokenRecord {
            token_id: TokenId::new(self.token_id)
                .map_err(|_| record_internal_error(AdminTokenRepositoryError::Invariant))?,
            user_id,
            key_prefix: self.key_prefix,
            name: self.name,
            status: self.status,
            group_id,
            remain_quota: self.remain_quota,
            unlimited_quota: self.unlimited_quota,
            used_quota: self.used_quota,
            expired_at: self.expired_at.map(|value| value.unix_timestamp()),
            model_limits,
            allow_ips,
            cross_group_retry: self.cross_group_retry,
            rate_limit_5h: self.rate_limit_5h,
            rate_limit_1d: self.rate_limit_1d,
            rate_limit_7d: self.rate_limit_7d,
            usage_5h: self.usage_5h,
            usage_1d: self.usage_1d,
            usage_7d: self.usage_7d,
            window_5h_start: self.window_5h_start.unix_timestamp(),
            window_1d_start: self.window_1d_start.unix_timestamp(),
            window_7d_start: self.window_7d_start.unix_timestamp(),
            max_requests: self.max_requests,
            used_requests: self.used_requests,
            created_at: self.created_at.unix_timestamp(),
            updated_at: self.updated_at.unix_timestamp(),
        })
    }
}

fn list_query(
    database_backend: DbBackend,
    after: Option<TokenId>,
    limit: usize,
) -> SelectStatement {
    let mut query = base_query(database_backend);
    query
        .and_where(Expr::col((tokens::Entity, tokens::Column::OrganizationId)).is_null())
        .and_where(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
        .and_where(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
        .order_by((tokens::Entity, tokens::Column::Id), Order::Asc)
        .limit((limit + 1) as u64);
    if let Some(after) = after {
        query.and_where(Expr::col((tokens::Entity, tokens::Column::Id)).gt(after.get()));
    }
    query.to_owned()
}

pub(super) fn detail_query(database_backend: DbBackend, token_id: TokenId) -> SelectStatement {
    base_query(database_backend)
        .and_where(Expr::col((tokens::Entity, tokens::Column::Id)).eq(token_id.get()))
        .and_where(Expr::col((tokens::Entity, tokens::Column::OrganizationId)).is_null())
        .and_where(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
        .and_where(Expr::col((tokens::Entity, tokens::Column::DeletedAt)).is_null())
        .limit(2)
        .to_owned()
}

pub(crate) fn base_query(database_backend: DbBackend) -> SelectStatement {
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
            Alias::new("effective_group_id"),
        )
        .expr_as(
            Expr::col((groups::Entity, groups::Column::DeletedAt)),
            Alias::new("effective_group_deleted_at"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::KeyPrefix)),
            Alias::new("key_prefix"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Name)),
            Alias::new("name"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Status)),
            Alias::new("status"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::GroupId)),
            Alias::new("group_id"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::RemainQuota)),
            Alias::new("remain_quota"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::UnlimitedQuota)),
            Alias::new("unlimited_quota"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::UsedQuota)),
            Alias::new("used_quota"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::ExpiredAt)),
            Alias::new("expired_at"),
        )
        .expr_as(
            Expr::case(
                model_limits_length
                    .clone()
                    .lte(MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES),
                model_limits_column(),
            )
            .finally(Expr::value(Option::<sea_orm::JsonValue>::None)),
            Alias::new("model_limits"),
        )
        .expr_as(
            Expr::case(
                model_limits_column()
                    .is_not_null()
                    .and(model_limits_length.gt(MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES)),
                true,
            )
            .finally(false),
            Alias::new("model_limits_oversized"),
        )
        .expr_as(
            Expr::case(
                allow_ips_length
                    .clone()
                    .lte(MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES),
                allow_ips_column(),
            )
            .finally(Expr::value(Option::<sea_orm::JsonValue>::None)),
            Alias::new("allow_ips"),
        )
        .expr_as(
            Expr::case(
                allow_ips_column()
                    .is_not_null()
                    .and(allow_ips_length.gt(MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES)),
                true,
            )
            .finally(false),
            Alias::new("allow_ips_oversized"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::CrossGroupRetry)),
            Alias::new("cross_group_retry"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::RateLimit5h)),
            Alias::new("rate_limit_5h"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::RateLimit1d)),
            Alias::new("rate_limit_1d"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::RateLimit7d)),
            Alias::new("rate_limit_7d"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Usage5h)),
            Alias::new("usage_5h"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Usage1d)),
            Alias::new("usage_1d"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Usage7d)),
            Alias::new("usage_7d"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Window5hStart)),
            Alias::new("window_5h_start"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Window1dStart)),
            Alias::new("window_1d_start"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::Window7dStart)),
            Alias::new("window_7d_start"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::MaxRequests)),
            Alias::new("max_requests"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::UsedRequests)),
            Alias::new("used_requests"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::CreatedAt)),
            Alias::new("created_at"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::UpdatedAt)),
            Alias::new("updated_at"),
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
        .to_owned()
}

fn validated_string_array(
    value: serde_json::Value,
) -> Result<Vec<String>, AdminTokenRepositoryError> {
    let serde_json::Value::Array(entries) = value else {
        return Err(record_internal_error(AdminTokenRepositoryError::Invariant));
    };
    entries
        .into_iter()
        .map(|entry| match entry {
            serde_json::Value::String(value) => Ok(value),
            _ => Err(record_internal_error(AdminTokenRepositoryError::Invariant)),
        })
        .collect()
}

fn valid_key_prefix(value: &str) -> bool {
    value.len() == 18
        && value.starts_with("sk-af-")
        && value[6..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

fn record_internal_error(error: AdminTokenRepositoryError) -> AdminTokenRepositoryError {
    let error_kind = match error {
        AdminTokenRepositoryError::Query => "admin_token_query",
        AdminTokenRepositoryError::Timeout => "admin_token_timeout",
        AdminTokenRepositoryError::Invariant => "admin_token_invariant",
    };
    tracing::error!(
        target: "af_db::admin_token",
        error_kind,
        "管理令牌仓储发生内部错误"
    );
    error
}
