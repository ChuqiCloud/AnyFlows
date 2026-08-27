use std::{fmt, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipStatus, OrganizationRole, OrganizationScimGroupResource,
    OrganizationScimLifecycleDisposition, OrganizationScimLifecycleFacts,
    OrganizationScimResourceId, OrganizationScimResourceKind, OrganizationScimResourceSet,
    OrganizationScimResourceVersion, OrganizationScimSyncDisposition, OrganizationScimToken,
    OrganizationScimTokenDigest, OrganizationScimTokenError, OrganizationScimTokenId,
    OrganizationScimTokenKey, OrganizationScimTokenStatus, OrganizationScimUserResource,
    OrganizationStatus, OrganizationTeamId, OrganizationTeamStatus, UserId,
    classify_organization_scim_lifecycle, classify_organization_scim_sync,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QuerySelect, Set, SqlErr,
    TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{
        SensitiveString, organization_entitlements, organization_memberships,
        organization_scim_group_resources, organization_scim_tokens,
        organization_scim_user_resources, organization_teams, organizations, tokens, users,
    },
};

const MAX_SCIM_TOKEN_NAME_BYTES: usize = 128;
const ENABLED_TOKEN_STATUS: i16 = 1;
const DISABLED_TOKEN_STATUS: i16 = 2;

/// 企业 SCIM 令牌待签发字段；不接受外部令牌原文或摘要。
pub struct OrganizationScimTokenCreateRecord {
    organization_id: OrganizationId,
    name: String,
    resource_scope: OrganizationScimResourceSet,
}

impl OrganizationScimTokenCreateRecord {
    /// 组装令牌签发请求，名称和资源范围在仓储边界再次校验。
    #[must_use]
    pub fn new(
        organization_id: OrganizationId,
        name: String,
        resource_scope: OrganizationScimResourceSet,
    ) -> Self {
        Self {
            organization_id,
            name,
            resource_scope,
        }
    }
}

impl fmt::Debug for OrganizationScimTokenCreateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimTokenCreateRecord")
            .field("organization_id", &self.organization_id)
            .field("name", &self.name)
            .field("resource_scope", &self.resource_scope)
            .finish()
    }
}

/// 令牌签发结果；明文令牌只允许由调用方在创建响应中消费一次。
pub struct OrganizationScimTokenIssued {
    record: OrganizationScimTokenRecord,
    token: OrganizationScimToken,
}

impl OrganizationScimTokenIssued {
    #[must_use]
    pub fn into_parts(self) -> (OrganizationScimTokenRecord, OrganizationScimToken) {
        (self.record, self.token)
    }
}

impl fmt::Debug for OrganizationScimTokenIssued {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimTokenIssued(<redacted>)")
    }
}

/// 企业 SCIM 令牌的脱敏持久化投影。
#[derive(Clone)]
pub struct OrganizationScimTokenRecord {
    id: OrganizationScimTokenId,
    organization_id: OrganizationId,
    token_key: OrganizationScimTokenKey,
    token_prefix: String,
    name: String,
    resource_scope: OrganizationScimResourceSet,
    status: OrganizationScimTokenStatus,
    version: i64,
    created_at: i64,
    updated_at: i64,
    revoked_at: Option<i64>,
}

impl OrganizationScimTokenRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationScimTokenId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn token_key(&self) -> &OrganizationScimTokenKey {
        &self.token_key
    }
    #[must_use]
    pub fn token_prefix(&self) -> &str {
        &self.token_prefix
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn resource_scope(&self) -> OrganizationScimResourceSet {
        self.resource_scope
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationScimTokenStatus {
        self.status
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
    #[must_use]
    pub const fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }
}

impl fmt::Debug for OrganizationScimTokenRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimTokenRecord")
            .field("id", &self.id)
            .field("organization_id", &self.organization_id)
            .field("token_key", &self.token_key)
            .field("token_prefix", &self.token_prefix)
            .field("name", &self.name)
            .field("resource_scope", &self.resource_scope)
            .field("status", &self.status)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// SCIM 令牌存储错误；不向边界泄露令牌、摘要或跨企业命中事实。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimTokenRepositoryError {
    #[error("企业 SCIM 令牌输入无效")]
    InvalidInput,
    #[error("企业 SCIM 令牌企业范围不可用")]
    Restricted,
    #[error("企业 SCIM 令牌不存在")]
    NotFound,
    #[error("企业 SCIM 令牌版本冲突")]
    Conflict,
    #[error("企业 SCIM 令牌数据库操作失败")]
    Query,
    #[error("企业 SCIM 令牌操作超时")]
    Timeout,
    #[error("企业 SCIM 令牌持久化状态损坏")]
    Invariant,
}

