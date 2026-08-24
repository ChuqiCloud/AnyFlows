use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_account::{OrganizationSsoDnsResolverError, OrganizationSsoDnsTxtResolver};
use af_db::{
    OrganizationSsoDomainCreate, OrganizationSsoDomainRecord, OrganizationSsoProviderRepository,
    OrganizationSsoRepositoryError,
};
use af_domain::{
    DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS, MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS,
    MIN_ORGANIZATION_SSO_PROOF_TTL_SECONDS, OrganizationEntitlementStatus, OrganizationId,
    OrganizationPermission, OrganizationRole, OrganizationSsoDnsVerification,
    OrganizationSsoDomain as DomainValue, OrganizationSsoDomainProof,
    OrganizationSsoDomainProofError, OrganizationSsoDomainProofToken, OrganizationSsoDomainStatus,
    OrganizationSsoProviderKey,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 企业 SSO 域名的脱敏管理投影，不包含证明摘要或 token。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationSsoDomain {
    id: i64,
    organization_id: OrganizationId,
    provider_key: String,
    domain: String,
    status: OrganizationSsoDomainStatus,
    proof_expires_at: Option<i64>,
    verified_at: Option<i64>,
    recheck_at: Option<i64>,
    revoked_at: Option<i64>,
    version: i64,
}

impl OrganizationSsoDomain {
    fn from_record(record: OrganizationSsoDomainRecord) -> Self {
        Self {
            id: record.id(),
            organization_id: record.organization_id(),
            provider_key: record.provider_key().persistence_key(),
            domain: record.domain().as_str().to_owned(),
            status: record.status(),
            proof_expires_at: record.proof_expires_at(),
            verified_at: record.verified_at(),
            recheck_at: record.recheck_at(),
            revoked_at: record.revoked_at(),
            version: record.version(),
        }
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
    #[must_use]
    pub fn domain(&self) -> &str {
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

/// 创建域名后仅在本次响应返回的 TXT 配置，服务端不会再次读取 token。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationSsoDomainCreated {
    domain: OrganizationSsoDomain,
    txt_record_name: String,
    txt_record_value: String,
}

impl OrganizationSsoDomainCreated {
    #[must_use]
    pub fn domain(&self) -> &OrganizationSsoDomain {
        &self.domain
    }
    #[must_use]
    pub fn txt_record_name(&self) -> &str {
        &self.txt_record_name
    }
    #[must_use]
    pub fn txt_record_value(&self) -> &str {
        &self.txt_record_value
    }
}

/// 域名证明创建命令，域名和值对象在进入仓储前完成规范化校验。
pub struct OrganizationSsoDomainCreateCommand {
    provider_key: OrganizationSsoProviderKey,
    domain: DomainValue,
    ttl_seconds: i64,
}

impl OrganizationSsoDomainCreateCommand {
    pub fn new(
        provider_key: String,
        domain: String,
        ttl_seconds: i64,
    ) -> Result<Self, OrganizationSsoDomainServiceError> {
        if !(MIN_ORGANIZATION_SSO_PROOF_TTL_SECONDS..=MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS)
            .contains(&ttl_seconds)
        {
            return Err(OrganizationSsoDomainServiceError::InvalidInput);
        }
        Ok(Self {
            provider_key: OrganizationSsoProviderKey::from_persistence_key(&provider_key)
                .map_err(|_| OrganizationSsoDomainServiceError::InvalidInput)?,
            domain: DomainValue::new(domain)
                .map_err(|_| OrganizationSsoDomainServiceError::InvalidInput)?,
            ttl_seconds,
        })
    }
}

impl fmt::Debug for OrganizationSsoDomainCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoDomainCreateCommand")
            .field("provider_key", &self.provider_key)
            .field("domain", &self.domain)
            .field("ttl_seconds", &self.ttl_seconds)
            .finish()
    }
}

/// 域名服务端验证命令。proof token 仅用于本次受控 DNS 验证，不接受客户端 DNS 结果。
pub struct OrganizationSsoDomainVerifyCommand {
    proof_token: OrganizationSsoDomainProofToken,
    expected_version: i64,
}

impl OrganizationSsoDomainVerifyCommand {
    pub fn new(
        proof_token: String,
        expected_version: i64,
    ) -> Result<Self, OrganizationSsoDomainServiceError> {
        if expected_version <= 0 {
            return Err(OrganizationSsoDomainServiceError::InvalidInput);
        }
        Ok(Self {
            proof_token: OrganizationSsoDomainProofToken::new(proof_token)
                .map_err(|_| OrganizationSsoDomainServiceError::InvalidInput)?,
            expected_version,
        })
    }
}

impl fmt::Debug for OrganizationSsoDomainVerifyCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoDomainVerifyCommand")
            .field("proof_token", &"<redacted>")
            .field("expected_version", &self.expected_version)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationSsoDomainServiceError {
    #[error("企业 SSO 域名输入无效")]
    InvalidInput,
    #[error("企业 SSO 域名权限不足")]
    Forbidden,
    #[error("企业 SSO 商业授权不可用")]
    EntitlementRestricted,
    #[error("企业 SSO 域名不存在")]
    NotFound,
    #[error("企业 SSO 域名版本冲突")]
    Conflict,
    #[error("企业 SSO 域名状态不可用")]
    InvalidTransition,
    #[error("企业 SSO 域名验证失败")]
    VerificationFailed,
    #[error("企业 SSO 域名服务内部失败")]
    Internal,
}

pub type OrganizationSsoDomainListFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<OrganizationSsoDomain>, OrganizationSsoDomainServiceError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationSsoDomainCreatedFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationSsoDomainCreated, OrganizationSsoDomainServiceError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationSsoDomainFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationSsoDomain, OrganizationSsoDomainServiceError>>
            + Send
            + 'a,
    >,
>;

/// 企业域名管理服务，统一封装 RBAC、租户边界、受控 DNS 与仓储 CAS。
pub trait OrganizationSsoDomainService: Send + Sync {
    fn list(&self, context: OrganizationContext) -> OrganizationSsoDomainListFuture<'_>;
    fn create(
        &self,
        context: OrganizationContext,
        command: OrganizationSsoDomainCreateCommand,
    ) -> OrganizationSsoDomainCreatedFuture<'_>;
    fn verify(
        &self,
        context: OrganizationContext,
        domain_id: i64,
        command: OrganizationSsoDomainVerifyCommand,
    ) -> OrganizationSsoDomainFuture<'_>;
}

#[derive(Clone)]
pub struct DatabaseOrganizationSsoDomainService {
    repository: OrganizationSsoProviderRepository,
    dns_resolver: Arc<dyn OrganizationSsoDnsTxtResolver>,
}

impl DatabaseOrganizationSsoDomainService {
    #[must_use]
    pub fn new(
        repository: OrganizationSsoProviderRepository,
        dns_resolver: Arc<dyn OrganizationSsoDnsTxtResolver>,
    ) -> Self {
        Self {
            repository,
            dns_resolver,
        }
    }
}

impl OrganizationSsoDomainService for DatabaseOrganizationSsoDomainService {
    fn list(&self, context: OrganizationContext) -> OrganizationSsoDomainListFuture<'_> {
        Box::pin(async move {
            require_read(&context)?;
            self.repository
                .list_domains(context.organization_id())
                .await
                .map(|records| {
                    records
                        .into_iter()
                        .map(OrganizationSsoDomain::from_record)
                        .collect()
                })
                .map_err(map_repository_error)
        })
    }

    fn create(
        &self,
        context: OrganizationContext,
        command: OrganizationSsoDomainCreateCommand,
    ) -> OrganizationSsoDomainCreatedFuture<'_> {
        Box::pin(async move {
            require_manage(&context)?;
            let now = unix_timestamp()?;
            let proof = OrganizationSsoDomainProof::issue(command.domain, now, command.ttl_seconds)
                .map_err(map_proof_error)?;
            let record = self
                .repository
                .create_domain(
                    context.organization_id(),
                    OrganizationSsoDomainCreate::new(
                        command.provider_key,
                        proof.domain().clone(),
                        proof.digest(),
                        proof.expires_at(),
                    ),
                )
                .await
                .map_err(map_repository_error)?;
            Ok(OrganizationSsoDomainCreated {
                domain: OrganizationSsoDomain::from_record(record),
                txt_record_name: proof.txt_record_name(),
                txt_record_value: proof.token().as_str().to_owned(),
            })
        })
    }

    fn verify(
        &self,
        context: OrganizationContext,
        domain_id: i64,
        command: OrganizationSsoDomainVerifyCommand,
    ) -> OrganizationSsoDomainFuture<'_> {
        Box::pin(async move {
            require_manage(&context)?;
            if domain_id <= 0 {
                return Err(OrganizationSsoDomainServiceError::InvalidInput);
            }
            let record = self
                .repository
                .domain(context.organization_id(), domain_id)
                .await
                .map_err(map_repository_error)?;
            let now = unix_timestamp()?;
            let expires_at = record
                .proof_expires_at()
                .ok_or(OrganizationSsoDomainServiceError::VerificationFailed)?;
            // 仓储只保存过期时间和摘要；用最大允许窗口重建内存证明，确保原过期时间不变，
            // 不会因为验证发生在最小 TTL 窗口内而被误判为非法。
            let issued_at = expires_at
                .checked_sub(MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS)
                .ok_or(OrganizationSsoDomainServiceError::VerificationFailed)?;
            let proof = OrganizationSsoDomainProof::from_token(
                record.domain().clone(),
                command.proof_token,
                issued_at,
                MAX_ORGANIZATION_SSO_PROOF_TTL_SECONDS,
            )
            .map_err(|_| OrganizationSsoDomainServiceError::VerificationFailed)?;
            let answers = self
                .dns_resolver
                .lookup_txt(&proof)
                .await
                .map_err(|_| OrganizationSsoDomainServiceError::VerificationFailed)?;
            let verification = OrganizationSsoDnsVerification::verify(
                &proof,
                record.status(),
                &answers,
                now,
                DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
            )
            .map_err(|_| OrganizationSsoDomainServiceError::VerificationFailed)?;
            self.repository
                .verify_domain(
                    context.organization_id(),
                    domain_id,
                    verification,
                    command.expected_version,
                )
                .await
                .map(OrganizationSsoDomain::from_record)
                .map_err(map_repository_error)
        })
    }
}

