//! Distribution-owned runtime services consumed through public contracts.

use std::sync::Arc;

use af_account::SystemSecretCipher;
use af_admin::{
    PasskeyAuthenticationService, SessionAuthenticator, UserProfileService, UserTopupService,
};
use af_billing::{ContractPriceSource, UsageRecordProjection};
use af_config::AppConfig;
use af_db::{
    AdminUsageLogRepository, DatabasePool, OrganizationTokenAuthValidator, QuotaExtension,
    RequestOutcomeRepository, TopupExtension,
};
use af_http::HttpExtensions;
use af_httpclient::HttpClientProvider;

use crate::{BackgroundTaskSupervisor, BootstrapError};

/// Shared services available after migrations and public authentication setup.
pub struct RuntimeExtensionContext<'a> {
    pub config: &'a AppConfig,
    pub database: &'a DatabasePool,
    pub session_authenticator: &'a Arc<dyn SessionAuthenticator>,
    pub user_profile_service: &'a Arc<dyn UserProfileService>,
    pub passkey_authentication_service: Option<&'a Arc<dyn PasskeyAuthenticationService>>,
    pub system_secret_cipher: &'a SystemSecretCipher,
    pub usage_log_repository: &'a AdminUsageLogRepository,
    pub request_outcome_repository: &'a RequestOutcomeRepository,
}

/// Services that become available after relay and billing initialization.
pub struct RuntimeHttpContext<'a> {
    pub config: &'a AppConfig,
    pub database: &'a DatabasePool,
    pub session_authenticator: &'a Arc<dyn SessionAuthenticator>,
    pub user_topup_service: &'a Arc<dyn UserTopupService>,
    pub upstream_clients: &'a HttpClientProvider,
}

pub type BackgroundTaskRegistrar =
    Arc<dyn Fn(&mut BackgroundTaskSupervisor, &AppConfig, &DatabasePool) + Send + Sync>;

pub type RelayExtensionInitializer =
    Arc<dyn Fn(&HttpClientProvider) -> Result<(), BootstrapError> + Send + Sync>;

pub type RuntimeHttpExtensionFactory =
    Arc<dyn Fn(RuntimeHttpContext<'_>) -> HttpExtensions + Send + Sync>;

/// Optional extension ports. The default binary starts with no private services.
#[derive(Default)]
pub struct RuntimeExtensions {
    pub organization_token_validator: Option<Arc<dyn OrganizationTokenAuthValidator>>,
    pub contract_prices: Option<Arc<dyn ContractPriceSource>>,
    pub usage_projection: Option<Arc<dyn UsageRecordProjection>>,
    pub quota: Option<Arc<dyn QuotaExtension>>,
    pub topup: Option<Arc<dyn TopupExtension>>,
    pub background_tasks: Option<BackgroundTaskRegistrar>,
    pub initialize_relay: Option<RelayExtensionInitializer>,
    pub http: Option<RuntimeHttpExtensionFactory>,
}