#[derive(Clone)]
pub struct OrganizationScimTokenRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationScimTokenRepository {
    /// 创建带截止时间的 SCIM 令牌仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationScimTokenRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationScimTokenRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在活动企业内签发令牌并只将摘要写入数据库。
    pub async fn create(
        &self,
        request: OrganizationScimTokenCreateRecord,
    ) -> Result<OrganizationScimTokenIssued, OrganizationScimTokenRepositoryError> {
        timeout(self.operation_timeout, self.create_inner(request))
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Timeout)?
    }

    /// 按企业范围校验令牌；撤销或跨企业令牌统一不可见。
    pub async fn authenticate_for_organization(
        &self,
        organization_id: OrganizationId,
        token: OrganizationScimToken,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        timeout(
            self.operation_timeout,
            self.authenticate_inner(organization_id, token.digest()),
        )
        .await
        .map_err(|_| OrganizationScimTokenRepositoryError::Timeout)?
    }

    /// 按企业和版本原子撤销令牌；重复撤销不恢复令牌或版本。
    pub async fn revoke(
        &self,
        organization_id: OrganizationId,
        token_id: OrganizationScimTokenId,
        expected_version: i64,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        if expected_version <= 0 {
            return Err(OrganizationScimTokenRepositoryError::InvalidInput);
        }
        timeout(
            self.operation_timeout,
            self.revoke_inner(organization_id, token_id, expected_version),
        )
        .await
        .map_err(|_| OrganizationScimTokenRepositoryError::Timeout)?
    }

    async fn create_inner(
        &self,
        request: OrganizationScimTokenCreateRecord,
    ) -> Result<OrganizationScimTokenIssued, OrganizationScimTokenRepositoryError> {
        if request.name.trim() != request.name
            || request.name.is_empty()
            || request.name.len() > MAX_SCIM_TOKEN_NAME_BYTES
            || request.name.chars().any(char::is_control)
        {
            return Err(OrganizationScimTokenRepositoryError::InvalidInput);
        }
        let organization_id = request.organization_id;
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let organization = organizations::Entity::find_by_id(organization_id.get())
            .lock(LockType::Update)
            .one(&transaction)
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?
            .ok_or(OrganizationScimTokenRepositoryError::Restricted)?;
        if organization.status != 1 {
            return Err(OrganizationScimTokenRepositoryError::Restricted);
        }

        let token = OrganizationScimToken::generate()
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let digest = token.digest();
        let token_key = OrganizationScimTokenKey::generate()
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_scim_tokens::ActiveModel {
            organization_id: Set(organization_id.get()),
            token_key: Set(token_key.as_str().to_owned()),
            token_prefix: Set(token.prefix().to_owned()),
            token_digest: Set(SensitiveString::from(digest.as_str())),
            name: Set(request.name),
            resource_scope: Set(request.resource_scope.code()),
            status: Set(OrganizationScimTokenStatus::Active.code()),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            revoked_at: Set(None),
            ..Default::default()
        };
        let inserted = model
            .insert(&transaction)
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let record = record_from_model(inserted)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        Ok(OrganizationScimTokenIssued { record, token })
    }

    async fn authenticate_inner(
        &self,
        organization_id: OrganizationId,
        digest: OrganizationScimTokenDigest,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        let model = organization_scim_tokens::Entity::find()
            .filter(organization_scim_tokens::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_scim_tokens::Column::TokenDigest.eq(digest.as_str()))
            .filter(
                organization_scim_tokens::Column::Status
                    .eq(OrganizationScimTokenStatus::Active.code()),
            )
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?
            .ok_or(OrganizationScimTokenRepositoryError::NotFound)?;
        // SCIM 入口必须随企业生命周期关闭，避免暂停或关闭的企业继续使用有效令牌。
        let organization_is_active = organizations::Entity::find_by_id(organization_id.get())
            .filter(organizations::Column::Status.eq(1_i16))
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?
            .is_some();
        if !organization_is_active {
            return Err(OrganizationScimTokenRepositoryError::Restricted);
        }
        record_from_model(model)
    }

    async fn revoke_inner(
        &self,
        organization_id: OrganizationId,
        token_id: OrganizationScimTokenId,
        expected_version: i64,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = organization_scim_tokens::Entity::update_many()
            .col_expr(
                organization_scim_tokens::Column::Status,
                Expr::value(OrganizationScimTokenStatus::Revoked.code()),
            )
            .col_expr(
                organization_scim_tokens::Column::Version,
                Expr::col(organization_scim_tokens::Column::Version).add(1_i64),
            )
            .col_expr(
                organization_scim_tokens::Column::UpdatedAt,
                Expr::value(now),
            )
            .col_expr(
                organization_scim_tokens::Column::RevokedAt,
                Expr::value(now),
            )
            .filter(organization_scim_tokens::Column::Id.eq(token_id.get()))
            .filter(organization_scim_tokens::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_scim_tokens::Column::Version.eq(expected_version))
            .filter(
                organization_scim_tokens::Column::Status
                    .eq(OrganizationScimTokenStatus::Active.code()),
            )
            .exec(self.pool.connection())
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        if result.rows_affected == 0 {
            return Err(OrganizationScimTokenRepositoryError::Conflict);
        }
        let model = organization_scim_tokens::Entity::find_by_id(token_id.get())
            .filter(organization_scim_tokens::Column::OrganizationId.eq(organization_id.get()))
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?
            .ok_or(OrganizationScimTokenRepositoryError::NotFound)?;
        record_from_model(model)
    }
}

fn record_from_model(
    model: organization_scim_tokens::Model,
) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
    let id = OrganizationScimTokenId::new(model.id)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let organization_id = OrganizationId::new(model.organization_id)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let token_key = OrganizationScimTokenKey::new(model.token_key)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    OrganizationScimTokenDigest::from_persistence_key(model.token_digest.as_str())
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let resource_scope = OrganizationScimResourceSet::from_code(model.resource_scope)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let status = OrganizationScimTokenStatus::from_code(model.status)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    if model.token_prefix != "scim_"
        || model.name.trim() != model.name
        || model.name.is_empty()
        || model.name.len() > MAX_SCIM_TOKEN_NAME_BYTES
        || model.version <= 0
        || (status == OrganizationScimTokenStatus::Active && model.revoked_at.is_some())
        || (status == OrganizationScimTokenStatus::Revoked && model.revoked_at.is_none())
    {
        return Err(OrganizationScimTokenRepositoryError::Invariant);
    }
    Ok(OrganizationScimTokenRecord {
        id,
        organization_id,
        token_key,
        token_prefix: model.token_prefix,
        name: model.name,
        resource_scope,
        status,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
        revoked_at: model.revoked_at.map(|value| value.unix_timestamp()),
    })
}

impl From<OrganizationScimTokenError> for OrganizationScimTokenRepositoryError {
    fn from(_: OrganizationScimTokenError) -> Self {
        Self::Invariant
    }
}

/// SCIM 用户资源同步后返回的受控绑定投影；不包含目录用户名或显示名。
pub struct OrganizationScimUserRecord {
    organization_id: OrganizationId,
    resource_id: OrganizationScimResourceId,
    user_id: UserId,
    resource_version: OrganizationScimResourceVersion,
    membership_status: OrganizationMembershipStatus,
}

impl OrganizationScimUserRecord {
    /// 返回已验证的企业范围。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回外部目录资源标识。
    #[must_use]
    pub fn resource_id(&self) -> &OrganizationScimResourceId {
        &self.resource_id
    }

    /// 返回已绑定的既有全局用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回已持久化的目录版本。
    #[must_use]
    pub const fn resource_version(&self) -> OrganizationScimResourceVersion {
        self.resource_version
    }

    /// 返回已投影的企业成员状态。
    #[must_use]
    pub const fn membership_status(&self) -> OrganizationMembershipStatus {
        self.membership_status
    }
}

impl fmt::Debug for OrganizationScimUserRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimUserRecord")
            .field("organization_id", &self.organization_id)
            .field("resource_id", &self.resource_id)
            .field("user_id", &self.user_id)
            .field("resource_version", &self.resource_version)
            .field("membership_status", &self.membership_status)
            .finish()
    }
}

/// SCIM 用户同步是否推进了目录资源版本。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimUserSyncOutcome {
    /// 新资源或更高版本已在同一事务内完成成员投影。
    Applied,
    /// 请求版本与已保存版本一致，未重复写入成员或资源绑定。
    Noop,
}

