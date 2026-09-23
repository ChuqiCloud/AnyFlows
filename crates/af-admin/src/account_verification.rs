use crate::{
    PlatformAuditEntry, PlatformAuditOutcome, PlatformAuditService, PlatformPolicy,
    SessionPrincipal,
};
use af_db::AccountVerificationRepository;
pub use af_db::{
    AccountVerificationError, AccountVerificationMaterial, AccountVerificationRecord,
    AccountVerificationSubmit, validate_verification_material,
};
use af_domain::{PlatformPermission, UserId};
use std::sync::Arc;

/// Stable extension boundary for identity providers. Provider credentials and
/// network clients stay outside the review repository so future plugins can be
/// registered without changing case storage or the manual review workflow.
pub trait AccountVerificationProvider: Send + Sync {
    fn key(&self) -> &'static str;
    fn configured(&self) -> bool;
}

#[derive(Debug, Default)]
pub struct ManualAccountVerificationProvider;
impl AccountVerificationProvider for ManualAccountVerificationProvider {
    fn key(&self) -> &'static str {
        "manual"
    }
    fn configured(&self) -> bool {
        true
    }
}

#[derive(Debug, Default)]
pub struct AlipayAccountVerificationProvider;
impl AccountVerificationProvider for AlipayAccountVerificationProvider {
    fn key(&self) -> &'static str {
        "alipay"
    }
    fn configured(&self) -> bool {
        false
    }
}

pub struct AccountVerificationService {
    repository: AccountVerificationRepository,
    audit: Arc<dyn PlatformAuditService>,
}

impl AccountVerificationService {
    #[must_use]
    pub fn new(
        repository: AccountVerificationRepository,
        audit: Arc<dyn PlatformAuditService>,
    ) -> Self {
        Self { repository, audit }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list(
        &self,
        principal: SessionPrincipal,
        admin: bool,
        before: Option<i64>,
        status: Option<i16>,
        limit: u64,
        request_id: &str,
    ) -> Result<Vec<AccountVerificationRecord>, AccountVerificationError> {
        let scope = Self::scope(principal, admin)?;
        if admin {
            self.audit_read(principal, "list", None, request_id).await?;
        }
        self.repository.list(scope, before, status, limit).await
    }

    pub async fn get(
        &self,
        principal: SessionPrincipal,
        admin: bool,
        id: i64,
        request_id: &str,
    ) -> Result<
        (AccountVerificationRecord, Vec<AccountVerificationMaterial>),
        AccountVerificationError,
    > {
        let scope = Self::scope(principal, admin)?;
        let record = self.repository.get(scope, id).await?;
        if admin {
            self.audit_read(principal, "read", Some(id), request_id)
                .await?;
        }
        let materials = self.repository.materials(scope, id).await?;
        Ok((record, materials))
    }

    pub async fn download(
        &self,
        principal: SessionPrincipal,
        admin: bool,
        id: i64,
        material_id: i64,
        request_id: &str,
    ) -> Result<(String, String, Vec<u8>), AccountVerificationError> {
        let scope = Self::scope(principal, admin)?;
        self.repository.get(scope, id).await?;
        if admin {
            self.audit_read(principal, "download", Some(id), request_id)
                .await?;
        }
        self.repository.download(scope, id, material_id).await
    }

    pub async fn submit(
        &self,
        principal: SessionPrincipal,
        write: AccountVerificationSubmit,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        self.repository.submit(principal.user_id(), write).await
    }

    pub async fn enterprise_verified(
        &self,
        principal: SessionPrincipal,
    ) -> Result<bool, AccountVerificationError> {
        self.repository
            .enterprise_verified(principal.user_id())
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn decide(
        &self,
        principal: SessionPrincipal,
        id: i64,
        version: i64,
        status: i16,
        reason: Option<String>,
        request_id: String,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsManage)
            .map_err(|_| AccountVerificationError::Forbidden)?;
        self.repository
            .decide(id, version, principal.user_id(), status, reason, request_id)
            .await
    }

    fn scope(
        principal: SessionPrincipal,
        admin: bool,
    ) -> Result<Option<UserId>, AccountVerificationError> {
        if admin {
            PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsReadAll)
                .map_err(|_| AccountVerificationError::Forbidden)?;
            Ok(None)
        } else {
            Ok(Some(principal.user_id()))
        }
    }

    async fn audit_read(
        &self,
        principal: SessionPrincipal,
        operation: &'static str,
        id: Option<i64>,
        request_id: &str,
    ) -> Result<(), AccountVerificationError> {
        let entry = PlatformAuditEntry::new(
            principal,
            PlatformPermission::OrganizationsReadAll,
            "/api/admin/account-verifications",
            operation,
            "account_verification",
            id.map(|i| i.to_string()),
            PlatformAuditOutcome::Succeeded,
            None,
            None,
            None,
            request_id,
        )
        .map_err(|_| AccountVerificationError::Unavailable)?;
        self.audit
            .record(entry)
            .await
            .map_err(|_| AccountVerificationError::Unavailable)
    }
}
