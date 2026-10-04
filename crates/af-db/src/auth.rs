use std::{fmt, future::Future, num::NonZeroU32, pin::Pin, sync::Arc, time::Duration};

use af_domain::{
    ConcurrencyLimit, GatewayPrincipal, GroupId, OrganizationDepartmentId,
    OrganizationGatewayPrincipal, OrganizationId, OrganizationMembershipId, OrganizationTeamId,
    PLAYGROUND_TOKEN_NAME, TokenId, TokenModelPolicy, TrustedClientIp, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, DbErr, EntityTrait, QueryFilter,
    QueryOrder, QueryResult, QuerySelect, Set, TransactionTrait,
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
    token_owner_guard::{TokenOwnerGuardError, lock_non_deleted_owner},
};

const PLAYGROUND_INTERNAL_KEY_PREFIX: &str = "internal-playground";

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
        /// 与用户状态同一次查询取得的并发限制快照；空值表示不限并发。
        user_concurrency: Option<ConcurrencyLimit>,
        /// 与用户和分组状态同一次查询取得的 RPM 限制快照；空值表示不限。
        user_rpm_limit: Option<NonZeroU32>,
        /// 当前令牌实际使用分组的 RPM 限制快照；空值表示不限。
        group_rpm_limit: Option<NonZeroU32>,
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

/// 企业令牌校验所需的稳定标识上下文。
///
/// 公共鉴权层只把数据库 ID 交给企业扩展，不传递 API Key 摘要、明文或企业敏感配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationTokenAuthContext {
    pub token_id: TokenId,
    pub user_id: UserId,
    pub organization_id: OrganizationId,
    pub membership_id: OrganizationMembershipId,
    pub team_id: Option<OrganizationTeamId>,
    pub department_id: Option<OrganizationDepartmentId>,
}

/// 企业令牌校验结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationTokenAuthValidation {
    Authenticated(OrganizationGatewayPrincipal),
    Rejected,
}

/// 企业扩展提供的令牌校验端口。
pub trait OrganizationTokenAuthValidator: Send + Sync {
    fn validate<'a>(
        &'a self,
        context: OrganizationTokenAuthContext,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError>>
                + Send
                + 'a,
        >,
    >;
}