/// SCIM 用户同步结果；即使成员状态无需变化，资源版本也可能已推进。
pub struct OrganizationScimUserSyncResult {
    record: OrganizationScimUserRecord,
    outcome: OrganizationScimUserSyncOutcome,
}

impl OrganizationScimUserSyncResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationScimUserRecord {
        &self.record
    }

    #[must_use]
    pub const fn outcome(&self) -> OrganizationScimUserSyncOutcome {
        self.outcome
    }
}

impl fmt::Debug for OrganizationScimUserSyncResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimUserSyncResult")
            .field("record", &self.record)
            .field("outcome", &self.outcome)
            .finish()
    }
}

/// SCIM 用户资源仓储错误；不向调用方泄漏用户名、成员或目录原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimUserRepositoryError {
    /// 令牌与资源不在同一企业范围，或令牌已经撤销、变更或失效。
    #[error("企业 SCIM 用户同步令牌不可用")]
    Restricted,
    /// 令牌未授予用户资源同步范围。
    #[error("企业 SCIM 用户同步范围不足")]
    Forbidden,
    /// 资源版本早于当前版本，或绑定/成员事实发生冲突。
    #[error("企业 SCIM 用户同步版本或绑定冲突")]
    Conflict,
    /// 首次绑定找不到精确匹配的活动全局用户，或该用户不可用。
    #[error("企业 SCIM 用户同步目标不可用")]
    UserUnavailable,
    /// 资源要求的成员生命周期不满足席位、授权或受管成员不变式。
    #[error("企业 SCIM 用户同步生命周期被拒绝")]
    LifecycleRejected,
    /// 数据库操作失败。
    #[error("企业 SCIM 用户同步数据库操作失败")]
    Query,
    /// 同步操作超过配置的截止时间。
    #[error("企业 SCIM 用户同步操作超时")]
    Timeout,
    /// 已保存的资源、成员、令牌或授权事实不满足强类型不变式。
    #[error("企业 SCIM 用户同步持久化状态损坏")]
    Invariant,
}

/// 在单个企业锁内推进 SCIM 用户资源与成员生命周期的仓储。
#[derive(Clone)]
pub struct OrganizationScimUserRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

/// SCIM 组资源绑定的安全投影；不返回目录组成员原文。
pub struct OrganizationScimGroupRecord {
    organization_id: OrganizationId,
    resource_id: OrganizationScimResourceId,
    team_id: OrganizationTeamId,
    resource_version: OrganizationScimResourceVersion,
    status: OrganizationScimGroupStatus,
}

impl OrganizationScimGroupRecord {
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn resource_id(&self) -> &OrganizationScimResourceId {
        &self.resource_id
    }
    #[must_use]
    pub const fn team_id(&self) -> OrganizationTeamId {
        self.team_id
    }
    #[must_use]
    pub const fn resource_version(&self) -> OrganizationScimResourceVersion {
        self.resource_version
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationScimGroupStatus {
        self.status
    }
}

impl fmt::Debug for OrganizationScimGroupRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimGroupRecord")
            .field("organization_id", &self.organization_id)
            .field("resource_id", &self.resource_id)
            .field("team_id", &self.team_id)
            .field("resource_version", &self.resource_version)
            .field("status", &self.status)
            .finish()
    }
}

/// SCIM 组资源是否仍可接受写入。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimGroupStatus {
    Active,
    Removed,
}

/// SCIM 组同步是否推进了资源版本。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimGroupSyncOutcome {
    Applied,
    Noop,
}

/// SCIM 组同步结果；成员和团队名称只在事务内部使用。
pub struct OrganizationScimGroupSyncResult {
    record: OrganizationScimGroupRecord,
    outcome: OrganizationScimGroupSyncOutcome,
}

impl OrganizationScimGroupSyncResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationScimGroupRecord {
        &self.record
    }
    #[must_use]
    pub const fn outcome(&self) -> OrganizationScimGroupSyncOutcome {
        self.outcome
    }
}

impl fmt::Debug for OrganizationScimGroupSyncResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimGroupSyncResult")
            .field("record", &self.record)
            .field("outcome", &self.outcome)
            .finish()
    }
}

/// SCIM 组资源同步错误；跨企业、陈旧版本和删除恢复统一关闭。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimGroupRepositoryError {
    #[error("企业 SCIM 组同步令牌不可用")]
    Restricted,
    #[error("企业 SCIM 组同步范围不足")]
    Forbidden,
    #[error("企业 SCIM 组同步版本或绑定冲突")]
    Conflict,
    #[error("企业 SCIM 组同步成员不可用")]
    MemberUnavailable,
    #[error("企业 SCIM 组同步团队不可用")]
    TeamUnavailable,
    #[error("企业 SCIM 组同步生命周期被拒绝")]
    LifecycleRejected,
    #[error("企业 SCIM 组同步数据库操作失败")]
    Query,
    #[error("企业 SCIM 组同步操作超时")]
    Timeout,
    #[error("企业 SCIM 组同步持久化状态损坏")]
    Invariant,
}

/// 在单个企业锁内推进 SCIM 组资源、团队和成员归属。
#[derive(Clone)]
pub struct OrganizationScimGroupRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationScimGroupRepository {
    /// 创建组同步仓储；零超时会被拒绝。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationScimGroupRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationScimGroupRepositoryError::Invariant);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 创建或更新企业组资源，并原子投影团队成员。
    pub async fn synchronize(
        &self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimGroupResource,
    ) -> Result<OrganizationScimGroupSyncResult, OrganizationScimGroupRepositoryError> {
        timeout(
            self.operation_timeout,
            self.synchronize_inner(token, resource),
        )
        .await
        .map_err(|_| OrganizationScimGroupRepositoryError::Timeout)?
    }

    /// 按 If-Match 版本删除组；删除后的团队和资源绑定不可恢复。
    pub async fn delete(
        &self,
        token: OrganizationScimTokenRecord,
        resource_id: OrganizationScimResourceId,
        expected_version: OrganizationScimResourceVersion,
    ) -> Result<OrganizationScimGroupSyncResult, OrganizationScimGroupRepositoryError> {
        timeout(
            self.operation_timeout,
            self.delete_inner(token, resource_id, expected_version),
        )
        .await
        .map_err(|_| OrganizationScimGroupRepositoryError::Timeout)?
    }

