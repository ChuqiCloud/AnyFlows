use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_config::AlipayVerificationSettings;
use af_db::{
    AccountVerificationProviderRequest, AccountVerificationProviderResult,
    AccountVerificationProviderStart, AccountVerificationSettingsError,
    AccountVerificationSettingsRecord, AccountVerificationSettingsRepository,
};
use af_httpclient::HttpClientProvider;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::{
    AccountVerificationProvider, AccountVerificationProviderError,
    AlipayAccountVerificationProvider, SessionPrincipal, SessionRole,
};

#[derive(Serialize, Deserialize)]
struct AlipayCredentials {
    private_key: String,
    public_key: String,
}

#[derive(Clone)]
pub struct AdminVerificationSettings {
    pub source: &'static str,
    pub manual_enabled: bool,
    pub enabled: bool,
    pub app_id: Option<String>,
    pub private_key_configured: bool,
    pub public_key_configured: bool,
    pub gateway_url: String,
    pub biz_code: String,
    pub timeout_secs: u64,
    pub version: i64,
}

pub struct VerificationSettingsCommand {
    pub expected_version: i64,
    pub manual_enabled: bool,
    pub enabled: bool,
    pub app_id: Option<String>,
    pub private_key: Option<String>,
    pub public_key: Option<String>,
    pub gateway_url: String,
    pub biz_code: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Error)]
pub enum VerificationSettingsError {
    #[error("实名认证配置无效")]
    Invalid,
    #[error("实名认证配置已更新，请刷新后重试")]
    Conflict,
    #[error("当前会话无权管理实名认证配置")]
    Forbidden,
    #[error("实名认证配置服务暂不可用")]
    Internal,
}

#[derive(Clone)]
pub struct DatabaseVerificationSettingsService {
    repository: AccountVerificationSettingsRepository,
    cipher: SystemSecretCipher,
    fallback: AlipayVerificationSettings,
    http_clients: HttpClientProvider,
}

impl DatabaseVerificationSettingsService {
    pub fn new(
        repository: AccountVerificationSettingsRepository,
        cipher: SystemSecretCipher,
        fallback: AlipayVerificationSettings,
        http_clients: HttpClientProvider,
    ) -> Self {
        Self {
            repository,
            cipher,
            fallback,
            http_clients,
        }
    }

    pub async fn settings(
        &self,
        principal: SessionPrincipal,
    ) -> Result<AdminVerificationSettings, VerificationSettingsError> {
        require_admin(principal)?;
        let record = self
            .repository
            .settings()
            .await
            .map_err(map_repository_error)?;
        if !record.initialized {
            return Ok(AdminVerificationSettings {
                source: "environment",
                manual_enabled: true,
                enabled: self.fallback.enabled(),
                app_id: self
                    .fallback
                    .app_id()
                    .map(|value| value.expose().to_owned()),
                private_key_configured: self.fallback.private_key().is_some(),
                public_key_configured: self.fallback.public_key().is_some(),
                gateway_url: self.fallback.gateway_url().to_owned(),
                biz_code: self.fallback.biz_code().to_owned(),
                timeout_secs: self.fallback.timeout_secs(),
                version: record.version,
            });
        }
        Ok(self.sanitized(&record))
    }

    pub async fn update(
        &self,
        principal: SessionPrincipal,
        command: VerificationSettingsCommand,
    ) -> Result<AdminVerificationSettings, VerificationSettingsError> {
        require_admin(principal)?;
        let current = self
            .repository
            .settings()
            .await
            .map_err(map_repository_error)?;
        if current.version != command.expected_version {
            return Err(VerificationSettingsError::Conflict);
        }
        let app_id = command
            .app_id
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let gateway =
            Url::parse(&command.gateway_url).map_err(|_| VerificationSettingsError::Invalid)?;
        if gateway.scheme() != "https"
            || gateway.host_str().is_none()
            || !gateway.username().is_empty()
            || gateway.password().is_some()
            || gateway.query().is_some()
            || gateway.fragment().is_some()
            || command.gateway_url.len() > 2048
            || command.biz_code.is_empty()
            || command.biz_code.len() > 64
            || !command
                .biz_code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            || !(1..=30).contains(&command.timeout_secs)
            || app_id.as_ref().is_some_and(|value| value.len() > 128)
        {
            return Err(VerificationSettingsError::Invalid);
        }
        let previous = if current.initialized {
            self.decrypt_credentials(&current)?
        } else {
            None
        };
        let private_key = command
            .private_key
            .filter(|value| !value.is_empty())
            .or_else(|| previous.as_ref().map(|value| value.private_key.clone()));
        let public_key = command
            .public_key
            .filter(|value| !value.is_empty())
            .or_else(|| previous.as_ref().map(|value| value.public_key.clone()));
        if private_key.is_some() != public_key.is_some() {
            return Err(VerificationSettingsError::Invalid);
        }
        if let (Some(private_key), Some(public_key)) = (&private_key, &public_key) {
            AlipayAccountVerificationProvider::validate_keys(private_key, public_key)
                .map_err(|_| VerificationSettingsError::Invalid)?;
        }
        AlipayAccountVerificationProvider::from_parts(
            command.enabled,
            app_id.as_deref(),
            private_key.as_deref(),
            public_key.as_deref(),
            &command.gateway_url,
            &command.biz_code,
            command.timeout_secs,
            self.http_clients.clone(),
        )
        .map_err(|_| VerificationSettingsError::Invalid)?;
        let credentials = match (private_key, public_key) {
            (Some(private_key), Some(public_key)) => {
                let json = serde_json::to_string(&AlipayCredentials {
                    private_key,
                    public_key,
                })
                .map_err(|_| VerificationSettingsError::Internal)?;
                let plaintext =
                    PlainSystemSecret::new(json).map_err(|_| VerificationSettingsError::Invalid)?;
                Some(
                    self.cipher
                        .encrypt(SystemSecretKind::AlipayVerificationCredentials, &plaintext)
                        .map_err(|_| VerificationSettingsError::Internal)?,
                )
            }
            _ => None,
        };
        let saved = self
            .repository
            .update(
                AccountVerificationSettingsRecord {
                    initialized: true,
                    manual_enabled: command.manual_enabled,
                    enabled: command.enabled,
                    app_id,
                    credentials,
                    gateway_url: command.gateway_url,
                    biz_code: command.biz_code,
                    timeout_secs: command.timeout_secs,
                    version: current.version,
                },
                command.expected_version,
            )
            .await
            .map_err(map_repository_error)?;
        Ok(self.sanitized(&saved))
    }

