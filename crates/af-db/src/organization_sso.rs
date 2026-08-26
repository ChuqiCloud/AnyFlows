use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationSsoDnsVerification,
    OrganizationSsoDomain, OrganizationSsoDomainStatus, OrganizationSsoIdentityBindingMethod,
    OrganizationSsoIdentityLoginFacts, OrganizationSsoIdentityStatus, OrganizationSsoProtocol,
    OrganizationSsoProviderKey, OrganizationSsoProviderStatus, OrganizationStatus, UserId,
    authorize_organization_sso_identity_login,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr,
    TransactionTrait,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{
        EncryptedJson, SensitiveString, organization_entitlements, organization_memberships,
        organization_sso_domains, organization_sso_identities, organization_sso_login_transactions,
        organization_sso_providers, organizations,
    },
};

/// 创建企业 SSO Provider 的配置事实。
pub struct OrganizationSsoProviderCreate {
    provider_key: OrganizationSsoProviderKey,
    protocol: OrganizationSsoProtocol,
    status: OrganizationSsoProviderStatus,
    display_name: String,
    metadata_projection: Option<Json>,
    secret_envelope: Option<Json>,
}

impl OrganizationSsoProviderCreate {
    /// 组装 Provider；密钥只接受已经加密的封套 JSON。
    #[must_use]
    pub fn new(
        provider_key: OrganizationSsoProviderKey,
        protocol: OrganizationSsoProtocol,
        status: OrganizationSsoProviderStatus,
        display_name: String,
        metadata_projection: Option<Json>,
        secret_envelope: Option<Json>,
    ) -> Self {
        Self {
            provider_key,
            protocol,
            status,
            display_name,
            metadata_projection,
            secret_envelope,
        }
    }
}

impl fmt::Debug for OrganizationSsoProviderCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoProviderCreate")
            .field("provider_key", &self.provider_key)
            .field("protocol", &self.protocol)
            .field("status", &self.status)
            .field("display_name", &self.display_name)
            .field("secret_configured", &self.secret_envelope.is_some())
            .finish_non_exhaustive()
    }
}

/// Provider 密钥封套的受控更新动作。
pub enum OrganizationSsoProviderSecretChange {
    /// 保留当前密文。
    Keep,
    /// 替换为新的版本化密文封套。
    Replace(Json),
    /// 清除当前密文。
    Clear,
}

impl fmt::Debug for OrganizationSsoProviderSecretChange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "Keep",
            Self::Replace(_) => "Replace(<redacted>)",
            Self::Clear => "Clear",
        })
    }
}

/// 按版本替换 Provider 的完整非密钥配置。
pub struct OrganizationSsoProviderUpdate {
    protocol: OrganizationSsoProtocol,
    status: OrganizationSsoProviderStatus,
    display_name: String,
    metadata_projection: Option<Json>,
    secret_change: OrganizationSsoProviderSecretChange,
    expected_version: i64,
}

impl OrganizationSsoProviderUpdate {
    /// 组装 Provider CAS 更新。
    #[must_use]
    pub fn new(
        protocol: OrganizationSsoProtocol,
        status: OrganizationSsoProviderStatus,
        display_name: String,
        metadata_projection: Option<Json>,
        secret_change: OrganizationSsoProviderSecretChange,
        expected_version: i64,
    ) -> Self {
        Self {
            protocol,
            status,
            display_name,
            metadata_projection,
            secret_change,
            expected_version,
        }
    }
}

impl fmt::Debug for OrganizationSsoProviderUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoProviderUpdate")
            .field("protocol", &self.protocol)
            .field("status", &self.status)
            .field("display_name", &self.display_name)
            .field("secret_change", &self.secret_change)
            .field("expected_version", &self.expected_version)
            .finish_non_exhaustive()
    }
}