    async fn synchronize_inner(
        &self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimGroupResource,
    ) -> Result<OrganizationScimGroupSyncResult, OrganizationScimGroupRepositoryError> {
        let organization_id = resource.key().organization_id();
        if token.organization_id() != organization_id {
            return Err(OrganizationScimGroupRepositoryError::Restricted);
        }
        if !token
            .resource_scope()
            .contains(OrganizationScimResourceKind::Groups)
        {
            return Err(OrganizationScimGroupRepositoryError::Forbidden);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
        let organization = lock_organization_for_scim(&transaction, organization_id)
            .await
            .map_err(map_group_user_error)?;
        ensure_active_organization(&organization).map_err(map_group_user_error)?;
        lock_scim_token_for_resource_sync(
            &transaction,
            &token,
            OrganizationScimResourceKind::Groups,
        )
        .await
        .map_err(map_group_user_error)?;
        let entitlement = lock_scim_entitlement(&transaction, organization_id)
            .await
            .map_err(map_group_user_error)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let capacity = OrganizationEntitlementCapacity::new(
            entitlement.seat_limit,
            entitlement.team_limit,
            entitlement.organization_key_limit,
        )
        .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?;
        let snapshot = OrganizationEntitlementSnapshot::new(
            capacity,
            entitlement.team_management_enabled,
            entitlement.sso_enabled,
            entitlement.custom_roles_enabled,
            entitlement.audit_retention_days,
            entitlement.valid_from.unix_timestamp(),
            entitlement.valid_until.unix_timestamp(),
            entitlement.grace_until.map(|value| value.unix_timestamp()),
        )
        .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?;
        let org_status = OrganizationStatus::try_from(organization.status)
            .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?;
        if !snapshot.team_management_enabled()
            || !snapshot
                .status(org_status, now.unix_timestamp())
                .allows_provisioning()
        {
            return Err(OrganizationScimGroupRepositoryError::LifecycleRejected);
        }
        let existing =
            lock_scim_group_resource(&transaction, organization_id, resource.key().resource_id())
                .await?;
        let (binding, team) = match existing {
            Some(binding) => {
                let status = group_status(binding.status)?;
                if status == OrganizationScimGroupStatus::Removed {
                    return Err(OrganizationScimGroupRepositoryError::LifecycleRejected);
                }
                match classify_organization_scim_sync(
                    Some(
                        OrganizationScimResourceVersion::from_persistence(binding.resource_version)
                            .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?,
                    ),
                    resource.version(),
                ) {
                    OrganizationScimSyncDisposition::RejectStale => {
                        return Err(OrganizationScimGroupRepositoryError::Conflict);
                    }
                    OrganizationScimSyncDisposition::Noop => {
                        let record = group_record_from_model(binding)?;
                        transaction
                            .commit()
                            .await
                            .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
                        return Ok(OrganizationScimGroupSyncResult {
                            record,
                            outcome: OrganizationScimGroupSyncOutcome::Noop,
                        });
                    }
                    OrganizationScimSyncDisposition::Apply => {}
                }
                let team = lock_active_scim_team(
                    &transaction,
                    organization_id,
                    OrganizationTeamId::new(binding.team_id)
                        .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?,
                )
                .await?;
                (Some(binding), team)
            }
            None => {
                let active_teams = organization_teams::Entity::find()
                    .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
                    .filter(
                        organization_teams::Column::Status
                            .eq(OrganizationTeamStatus::Active.code()),
                    )
                    .count(&transaction)
                    .await
                    .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
                if i64::try_from(active_teams)
                    .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?
                    >= snapshot.capacity().team_limit()
                {
                    return Err(OrganizationScimGroupRepositoryError::LifecycleRejected);
                }
                let team = organization_teams::ActiveModel {
                    organization_id: Set(organization_id.get()),
                    name: Set(resource.display_name().to_owned()),
                    description: Set(None),
                    status: Set(OrganizationTeamStatus::Active.code()),
                    version: Set(1),
                    created_at: Set(now),
                    updated_at: Set(now),
                    deleted_at: Set(None),
                    ..Default::default()
                }
                .insert(&transaction)
                .await
                .map_err(map_group_db_error)?;
                (None, team)
            }
        };
        let member_user_ids =
            resolve_group_members(&transaction, organization_id, resource.member_ids()).await?;
        project_group_members(&transaction, organization_id, team.id, &member_user_ids).await?;
        let team = if team.name != resource.display_name() {
            let next_version = team
                .version
                .checked_add(1)
                .ok_or(OrganizationScimGroupRepositoryError::Invariant)?;
            let mut active = team.into_active_model();
            active.name = Set(resource.display_name().to_owned());
            active.version = Set(next_version);
            active.updated_at = Set(now);
            active
                .update(&transaction)
                .await
                .map_err(map_group_db_error)?
        } else {
            team
        };
        let model = match binding {
            Some(binding) => {
                let mut active = binding.into_active_model();
                active.resource_version = Set(i64::try_from(resource.version().get())
                    .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?);
                active.updated_at = Set(now);
                active
                    .update(&transaction)
                    .await
                    .map_err(map_group_db_error)?
            }
            None => organization_scim_group_resources::ActiveModel {
                organization_id: Set(organization_id.get()),
                resource_id: Set(resource.key().resource_id().as_str().to_owned()),
                team_id: Set(team.id),
                resource_version: Set(i64::try_from(resource.version().get())
                    .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?),
                status: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            }
            .insert(&transaction)
            .await
            .map_err(map_group_db_error)?,
        };
        let record = group_record_from_model(model)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
        Ok(OrganizationScimGroupSyncResult {
            record,
            outcome: OrganizationScimGroupSyncOutcome::Applied,
        })
    }

    async fn delete_inner(
        &self,
        token: OrganizationScimTokenRecord,
        resource_id: OrganizationScimResourceId,
        expected_version: OrganizationScimResourceVersion,
    ) -> Result<OrganizationScimGroupSyncResult, OrganizationScimGroupRepositoryError> {
        let organization_id = token.organization_id();
        if !token
            .resource_scope()
            .contains(OrganizationScimResourceKind::Groups)
        {
            return Err(OrganizationScimGroupRepositoryError::Forbidden);
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
        let organization = lock_organization_for_scim(&transaction, organization_id)
            .await
            .map_err(map_group_user_error)?;
        ensure_active_organization(&organization).map_err(map_group_user_error)?;
        lock_scim_token_for_resource_sync(
            &transaction,
            &token,
            OrganizationScimResourceKind::Groups,
        )
        .await
        .map_err(map_group_user_error)?;
        let Some(binding) =
            lock_scim_group_resource(&transaction, organization_id, &resource_id).await?
        else {
            return Err(OrganizationScimGroupRepositoryError::TeamUnavailable);
        };
        let current_version =
            OrganizationScimResourceVersion::from_persistence(binding.resource_version)
                .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?;
        // DELETE 的 If-Match 是严格 CAS；未来版本也必须拒绝，不能沿用同步的单调推进规则。
        if expected_version != current_version {
            return Err(OrganizationScimGroupRepositoryError::Conflict);
        }
        if group_status(binding.status)? == OrganizationScimGroupStatus::Removed {
            let record = group_record_from_model(binding)?;
            transaction
                .commit()
                .await
                .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
            return Ok(OrganizationScimGroupSyncResult {
                record,
                outcome: OrganizationScimGroupSyncOutcome::Noop,
            });
        }
        let team_id = OrganizationTeamId::new(binding.team_id)
            .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?;
        let team = lock_scim_team(&transaction, organization_id, team_id).await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        if group_status(binding.status)? == OrganizationScimGroupStatus::Active {
            organization_memberships::Entity::update_many()
                .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
                .filter(organization_memberships::Column::TeamId.eq(team_id.get()))
                .col_expr(
                    organization_memberships::Column::TeamId,
                    Expr::value(Option::<i64>::None),
                )
                .col_expr(
                    organization_memberships::Column::Version,
                    Expr::col(organization_memberships::Column::Version).add(1_i64),
                )
                .exec(&transaction)
                .await
                .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
        }
        let next_team_version = team
            .version
            .checked_add(1)
            .ok_or(OrganizationScimGroupRepositoryError::Invariant)?;
        let mut team_active = team.into_active_model();
        team_active.status = Set(OrganizationTeamStatus::Disabled.code());
        team_active.version = Set(next_team_version);
        team_active.updated_at = Set(now);
        team_active.deleted_at = Set(Some(now));
        team_active
            .update(&transaction)
            .await
            .map_err(map_group_db_error)?;
        let mut active = binding.into_active_model();
        active.status = Set(2);
        active.updated_at = Set(now);
        active.deleted_at = Set(Some(now));
        let model = active
            .update(&transaction)
            .await
            .map_err(map_group_db_error)?;
        let record = group_record_from_model(model)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
        Ok(OrganizationScimGroupSyncResult {
            record,
            outcome: OrganizationScimGroupSyncOutcome::Applied,
        })
    }
}

impl fmt::Debug for OrganizationScimGroupRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimGroupRepository(<pool>)")
    }
}