    fn sanitized(&self, record: &AccountVerificationSettingsRecord) -> AdminVerificationSettings {
        AdminVerificationSettings {
            source: "database",
            manual_enabled: record.manual_enabled,
            enabled: record.enabled,
            app_id: record.app_id.clone(),
            private_key_configured: record.credentials.is_some(),
            public_key_configured: record.credentials.is_some(),
            gateway_url: record.gateway_url.clone(),
            biz_code: record.biz_code.clone(),
            timeout_secs: record.timeout_secs,
            version: record.version,
        }
    }

    fn decrypt_credentials(
        &self,
        record: &AccountVerificationSettingsRecord,
    ) -> Result<Option<AlipayCredentials>, VerificationSettingsError> {
        record
            .credentials
            .as_ref()
            .map(|envelope| {
                let secret = self
                    .cipher
                    .decrypt(SystemSecretKind::AlipayVerificationCredentials, envelope)
                    .map_err(|_| VerificationSettingsError::Internal)?;
                serde_json::from_str(secret.expose_secret())
                    .map_err(|_| VerificationSettingsError::Internal)
            })
            .transpose()
    }

    pub async fn manual_enabled(&self) -> Result<bool, VerificationSettingsError> {
        let record = self
            .repository
            .settings()
            .await
            .map_err(map_repository_error)?;
        Ok(if record.initialized {
            record.manual_enabled
        } else {
            true
        })
    }

    async fn provider(
        &self,
    ) -> Result<AlipayAccountVerificationProvider, VerificationSettingsError> {
        let record = self
            .repository
            .settings()
            .await
            .map_err(map_repository_error)?;
        if !record.initialized {
            return AlipayAccountVerificationProvider::from_config(
                &self.fallback,
                self.http_clients.clone(),
            )
            .map_err(|_| VerificationSettingsError::Internal);
        }
        let credentials = self.decrypt_credentials(&record)?;
        AlipayAccountVerificationProvider::from_parts(
            record.enabled,
            record.app_id.as_deref(),
            credentials.as_ref().map(|value| value.private_key.as_str()),
            credentials.as_ref().map(|value| value.public_key.as_str()),
            &record.gateway_url,
            &record.biz_code,
            record.timeout_secs,
            self.http_clients.clone(),
        )
        .map_err(|_| VerificationSettingsError::Internal)
    }
}

pub struct DatabaseManualAccountVerificationProvider {
    settings: std::sync::Arc<DatabaseVerificationSettingsService>,
}

impl DatabaseManualAccountVerificationProvider {
    #[must_use]
    pub fn new(settings: std::sync::Arc<DatabaseVerificationSettingsService>) -> Self {
        Self { settings }
    }
}

#[async_trait]
impl AccountVerificationProvider for DatabaseManualAccountVerificationProvider {
    fn key(&self) -> &'static str {
        "manual"
    }

    fn configured(&self) -> bool {
        true
    }

    async fn available(&self) -> bool {
        self.settings.manual_enabled().await.unwrap_or(false)
    }
}

#[async_trait]
impl AccountVerificationProvider for DatabaseVerificationSettingsService {
    fn key(&self) -> &'static str {
        "alipay"
    }
    fn configured(&self) -> bool {
        true
    }

    async fn available(&self) -> bool {
        self.provider()
            .await
            .map(|provider| provider.configured())
            .unwrap_or(false)
    }

    async fn initialize(
        &self,
        request: &AccountVerificationProviderRequest,
    ) -> Result<AccountVerificationProviderStart, AccountVerificationProviderError> {
        self.provider()
            .await
            .map_err(|_| AccountVerificationProviderError::Unavailable)?
            .initialize(request)
            .await
    }

    async fn query(
        &self,
        reference: &str,
    ) -> Result<AccountVerificationProviderResult, AccountVerificationProviderError> {
        self.provider()
            .await
            .map_err(|_| AccountVerificationProviderError::Unavailable)?
            .query(reference)
            .await
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), VerificationSettingsError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(VerificationSettingsError::Forbidden)
    }
}

fn map_repository_error(error: AccountVerificationSettingsError) -> VerificationSettingsError {
    match error {
        AccountVerificationSettingsError::Invalid => VerificationSettingsError::Invalid,
        AccountVerificationSettingsError::Conflict => VerificationSettingsError::Conflict,
        AccountVerificationSettingsError::Internal => VerificationSettingsError::Internal,
    }
}