/// 已持久化的 Provider 安全投影；不返回密文原文。
#[derive(Clone, PartialEq)]
pub struct OrganizationSsoProviderRecord {
    id: i64,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
    protocol: OrganizationSsoProtocol,
    status: OrganizationSsoProviderStatus,
    display_name: String,
    metadata_projection: Option<Json>,
    secret_configured: bool,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

impl OrganizationSsoProviderRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn provider_key(&self) -> OrganizationSsoProviderKey {
        self.provider_key
    }
    #[must_use]
    pub const fn protocol(&self) -> OrganizationSsoProtocol {
        self.protocol
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationSsoProviderStatus {
        self.status
    }
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    #[must_use]
    pub fn metadata_projection(&self) -> Option<&Json> {
        self.metadata_projection.as_ref()
    }
    #[must_use]
    pub const fn secret_configured(&self) -> bool {
        self.secret_configured
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
}

impl fmt::Debug for OrganizationSsoProviderRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoProviderRecord")
            .field("id", &self.id)
            .field("organization_id", &self.organization_id)
            .field("provider_key", &self.provider_key)
            .field("protocol", &self.protocol)
            .field("status", &self.status)
            .field("secret_configured", &self.secret_configured)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 创建企业 SSO 域名认领事实。
pub struct OrganizationSsoDomainCreate {
    provider_key: OrganizationSsoProviderKey,
    domain: OrganizationSsoDomain,
    proof_digest: String,
    proof_expires_at: i64,
}

impl OrganizationSsoDomainCreate {
    /// 组装待验证域名；证明令牌只允许传入摘要。
    #[must_use]
    pub fn new(
        provider_key: OrganizationSsoProviderKey,
        domain: OrganizationSsoDomain,
        proof_digest: String,
        proof_expires_at: i64,
    ) -> Self {
        Self {
            provider_key,
            domain,
            proof_digest,
            proof_expires_at,
        }
    }
}

/// 域名状态和证明时间的 CAS 更新。
pub struct OrganizationSsoDomainUpdate {
    status: OrganizationSsoDomainStatus,
    proof_expires_at: Option<i64>,
    verified_at: Option<i64>,
    recheck_at: Option<i64>,
    revoked_at: Option<i64>,
    expected_version: i64,
}

impl OrganizationSsoDomainUpdate {
    /// 组装域名状态更新；状态与时间字段由仓储再次闭合校验。
    #[must_use]
    pub fn new(
        status: OrganizationSsoDomainStatus,
        proof_expires_at: Option<i64>,
        verified_at: Option<i64>,
        recheck_at: Option<i64>,
        revoked_at: Option<i64>,
        expected_version: i64,
    ) -> Self {
        Self {
            status,
            proof_expires_at,
            verified_at,
            recheck_at,
            revoked_at,
            expected_version,
        }
    }
}

/// 已持久化的企业域名事实。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationSsoDomainRecord {
    id: i64,
    organization_id: OrganizationId,
    provider_id: i64,
    provider_key: OrganizationSsoProviderKey,
    domain: OrganizationSsoDomain,
    status: OrganizationSsoDomainStatus,
    proof_expires_at: Option<i64>,
    verified_at: Option<i64>,
    recheck_at: Option<i64>,
    revoked_at: Option<i64>,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

impl OrganizationSsoDomainRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn provider_id(&self) -> i64 {
        self.provider_id
    }
    #[must_use]
    pub const fn provider_key(&self) -> OrganizationSsoProviderKey {
        self.provider_key
    }
    #[must_use]
    pub fn domain(&self) -> &OrganizationSsoDomain {
        &self.domain
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationSsoDomainStatus {
        self.status
    }
    #[must_use]
    pub const fn proof_expires_at(&self) -> Option<i64> {
        self.proof_expires_at
    }
    #[must_use]
    pub const fn verified_at(&self) -> Option<i64> {
        self.verified_at
    }
    #[must_use]
    pub const fn recheck_at(&self) -> Option<i64> {
        self.recheck_at
    }
    #[must_use]
    pub const fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 创建企业 SSO 身份绑定事实。
pub struct OrganizationSsoIdentityCreate {
    provider_key: OrganizationSsoProviderKey,
    subject_digest: String,
    membership_id: OrganizationMembershipId,
    binding_method: OrganizationSsoIdentityBindingMethod,
}

impl OrganizationSsoIdentityCreate {
    /// 组装身份绑定；subject 只允许传入域分离摘要。
    #[must_use]
    pub fn new(
        provider_key: OrganizationSsoProviderKey,
        subject_digest: String,
        membership_id: OrganizationMembershipId,
        binding_method: OrganizationSsoIdentityBindingMethod,
    ) -> Self {
        Self {
            provider_key,
            subject_digest,
            membership_id,
            binding_method,
        }
    }
}

/// 已持久化的企业身份绑定事实。
#[derive(Clone, PartialEq)]
pub struct OrganizationSsoIdentityRecord {
    id: i64,
    organization_id: OrganizationId,
    provider_id: i64,
    subject_digest: String,
    membership_id: OrganizationMembershipId,
    status: OrganizationSsoIdentityStatus,
    binding_method: OrganizationSsoIdentityBindingMethod,
    version: i64,
    disabled_at: Option<i64>,
}

impl fmt::Debug for OrganizationSsoIdentityRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoIdentityRecord")
            .field("id", &self.id)
            .field("organization_id", &self.organization_id)
            .field("provider_id", &self.provider_id)
            .field("membership_id", &self.membership_id)
            .field("status", &self.status)
            .field("binding_method", &self.binding_method)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

impl OrganizationSsoIdentityRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn provider_id(&self) -> i64 {
        self.provider_id
    }
    #[must_use]
    pub fn subject_digest(&self) -> &str {
        &self.subject_digest
    }
    #[must_use]
    pub const fn membership_id(&self) -> OrganizationMembershipId {
        self.membership_id
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationSsoIdentityStatus {
        self.status
    }
    #[must_use]
    pub const fn binding_method(&self) -> OrganizationSsoIdentityBindingMethod {
        self.binding_method
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn disabled_at(&self) -> Option<i64> {
        self.disabled_at
    }
}

/// 创建单次 SSO 登录事务。
pub struct OrganizationSsoLoginTransactionCreate {
    provider_key: OrganizationSsoProviderKey,
    protocol: OrganizationSsoProtocol,
    provider_version: i64,
    state_digest: String,
    request_digest: String,
    login_material_envelope: Option<Json>,
    expires_at: i64,
}

impl OrganizationSsoLoginTransactionCreate {
    /// 组装登录事务；state/request 只允许传入摘要。
    #[must_use]
    pub fn new(
        provider_key: OrganizationSsoProviderKey,
        protocol: OrganizationSsoProtocol,
        provider_version: i64,
        state_digest: String,
        request_digest: String,
        expires_at: i64,
    ) -> Self {
        Self {
            provider_key,
            protocol,
            provider_version,
            state_digest,
            request_digest,
            login_material_envelope: None,
            expires_at,
        }
    }

    /// 绑定仅含 PKCE verifier 与 nonce 的加密封套；仓储会再次校验固定封套形状。
    #[must_use]
    pub fn with_login_material_envelope(mut self, envelope: Json) -> Self {
        self.login_material_envelope = Some(envelope);
        self
    }
}

/// 登录事务完成时一次性写入的票据和断言摘要。
pub struct OrganizationSsoLoginTransactionResult {
    result_code: i16,
    ticket_digest: Option<String>,
    ticket_expires_at: Option<i64>,
    assertion_digest: Option<String>,
    assertion_expires_at: Option<i64>,
    invitation_email_fingerprint: Option<[u8; 32]>,
}

impl OrganizationSsoLoginTransactionResult {
    /// 组装结果；摘要与过期时间必须成对出现。
    #[must_use]
    pub fn new(
        result_code: i16,
        ticket_digest: Option<String>,
        ticket_expires_at: Option<i64>,
        assertion_digest: Option<String>,
        assertion_expires_at: Option<i64>,
    ) -> Self {
        Self {
            result_code,
            ticket_digest,
            ticket_expires_at,
            assertion_digest,
            assertion_expires_at,
            invitation_email_fingerprint: None,
        }
    }

    /// 绑定已验证 OIDC 邮箱的企业隔离指纹；调用方不得传入邮箱原文。
    #[must_use]
    pub fn with_invitation_email_fingerprint(mut self, fingerprint: [u8; 32]) -> Self {
        self.invitation_email_fingerprint = Some(fingerprint);
        self
    }
}

/// 已持久化的单次登录事务状态。
#[derive(Clone, PartialEq)]
pub struct OrganizationSsoLoginTransactionRecord {
    id: i64,
    organization_id: OrganizationId,
    provider_id: i64,
    protocol: OrganizationSsoProtocol,
    provider_version: i64,
    state_digest: String,
    request_digest: String,
    login_material_envelope: Option<EncryptedCredentialEnvelope>,
    expires_at: i64,
    claimed_at: Option<i64>,
    result_code: Option<i16>,
    ticket_digest: Option<String>,
    ticket_expires_at: Option<i64>,
    exchanged_at: Option<i64>,
    assertion_digest: Option<String>,
    assertion_expires_at: Option<i64>,
    invitation_email_fingerprint: Option<[u8; 32]>,
}

impl fmt::Debug for OrganizationSsoLoginTransactionRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoLoginTransactionRecord")
            .field("id", &self.id)
            .field("organization_id", &self.organization_id)
            .field("provider_id", &self.provider_id)
            .field("protocol", &self.protocol)
            .field("provider_version", &self.provider_version)
            .field(
                "has_login_material",
                &self.login_material_envelope.is_some(),
            )
            .field("expires_at", &self.expires_at)
            .field("claimed_at", &self.claimed_at)
            .field("result_code", &self.result_code)
            .field("has_ticket", &self.ticket_digest.is_some())
            .field("ticket_expires_at", &self.ticket_expires_at)
            .field("exchanged_at", &self.exchanged_at)
            .field("has_assertion", &self.assertion_digest.is_some())
            .field("assertion_expires_at", &self.assertion_expires_at)
            .field(
                "has_invitation_email_fingerprint",
                &self.invitation_email_fingerprint.is_some(),
            )
            .finish_non_exhaustive()
    }
}

impl OrganizationSsoLoginTransactionRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn provider_id(&self) -> i64 {
        self.provider_id
    }
    #[must_use]
    pub const fn protocol(&self) -> OrganizationSsoProtocol {
        self.protocol
    }
    #[must_use]
    pub const fn provider_version(&self) -> i64 {
        self.provider_version
    }
    #[must_use]
    pub fn state_digest(&self) -> &str {
        &self.state_digest
    }
    #[must_use]
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }
    /// 返回受校验的短期登录材料封套；调用方只能交给对应 AAD 的系统密钥解密器。
    #[must_use]
    pub fn login_material_envelope(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.login_material_envelope.as_ref()
    }
    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }
    #[must_use]
    pub const fn claimed_at(&self) -> Option<i64> {
        self.claimed_at
    }
    #[must_use]
    pub const fn result_code(&self) -> Option<i16> {
        self.result_code
    }
    #[must_use]
    pub fn ticket_digest(&self) -> Option<&str> {
        self.ticket_digest.as_deref()
    }
    #[must_use]
    pub const fn ticket_expires_at(&self) -> Option<i64> {
        self.ticket_expires_at
    }
    #[must_use]
    pub const fn exchanged_at(&self) -> Option<i64> {
        self.exchanged_at
    }
    #[must_use]
    pub fn assertion_digest(&self) -> Option<&str> {
        self.assertion_digest.as_deref()
    }
    #[must_use]
    pub const fn assertion_expires_at(&self) -> Option<i64> {
        self.assertion_expires_at
    }
    /// 返回已验证 OIDC 邮箱的企业隔离指纹；SAML 与旧 ticket 不具备该投影。
    #[must_use]
    pub fn invitation_email_fingerprint(&self) -> Option<&[u8; 32]> {
        self.invitation_email_fingerprint.as_ref()
    }
}

/// SSO 仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoProviderRepositoryConfigError {
    /// 零超时无法形成数据库硬截止时间。
    #[error("企业 SSO 仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// SSO 仓储错误；不携带企业、域名、subject 或票据原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoRepositoryError {
    /// 输入不满足值对象、摘要、时间或 CAS 版本边界。
    #[error("企业 SSO 输入无效")]
    InvalidInput,
    /// 企业范围内不存在目标记录。
    #[error("企业 SSO 记录不存在")]
    NotFound,
    /// 唯一绑定、Provider 活动状态或 CAS 发生冲突。
    #[error("企业 SSO 状态冲突")]
    Conflict,
    /// 记录已被领取、交换或完成，不能重复推进。
    #[error("企业 SSO 事务已处理")]
    AlreadyProcessed,
    /// 事务或票据已过期。
    #[error("企业 SSO 事务已过期")]
    Expired,
    /// 状态转换不满足登录事务生命周期。
    #[error("企业 SSO 状态转换无效")]
    InvalidTransition,
    /// Provider、成员或持久化状态不可用于当前操作。
    #[error("企业 SSO 记录不可用")]
    Unavailable,
    /// 数据库查询、事务或提交失败。
    #[error("企业 SSO 数据库操作失败")]
    Query,
    /// 单次数据库操作超过硬截止时间。
    #[error("企业 SSO 数据库操作超时")]
    Timeout,
    /// 持久化记录违反强类型或同企业边界。
    #[error("企业 SSO 持久化状态损坏")]
    Invariant,
}

/// 企业 SSO Provider、域名、身份和单次登录事务仓储。
#[derive(Clone)]
pub struct OrganizationSsoProviderRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationSsoProviderRepository {
    /// 使用共享连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationSsoProviderRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationSsoProviderRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在企业范围创建 Provider。
    pub async fn create_provider(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoProviderCreate,
    ) -> Result<OrganizationSsoProviderRecord, OrganizationSsoRepositoryError> {
        self.run(self.create_provider_inner(organization_id, write))
            .await
    }