fn group_status(
    value: i16,
) -> Result<OrganizationScimGroupStatus, OrganizationScimGroupRepositoryError> {
    match value {
        1 => Ok(OrganizationScimGroupStatus::Active),
        2 => Ok(OrganizationScimGroupStatus::Removed),
        _ => Err(OrganizationScimGroupRepositoryError::Invariant),
    }
}

fn group_record_from_model(
    model: organization_scim_group_resources::Model,
) -> Result<OrganizationScimGroupRecord, OrganizationScimGroupRepositoryError> {
    Ok(OrganizationScimGroupRecord {
        organization_id: OrganizationId::new(model.organization_id)
            .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?,
        resource_id: OrganizationScimResourceId::new(model.resource_id)
            .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?,
        team_id: OrganizationTeamId::new(model.team_id)
            .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?,
        resource_version: OrganizationScimResourceVersion::from_persistence(model.resource_version)
            .map_err(|_| OrganizationScimGroupRepositoryError::Invariant)?,
        status: group_status(model.status)?,
    })
}

async fn lock_scim_group_resource(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    resource_id: &OrganizationScimResourceId,
) -> Result<Option<organization_scim_group_resources::Model>, OrganizationScimGroupRepositoryError>
{
    let mut query = organization_scim_group_resources::Entity::find()
        .filter(organization_scim_group_resources::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_scim_group_resources::Column::ResourceId.eq(resource_id.as_str()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimGroupRepositoryError::Query)
}

async fn lock_scim_team(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    team_id: OrganizationTeamId,
) -> Result<organization_teams::Model, OrganizationScimGroupRepositoryError> {
    let mut query = organization_teams::Entity::find_by_id(team_id.get())
        .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimGroupRepositoryError::Query)?
        .ok_or(OrganizationScimGroupRepositoryError::TeamUnavailable)
}

async fn lock_active_scim_team(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    team_id: OrganizationTeamId,
) -> Result<organization_teams::Model, OrganizationScimGroupRepositoryError> {
    let team = lock_scim_team(transaction, organization_id, team_id).await?;
    if team.status != OrganizationTeamStatus::Active.code() || team.deleted_at.is_some() {
        return Err(OrganizationScimGroupRepositoryError::TeamUnavailable);
    }
    Ok(team)
}

async fn resolve_group_members(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    member_ids: &[OrganizationScimResourceId],
) -> Result<Vec<i64>, OrganizationScimGroupRepositoryError> {
    if member_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = member_ids
        .iter()
        .map(OrganizationScimResourceId::as_str)
        .collect::<Vec<_>>();
    let bindings = organization_scim_user_resources::Entity::find()
        .filter(organization_scim_user_resources::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_scim_user_resources::Column::ResourceId.is_in(ids))
        .all(transaction)
        .await
        .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
    if bindings.len() != member_ids.len() {
        return Err(OrganizationScimGroupRepositoryError::MemberUnavailable);
    }
    let mut user_ids = Vec::with_capacity(bindings.len());
    for binding in bindings {
        let membership = organization_memberships::Entity::find()
            .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_memberships::Column::UserId.eq(binding.user_id))
            .one(transaction)
            .await
            .map_err(|_| OrganizationScimGroupRepositoryError::Query)?
            .ok_or(OrganizationScimGroupRepositoryError::MemberUnavailable)?;
        if membership.status != OrganizationMembershipStatus::Active.code() {
            return Err(OrganizationScimGroupRepositoryError::MemberUnavailable);
        }
        user_ids.push(binding.user_id);
    }
    Ok(user_ids)
}

async fn project_group_members(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    team_id: i64,
    user_ids: &[i64],
) -> Result<(), OrganizationScimGroupRepositoryError> {
    let current = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::TeamId.eq(team_id))
        .all(transaction)
        .await
        .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
    for membership in current {
        if !user_ids.contains(&membership.user_id) {
            let next_version = membership
                .version
                .checked_add(1)
                .ok_or(OrganizationScimGroupRepositoryError::Invariant)?;
            let mut active = membership.into_active_model();
            active.team_id = Set(None);
            active.version = Set(next_version);
            active
                .update(transaction)
                .await
                .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
        }
    }
    for user_id in user_ids {
        let membership = organization_memberships::Entity::find()
            .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_memberships::Column::UserId.eq(*user_id))
            .one(transaction)
            .await
            .map_err(|_| OrganizationScimGroupRepositoryError::Query)?
            .ok_or(OrganizationScimGroupRepositoryError::MemberUnavailable)?;
        if membership.team_id != Some(team_id) {
            let next_version = membership
                .version
                .checked_add(1)
                .ok_or(OrganizationScimGroupRepositoryError::Invariant)?;
            let mut active = membership.into_active_model();
            active.team_id = Set(Some(team_id));
            active.version = Set(next_version);
            active
                .update(transaction)
                .await
                .map_err(|_| OrganizationScimGroupRepositoryError::Query)?;
        }
    }
    Ok(())
}

