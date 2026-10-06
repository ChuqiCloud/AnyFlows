use crate::{DatabaseVerificationSettingsService, VerificationPolicy};
use crate::{
    PlatformAuditEntry, PlatformAuditOutcome, PlatformAuditService, PlatformPolicy,
    SessionPrincipal,
};
pub use af_db::{
    AccountVerificationError, AccountVerificationMaterial, AccountVerificationRecord,
    AccountVerificationSubmit, VerificationMaterialWrite, validate_verification_material,
};
use af_db::{
    AccountVerificationProviderRequest, AccountVerificationProviderResult,
    AccountVerificationProviderStart, AccountVerificationRepository,
    validate_account_verification_submit,
};
use af_domain::{PlatformPermission, UserId};
use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};

/// Stable extension boundary for identity providers. Provider credentials and
/// network clients stay outside the review repository so future plugins can be
/// registered without changing case storage or the manual review workflow.
#[async_trait]
pub trait AccountVerificationProvider: Send + Sync {
    fn key(&self) -> &'static str;
    fn configured(&self) -> bool;

    /// Returns the current user-visible availability. Providers backed by
    /// mutable database settings can override this without rebuilding the
    /// service or changing the case storage contract.
    async fn available(&self) -> bool {
        self.configured()
    }

    async fn initialize(
        &self,
        _request: &AccountVerificationProviderRequest,
    ) -> Result<AccountVerificationProviderStart, AccountVerificationProviderError> {
        Err(AccountVerificationProviderError::Unavailable)
    }

    async fn query(
        &self,
        _reference: &str,
    ) -> Result<AccountVerificationProviderResult, AccountVerificationProviderError> {
        Err(AccountVerificationProviderError::Unavailable)
    }

    /// Complete a provider flow after the provider redirects back with an
    /// authorization code. Providers that do not use browser authorization
    /// keep the default unavailable result.
    async fn complete(
        &self,
        _reference: &str,
        _authorization_code: &str,
    ) -> Result<AccountVerificationProviderResult, AccountVerificationProviderError> {
        Err(AccountVerificationProviderError::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, Eq, thiserror::Error, PartialEq)]
pub enum AccountVerificationProviderError {
    #[error("认证服务暂不可用")]
    Unavailable,
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

pub struct AccountVerificationService {
    repository: AccountVerificationRepository,
    audit: Arc<dyn PlatformAuditService>,
    providers: HashMap<&'static str, Arc<dyn AccountVerificationProvider>>,
    settings: Option<Arc<DatabaseVerificationSettingsService>>,
}

impl AccountVerificationService {
    #[must_use]
    pub fn new(
        repository: AccountVerificationRepository,
        audit: Arc<dyn PlatformAuditService>,
    ) -> Self {
        let manual: Arc<dyn AccountVerificationProvider> =
            Arc::new(ManualAccountVerificationProvider);
        Self {
            repository,
            audit,
            providers: HashMap::from([("manual", manual)]),
            settings: None,
        }
    }

    #[must_use]
    pub fn with_provider(mut self, provider: Arc<dyn AccountVerificationProvider>) -> Self {
        self.providers.insert(provider.key(), provider);
        self
    }

    #[must_use]
    pub fn with_settings(mut self, settings: Arc<DatabaseVerificationSettingsService>) -> Self {
        self.settings = Some(settings);
        self
    }