    /// 按企业和 Provider 公开标识读取配置投影。
    pub async fn provider(
        &self,
        organization_id: OrganizationId,
        provider_key: OrganizationSsoProviderKey,
    ) -> Result<OrganizationSsoProviderRecord, OrganizationSsoRepositoryError> {
        self.run(async move {
            let model = organization_sso_providers::Entity::find()
                .filter(
                    organization_sso_providers::Column::OrganizationId.eq(organization_id.get()),
                )
                .filter(
                    organization_sso_providers::Column::ProviderKey
                        .eq(provider_key.persistence_key()),
                )
                .one(self.pool.connection())
                .await
                .map_err(query_error)?
                .ok_or(OrganizationSsoRepositoryError::NotFound)?;
            provider_record(model)
        })
        .await
    }

    /// 按已领取事务绑定的数据库 Provider ID 读取同一企业内的配置投影。
    pub async fn provider_by_id(
        &self,
        organization_id: OrganizationId,
        provider_id: i64,
    ) -> Result<OrganizationSsoProviderRecord, OrganizationSsoRepositoryError> {
        if provider_id <= 0 {
            return Err(OrganizationSsoRepositoryError::InvalidInput);
        }
        self.run(async move {
            let model = organization_sso_providers::Entity::find_by_id(provider_id)
                .filter(
                    organization_sso_providers::Column::OrganizationId.eq(organization_id.get()),
                )
                .one(self.pool.connection())
                .await
                .map_err(query_error)?
                .ok_or(OrganizationSsoRepositoryError::NotFound)?;
            provider_record(model)
        })
        .await
    }

    /// 按企业稳定 ID 顺序列出 Provider。
    pub async fn list_providers(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Vec<OrganizationSsoProviderRecord>, OrganizationSsoRepositoryError> {
        self.run(async move {
            let models = organization_sso_providers::Entity::find()
                .filter(
                    organization_sso_providers::Column::OrganizationId.eq(organization_id.get()),
                )
                .order_by_asc(organization_sso_providers::Column::Id)
                .all(self.pool.connection())
                .await
                .map_err(query_error)?;
            models.into_iter().map(provider_record).collect()
        })
        .await
    }

    /// 仅供受控服务重封装密钥使用；不把密钥 envelope 放入公开 Provider 记录。
    pub async fn provider_secret(
        &self,
        organization_id: OrganizationId,
        provider_key: OrganizationSsoProviderKey,
    ) -> Result<Option<EncryptedCredentialEnvelope>, OrganizationSsoRepositoryError> {
        self.run(async move {
            let model = organization_sso_providers::Entity::find()
                .filter(
                    organization_sso_providers::Column::OrganizationId.eq(organization_id.get()),
                )
                .filter(
                    organization_sso_providers::Column::ProviderKey
                        .eq(provider_key.persistence_key()),
                )
                .one(self.pool.connection())
                .await
                .map_err(query_error)?
                .ok_or(OrganizationSsoRepositoryError::NotFound)?;
            model
                .secret_envelope
                .map(|value| {
                    let (key_id, nonce, ciphertext) = value
                        .envelope_parts()
                        .map_err(|_| OrganizationSsoRepositoryError::InvalidInput)?;
                    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                        .map_err(|_| OrganizationSsoRepositoryError::InvalidInput)
                })
                .transpose()
        })
        .await
    }

    /// 按已领取事务绑定的 Provider ID 读取密钥封套，并再次限定企业边界。
    pub async fn provider_secret_by_id(
        &self,
        organization_id: OrganizationId,
        provider_id: i64,
    ) -> Result<Option<EncryptedCredentialEnvelope>, OrganizationSsoRepositoryError> {
        if provider_id <= 0 {
            return Err(OrganizationSsoRepositoryError::InvalidInput);
        }
        self.run(async move {
            let model = organization_sso_providers::Entity::find_by_id(provider_id)
                .filter(
                    organization_sso_providers::Column::OrganizationId.eq(organization_id.get()),
                )
                .one(self.pool.connection())
                .await
                .map_err(query_error)?
                .ok_or(OrganizationSsoRepositoryError::NotFound)?;
            model
                .secret_envelope
                .map(|value| {
                    let (key_id, nonce, ciphertext) = value
                        .envelope_parts()
                        .map_err(|_| OrganizationSsoRepositoryError::InvalidInput)?;
                    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                        .map_err(|_| OrganizationSsoRepositoryError::InvalidInput)
                })
                .transpose()
        })
        .await
    }

    /// 按 Provider 版本 CAS 替换配置；密钥动作显式区分保留、替换和清除。
    pub async fn update_provider(
        &self,
        organization_id: OrganizationId,
        provider_key: OrganizationSsoProviderKey,
        write: OrganizationSsoProviderUpdate,
    ) -> Result<OrganizationSsoProviderRecord, OrganizationSsoRepositoryError> {
        self.run(self.update_provider_inner(organization_id, provider_key, write))
            .await
    }

    /// 在企业范围创建待验证域名。
    pub async fn create_domain(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoDomainCreate,
    ) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
        self.run(self.create_domain_inner(organization_id, write))
            .await
    }