fn map_group_user_error(
    error: OrganizationScimUserRepositoryError,
) -> OrganizationScimGroupRepositoryError {
    match error {
        OrganizationScimUserRepositoryError::Restricted => {
            OrganizationScimGroupRepositoryError::Restricted
        }
        OrganizationScimUserRepositoryError::Forbidden => {
            OrganizationScimGroupRepositoryError::Forbidden
        }
        OrganizationScimUserRepositoryError::Conflict => {
            OrganizationScimGroupRepositoryError::Conflict
        }
        OrganizationScimUserRepositoryError::UserUnavailable => {
            OrganizationScimGroupRepositoryError::MemberUnavailable
        }
        OrganizationScimUserRepositoryError::LifecycleRejected => {
            OrganizationScimGroupRepositoryError::LifecycleRejected
        }
        OrganizationScimUserRepositoryError::Query => OrganizationScimGroupRepositoryError::Query,
        OrganizationScimUserRepositoryError::Timeout => {
            OrganizationScimGroupRepositoryError::Timeout
        }
        OrganizationScimUserRepositoryError::Invariant => {
            OrganizationScimGroupRepositoryError::Invariant
        }
    }
}

fn map_group_db_error(error: DbErr) -> OrganizationScimGroupRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationScimGroupRepositoryError::Conflict
    } else {
        OrganizationScimGroupRepositoryError::Query
    }
}

impl OrganizationScimUserRepository {
    /// 创建用户同步仓储；零超时会被拒绝，避免目录写入无限等待。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationScimUserRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationScimUserRepositoryError::Invariant);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 根据已认证令牌同步一个企业用户资源，并在事务内再次验证令牌版本和范围。
    pub async fn synchronize(
        &self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimUserResource,
    ) -> Result<OrganizationScimUserSyncResult, OrganizationScimUserRepositoryError> {
        timeout(
            self.operation_timeout,
            self.synchronize_inner(token, resource),
        )
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Timeout)?
    }

    async fn synchronize_inner(
        &self,
        token: OrganizationScimTokenRecord,
        resource: OrganizationScimUserResource,
    ) -> Result<OrganizationScimUserSyncResult, OrganizationScimUserRepositoryError> {
        let organization_id = resource.key().organization_id();
        if token.organization_id() != organization_id {
            return Err(OrganizationScimUserRepositoryError::Restricted);
        }
        if !token
            .resource_scope()
            .contains(OrganizationScimResourceKind::Users)
        {
            return Err(OrganizationScimUserRepositoryError::Forbidden);
        }

        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationScimUserRepositoryError::Query)?;
        let organization = lock_organization_for_scim(&transaction, organization_id).await?;
        ensure_active_organization(&organization)?;
        lock_scim_token_for_resource_sync(
            &transaction,
            &token,
            OrganizationScimResourceKind::Users,
        )
        .await?;

        let entitlement = lock_scim_entitlement(&transaction, organization_id).await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let facts = scim_lifecycle_facts(
            &organization,
            &entitlement,
            &transaction,
            organization_id,
            now,
        )
        .await?;
        let resource_id = resource.key().resource_id().clone();
        let binding = lock_scim_user_resource(&transaction, organization_id, &resource_id).await?;

        let (binding, membership) = match binding {
            Some(binding) => {
                let current =
                    OrganizationScimResourceVersion::from_persistence(binding.resource_version)
                        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
                match classify_organization_scim_sync(Some(current), resource.version()) {
                    OrganizationScimSyncDisposition::RejectStale => {
                        return Err(OrganizationScimUserRepositoryError::Conflict);
                    }
                    OrganizationScimSyncDisposition::Noop => {
                        let membership = lock_scim_membership(
                            &transaction,
                            organization_id,
                            UserId::new(binding.user_id)
                                .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
                        )
                        .await?
                        .ok_or(OrganizationScimUserRepositoryError::Invariant)?;
                        ensure_scim_managed_membership(&membership)?;
                        let user = ensure_active_scim_user(
                            &transaction,
                            UserId::new(binding.user_id)
                                .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
                        )
                        .await?;
                        if user.username != resource.user_name() {
                            return Err(OrganizationScimUserRepositoryError::Conflict);
                        }
                        let status = membership_status(&membership)?;
                        let record = scim_user_record_from_binding(binding, status)?;
                        transaction
                            .commit()
                            .await
                            .map_err(|_| OrganizationScimUserRepositoryError::Query)?;
                        return Ok(OrganizationScimUserSyncResult {
                            record,
                            outcome: OrganizationScimUserSyncOutcome::Noop,
                        });
                    }
                    OrganizationScimSyncDisposition::Apply => {
                        let membership = lock_scim_membership(
                            &transaction,
                            organization_id,
                            UserId::new(binding.user_id)
                                .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
                        )
                        .await?
                        .ok_or(OrganizationScimUserRepositoryError::Invariant)?;
                        ensure_scim_managed_membership(&membership)?;
                        let user = ensure_active_scim_user(
                            &transaction,
                            UserId::new(binding.user_id)
                                .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
                        )
                        .await?;
                        if user.username != resource.user_name() {
                            return Err(OrganizationScimUserRepositoryError::Conflict);
                        }
                        (Some(binding), membership)
                    }
                }
            }
            None => {
                let user = lock_active_user_by_exact_username(&transaction, resource.user_name())
                    .await?
                    .ok_or(OrganizationScimUserRepositoryError::UserUnavailable)?;
                let user_id = UserId::new(user.id)
                    .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
                if lock_scim_user_resource_by_user(&transaction, organization_id, user_id)
                    .await?
                    .is_some()
                {
                    return Err(OrganizationScimUserRepositoryError::Conflict);
                }
                let membership =
                    lock_scim_membership(&transaction, organization_id, user_id).await?;
                match membership {
                    Some(membership) => {
                        ensure_scim_managed_membership(&membership)?;
                        (None, membership)
                    }
                    None => {
                        let disposition = classify_organization_scim_lifecycle(
                            None,
                            resource_lifecycle_intent(&resource),
                            facts,
                        );
                        if disposition != OrganizationScimLifecycleDisposition::Provision {
                            return Err(OrganizationScimUserRepositoryError::LifecycleRejected);
                        }
                        let membership =
                            create_scim_membership(&transaction, organization_id, user_id, now)
                                .await?;
                        (None, membership)
                    }
                }
            }
        };

        let current_status = membership_status(&membership)?;
        let disposition = classify_organization_scim_lifecycle(
            Some(current_status),
            resource_lifecycle_intent(&resource),
            facts,
        );
        let membership = apply_scim_membership_lifecycle(
            &transaction,
            organization_id,
            membership,
            disposition,
            now,
        )
        .await?;
        let membership_status = membership_status(&membership)?;

        let binding = match binding {
            Some(binding) => {
                update_scim_user_resource_version(&transaction, binding, resource.version(), now)
                    .await?
            }
            None => {
                create_scim_user_resource(
                    &transaction,
                    organization_id,
                    &resource_id,
                    UserId::new(membership.user_id)
                        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
                    resource.version(),
                    now,
                )
                .await?
            }
        };
        let record = scim_user_record_from_binding(binding, membership_status)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationScimUserRepositoryError::Query)?;
        Ok(OrganizationScimUserSyncResult {
            record,
            outcome: OrganizationScimUserSyncOutcome::Applied,
        })
    }
}