    pub async fn policy(&self) -> Result<VerificationPolicy, AccountVerificationError> {
        match &self.settings {
            Some(settings) => settings
                .policy()
                .await
                .map_err(|_| AccountVerificationError::Unavailable),
            None => Ok(VerificationPolicy {
                individual_manual_enabled: true,
                enterprise_manual_enabled: true,
                individual_reason_required: true,
                enterprise_reason_required: true,
            }),
        }
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
        mut write: AccountVerificationSubmit,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        validate_account_verification_submit(&write)?;
        let policy = self.policy().await?;
        if write.summary.trim().is_empty() && policy.reason_required_for(&write.kind) {
            return Err(AccountVerificationError::Invalid);
        }
        if write.provider == "manual" && !policy.manual_enabled_for(&write.kind) {
            return Err(AccountVerificationError::Unavailable);
        }
        let provider = self
            .providers
            .get(write.provider.as_str())
            .ok_or(AccountVerificationError::Unavailable)?;
        if !provider.available().await {
            return Err(AccountVerificationError::Unavailable);
        }
        self.repository
            .ensure_can_submit(principal.user_id(), &write.kind)
            .await?;
        if write.provider != "manual" {
            let document_number = write
                .document_number
                .clone()
                .ok_or(AccountVerificationError::Invalid)?;
            let start = provider
                .initialize(&AccountVerificationProviderRequest {
                    kind: write.kind.clone(),
                    document_country: write.document_country.clone(),
                    document_type: write.document_type.clone(),
                    document_number,
                    subject_name: write.subject_name.clone(),
                })
                .await
                .map_err(|_| AccountVerificationError::Unavailable)?;
            write.provider_reference = Some(start.reference);
            write.provider_action_url = Some(start.action_url);
            write.provider_status = Some(start.status);
        }
        self.repository.submit(principal.user_id(), write).await
    }

    pub async fn available_providers(&self) -> Vec<String> {
        let mut providers = Vec::new();
        for provider in self.providers.values() {
            if provider.available().await {
                providers.push(provider.key().to_owned());
            }
        }
        providers.sort();
        providers
    }

    pub async fn available_providers_for(&self, kind: &str) -> Vec<String> {
        let Ok(policy) = self.policy().await else {
            return Vec::new();
        };
        let mut providers = Vec::new();
        for provider in self.providers.values() {
            if (provider.key() != "manual" || policy.manual_enabled_for(kind))
                && (provider.key() != "alipay" || kind == "individual")
                && provider.available().await
            {
                providers.push(provider.key().to_owned());
            }
        }
        providers.sort();
        providers
    }

    pub async fn sync_provider(
        &self,
        principal: SessionPrincipal,
        id: i64,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        let (provider_key, reference) = self
            .repository
            .provider_reference(principal.user_id(), id)
            .await?;
        let provider = self
            .providers
            .get(provider_key.as_str())
            .ok_or(AccountVerificationError::Unavailable)?;
        if !provider.available().await {
            return Err(AccountVerificationError::Unavailable);
        }
        let result: AccountVerificationProviderResult = provider
            .query(&reference)
            .await
            .map_err(|_| AccountVerificationError::Unavailable)?;
        self.repository
            .apply_provider_result(
                principal.user_id(),
                id,
                result.status,
                result.terminal_status,
                result.reason,
            )
            .await
    }

    pub async fn complete_provider_callback(
        &self,
        state: &str,
        authorization_code: &str,
    ) -> Result<AccountVerificationRecord, AccountVerificationError> {
        if authorization_code.trim().is_empty() || authorization_code.len() > 4_096 {
            return Err(AccountVerificationError::Invalid);
        }
        let (user, id, provider_key, reference) =
            self.repository.provider_reference_by_state(state).await?;
        let provider = self
            .providers
            .get(provider_key.as_str())
            .ok_or(AccountVerificationError::Unavailable)?;
        if !provider.available().await {
            return Err(AccountVerificationError::Unavailable);
        }
        let result = provider
            .complete(&reference, authorization_code)
            .await
            .map_err(|_| AccountVerificationError::Unavailable)?;
        self.repository
            .apply_provider_result(
                user,
                id,
                result.status,
                result.terminal_status,
                result.reason,
            )
            .await
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
        PlatformPolicy::authorize(principal, PlatformPermission::AccountVerificationsManage)
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
            PlatformPolicy::authorize(principal, PlatformPermission::AccountVerificationsReadAll)
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
            PlatformPermission::AccountVerificationsReadAll,
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
