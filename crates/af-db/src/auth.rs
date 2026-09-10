use std::{fmt, num::NonZeroU32, time::Duration};

use af_domain::{
    ConcurrencyLimit, GatewayPrincipal, GroupId, OrganizationDepartmentId,
    OrganizationDepartmentMembershipStatus, OrganizationDepartmentStatus,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationGatewayPrincipal,
    OrganizationId, OrganizationMembershipId, OrganizationMembershipStatus, OrganizationStatus,
    OrganizationTeamId, OrganizationTeamStatus, PLAYGROUND_TOKEN_NAME, TokenId, TokenModelPolicy,
    TrustedClientIp, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QueryResult, QuerySelect, Set, TransactionTrait,
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
        TokenHash, TokenIpAllowlist, TokenModelAllowlist, groups,
        organization_department_memberships, organization_departments, organization_entitlements,
        organization_memberships, organization_teams, organizations, tokens, users,
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
        row.validate(client_ip, connection).await
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
        .expr_as(
            Expr::col((organizations::Entity, organizations::Column::Status)),
            Alias::new("organization_status"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::Id,
            )),
            Alias::new("membership_id"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::Status,
            )),
            Alias::new("membership_status"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::OrganizationId,
            )),
            Alias::new("membership_organization_id"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::UserId,
            )),
            Alias::new("membership_user_id"),
        )
        .expr_as(
            Expr::col((
                organization_memberships::Entity,
                organization_memberships::Column::TeamId,
            )),
            Alias::new("membership_team_id"),
        )
        .expr_as(
            Expr::col((
                organization_teams::Entity,
                organization_teams::Column::Status,
            )),
            Alias::new("organization_team_status"),
        )
        .expr_as(
            Expr::col((
                organization_teams::Entity,
                organization_teams::Column::OrganizationId,
            )),
            Alias::new("organization_team_organization_id"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::TeamManagementEnabled,
            )),
            Alias::new("entitlement_team_management_enabled"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::SeatLimit,
            )),
            Alias::new("entitlement_seat_limit"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::TeamLimit,
            )),
            Alias::new("entitlement_team_limit"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::OrganizationKeyLimit,
            )),
            Alias::new("entitlement_organization_key_limit"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::AuditRetentionDays,
            )),
            Alias::new("entitlement_audit_retention_days"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::SsoEnabled,
            )),
            Alias::new("entitlement_sso_enabled"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::CustomRolesEnabled,
            )),
            Alias::new("entitlement_custom_roles_enabled"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::ValidFrom,
            )),
            Alias::new("entitlement_valid_from"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::ValidUntil,
            )),
            Alias::new("entitlement_valid_until"),
        )
        .expr_as(
            Expr::col((
                organization_entitlements::Entity,
                organization_entitlements::Column::GraceUntil,
            )),
            Alias::new("entitlement_grace_until"),
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
        .left_join(
            organizations::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationId))
                .equals((organizations::Entity, organizations::Column::Id)),
        )
        .left_join(
            organization_memberships::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationMembershipId)).equals((
                organization_memberships::Entity,
                organization_memberships::Column::Id,
            )),
        )
        .left_join(
            organization_teams::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationTeamId))
                .equals((organization_teams::Entity, organization_teams::Column::Id)),
        )
        .left_join(
            organization_entitlements::Entity,
            Expr::col((tokens::Entity, tokens::Column::OrganizationId)).equals((
                organization_entitlements::Entity,
                organization_entitlements::Column::OrganizationId,
            )),
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
    organization_status: Option<i16>,
    membership_id: Option<i64>,
    membership_status: Option<i16>,
    membership_organization_id: Option<i64>,
    membership_user_id: Option<i64>,
    membership_team_id: Option<i64>,
    organization_team_status: Option<i16>,
    organization_team_organization_id: Option<i64>,
    entitlement_team_management_enabled: Option<bool>,
    entitlement_seat_limit: Option<i64>,
    entitlement_team_limit: Option<i64>,
    entitlement_organization_key_limit: Option<i64>,
    entitlement_audit_retention_days: Option<i64>,
    entitlement_sso_enabled: Option<bool>,
    entitlement_custom_roles_enabled: Option<bool>,
    entitlement_valid_from: Option<TimeDateTimeWithTimeZone>,
    entitlement_valid_until: Option<TimeDateTimeWithTimeZone>,
    entitlement_grace_until: Option<TimeDateTimeWithTimeZone>,
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
            organization_status: result.try_get("", "organization_status")?,
            membership_id: result.try_get("", "membership_id")?,
            membership_status: result.try_get("", "membership_status")?,
            membership_organization_id: result.try_get("", "membership_organization_id")?,
            membership_user_id: result.try_get("", "membership_user_id")?,
            membership_team_id: result.try_get("", "membership_team_id")?,
            organization_team_status: result.try_get("", "organization_team_status")?,
            organization_team_organization_id: result
                .try_get("", "organization_team_organization_id")?,
            entitlement_team_management_enabled: result
                .try_get("", "entitlement_team_management_enabled")?,
            entitlement_seat_limit: result.try_get("", "entitlement_seat_limit")?,
            entitlement_team_limit: result.try_get("", "entitlement_team_limit")?,
            entitlement_organization_key_limit: result
                .try_get("", "entitlement_organization_key_limit")?,
            entitlement_audit_retention_days: result
                .try_get("", "entitlement_audit_retention_days")?,
            entitlement_sso_enabled: result.try_get("", "entitlement_sso_enabled")?,
            entitlement_custom_roles_enabled: result
                .try_get("", "entitlement_custom_roles_enabled")?,
            entitlement_valid_from: result.try_get("", "entitlement_valid_from")?,
            entitlement_valid_until: result.try_get("", "entitlement_valid_until")?,
            entitlement_grace_until: result.try_get("", "entitlement_grace_until")?,
        })
    }

    async fn validate(
        self,
        client_ip: TrustedClientIp,
        connection: &impl ConnectionTrait,
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
        let organization = self.validate_organization(connection, user_id).await?;
        let model_policy = self.token_model_limits.map_or_else(
            TokenModelPolicy::unrestricted,
            TokenModelAllowlist::into_policy,
        );
        let principal = if let Some(organization) = organization {
            GatewayPrincipal::organization(token_id, user_id, group_id, organization)
        } else {
            GatewayPrincipal::new(token_id, user_id, group_id)
        };

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
            user_concurrency,
            user_rpm_limit,
            group_rpm_limit,
        })
    }

    /// 企业 Key 每次使用都重新核对成员、授权、团队和当前容量。
    async fn validate_organization(
        &self,
        connection: &impl ConnectionTrait,
        user_id: UserId,
    ) -> Result<Option<OrganizationGatewayPrincipal>, TokenAuthRepositoryError> {
        let Some(organization_value) = self.token_organization_id else {
            if self.token_organization_membership_id.is_some()
                || self.token_organization_team_id.is_some()
                || self.token_organization_department_id.is_some()
            {
                return Err(record_internal_error(TokenAuthRepositoryError::Invariant));
            }
            return Ok(None);
        };
        let organization_id = OrganizationId::new(organization_value)
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        let membership_value = self
            .token_organization_membership_id
            .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        if self.membership_id != Some(membership_value)
            || self.membership_organization_id != Some(organization_value)
            || self.membership_user_id != Some(user_id.get())
            || self.membership_team_id != self.token_organization_team_id
        {
            return Ok(None);
        }
        let membership_status = OrganizationMembershipStatus::try_from(
            self.membership_status
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
        )
        .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        if !membership_status.is_active() {
            return Ok(None);
        }
        let organization_status = OrganizationStatus::try_from(
            self.organization_status
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
        )
        .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        let capacity = OrganizationEntitlementCapacity::new(
            self.entitlement_seat_limit
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            self.entitlement_team_limit
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            self.entitlement_organization_key_limit
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
        )
        .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        let entitlement = OrganizationEntitlementSnapshot::new(
            capacity,
            self.entitlement_team_management_enabled
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            self.entitlement_sso_enabled
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            self.entitlement_custom_roles_enabled
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            self.entitlement_audit_retention_days
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?,
            self.entitlement_valid_from
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?
                .unix_timestamp(),
            self.entitlement_valid_until
                .ok_or_else(|| record_internal_error(TokenAuthRepositoryError::Invariant))?
                .unix_timestamp(),
            self.entitlement_grace_until
                .map(|value| value.unix_timestamp()),
        )
        .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        if !entitlement
            .status(
                organization_status,
                TimeDateTimeWithTimeZone::now_utc().unix_timestamp(),
            )
            .allows_existing_consumption()
        {
            return Ok(None);
        }
        let team_id = match self.token_organization_team_id {
            Some(value) => {
                let status =
                    OrganizationTeamStatus::try_from(self.organization_team_status.ok_or_else(
                        || record_internal_error(TokenAuthRepositoryError::Invariant),
                    )?)
                    .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
                if !status.is_active()
                    || self.organization_team_organization_id != Some(organization_value)
                    || !entitlement.team_management_enabled()
                {
                    return Ok(None);
                }
                Some(
                    OrganizationTeamId::new(value)
                        .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?,
                )
            }
            None => None,
        };
        let department_id = match self.token_organization_department_id {
            Some(value) => {
                let department_id = OrganizationDepartmentId::new(value)
                    .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
                let department = organization_departments::Entity::find_by_id(value)
                    .filter(organization_departments::Column::OrganizationId.eq(organization_value))
                    .one(connection)
                    .await
                    .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
                let Some(department) = department else {
                    return Ok(None);
                };
                if OrganizationDepartmentStatus::try_from(department.status)
                    .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?
                    != OrganizationDepartmentStatus::Active
                {
                    return Ok(None);
                }
                let binding = organization_department_memberships::Entity::find()
                    .filter(
                        organization_department_memberships::Column::OrganizationId
                            .eq(organization_value),
                    )
                    .filter(
                        organization_department_memberships::Column::MembershipId
                            .eq(membership_value),
                    )
                    .filter(organization_department_memberships::Column::DepartmentId.eq(value))
                    .filter(
                        organization_department_memberships::Column::Status
                            .eq(OrganizationDepartmentMembershipStatus::Active.code()),
                    )
                    .one(connection)
                    .await
                    .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
                if binding.is_none() {
                    return Ok(None);
                }
                Some(department_id)
            }
            None => None,
        };
        let key_count = tokens::Entity::find()
            .filter(tokens::Column::OrganizationId.eq(organization_value))
            .filter(tokens::Column::DeletedAt.is_null())
            .count(connection)
            .await
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Query))?;
        let key_limit = u64::try_from(capacity.organization_key_limit())
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        if key_count > key_limit {
            return Ok(None);
        }
        let membership_id = OrganizationMembershipId::new(membership_value)
            .map_err(|_| record_internal_error(TokenAuthRepositoryError::Invariant))?;
        Ok(Some(match department_id {
            Some(department_id) => OrganizationGatewayPrincipal::with_department(
                organization_id,
                membership_id,
                team_id,
                department_id,
            ),
            None => OrganizationGatewayPrincipal::new(organization_id, membership_id, team_id),
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