impl fmt::Debug for DatabaseOrganizationSsoDomainService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationSsoDomainService(<repository,dns_resolver>)")
    }
}

fn require_read(context: &OrganizationContext) -> Result<(), OrganizationSsoDomainServiceError> {
    if !matches!(
        context.role(),
        OrganizationRole::Owner | OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::SsoRead)
    {
        return Err(OrganizationSsoDomainServiceError::Forbidden);
    }
    if !context.entitlement().sso_enabled()
        || context.entitlement().status() != OrganizationEntitlementStatus::Active
    {
        return Err(OrganizationSsoDomainServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn require_manage(context: &OrganizationContext) -> Result<(), OrganizationSsoDomainServiceError> {
    if !matches!(
        context.role(),
        OrganizationRole::Owner | OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::SsoManage)
    {
        return Err(OrganizationSsoDomainServiceError::Forbidden);
    }
    if !context.entitlement().sso_enabled()
        || context.entitlement().status() != OrganizationEntitlementStatus::Active
    {
        return Err(OrganizationSsoDomainServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn unix_timestamp() -> Result<i64, OrganizationSsoDomainServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .map_err(|_| OrganizationSsoDomainServiceError::Internal)
}

fn map_proof_error(error: OrganizationSsoDomainProofError) -> OrganizationSsoDomainServiceError {
    match error {
        OrganizationSsoDomainProofError::EntropyUnavailable => {
            OrganizationSsoDomainServiceError::Internal
        }
        OrganizationSsoDomainProofError::PublicSuffix
        | OrganizationSsoDomainProofError::PrivateSuffix
        | OrganizationSsoDomainProofError::InvalidToken
        | OrganizationSsoDomainProofError::InvalidTtl
        | OrganizationSsoDomainProofError::InvalidTime => {
            OrganizationSsoDomainServiceError::InvalidInput
        }
    }
}

fn map_repository_error(
    error: OrganizationSsoRepositoryError,
) -> OrganizationSsoDomainServiceError {
    match error {
        OrganizationSsoRepositoryError::InvalidInput => {
            OrganizationSsoDomainServiceError::InvalidInput
        }
        OrganizationSsoRepositoryError::NotFound => OrganizationSsoDomainServiceError::NotFound,
        OrganizationSsoRepositoryError::Conflict => OrganizationSsoDomainServiceError::Conflict,
        OrganizationSsoRepositoryError::InvalidTransition
        | OrganizationSsoRepositoryError::AlreadyProcessed
        | OrganizationSsoRepositoryError::Expired
        | OrganizationSsoRepositoryError::Unavailable => {
            OrganizationSsoDomainServiceError::InvalidTransition
        }
        OrganizationSsoRepositoryError::Query
        | OrganizationSsoRepositoryError::Timeout
        | OrganizationSsoRepositoryError::Invariant => OrganizationSsoDomainServiceError::Internal,
    }
}

impl From<OrganizationSsoDnsResolverError> for OrganizationSsoDomainServiceError {
    fn from(_: OrganizationSsoDnsResolverError) -> Self {
        Self::VerificationFailed
    }
}