    /// 按企业和域名 ID 读取域名事实。
    pub async fn domain(
        &self,
        organization_id: OrganizationId,
        domain_id: i64,
    ) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
        self.run(async move {
            let model = organization_sso_domains::Entity::find_by_id(domain_id)
                .filter(organization_sso_domains::Column::OrganizationId.eq(organization_id.get()))
                .one(self.pool.connection())
                .await
                .map_err(query_error)?
                .ok_or(OrganizationSsoRepositoryError::NotFound)?;
            domain_record(self.pool.connection(), model).await
        })
        .await
    }

    /// 按企业范围稳定列出 SSO 域名，不允许通过域名 ID 跨企业读取。
    pub async fn list_domains(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Vec<OrganizationSsoDomainRecord>, OrganizationSsoRepositoryError> {
        self.run(async move {
            let models = organization_sso_domains::Entity::find()
                .filter(organization_sso_domains::Column::OrganizationId.eq(organization_id.get()))
                .order_by_asc(organization_sso_domains::Column::Id)
                .all(self.pool.connection())
                .await
                .map_err(query_error)?;
            let mut records = Vec::with_capacity(models.len());
            for model in models {
                records.push(domain_record(self.pool.connection(), model).await?);
            }
            Ok(records)
        })
        .await
    }

    /// 按 CAS 推进域名 Pending/Verified/Revoked 状态。
    pub async fn update_domain(
        &self,
        organization_id: OrganizationId,
        domain_id: i64,
        write: OrganizationSsoDomainUpdate,
    ) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
        // Verified 必须来自受控 DNS 解析结果，禁止通用更新接口伪造认领事实。
        if write.status == OrganizationSsoDomainStatus::Verified {
            return Err(OrganizationSsoRepositoryError::InvalidTransition);
        }
        self.run(self.update_domain_inner(organization_id, domain_id, write))
            .await
    }

    /// 使用受控 DNS 证明结果以版本 CAS 推进域名认领，并让数据库原子裁决跨企业竞争。
    pub async fn verify_domain(
        &self,
        organization_id: OrganizationId,
        domain_id: i64,
        verification: OrganizationSsoDnsVerification,
        expected_version: i64,
    ) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
        validate_expected_version(expected_version)?;
        self.run(self.verify_domain_inner(
            organization_id,
            domain_id,
            verification,
            expected_version,
        ))
        .await
    }

    /// 创建唯一的活动身份绑定。
    pub async fn create_identity(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoIdentityCreate,
    ) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
        self.run(self.create_identity_inner(organization_id, write))
            .await
    }

    /// 仅为已登录成员写入经二次验证确认的显式 SSO 身份绑定。
    pub async fn bind_explicit_identity(
        &self,
        organization_id: OrganizationId,
        user_id: UserId,
        provider_id: i64,
        subject_digest: String,
    ) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
        if provider_id <= 0 {
            return Err(OrganizationSsoRepositoryError::InvalidInput);
        }
        validate_digest(&subject_digest)?;
        self.run(self.bind_explicit_identity_inner(
            organization_id,
            user_id,
            provider_id,
            subject_digest,
        ))
        .await
    }

    /// 按企业、Provider 和 subject 摘要读取身份绑定。
    pub async fn identity(
        &self,
        organization_id: OrganizationId,
        provider_key: OrganizationSsoProviderKey,
        subject_digest: String,
    ) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
        validate_digest(&subject_digest)?;
        self.run(async move {
            let provider =
                provider_by_key(self.pool.connection(), organization_id, provider_key).await?;
            let model = organization_sso_identities::Entity::find()
                .filter(
                    organization_sso_identities::Column::OrganizationId.eq(organization_id.get()),
                )
                .filter(organization_sso_identities::Column::ProviderId.eq(provider.id))
                .filter(organization_sso_identities::Column::SubjectDigest.eq(subject_digest))
                .one(self.pool.connection())
                .await
                .map_err(query_error)?
                .ok_or(OrganizationSsoRepositoryError::NotFound)?;
            identity_record(model)
        })
        .await
    }

    /// 按 CAS 禁用身份绑定，保留历史事实但不再允许登录命中。
    pub async fn disable_identity(
        &self,
        organization_id: OrganizationId,
        identity_id: i64,
        expected_version: i64,
    ) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
        validate_expected_version(expected_version)?;
        self.run(self.disable_identity_inner(organization_id, identity_id, expected_version))
            .await
    }

    /// 创建短期单次登录事务，并锁定 Provider 版本快照。
    pub async fn create_login_transaction(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoLoginTransactionCreate,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        self.run(self.create_login_transaction_inner(organization_id, write))
            .await
    }

    /// 按企业和 state 摘要读取登录事务。
    pub async fn login_transaction(
        &self,
        organization_id: OrganizationId,
        state_digest: String,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        validate_digest(&state_digest)?;
        self.run(async move {
            let model = organization_sso_login_transactions::Entity::find()
                .filter(
                    organization_sso_login_transactions::Column::OrganizationId
                        .eq(organization_id.get()),
                )
                .filter(organization_sso_login_transactions::Column::StateDigest.eq(state_digest))
                .one(self.pool.connection())
                .await
                .map_err(query_error)?
                .ok_or(OrganizationSsoRepositoryError::NotFound)?;
            login_transaction_record(model)
        })
        .await
    }

    /// 在未过期条件下原子领取 state；重复领取和过期分别失败关闭。
    pub async fn claim_login_transaction(
        &self,
        organization_id: OrganizationId,
        state_digest: String,
        now: i64,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        validate_digest(&state_digest)?;
        let now = timestamp(now)?;
        self.run(self.claim_login_transaction_inner(organization_id, state_digest, now))
            .await
    }

    /// 在尚不知道企业标识的固定回调入口中原子领取全局唯一 state。
    ///
    /// state 摘要由数据库唯一索引约束，领取后仍会锁定同企业 Provider，避免
    /// 回调层通过猜测企业标识绕过 Provider 范围校验。
    pub async fn claim_login_transaction_by_state(
        &self,
        state_digest: String,
        now: i64,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        validate_digest(&state_digest)?;
        let now = timestamp(now)?;
        self.run(self.claim_login_transaction_by_state_inner(state_digest, now))
            .await
    }

    /// 将已领取事务一次性推进到结果并写入短期票据/断言摘要。
    pub async fn complete_login_transaction(
        &self,
        organization_id: OrganizationId,
        transaction_id: i64,
        result: OrganizationSsoLoginTransactionResult,
        now: i64,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        let now = timestamp(now)?;
        validate_result(&result, now)?;
        self.run(self.complete_login_transaction_inner(
            organization_id,
            transaction_id,
            result,
            now,
        ))
        .await
    }

    /// 封存已领取但未完成的事务，不产生票据或断言；即使刚好过期也必须允许失败补偿。
    pub async fn reject_login_transaction(
        &self,
        organization_id: OrganizationId,
        transaction_id: i64,
        result_code: i16,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        if result_code <= 0 {
            return Err(OrganizationSsoRepositoryError::InvalidInput);
        }
        self.run(self.reject_login_transaction_inner(organization_id, transaction_id, result_code))
            .await
    }

    /// 原子交换一次性票据；票据过期或重复交换均拒绝。
    pub async fn exchange_ticket(
        &self,
        organization_id: OrganizationId,
        ticket_digest: String,
        now: i64,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        validate_digest(&ticket_digest)?;
        let now = timestamp(now)?;
        self.run(self.exchange_ticket_inner(organization_id, ticket_digest, now))
            .await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationSsoRepositoryError>>,
    ) -> Result<T, OrganizationSsoRepositoryError> {
        match timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                OrganizationSsoRepositoryError::Timeout,
            )),
        }
    }

    async fn create_provider_inner(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoProviderCreate,
    ) -> Result<OrganizationSsoProviderRecord, OrganizationSsoRepositoryError> {
        validate_display_name(&write.display_name)?;
        let secret = write.secret_envelope.map(parse_secret).transpose()?;
        let transaction = begin(&self.pool).await?;
        organization_exists(&transaction, organization_id).await?;
        if organization_sso_providers::Entity::find()
            .filter(organization_sso_providers::Column::OrganizationId.eq(organization_id.get()))
            .filter(
                organization_sso_providers::Column::ProviderKey
                    .eq(write.provider_key.persistence_key()),
            )
            .count(&transaction)
            .await
            .map_err(query_error)?
            > 0
        {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_sso_providers::ActiveModel {
            organization_id: Set(organization_id.get()),
            provider_key: Set(write.provider_key.persistence_key()),
            protocol: Set(write.protocol.code()),
            status: Set(write.status.code()),
            display_name: Set(write.display_name),
            metadata_projection: Set(write.metadata_projection),
            secret_envelope: Set(secret),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(map_write_error)?;
        let record = provider_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn update_provider_inner(
        &self,
        organization_id: OrganizationId,
        provider_key: OrganizationSsoProviderKey,
        write: OrganizationSsoProviderUpdate,
    ) -> Result<OrganizationSsoProviderRecord, OrganizationSsoRepositoryError> {
        validate_display_name(&write.display_name)?;
        validate_expected_version(write.expected_version)?;
        let transaction = begin(&self.pool).await?;
        let current = lock_provider(&transaction, organization_id, provider_key).await?;
        if current.version != write.expected_version {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let secret = match write.secret_change {
            OrganizationSsoProviderSecretChange::Keep => current.secret_envelope.clone(),
            OrganizationSsoProviderSecretChange::Replace(value) => Some(parse_secret(value)?),
            OrganizationSsoProviderSecretChange::Clear => None,
        };
        let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
        let result = organization_sso_providers::Entity::update_many()
            .filter(organization_sso_providers::Column::Id.eq(current.id))
            .filter(organization_sso_providers::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_sso_providers::Column::Version.eq(current.version))
            .col_expr(
                organization_sso_providers::Column::Protocol,
                Expr::value(write.protocol.code()),
            )
            .col_expr(
                organization_sso_providers::Column::Status,
                Expr::value(write.status.code()),
            )
            .col_expr(
                organization_sso_providers::Column::DisplayName,
                Expr::value(write.display_name),
            )
            .col_expr(
                organization_sso_providers::Column::MetadataProjection,
                Expr::value(write.metadata_projection),
            )
            .col_expr(
                organization_sso_providers::Column::SecretEnvelope,
                Expr::value(secret),
            )
            .col_expr(
                organization_sso_providers::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_sso_providers::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .await
            .map_err(map_write_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let model = organization_sso_providers::Entity::find_by_id(current.id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = provider_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn create_domain_inner(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoDomainCreate,
    ) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
        validate_digest(&write.proof_digest)?;
        let proof_expires_at = timestamp(write.proof_expires_at)?;
        let transaction = begin(&self.pool).await?;
        let provider = lock_provider(&transaction, organization_id, write.provider_key).await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_sso_domains::ActiveModel {
            organization_id: Set(organization_id.get()),
            provider_id: Set(provider.id),
            domain: Set(write.domain.as_str().to_owned()),
            status: Set(OrganizationSsoDomainStatus::Pending.code()),
            proof_digest: Set(write.proof_digest.into()),
            proof_expires_at: Set(Some(proof_expires_at)),
            verified_at: Set(None),
            recheck_at: Set(None),
            revoked_at: Set(None),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(map_write_error)?;
        let record = domain_record(&transaction, model).await?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn update_domain_inner(
        &self,
        organization_id: OrganizationId,
        domain_id: i64,
        write: OrganizationSsoDomainUpdate,
    ) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
        validate_expected_version(write.expected_version)?;
        validate_domain_update(&write)?;
        let transaction = begin(&self.pool).await?;
        let current = lock_domain(&transaction, organization_id, domain_id).await?;
        if current.version != write.expected_version {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let current_status =
            OrganizationSsoDomainStatus::try_from(current.status).map_err(|_| invariant())?;
        if !domain_transition_allowed(current_status, write.status) {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::InvalidTransition);
        }
        let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
        let result = organization_sso_domains::Entity::update_many()
            .filter(organization_sso_domains::Column::Id.eq(domain_id))
            .filter(organization_sso_domains::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_sso_domains::Column::Version.eq(current.version))
            .col_expr(
                organization_sso_domains::Column::Status,
                Expr::value(write.status.code()),
            )
            .col_expr(
                organization_sso_domains::Column::ProofExpiresAt,
                Expr::value(write.proof_expires_at.map(timestamp).transpose()?),
            )
            .col_expr(
                organization_sso_domains::Column::VerifiedAt,
                Expr::value(write.verified_at.map(timestamp).transpose()?),
            )
            .col_expr(
                organization_sso_domains::Column::RecheckAt,
                Expr::value(write.recheck_at.map(timestamp).transpose()?),
            )
            .col_expr(
                organization_sso_domains::Column::RevokedAt,
                Expr::value(write.revoked_at.map(timestamp).transpose()?),
            )
            .col_expr(
                organization_sso_domains::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_sso_domains::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let model = organization_sso_domains::Entity::find_by_id(domain_id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = domain_record(&transaction, model).await?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn verify_domain_inner(
        &self,
        organization_id: OrganizationId,
        domain_id: i64,
        verification: OrganizationSsoDnsVerification,
        expected_version: i64,
    ) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
        let verified_at = timestamp(verification.verified_at())?;
        let recheck_at = timestamp(verification.recheck_at())?;
        validate_digest(verification.proof_digest())?;
        if verification.recheck_at() <= verification.verified_at() {
            return Err(OrganizationSsoRepositoryError::InvalidInput);
        }
        let transaction = begin(&self.pool).await?;
        let current = lock_domain(&transaction, organization_id, domain_id).await?;
        if current.version != expected_version {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let current_status =
            OrganizationSsoDomainStatus::try_from(current.status).map_err(|_| invariant())?;
        if !matches!(
            current_status,
            OrganizationSsoDomainStatus::Pending | OrganizationSsoDomainStatus::Verified
        ) {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::InvalidTransition);
        }
        if current.proof_digest.as_str() != verification.proof_digest() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        if current
            .proof_expires_at
            .is_some_and(|expires_at| verified_at >= expires_at)
        {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Expired);
        }
        let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
        let result = organization_sso_domains::Entity::update_many()
            .filter(organization_sso_domains::Column::Id.eq(domain_id))
            .filter(organization_sso_domains::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_sso_domains::Column::Version.eq(current.version))
            .col_expr(
                organization_sso_domains::Column::Status,
                Expr::value(OrganizationSsoDomainStatus::Verified.code()),
            )
            .col_expr(
                organization_sso_domains::Column::VerifiedAt,
                Expr::value(Some(verified_at)),
            )
            .col_expr(
                organization_sso_domains::Column::RecheckAt,
                Expr::value(Some(recheck_at)),
            )
            .col_expr(
                organization_sso_domains::Column::RevokedAt,
                Expr::value(None::<TimeDateTimeWithTimeZone>),
            )
            .col_expr(
                organization_sso_domains::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_sso_domains::Column::UpdatedAt,
                Expr::value(TimeDateTimeWithTimeZone::now_utc()),
            )
            .exec(&transaction)
            .await
            .map_err(map_write_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let model = organization_sso_domains::Entity::find_by_id(domain_id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = domain_record(&transaction, model).await?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn create_identity_inner(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoIdentityCreate,
    ) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
        validate_digest(&write.subject_digest)?;
        let transaction = begin(&self.pool).await?;
        let provider =
            lock_active_provider(&transaction, organization_id, write.provider_key).await?;
        let membership = organization_memberships::Entity::find_by_id(write.membership_id.get())
            .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationSsoRepositoryError::NotFound)?;
        if membership.status != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Unavailable);
        }
        if organization_sso_identities::Entity::find()
            .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_sso_identities::Column::ProviderId.eq(provider.id))
            .filter(
                organization_sso_identities::Column::SubjectDigest.eq(write.subject_digest.clone()),
            )
            .count(&transaction)
            .await
            .map_err(query_error)?
            > 0
            || organization_sso_identities::Entity::find()
                .filter(
                    organization_sso_identities::Column::OrganizationId.eq(organization_id.get()),
                )
                .filter(organization_sso_identities::Column::ProviderId.eq(provider.id))
                .filter(
                    organization_sso_identities::Column::OrganizationMembershipId
                        .eq(write.membership_id.get()),
                )
                .count(&transaction)
                .await
                .map_err(query_error)?
                > 0
        {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_sso_identities::ActiveModel {
            organization_id: Set(organization_id.get()),
            provider_id: Set(provider.id),
            subject_digest: Set(write.subject_digest.into()),
            organization_membership_id: Set(write.membership_id.get()),
            status: Set(OrganizationSsoIdentityStatus::Active.code()),
            binding_method: Set(write.binding_method.code()),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            disabled_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(map_write_error)?;
        let record = identity_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn bind_explicit_identity_inner(
        &self,
        organization_id: OrganizationId,
        user_id: UserId,
        provider_id: i64,
        subject_digest: String,
    ) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let organization = lock_organization(&transaction, organization_id).await?;
        let entitlement = lock_entitlement(&transaction, organization_id).await?;
        let provider =
            lock_active_provider_by_id(&transaction, organization_id, provider_id).await?;
        if OrganizationSsoProtocol::try_from(provider.protocol).map_err(|_| invariant())?
            != OrganizationSsoProtocol::Oidc
        {
            return Err(OrganizationSsoRepositoryError::Unavailable);
        }
        let domain = lock_verified_domain(&transaction, organization_id, provider.id).await?;
        let membership =
            lock_active_membership_by_user(&transaction, organization_id, user_id).await?;

        let organization_status =
            OrganizationStatus::try_from(organization.status).map_err(|_| invariant())?;
        let entitlement_snapshot = entitlement_snapshot(&entitlement)?;
        let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
        let entitlement_status = entitlement_snapshot.status(organization_status, now);
        let provider_status =
            OrganizationSsoProviderStatus::try_from(provider.status).map_err(|_| invariant())?;
        let domain_status =
            OrganizationSsoDomainStatus::try_from(domain.status).map_err(|_| invariant())?;
        let membership_status =
            OrganizationMembershipStatus::try_from(membership.status).map_err(|_| invariant())?;
        authorize_organization_sso_identity_login(OrganizationSsoIdentityLoginFacts::new(
            organization_status,
            entitlement_status,
            entitlement_snapshot.sso_enabled(),
            provider_status,
            domain_status,
            OrganizationSsoIdentityStatus::Active,
            membership_status,
        ))
        .map_err(|_| OrganizationSsoRepositoryError::Unavailable)?;

        if identity_exists(&transaction, organization_id, provider.id, &subject_digest).await?
            || identity_exists_for_membership(
                &transaction,
                organization_id,
                provider.id,
                membership.id,
            )
            .await?
        {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }

        let timestamp = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_sso_identities::ActiveModel {
            organization_id: Set(organization_id.get()),
            provider_id: Set(provider.id),
            subject_digest: Set(subject_digest.into()),
            organization_membership_id: Set(membership.id),
            status: Set(OrganizationSsoIdentityStatus::Active.code()),
            binding_method: Set(OrganizationSsoIdentityBindingMethod::Explicit.code()),
            version: Set(1),
            created_at: Set(timestamp),
            updated_at: Set(timestamp),
            disabled_at: Set(None),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(map_write_error)?;
        let record = identity_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn disable_identity_inner(
        &self,
        organization_id: OrganizationId,
        identity_id: i64,
        expected_version: i64,
    ) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let current = lock_identity(&transaction, organization_id, identity_id).await?;
        if current.version != expected_version {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let current_status =
            OrganizationSsoIdentityStatus::try_from(current.status).map_err(|_| invariant())?;
        if !current_status.is_active() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::InvalidTransition);
        }
        let next_version = current.version.checked_add(1).ok_or_else(invariant)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = organization_sso_identities::Entity::update_many()
            .filter(organization_sso_identities::Column::Id.eq(identity_id))
            .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_sso_identities::Column::Version.eq(current.version))
            .filter(
                organization_sso_identities::Column::Status
                    .eq(OrganizationSsoIdentityStatus::Active.code()),
            )
            .col_expr(
                organization_sso_identities::Column::Status,
                Expr::value(OrganizationSsoIdentityStatus::Disabled.code()),
            )
            .col_expr(
                organization_sso_identities::Column::DisabledAt,
                Expr::value(Some(now)),
            )
            .col_expr(
                organization_sso_identities::Column::Version,
                Expr::value(next_version),
            )
            .col_expr(
                organization_sso_identities::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let model = organization_sso_identities::Entity::find_by_id(identity_id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = identity_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn create_login_transaction_inner(
        &self,
        organization_id: OrganizationId,
        write: OrganizationSsoLoginTransactionCreate,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        validate_expected_version(write.provider_version)?;
        validate_digest(&write.state_digest)?;
        validate_digest(&write.request_digest)?;
        let login_material_envelope = write
            .login_material_envelope
            .map(parse_secret)
            .transpose()?;
        let expires_at = timestamp(write.expires_at)?;
        let transaction = begin(&self.pool).await?;
        let provider =
            lock_active_provider(&transaction, organization_id, write.provider_key).await?;
        if provider.protocol != write.protocol.code() || provider.version != write.provider_version
        {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        if expires_at <= TimeDateTimeWithTimeZone::now_utc() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::InvalidInput);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_sso_login_transactions::ActiveModel {
            organization_id: Set(organization_id.get()),
            provider_id: Set(provider.id),
            protocol: Set(write.protocol.code()),
            provider_version: Set(write.provider_version),
            state_digest: Set(write.state_digest.into()),
            request_digest: Set(write.request_digest.into()),
            login_material_envelope: Set(login_material_envelope),
            expires_at: Set(expires_at),
            claimed_at: Set(None),
            result_code: Set(None),
            ticket_digest: Set(None),
            ticket_expires_at: Set(None),
            exchanged_at: Set(None),
            assertion_digest: Set(None),
            assertion_expires_at: Set(None),
            invitation_email_fingerprint: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&transaction)
        .await
        .map_err(map_write_error)?;
        let record = login_transaction_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn claim_login_transaction_inner(
        &self,
        organization_id: OrganizationId,
        state_digest: String,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let current = lock_login_by_state(&transaction, organization_id, &state_digest).await?;
        if current.claimed_at.is_some() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        if current.expires_at <= now {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Expired);
        }
        lock_active_provider_by_id(&transaction, organization_id, current.provider_id).await?;
        let result = organization_sso_login_transactions::Entity::update_many()
            .filter(organization_sso_login_transactions::Column::Id.eq(current.id))
            .filter(
                organization_sso_login_transactions::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(organization_sso_login_transactions::Column::ClaimedAt.is_null())
            .filter(organization_sso_login_transactions::Column::ExpiresAt.gt(now))
            .col_expr(
                organization_sso_login_transactions::Column::ClaimedAt,
                Expr::value(now),
            )
            .col_expr(
                organization_sso_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        let model = organization_sso_login_transactions::Entity::find_by_id(current.id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = login_transaction_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn claim_login_transaction_by_state_inner(
        &self,
        state_digest: String,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let mut query = organization_sso_login_transactions::Entity::find()
            .filter(organization_sso_login_transactions::Column::StateDigest.eq(&state_digest));
        if transaction.get_database_backend() != DbBackend::Sqlite {
            query = query.lock(LockType::Update);
        }
        let current = query
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationSsoRepositoryError::NotFound)?;
        let organization_id =
            OrganizationId::new(current.organization_id).map_err(|_| invariant())?;
        if current.claimed_at.is_some() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        if current.expires_at <= now {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Expired);
        }
        lock_active_provider_by_id(&transaction, organization_id, current.provider_id).await?;
        let result = organization_sso_login_transactions::Entity::update_many()
            .filter(organization_sso_login_transactions::Column::Id.eq(current.id))
            .filter(
                organization_sso_login_transactions::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(organization_sso_login_transactions::Column::StateDigest.eq(&state_digest))
            .filter(organization_sso_login_transactions::Column::ClaimedAt.is_null())
            .filter(organization_sso_login_transactions::Column::ExpiresAt.gt(now))
            .col_expr(
                organization_sso_login_transactions::Column::ClaimedAt,
                Expr::value(now),
            )
            .col_expr(
                organization_sso_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        let model = organization_sso_login_transactions::Entity::find_by_id(current.id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = login_transaction_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn complete_login_transaction_inner(
        &self,
        organization_id: OrganizationId,
        transaction_id: i64,
        result: OrganizationSsoLoginTransactionResult,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let current = lock_login(&transaction, organization_id, transaction_id).await?;
        if current.claimed_at.is_none() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::InvalidTransition);
        }
        if current.result_code.is_some() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        if current.expires_at <= now {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Expired);
        }
        // 邮箱指纹只来自已验证的 OIDC ID Token，SAML 事务不得写入该投影。
        if result.invitation_email_fingerprint.is_some()
            && current.protocol != OrganizationSsoProtocol::Oidc.code()
        {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::InvalidInput);
        }
        let next = organization_sso_login_transactions::Entity::update_many()
            .filter(organization_sso_login_transactions::Column::Id.eq(transaction_id))
            .filter(
                organization_sso_login_transactions::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(organization_sso_login_transactions::Column::ResultCode.is_null())
            .col_expr(
                organization_sso_login_transactions::Column::ResultCode,
                Expr::value(result.result_code),
            )
            .col_expr(
                organization_sso_login_transactions::Column::TicketDigest,
                Expr::value(result.ticket_digest.clone().map(SensitiveString::from)),
            )
            .col_expr(
                organization_sso_login_transactions::Column::TicketExpiresAt,
                Expr::value(result.ticket_expires_at.map(timestamp).transpose()?),
            )
            .col_expr(
                organization_sso_login_transactions::Column::AssertionDigest,
                Expr::value(result.assertion_digest.clone().map(SensitiveString::from)),
            )
            .col_expr(
                organization_sso_login_transactions::Column::AssertionExpiresAt,
                Expr::value(result.assertion_expires_at.map(timestamp).transpose()?),
            )
            .col_expr(
                organization_sso_login_transactions::Column::InvitationEmailFingerprint,
                Expr::value(
                    result
                        .invitation_email_fingerprint
                        .map(|value| SensitiveString::from(encode_digest(&value))),
                ),
            )
            .col_expr(
                organization_sso_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if next.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let model = organization_sso_login_transactions::Entity::find_by_id(transaction_id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = login_transaction_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn reject_login_transaction_inner(
        &self,
        organization_id: OrganizationId,
        transaction_id: i64,
        result_code: i16,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let current = lock_login(&transaction, organization_id, transaction_id).await?;
        if current.claimed_at.is_none() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::InvalidTransition);
        }
        if current.result_code.is_some() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        // 解密失败发生在领取之后时仍须封存，不能因刚好越过短期过期线留下半完成事务。
        let now = TimeDateTimeWithTimeZone::now_utc();
        let next = organization_sso_login_transactions::Entity::update_many()
            .filter(organization_sso_login_transactions::Column::Id.eq(transaction_id))
            .filter(
                organization_sso_login_transactions::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(organization_sso_login_transactions::Column::ResultCode.is_null())
            .col_expr(
                organization_sso_login_transactions::Column::ResultCode,
                Expr::value(result_code),
            )
            .col_expr(
                organization_sso_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if next.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Conflict);
        }
        let model = organization_sso_login_transactions::Entity::find_by_id(transaction_id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = login_transaction_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }

    async fn exchange_ticket_inner(
        &self,
        organization_id: OrganizationId,
        ticket_digest: String,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
        let transaction = begin(&self.pool).await?;
        let current = organization_sso_login_transactions::Entity::find()
            .filter(
                organization_sso_login_transactions::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(
                organization_sso_login_transactions::Column::TicketDigest.eq(ticket_digest.clone()),
            )
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationSsoRepositoryError::NotFound)?;
        if current.exchanged_at.is_some() {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        if current
            .ticket_expires_at
            .is_none_or(|expires| expires <= now)
        {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::Expired);
        }
        let result = organization_sso_login_transactions::Entity::update_many()
            .filter(organization_sso_login_transactions::Column::Id.eq(current.id))
            .filter(
                organization_sso_login_transactions::Column::OrganizationId
                    .eq(organization_id.get()),
            )
            .filter(organization_sso_login_transactions::Column::TicketDigest.eq(ticket_digest))
            .filter(organization_sso_login_transactions::Column::ExchangedAt.is_null())
            .filter(organization_sso_login_transactions::Column::TicketExpiresAt.gt(now))
            .col_expr(
                organization_sso_login_transactions::Column::ExchangedAt,
                Expr::value(now),
            )
            .col_expr(
                organization_sso_login_transactions::Column::UpdatedAt,
                Expr::value(now),
            )
            .exec(&transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            rollback(transaction).await?;
            return Err(OrganizationSsoRepositoryError::AlreadyProcessed);
        }
        let model = organization_sso_login_transactions::Entity::find_by_id(current.id)
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or_else(invariant)?;
        let record = login_transaction_record(model)?;
        commit(transaction).await?;
        Ok(record)
    }
}

impl fmt::Debug for OrganizationSsoProviderRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoProviderRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn organization_exists(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationSsoRepositoryError> {
    if organizations::Entity::find_by_id(organization_id.get())
        .count(transaction)
        .await
        .map_err(query_error)?
        != 1
    {
        return Err(OrganizationSsoRepositoryError::NotFound);
    }
    Ok(())
}

/// 锁定企业主体，避免授权和成员校验与企业状态变更交错。
async fn lock_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<organizations::Model, OrganizationSsoRepositoryError> {
    let mut query = organizations::Entity::find_by_id(organization_id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::NotFound)
}

/// 锁定企业商业授权，缺失或损坏的授权均不允许身份绑定。
async fn lock_entitlement(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<organization_entitlements::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let models = query.all(transaction).await.map_err(query_error)?;
    match models.as_slice() {
        [model] => Ok(model.clone()),
        [] => Err(OrganizationSsoRepositoryError::Unavailable),
        _ => Err(invariant()),
    }
}

/// 将持久化授权完整投影为领域快照，禁止用裸字段绕过时间和能力约束。
fn entitlement_snapshot(
    model: &organization_entitlements::Model,
) -> Result<OrganizationEntitlementSnapshot, OrganizationSsoRepositoryError> {
    let capacity = OrganizationEntitlementCapacity::new(
        model.seat_limit,
        model.team_limit,
        model.organization_key_limit,
    )
    .map_err(|_| invariant())?;
    OrganizationEntitlementSnapshot::new(
        capacity,
        model.team_management_enabled,
        model.sso_enabled,
        model.custom_roles_enabled,
        model.audit_retention_days,
        model.valid_from.unix_timestamp(),
        model.valid_until.unix_timestamp(),
        model.grace_until.map(|value| value.unix_timestamp()),
    )
    .map_err(|_| invariant())
}

/// 锁定尚未超过复验期限的已验证域名，避免撤销或过期域名继续放行绑定。
async fn lock_verified_domain(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_id: i64,
) -> Result<organization_sso_domains::Model, OrganizationSsoRepositoryError> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let mut query = organization_sso_domains::Entity::find()
        .filter(organization_sso_domains::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_domains::Column::ProviderId.eq(provider_id))
        .filter(
            organization_sso_domains::Column::Status
                .eq(OrganizationSsoDomainStatus::Verified.code()),
        )
        .filter(organization_sso_domains::Column::RecheckAt.gt(now))
        .order_by_asc(organization_sso_domains::Column::Id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::Unavailable)
}

/// 锁定当前用户在企业内的成员事实，并拒绝已暂停或已移除关系。
async fn lock_active_membership_by_user(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<organization_memberships::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let models = query.all(transaction).await.map_err(query_error)?;
    let membership = match models.as_slice() {
        [membership] => membership.clone(),
        [] => return Err(OrganizationSsoRepositoryError::NotFound),
        _ => return Err(invariant()),
    };
    if OrganizationMembershipStatus::try_from(membership.status).map_err(|_| invariant())?
        != OrganizationMembershipStatus::Active
        || membership.suspended_at.is_some()
        || membership.removed_at.is_some()
    {
        return Err(OrganizationSsoRepositoryError::Unavailable);
    }
    Ok(membership)
}

async fn identity_exists(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_id: i64,
    subject_digest: &str,
) -> Result<bool, OrganizationSsoRepositoryError> {
    organization_sso_identities::Entity::find()
        .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_identities::Column::ProviderId.eq(provider_id))
        .filter(organization_sso_identities::Column::SubjectDigest.eq(subject_digest))
        .one(transaction)
        .await
        .map_err(query_error)
        .map(|identity| identity.is_some())
}

async fn identity_exists_for_membership(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_id: i64,
    membership_id: i64,
) -> Result<bool, OrganizationSsoRepositoryError> {
    organization_sso_identities::Entity::find()
        .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_identities::Column::ProviderId.eq(provider_id))
        .filter(organization_sso_identities::Column::OrganizationMembershipId.eq(membership_id))
        .one(transaction)
        .await
        .map_err(query_error)
        .map(|identity| identity.is_some())
}

async fn provider_by_key<C: ConnectionTrait>(
    connection: &C,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
) -> Result<organization_sso_providers::Model, OrganizationSsoRepositoryError> {
    organization_sso_providers::Entity::find()
        .filter(organization_sso_providers::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_providers::Column::ProviderKey.eq(provider_key.persistence_key()))
        .one(connection)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::NotFound)
}

async fn lock_provider(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
) -> Result<organization_sso_providers::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_sso_providers::Entity::find()
        .filter(organization_sso_providers::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_providers::Column::ProviderKey.eq(provider_key.persistence_key()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::NotFound)
}

async fn lock_active_provider(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
) -> Result<organization_sso_providers::Model, OrganizationSsoRepositoryError> {
    let model = lock_provider(transaction, organization_id, provider_key).await?;
    if OrganizationSsoProviderStatus::try_from(model.status).map_err(|_| invariant())?
        != OrganizationSsoProviderStatus::Active
    {
        return Err(OrganizationSsoRepositoryError::Unavailable);
    }
    Ok(model)
}

async fn lock_active_provider_by_id(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    provider_id: i64,
) -> Result<organization_sso_providers::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_sso_providers::Entity::find_by_id(provider_id)
        .filter(organization_sso_providers::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    let model = query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::Invariant)?;
    if OrganizationSsoProviderStatus::try_from(model.status).map_err(|_| invariant())?
        != OrganizationSsoProviderStatus::Active
    {
        return Err(OrganizationSsoRepositoryError::Unavailable);
    }
    Ok(model)
}

async fn lock_domain(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    domain_id: i64,
) -> Result<organization_sso_domains::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_sso_domains::Entity::find_by_id(domain_id)
        .filter(organization_sso_domains::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::NotFound)
}

async fn lock_identity(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    identity_id: i64,
) -> Result<organization_sso_identities::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_sso_identities::Entity::find_by_id(identity_id)
        .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::NotFound)
}

async fn lock_login(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    transaction_id: i64,
) -> Result<organization_sso_login_transactions::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_sso_login_transactions::Entity::find_by_id(transaction_id).filter(
        organization_sso_login_transactions::Column::OrganizationId.eq(organization_id.get()),
    );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::NotFound)
}

async fn lock_login_by_state(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    state_digest: &str,
) -> Result<organization_sso_login_transactions::Model, OrganizationSsoRepositoryError> {
    let mut query = organization_sso_login_transactions::Entity::find()
        .filter(
            organization_sso_login_transactions::Column::OrganizationId.eq(organization_id.get()),
        )
        .filter(organization_sso_login_transactions::Column::StateDigest.eq(state_digest));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationSsoRepositoryError::NotFound)
}

async fn domain_record<C: ConnectionTrait>(
    connection: &C,
    model: organization_sso_domains::Model,
) -> Result<OrganizationSsoDomainRecord, OrganizationSsoRepositoryError> {
    let provider = organization_sso_providers::Entity::find_by_id(model.provider_id)
        .filter(organization_sso_providers::Column::OrganizationId.eq(model.organization_id))
        .one(connection)
        .await
        .map_err(query_error)?
        .ok_or_else(invariant)?;
    let status = OrganizationSsoDomainStatus::try_from(model.status).map_err(|_| invariant())?;
    let domain = OrganizationSsoDomain::new(model.domain).map_err(|_| invariant())?;
    if model.version <= 0 {
        return Err(invariant());
    }
    Ok(OrganizationSsoDomainRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        provider_id: model.provider_id,
        provider_key: OrganizationSsoProviderKey::from_persistence_key(&provider.provider_key)
            .map_err(|_| invariant())?,
        domain,
        status,
        proof_expires_at: model.proof_expires_at.map(|value| value.unix_timestamp()),
        verified_at: model.verified_at.map(|value| value.unix_timestamp()),
        recheck_at: model.recheck_at.map(|value| value.unix_timestamp()),
        revoked_at: model.revoked_at.map(|value| value.unix_timestamp()),
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
    })
}

fn provider_record(
    model: organization_sso_providers::Model,
) -> Result<OrganizationSsoProviderRecord, OrganizationSsoRepositoryError> {
    if model.id <= 0
        || model.version <= 0
        || model.display_name.trim() != model.display_name
        || model.display_name.is_empty()
    {
        return Err(invariant());
    }
    Ok(OrganizationSsoProviderRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        provider_key: OrganizationSsoProviderKey::from_persistence_key(&model.provider_key)
            .map_err(|_| invariant())?,
        protocol: OrganizationSsoProtocol::try_from(model.protocol).map_err(|_| invariant())?,
        status: OrganizationSsoProviderStatus::try_from(model.status).map_err(|_| invariant())?,
        display_name: model.display_name,
        metadata_projection: model.metadata_projection,
        secret_configured: model.secret_envelope.is_some(),
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
    })
}

fn identity_record(
    model: organization_sso_identities::Model,
) -> Result<OrganizationSsoIdentityRecord, OrganizationSsoRepositoryError> {
    validate_digest(model.subject_digest.as_str()).map_err(|_| invariant())?;
    let status = OrganizationSsoIdentityStatus::try_from(model.status).map_err(|_| invariant())?;
    let binding_method = OrganizationSsoIdentityBindingMethod::try_from(model.binding_method)
        .map_err(|_| invariant())?;
    if model.id <= 0
        || model.version <= 0
        || !matches!(
            (status, model.disabled_at.is_some()),
            (OrganizationSsoIdentityStatus::Active, false)
                | (OrganizationSsoIdentityStatus::Disabled, true)
        )
    {
        return Err(invariant());
    }
    Ok(OrganizationSsoIdentityRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        provider_id: model.provider_id,
        subject_digest: model.subject_digest.as_str().to_owned(),
        membership_id: OrganizationMembershipId::new(model.organization_membership_id)
            .map_err(|_| invariant())?,
        status,
        binding_method,
        version: model.version,
        disabled_at: model.disabled_at.map(|value| value.unix_timestamp()),
    })
}

fn login_transaction_record(
    model: organization_sso_login_transactions::Model,
) -> Result<OrganizationSsoLoginTransactionRecord, OrganizationSsoRepositoryError> {
    validate_digest(model.state_digest.as_str()).map_err(|_| invariant())?;
    validate_digest(model.request_digest.as_str()).map_err(|_| invariant())?;
    if let Some(value) = model.ticket_digest.as_ref() {
        validate_digest(value.as_str()).map_err(|_| invariant())?;
    }
    if let Some(value) = model.assertion_digest.as_ref() {
        validate_digest(value.as_str()).map_err(|_| invariant())?;
    }
    let invitation_email_fingerprint = model
        .invitation_email_fingerprint
        .as_ref()
        .map(|value| decode_digest(value.as_str()))
        .transpose()
        .map_err(|_| invariant())?;
    if model.id <= 0 || model.provider_version <= 0 {
        return Err(invariant());
    }
    Ok(OrganizationSsoLoginTransactionRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        provider_id: model.provider_id,
        protocol: OrganizationSsoProtocol::try_from(model.protocol).map_err(|_| invariant())?,
        provider_version: model.provider_version,
        state_digest: model.state_digest.as_str().to_owned(),
        request_digest: model.request_digest.as_str().to_owned(),
        login_material_envelope: model
            .login_material_envelope
            .map(encrypted_envelope)
            .transpose()?,
        expires_at: model.expires_at.unix_timestamp(),
        claimed_at: model.claimed_at.map(|value| value.unix_timestamp()),
        result_code: model.result_code,
        ticket_digest: model.ticket_digest.map(|value| value.as_str().to_owned()),
        ticket_expires_at: model.ticket_expires_at.map(|value| value.unix_timestamp()),
        exchanged_at: model.exchanged_at.map(|value| value.unix_timestamp()),
        assertion_digest: model
            .assertion_digest
            .map(|value| value.as_str().to_owned()),
        assertion_expires_at: model
            .assertion_expires_at
            .map(|value| value.unix_timestamp()),
        invitation_email_fingerprint,
    })
}

fn validate_display_name(value: &str) -> Result<(), OrganizationSsoRepositoryError> {
    if value.is_empty()
        || value.len() > 128
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(OrganizationSsoRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}
fn validate_digest(value: &str) -> Result<(), OrganizationSsoRepositoryError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Err(OrganizationSsoRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}
fn decode_digest(value: &str) -> Result<[u8; 32], OrganizationSsoRepositoryError> {
    validate_digest(value)?;
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        let high = value.as_bytes()[index * 2];
        let low = value.as_bytes()[index * 2 + 1];
        *byte = (hex_nibble(high)? << 4) | hex_nibble(low)?;
    }
    Ok(output)
}

/// 将固定长度摘要编码为规范小写十六进制，供敏感持久化字段使用。
fn encode_digest(value: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

fn hex_nibble(value: u8) -> Result<u8, OrganizationSsoRepositoryError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(OrganizationSsoRepositoryError::InvalidInput),
    }
}
fn validate_expected_version(value: i64) -> Result<(), OrganizationSsoRepositoryError> {
    if value <= 0 || value == i64::MAX {
        Err(OrganizationSsoRepositoryError::InvalidInput)
    } else {
        Ok(())
    }
}
fn timestamp(value: i64) -> Result<TimeDateTimeWithTimeZone, OrganizationSsoRepositoryError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| OrganizationSsoRepositoryError::InvalidInput)
}
fn parse_secret(value: Json) -> Result<EncryptedJson, OrganizationSsoRepositoryError> {
    EncryptedJson::from_envelope(value).map_err(|_| OrganizationSsoRepositoryError::InvalidInput)
}