/// 使用数据库持久化状态与可信客户端 IP 校验下游 API Key 的仓储。
#[derive(Clone)]
pub struct TokenAuthRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
    organization_validator: Option<Arc<dyn OrganizationTokenAuthValidator>>,
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
            organization_validator: None,
        })
    }

    /// 注入企业扩展的校验器。未注入时，企业令牌默认拒绝，避免降级为个人身份。
    #[must_use]
    pub fn with_organization_validator(
        mut self,
        validator: Arc<dyn OrganizationTokenAuthValidator>,
    ) -> Self {
        self.organization_validator = Some(validator);
        self
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

    /// 按已认证会话解析不可外部使用的试炼场计费主体，并合并历史重复托管 Key。
    pub async fn lookup_playground(
        &self,
        user_id: UserId,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        match timeout(self.lookup_timeout, self.lookup_playground_inner(user_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(TokenAuthRepositoryError::Timeout)),
        }
    }

    async fn lookup_playground_inner(
        &self,
        user_id: UserId,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
        let Some(owner) = lock_non_deleted_owner(&transaction, user_id)
            .await
            .map_err(map_owner_guard_error)?
        else {
            return Ok(TokenAuthLookupOutcome::Rejected);
        };
        if owner.status() != 1 {
            return Ok(TokenAuthLookupOutcome::Rejected);
        }

        let group_id = owner.default_group_id();
        let Some((group_rpm_limit, group_deleted_at)) = groups::Entity::find()
            .select_only()
            .column(groups::Column::RpmLimit)
            .column(groups::Column::DeletedAt)
            .filter(groups::Column::Id.eq(group_id.get()))
            .into_tuple::<(Option<i32>, Option<TimeDateTimeWithTimeZone>)>()
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?
        else {
            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
        };
        if group_deleted_at.is_some() {
            return Ok(TokenAuthLookupOutcome::Rejected);
        }

        let candidates = tokens::Entity::find()
            .filter(tokens::Column::UserId.eq(user_id.get()))
            .filter(tokens::Column::OrganizationId.is_null())
            .filter(tokens::Column::Name.eq(PLAYGROUND_TOKEN_NAME))
            .order_by_asc(tokens::Column::Id)
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
        let canonical = candidates
            .iter()
            .find(|token| {
                token.deleted_at.is_none() && token.key_prefix == PLAYGROUND_INTERNAL_KEY_PREFIX
            })
            .or_else(|| {
                candidates
                    .iter()
                    .find(|token| token.key_prefix == PLAYGROUND_INTERNAL_KEY_PREFIX)
            })
            .or_else(|| candidates.iter().find(|token| token.deleted_at.is_none()))
            .or_else(|| candidates.first());
        let now = TimeDateTimeWithTimeZone::now_utc();
        let token_id = if let Some(canonical) = canonical {
            let token_id = TokenId::new(canonical.id)
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
            let key_hash = if canonical.key_prefix == PLAYGROUND_INTERNAL_KEY_PREFIX {
                canonical.key_hash.clone()
            } else {
                random_internal_token_hash()?
            };
            let updated = tokens::Entity::update_many()
                .filter(tokens::Column::Id.eq(canonical.id))
                .filter(tokens::Column::UserId.eq(user_id.get()))
                .col_expr(tokens::Column::KeyHash, Expr::value(key_hash))
                .col_expr(
                    tokens::Column::KeyPrefix,
                    Expr::value(PLAYGROUND_INTERNAL_KEY_PREFIX),
                )
                .col_expr(tokens::Column::Status, Expr::value(1_i16))
                .col_expr(tokens::Column::GroupId, Expr::value(Option::<i64>::None))
                .col_expr(
                    tokens::Column::OrganizationId,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    tokens::Column::OrganizationMembershipId,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    tokens::Column::OrganizationTeamId,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    tokens::Column::OrganizationDepartmentId,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(tokens::Column::RemainQuota, Expr::value(0_i64))
                .col_expr(tokens::Column::UnlimitedQuota, Expr::value(true))
                .col_expr(
                    tokens::Column::ExpiredAt,
                    Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
                )
                .col_expr(
                    tokens::Column::ModelLimits,
                    Expr::value(Option::<TokenModelAllowlist>::None),
                )
                .col_expr(
                    tokens::Column::AllowIps,
                    Expr::value(Option::<TokenIpAllowlist>::None),
                )
                .col_expr(tokens::Column::CrossGroupRetry, Expr::value(false))
                .col_expr(
                    tokens::Column::RateLimit5h,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    tokens::Column::RateLimit1d,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    tokens::Column::RateLimit7d,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    tokens::Column::MaxRequests,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    tokens::Column::DeletedAt,
                    Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
                )
                .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
            if updated.rows_affected != 1 {
                return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
            }
            token_id
        } else {
            let inserted = tokens::ActiveModel {
                user_id: Set(user_id.get()),
                key_hash: Set(random_internal_token_hash()?),
                key_prefix: Set(PLAYGROUND_INTERNAL_KEY_PREFIX.to_owned()),
                name: Set(PLAYGROUND_TOKEN_NAME.to_owned()),
                status: Set(1),
                group_id: Set(None),
                organization_id: Set(None),
                organization_membership_id: Set(None),
                organization_team_id: Set(None),
                organization_department_id: Set(None),
                remain_quota: Set(0),
                unlimited_quota: Set(true),
                expired_at: Set(None),
                model_limits: Set(None),
                allow_ips: Set(None),
                cross_group_retry: Set(false),
                window_5h_start: Set(now),
                window_1d_start: Set(now),
                window_7d_start: Set(now),
                max_requests: Set(None),
                deleted_at: Set(None),
                ..Default::default()
            }
            .insert(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
            TokenId::new(inserted.id)
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?
        };

        tokens::Entity::update_many()
            .filter(tokens::Column::UserId.eq(user_id.get()))
            .filter(tokens::Column::OrganizationId.is_null())
            .filter(tokens::Column::Name.eq(PLAYGROUND_TOKEN_NAME))
            .filter(tokens::Column::Id.ne(token_id.get()))
            .filter(tokens::Column::DeletedAt.is_null())
            .col_expr(tokens::Column::DeletedAt, Expr::value(now))
            .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;

        Ok(TokenAuthLookupOutcome::Authenticated {
            principal: GatewayPrincipal::playground(token_id, user_id, group_id),
            model_policy: TokenModelPolicy::unrestricted(),
            user_concurrency: normalize_concurrency(owner.concurrency())?,
            user_rpm_limit: normalize_rpm_limit(owner.rpm_limit())?,
            group_rpm_limit: normalize_rpm_limit(group_rpm_limit)?,
        })
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
        row.validate(client_ip, self.organization_validator.as_deref())
            .await
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
            Expr::col((tokens::Entity, tokens::Column::OrganizationId)),
            Alias::new("token_organization_id"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::OrganizationMembershipId)),
            Alias::new("token_organization_membership_id"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::OrganizationTeamId)),
            Alias::new("token_organization_team_id"),
        )
        .expr_as(
            Expr::col((tokens::Entity, tokens::Column::OrganizationDepartmentId)),
            Alias::new("token_organization_department_id"),
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
            Expr::col((users::Entity, users::Column::Concurrency)),
            Alias::new("user_concurrency"),
        )
        .expr_as(
            Expr::col((users::Entity, users::Column::RpmLimit)),
            Alias::new("user_rpm_limit"),
        )
        .expr_as(
            Expr::col((groups::Entity, groups::Column::Id)),
            Alias::new("group_id"),
        )
        .expr_as(
            Expr::col((groups::Entity, groups::Column::DeletedAt)),
            Alias::new("group_deleted_at"),
        )
        .expr_as(
            Expr::col((groups::Entity, groups::Column::RpmLimit)),
            Alias::new("group_rpm_limit"),
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
        // 保留名称只允许由会话试炼场入口解析，绝不接受外部 API Key 鉴权。
        .and_where(Expr::col((tokens::Entity, tokens::Column::Name)).ne(PLAYGROUND_TOKEN_NAME))
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
    token_organization_id: Option<i64>,
    token_organization_membership_id: Option<i64>,
    token_organization_team_id: Option<i64>,
    token_organization_department_id: Option<i64>,
    token_model_limits: Option<TokenModelAllowlist>,
    token_model_limits_oversized: bool,
    token_allow_ips: Option<TokenIpAllowlist>,
    token_allow_ips_oversized: bool,
    user_id: Option<i64>,
    user_status: Option<i16>,
    user_deleted_at: Option<TimeDateTimeWithTimeZone>,
    user_concurrency: Option<i32>,
    user_rpm_limit: Option<i32>,
    group_id: Option<i64>,
    group_deleted_at: Option<TimeDateTimeWithTimeZone>,
    group_rpm_limit: Option<i32>,
}

impl TokenAuthRow {
    fn try_from_query_result(result: &QueryResult) -> Result<Self, DbErr> {
        Ok(Self {
            token_id: result.try_get("", "token_id")?,
            token_status: result.try_get("", "token_status")?,
            token_expired_at: result.try_get("", "token_expired_at")?,
            token_deleted_at: result.try_get("", "token_deleted_at")?,
            token_organization_id: result.try_get("", "token_organization_id")?,
            token_organization_membership_id: result
                .try_get("", "token_organization_membership_id")?,
            token_organization_team_id: result.try_get("", "token_organization_team_id")?,
            token_organization_department_id: result
                .try_get("", "token_organization_department_id")?,
            token_model_limits: result.try_get("", "token_model_limits")?,
            token_model_limits_oversized: result.try_get("", "token_model_limits_oversized")?,
            token_allow_ips: result.try_get("", "token_allow_ips")?,
            token_allow_ips_oversized: result.try_get("", "token_allow_ips_oversized")?,
            user_id: result.try_get("", "user_id")?,
            user_status: result.try_get("", "user_status")?,
            user_deleted_at: result.try_get("", "user_deleted_at")?,
            user_concurrency: result.try_get("", "user_concurrency")?,
            user_rpm_limit: result.try_get("", "user_rpm_limit")?,
            group_id: result.try_get("", "group_id")?,
            group_deleted_at: result.try_get("", "group_deleted_at")?,
            group_rpm_limit: result.try_get("", "group_rpm_limit")?,
        })
    }

    async fn validate(
        self,
        client_ip: TrustedClientIp,
        organization_validator: Option<&dyn OrganizationTokenAuthValidator>,
    ) -> Result<TokenAuthLookupOutcome, TokenAuthRepositoryError> {
        if self.token_model_limits_oversized || self.token_allow_ips_oversized {
            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
        }
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
        let user_concurrency = normalize_concurrency(self.user_concurrency)?;
        let user_rpm_limit = normalize_rpm_limit(self.user_rpm_limit)?;
        let Some(group_id) = self.group_id else {
            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
        };
        let group_rpm_limit = normalize_rpm_limit(self.group_rpm_limit)?;
        let token_id = TokenId::new(self.token_id)
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        let user_id = UserId::new(user_id)
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        let group_id = GroupId::new(group_id)
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        let organization_context = self.organization_context(token_id, user_id)?;
        let model_policy = self.token_model_limits.map_or_else(
            TokenModelPolicy::unrestricted,
            TokenModelAllowlist::into_policy,
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
        let principal = match organization_context {
            Some(context) => {
                let Some(validator) = organization_validator else {
                    return Ok(TokenAuthLookupOutcome::Rejected);
                };
                match validator
                    .validate(context)
                    .with_subscriber(NoSubscriber::default())
                    .await
                    .map_err(record_internal_error)?
                {
                    OrganizationTokenAuthValidation::Authenticated(organization) => {
                        if organization.organization_id() != context.organization_id
                            || organization.membership_id() != context.membership_id
                            || organization.team_id() != context.team_id
                            || organization.department_id() != context.department_id
                        {
                            return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
                        }
                        GatewayPrincipal::organization(token_id, user_id, group_id, organization)
                    }
                    OrganizationTokenAuthValidation::Rejected => {
                        return Ok(TokenAuthLookupOutcome::Rejected);
                    }
                }
            }
            None => GatewayPrincipal::new(token_id, user_id, group_id),
        };
        Ok(TokenAuthLookupOutcome::Authenticated {
            principal,
            model_policy,
            user_concurrency,
            user_rpm_limit,
            group_rpm_limit,
        })
    }

    fn organization_context(
        &self,
        token_id: TokenId,
        user_id: UserId,
    ) -> Result<Option<OrganizationTokenAuthContext>, TokenAuthRepositoryError> {
        let Some(organization_value) = self.token_organization_id else {
            if self.token_organization_membership_id.is_some()
                || self.token_organization_team_id.is_some()
                || self.token_organization_department_id.is_some()
            {
                return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
            }
            return Ok(None);
        };
        Ok(Some(OrganizationTokenAuthContext {
            token_id,
            user_id,
            organization_id: OrganizationId::new(organization_value)
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            membership_id: OrganizationMembershipId::new(
                self.token_organization_membership_id
                    .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            )
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            team_id: self
                .token_organization_team_id
                .map(OrganizationTeamId::new)
                .transpose()
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            department_id: self
                .token_organization_department_id
                .map(OrganizationDepartmentId::new)
                .transpose()
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
        }))
    }
}

/// 数据库空值和零值都表示不限并发，负数或无法表示的值视为持久化损坏。
fn normalize_concurrency(
    value: Option<i32>,
) -> Result<Option<ConcurrencyLimit>, TokenAuthRepositoryError> {
    match value {
        None | Some(0) => Ok(None),
        Some(value) if value > 0 => u32::try_from(value)
            .ok()
            .and_then(|value| ConcurrencyLimit::new(value).ok())
            .map(Some)
            .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant)),
        Some(_) => Err(record_internal_error(TokenAuthRepositoryError::Invariant)),
    }
}