impl fmt::Debug for OrganizationScimUserRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimUserRepository(<pool>)")
    }
}

fn resource_lifecycle_intent(
    resource: &OrganizationScimUserResource,
) -> af_domain::OrganizationScimLifecycleIntent {
    af_domain::OrganizationScimLifecycleIntent::from_active(resource.active())
}

async fn lock_organization_for_scim(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<organizations::Model, OrganizationScimUserRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 通过受控无变更更新串行化同企业同步，避免席位和绑定出现竞争窗口。
        let result = organizations::Entity::update_many()
            .filter(organizations::Column::Id.eq(organization_id.get()))
            .col_expr(
                organizations::Column::Version,
                Expr::col(organizations::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(|_| OrganizationScimUserRepositoryError::Query)?;
        if result.rows_affected != 1 {
            return Err(OrganizationScimUserRepositoryError::Restricted);
        }
    }
    let mut query = organizations::Entity::find_by_id(organization_id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)?
        .ok_or(OrganizationScimUserRepositoryError::Restricted)
}

fn ensure_active_organization(
    organization: &organizations::Model,
) -> Result<(), OrganizationScimUserRepositoryError> {
    if OrganizationStatus::try_from(organization.status)
        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?
        != OrganizationStatus::Active
        || organization.closed_at.is_some()
    {
        return Err(OrganizationScimUserRepositoryError::Restricted);
    }
    Ok(())
}

async fn lock_scim_token_for_resource_sync(
    transaction: &DatabaseTransaction,
    token: &OrganizationScimTokenRecord,
    resource_kind: OrganizationScimResourceKind,
) -> Result<(), OrganizationScimUserRepositoryError> {
    let mut query = organization_scim_tokens::Entity::find_by_id(token.id().get())
        .filter(organization_scim_tokens::Column::OrganizationId.eq(token.organization_id().get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let stored = query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)?
        .ok_or(OrganizationScimUserRepositoryError::Restricted)?;
    let stored =
        record_from_model(stored).map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
    if stored.version() != token.version()
        || stored.status() != OrganizationScimTokenStatus::Active
        || stored.organization_id() != token.organization_id()
    {
        return Err(OrganizationScimUserRepositoryError::Restricted);
    }
    if !stored.resource_scope().contains(resource_kind) {
        return Err(OrganizationScimUserRepositoryError::Forbidden);
    }
    Ok(())
}

async fn lock_scim_entitlement(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<organization_entitlements::Model, OrganizationScimUserRepositoryError> {
    let mut query = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)?
        .ok_or(OrganizationScimUserRepositoryError::Invariant)
}

async fn scim_lifecycle_facts(
    organization: &organizations::Model,
    entitlement: &organization_entitlements::Model,
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    now: TimeDateTimeWithTimeZone,
) -> Result<OrganizationScimLifecycleFacts, OrganizationScimUserRepositoryError> {
    let capacity = OrganizationEntitlementCapacity::new(
        entitlement.seat_limit,
        entitlement.team_limit,
        entitlement.organization_key_limit,
    )
    .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
    let snapshot = af_domain::OrganizationEntitlementSnapshot::new(
        capacity,
        entitlement.team_management_enabled,
        entitlement.sso_enabled,
        entitlement.custom_roles_enabled,
        entitlement.audit_retention_days,
        entitlement.valid_from.unix_timestamp(),
        entitlement.valid_until.unix_timestamp(),
        entitlement.grace_until.map(|value| value.unix_timestamp()),
    )
    .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
    let status = OrganizationStatus::try_from(organization.status)
        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
    let active_member_count = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .count(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)?;
    let active_member_count = i64::try_from(active_member_count)
        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
    OrganizationScimLifecycleFacts::new(
        snapshot.status(status, now.unix_timestamp()),
        active_member_count,
        snapshot.capacity().seat_limit(),
    )
    .map_err(|_| OrganizationScimUserRepositoryError::Invariant)
}

async fn lock_scim_user_resource(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    resource_id: &OrganizationScimResourceId,
) -> Result<Option<organization_scim_user_resources::Model>, OrganizationScimUserRepositoryError> {
    let mut query = organization_scim_user_resources::Entity::find()
        .filter(organization_scim_user_resources::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_scim_user_resources::Column::ResourceId.eq(resource_id.as_str()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)
}

async fn lock_scim_user_resource_by_user(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<Option<organization_scim_user_resources::Model>, OrganizationScimUserRepositoryError> {
    let mut query = organization_scim_user_resources::Entity::find()
        .filter(organization_scim_user_resources::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_scim_user_resources::Column::UserId.eq(user_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)
}

async fn lock_active_user_by_exact_username(
    transaction: &DatabaseTransaction,
    username: &str,
) -> Result<Option<users::Model>, OrganizationScimUserRepositoryError> {
    let mut query = users::Entity::find()
        .filter(users::Column::Username.eq(username))
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)
}

async fn ensure_active_scim_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<users::Model, OrganizationScimUserRepositoryError> {
    let mut query = users::Entity::find_by_id(user_id.get())
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)?
        .ok_or(OrganizationScimUserRepositoryError::UserUnavailable)
}

async fn lock_scim_membership(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<Option<organization_memberships::Model>, OrganizationScimUserRepositoryError> {
    let mut query = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)
}

fn ensure_scim_managed_membership(
    membership: &organization_memberships::Model,
) -> Result<(), OrganizationScimUserRepositoryError> {
    let role = OrganizationRole::try_from(membership.role)
        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
    if role != OrganizationRole::Member || membership.custom_role_id.is_some() {
        return Err(OrganizationScimUserRepositoryError::LifecycleRejected);
    }
    membership_status(membership).map(|_| ())
}

fn membership_status(
    membership: &organization_memberships::Model,
) -> Result<OrganizationMembershipStatus, OrganizationScimUserRepositoryError> {
    let status = OrganizationMembershipStatus::try_from(membership.status)
        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?;
    let valid_times = match status {
        OrganizationMembershipStatus::Active => {
            membership.suspended_at.is_none() && membership.removed_at.is_none()
        }
        OrganizationMembershipStatus::Suspended => {
            membership.suspended_at.is_some() && membership.removed_at.is_none()
        }
        OrganizationMembershipStatus::Removed => membership.removed_at.is_some(),
    };
    if membership.version <= 0 || !valid_times {
        return Err(OrganizationScimUserRepositoryError::Invariant);
    }
    Ok(status)
}

async fn create_scim_membership(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
    now: TimeDateTimeWithTimeZone,
) -> Result<organization_memberships::Model, OrganizationScimUserRepositoryError> {
    organization_memberships::ActiveModel {
        organization_id: Set(organization_id.get()),
        user_id: Set(user_id.get()),
        role: Set(OrganizationRole::Member.code()),
        team_id: Set(None),
        custom_role_id: Set(None),
        status: Set(OrganizationMembershipStatus::Active.code()),
        joined_at: Set(now),
        suspended_at: Set(None),
        removed_at: Set(None),
        invited_by_user_id: Set(None),
        version: Set(1),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(map_scim_user_write_error)
}

async fn apply_scim_membership_lifecycle(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    membership: organization_memberships::Model,
    disposition: OrganizationScimLifecycleDisposition,
    now: TimeDateTimeWithTimeZone,
) -> Result<organization_memberships::Model, OrganizationScimUserRepositoryError> {
    let target = match disposition {
        OrganizationScimLifecycleDisposition::Noop
        | OrganizationScimLifecycleDisposition::Provision => {
            return Ok(membership);
        }
        OrganizationScimLifecycleDisposition::Suspend => OrganizationMembershipStatus::Suspended,
        OrganizationScimLifecycleDisposition::Restore => OrganizationMembershipStatus::Active,
        OrganizationScimLifecycleDisposition::SoftDelete => OrganizationMembershipStatus::Removed,
        OrganizationScimLifecycleDisposition::Reject(_) => {
            return Err(OrganizationScimUserRepositoryError::LifecycleRejected);
        }
    };
    let next_version = membership
        .version
        .checked_add(1)
        .ok_or(OrganizationScimUserRepositoryError::Invariant)?;
    let (suspended_at, removed_at) = match target {
        OrganizationMembershipStatus::Active => (None, None),
        OrganizationMembershipStatus::Suspended => (membership.suspended_at.or(Some(now)), None),
        OrganizationMembershipStatus::Removed => (membership.suspended_at, Some(now)),
    };
    let membership_id = membership.id;
    let mut active = membership.into_active_model();
    active.status = Set(target.code());
    active.suspended_at = Set(suspended_at);
    active.removed_at = Set(removed_at);
    active.version = Set(next_version);
    let updated = active
        .update(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)?;
    if target != OrganizationMembershipStatus::Active {
        // 停用或移除时使旧企业 Key 失效；恢复成员不会重新启用旧 Key。
        tokens::Entity::update_many()
            .filter(tokens::Column::OrganizationId.eq(organization_id.get()))
            .filter(tokens::Column::OrganizationMembershipId.eq(membership_id))
            .filter(tokens::Column::DeletedAt.is_null())
            .filter(tokens::Column::Status.eq(ENABLED_TOKEN_STATUS))
            .col_expr(tokens::Column::Status, Expr::value(DISABLED_TOKEN_STATUS))
            .col_expr(tokens::Column::UpdatedAt, Expr::value(now))
            .exec(transaction)
            .await
            .map_err(|_| OrganizationScimUserRepositoryError::Query)?;
    }
    Ok(updated)
}

async fn create_scim_user_resource(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    resource_id: &OrganizationScimResourceId,
    user_id: UserId,
    resource_version: OrganizationScimResourceVersion,
    now: TimeDateTimeWithTimeZone,
) -> Result<organization_scim_user_resources::Model, OrganizationScimUserRepositoryError> {
    organization_scim_user_resources::ActiveModel {
        organization_id: Set(organization_id.get()),
        resource_id: Set(resource_id.as_str().to_owned()),
        user_id: Set(user_id.get()),
        resource_version: Set(i64::try_from(resource_version.get())
            .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(map_scim_user_write_error)
}

async fn update_scim_user_resource_version(
    transaction: &DatabaseTransaction,
    binding: organization_scim_user_resources::Model,
    resource_version: OrganizationScimResourceVersion,
    now: TimeDateTimeWithTimeZone,
) -> Result<organization_scim_user_resources::Model, OrganizationScimUserRepositoryError> {
    let mut active = binding.into_active_model();
    active.resource_version = Set(i64::try_from(resource_version.get())
        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?);
    active.updated_at = Set(now);
    active
        .update(transaction)
        .await
        .map_err(|_| OrganizationScimUserRepositoryError::Query)
}

fn scim_user_record_from_binding(
    binding: organization_scim_user_resources::Model,
    membership_status: OrganizationMembershipStatus,
) -> Result<OrganizationScimUserRecord, OrganizationScimUserRepositoryError> {
    Ok(OrganizationScimUserRecord {
        organization_id: OrganizationId::new(binding.organization_id)
            .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
        resource_id: OrganizationScimResourceId::new(binding.resource_id)
            .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
        user_id: UserId::new(binding.user_id)
            .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
        resource_version: OrganizationScimResourceVersion::from_persistence(
            binding.resource_version,
        )
        .map_err(|_| OrganizationScimUserRepositoryError::Invariant)?,
        membership_status,
    })
}

fn map_scim_user_write_error(error: DbErr) -> OrganizationScimUserRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationScimUserRepositoryError::Conflict
    } else {
        OrganizationScimUserRepositoryError::Query
    }
}