fn encrypted_envelope(
    value: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, OrganizationSsoRepositoryError> {
    let (key_id, nonce, ciphertext) = value.envelope_parts().map_err(|_| invariant())?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext).map_err(|_| invariant())
}

fn validate_domain_update(
    write: &OrganizationSsoDomainUpdate,
) -> Result<(), OrganizationSsoRepositoryError> {
    for value in [
        write.proof_expires_at,
        write.verified_at,
        write.recheck_at,
        write.revoked_at,
    ]
    .into_iter()
    .flatten()
    {
        timestamp(value)?;
    }
    match write.status {
        OrganizationSsoDomainStatus::Pending
            if write.verified_at.is_none() && write.revoked_at.is_none() =>
        {
            Ok(())
        }
        OrganizationSsoDomainStatus::Verified
            if write.verified_at.is_some() && write.revoked_at.is_none() =>
        {
            Ok(())
        }
        OrganizationSsoDomainStatus::Revoked if write.revoked_at.is_some() => Ok(()),
        _ => Err(OrganizationSsoRepositoryError::InvalidInput),
    }
}

fn domain_transition_allowed(
    current: OrganizationSsoDomainStatus,
    next: OrganizationSsoDomainStatus,
) -> bool {
    matches!(
        (current, next),
        (
            OrganizationSsoDomainStatus::Pending,
            OrganizationSsoDomainStatus::Pending
        ) | (
            OrganizationSsoDomainStatus::Pending,
            OrganizationSsoDomainStatus::Verified
        ) | (
            OrganizationSsoDomainStatus::Pending,
            OrganizationSsoDomainStatus::Revoked
        ) | (
            OrganizationSsoDomainStatus::Verified,
            OrganizationSsoDomainStatus::Verified
        ) | (
            OrganizationSsoDomainStatus::Verified,
            OrganizationSsoDomainStatus::Revoked
        )
    )
}