/// 将持久化的 RPM 字段收敛为安全的正数快照；负值表示数据损坏并拒绝请求。
fn normalize_rpm_limit(value: Option<i32>) -> Result<Option<NonZeroU32>, TokenAuthRepositoryError> {
    match value {
        None | Some(0) => Ok(None),
        Some(value) if value > 0 => NonZeroU32::new(
            u32::try_from(value)
                .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
        )
        .map(Some)
        .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant)),
        Some(_) => Err(record_internal_error(TokenAuthRepositoryError::Invariant)),
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

fn map_owner_guard_error(error: TokenOwnerGuardError) -> TokenAuthRepositoryError {
    match error {
        TokenOwnerGuardError::Query => record_internal_error(TokenAuthRepositoryError::Query),
        TokenOwnerGuardError::Invariant => {
            record_internal_error(TokenAuthRepositoryError::Invariant)
        }
    }
}

fn random_internal_token_hash() -> Result<TokenHash, TokenAuthRepositoryError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}")
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
    }
    TokenHash::parse(&encoded)
        .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestOrganizationValidator {
        calls: AtomicUsize,
        result: Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError>,
    }

    impl OrganizationTokenAuthValidator for TestOrganizationValidator {
        fn validate<'a>(
            &'a self,
            _: OrganizationTokenAuthContext,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<OrganizationTokenAuthValidation, TokenAuthRepositoryError>,
                    > + Send
                    + 'a,
            >,
        > {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn auth_row(organization: bool) -> TokenAuthRow {
        TokenAuthRow {
            token_id: 1,
            token_status: 1,
            token_expired_at: None,
            token_deleted_at: None,
            token_organization_id: organization.then_some(10),
            token_organization_membership_id: organization.then_some(11),
            token_organization_team_id: None,
            token_organization_department_id: None,
            token_model_limits: None,
            token_model_limits_oversized: false,
            token_allow_ips: None,
            token_allow_ips_oversized: false,
            user_id: Some(2),
            user_status: Some(1),
            user_deleted_at: None,
            user_concurrency: None,
            user_rpm_limit: None,
            group_id: Some(3),
            group_deleted_at: None,
            group_rpm_limit: None,
        }
    }

    fn client_ip() -> TrustedClientIp {
        TrustedClientIp::new("192.0.2.10".parse().expect("test IP must be valid"))
    }

    fn organization_principal() -> OrganizationGatewayPrincipal {
        OrganizationGatewayPrincipal::new(
            OrganizationId::new(10).expect("test organization ID must be valid"),
            OrganizationMembershipId::new(11).expect("test membership ID must be valid"),
            None,
        )
    }

    #[tokio::test]
    async fn organization_token_requires_an_extension_validator() {
        assert_eq!(
            auth_row(true).validate(client_ip(), None).await,
            Ok(TokenAuthLookupOutcome::Rejected)
        );
    }

    #[tokio::test]
    async fn organization_extension_rejection_never_becomes_a_personal_principal() {
        let validator = TestOrganizationValidator {
            calls: AtomicUsize::new(0),
            result: Ok(OrganizationTokenAuthValidation::Rejected),
        };
        assert_eq!(
            auth_row(true).validate(client_ip(), Some(&validator)).await,
            Ok(TokenAuthLookupOutcome::Rejected)
        );
        assert_eq!(validator.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn organization_extension_errors_never_become_a_personal_principal() {
        let validator = TestOrganizationValidator {
            calls: AtomicUsize::new(0),
            result: Err(TokenAuthRepositoryError::Query),
        };
        assert_eq!(
            auth_row(true).validate(client_ip(), Some(&validator)).await,
            Err(TokenAuthRepositoryError::Query)
        );
        assert_eq!(validator.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn organization_extension_must_preserve_token_ownership() {
        let validator = TestOrganizationValidator {
            calls: AtomicUsize::new(0),
            result: Ok(OrganizationTokenAuthValidation::Authenticated(
                OrganizationGatewayPrincipal::new(
                    OrganizationId::new(20).expect("test organization ID must be valid"),
                    OrganizationMembershipId::new(11).expect("test membership ID must be valid"),
                    None,
                ),
            )),
        };
        assert_eq!(
            auth_row(true).validate(client_ip(), Some(&validator)).await,
            Err(TokenAuthRepositoryError::Invariant)
        );
    }

    #[tokio::test]
    async fn personal_tokens_do_not_call_the_organization_extension() {
        let validator = TestOrganizationValidator {
            calls: AtomicUsize::new(0),
            result: Err(TokenAuthRepositoryError::Query),
        };
        let outcome = auth_row(false)
            .validate(client_ip(), Some(&validator))
            .await
            .expect("personal token should authenticate");
        let TokenAuthLookupOutcome::Authenticated { principal, .. } = outcome else {
            panic!("personal token should authenticate");
        };
        assert_eq!(principal.organization_principal(), None);
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn matching_organization_extension_creates_an_organization_principal() {
        let validator = TestOrganizationValidator {
            calls: AtomicUsize::new(0),
            result: Ok(OrganizationTokenAuthValidation::Authenticated(
                organization_principal(),
            )),
        };
        let outcome = auth_row(true)
            .validate(client_ip(), Some(&validator))
            .await
            .expect("valid organization token should authenticate");
        let TokenAuthLookupOutcome::Authenticated { principal, .. } = outcome else {
            panic!("valid organization token should authenticate");
        };
        assert_eq!(
            principal.organization_principal(),
            Some(organization_principal())
        );
        assert_eq!(validator.calls.load(Ordering::SeqCst), 1);
    }

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