fn validate_result(
    result: &OrganizationSsoLoginTransactionResult,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationSsoRepositoryError> {
    if result.result_code == 0 {
        return Err(OrganizationSsoRepositoryError::InvalidInput);
    }
    match (&result.ticket_digest, result.ticket_expires_at) {
        (Some(digest), Some(expires)) => {
            validate_digest(digest)?;
            if timestamp(expires)? <= now {
                return Err(OrganizationSsoRepositoryError::InvalidInput);
            }
        }
        (None, None) => {}
        _ => return Err(OrganizationSsoRepositoryError::InvalidInput),
    }
    match (&result.assertion_digest, result.assertion_expires_at) {
        (Some(digest), Some(expires)) => {
            validate_digest(digest)?;
            if timestamp(expires)? <= now {
                return Err(OrganizationSsoRepositoryError::InvalidInput);
            }
        }
        (None, None) => {}
        _ => return Err(OrganizationSsoRepositoryError::InvalidInput),
    }
    if result.invitation_email_fingerprint.is_some() && result.assertion_digest.is_none() {
        return Err(OrganizationSsoRepositoryError::InvalidInput);
    }
    Ok(())
}

async fn begin(pool: &DatabasePool) -> Result<DatabaseTransaction, OrganizationSsoRepositoryError> {
    pool.connection().begin().await.map_err(query_error)
}
async fn commit(transaction: DatabaseTransaction) -> Result<(), OrganizationSsoRepositoryError> {
    transaction.commit().await.map_err(query_error)
}
async fn rollback(transaction: DatabaseTransaction) -> Result<(), OrganizationSsoRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}
fn map_write_error(error: DbErr) -> OrganizationSsoRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        OrganizationSsoRepositoryError::Conflict
    } else {
        query_error(error)
    }
}
fn query_error(_error: DbErr) -> OrganizationSsoRepositoryError {
    record_internal_error(OrganizationSsoRepositoryError::Query)
}
fn invariant() -> OrganizationSsoRepositoryError {
    record_internal_error(OrganizationSsoRepositoryError::Invariant)
}
fn record_internal_error(error: OrganizationSsoRepositoryError) -> OrganizationSsoRepositoryError {
    let kind = match error {
        OrganizationSsoRepositoryError::Query => "organization_sso_query",
        OrganizationSsoRepositoryError::Timeout => "organization_sso_timeout",
        OrganizationSsoRepositoryError::Invariant => "organization_sso_invariant",
        _ => return error,
    };
    tracing::error!(target: "af_db::organization_sso", error_kind = kind, "企业 SSO 仓储发生内部错误");
    error
}
