use std::{fmt, future::Future, pin::Pin};

use af_account::{CustomOAuth2ProviderKey, PlainSystemSecret, SystemSecretCipher};
use af_db::{
    CustomOAuth2ProviderRecord, CustomOAuth2ProviderRepository,
    CustomOAuth2ProviderRepositoryError, CustomOAuth2ProviderSecretUpdate,
    CustomOAuth2ProviderWriteRecord,
};
use af_domain::PlatformPermission;
use thiserror::Error;

use crate::{PlatformPolicy, SessionPrincipal};

/// 管理端可见的自定义 OAuth2 Provider 脱敏投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminCustomOAuth2Provider {
    provider_key: String,
    display_name: String,
    client_id: String,
    authorization_endpoint_configured: bool,
    token_endpoint_configured: bool,
    userinfo_endpoint_configured: bool,
    scope_configured: bool,
    subject_field_configured: bool,
    enabled: bool,
    secret_configured: bool,
    version: i64,
}

impl AdminCustomOAuth2Provider {
    /// 构造管理员范围的脱敏投影，供替代服务实现和 HTTP 回归使用。
    #[allow(clippy::too_many_arguments, reason = "字段与管理员脱敏投影一一对应")]
    pub fn new(
        provider_key: String,
        display_name: String,
        client_id: String,
        authorization_endpoint_configured: bool,
        token_endpoint_configured: bool,
        userinfo_endpoint_configured: bool,
        scope_configured: bool,
        subject_field_configured: bool,
        enabled: bool,
        secret_configured: bool,
        version: i64,
    ) -> Result<Self, AdminCustomOAuth2ProviderError> {
        CustomOAuth2ProviderKey::new(provider_key.clone())
            .map_err(|_| AdminCustomOAuth2ProviderError::InvalidInput)?;
        if display_name.is_empty() || client_id.is_empty() || version <= 0 {
            return Err(AdminCustomOAuth2ProviderError::InvalidInput);
        }
        Ok(Self {
            provider_key,
            display_name,
            client_id,
            authorization_endpoint_configured,
            token_endpoint_configured,
            userinfo_endpoint_configured,
            scope_configured,
            subject_field_configured,
            enabled,
            secret_configured,
            version,
        })
    }

    fn from_record(record: &CustomOAuth2ProviderRecord) -> Self {
        Self {
            provider_key: record.provider_key().to_owned(),
            display_name: record.display_name().to_owned(),
            client_id: record.client_id().to_owned(),
            authorization_endpoint_configured: !record.authorization_endpoint().is_empty(),
            token_endpoint_configured: !record.token_endpoint().is_empty(),
            userinfo_endpoint_configured: !record.userinfo_endpoint().is_empty(),
            scope_configured: !record.scope().is_empty(),
            subject_field_configured: !record.subject_field().is_empty(),
            enabled: record.enabled(),
            secret_configured: record.secret_configured(),
            version: record.version(),
        }
    }

    #[must_use]
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    #[must_use]
    pub const fn authorization_endpoint_configured(&self) -> bool {
        self.authorization_endpoint_configured
    }

    #[must_use]
    pub const fn token_endpoint_configured(&self) -> bool {
        self.token_endpoint_configured
    }

    #[must_use]
    pub const fn userinfo_endpoint_configured(&self) -> bool {
        self.userinfo_endpoint_configured
    }

    #[must_use]
    pub const fn scope_configured(&self) -> bool {
        self.scope_configured
    }

    #[must_use]
    pub const fn subject_field_configured(&self) -> bool {
        self.subject_field_configured
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn secret_configured(&self) -> bool {
        self.secret_configured
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 管理员保存自定义 Provider 的完整配置；密钥只在请求内存中短暂存在。
pub struct AdminCustomOAuth2ProviderCommand {
    expected_version: i64,
    display_name: String,
    client_id: String,
    authorization_endpoint: String,
    token_endpoint: String,
    userinfo_endpoint: String,
    scope: String,
    subject_field: String,
    enabled: bool,
    client_secret: Option<PlainSystemSecret>,
    clear_client_secret: bool,
}

/// 管理员保存自定义 Provider 的具名输入；明文密钥不会被持久化或输出。
pub struct AdminCustomOAuth2ProviderCommandInput {
    pub expected_version: i64,
    pub display_name: String,
    pub client_id: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub scope: String,
    pub subject_field: String,
    pub enabled: bool,
    pub client_secret: Option<String>,
    pub clear_client_secret: bool,
}

impl AdminCustomOAuth2ProviderCommand {
    /// 构造 CAS 写入命令；明文密钥为空时表示保留既有密文。
    pub fn new(
        input: AdminCustomOAuth2ProviderCommandInput,
    ) -> Result<Self, AdminCustomOAuth2ProviderError> {
        let AdminCustomOAuth2ProviderCommandInput {
            expected_version,
            display_name,
            client_id,
            authorization_endpoint,
            token_endpoint,
            userinfo_endpoint,
            scope,
            subject_field,
            enabled,
            client_secret,
            clear_client_secret,
        } = input;
        let client_secret = client_secret
            .and_then(|secret| (!secret.is_empty()).then_some(secret))
            .map(PlainSystemSecret::new)
            .transpose()
            .map_err(|_| AdminCustomOAuth2ProviderError::InvalidInput)?;
        if expected_version < 0 || (client_secret.is_some() && clear_client_secret) {
            return Err(AdminCustomOAuth2ProviderError::InvalidInput);
        }
        Ok(Self {
            expected_version,
            display_name,
            client_id,
            authorization_endpoint,
            token_endpoint,
            userinfo_endpoint,
            scope,
            subject_field,
            enabled,
            client_secret,
            clear_client_secret,
        })
    }

    fn into_write_record(
        self,
        cipher: &SystemSecretCipher,
        provider_key: &CustomOAuth2ProviderKey,
    ) -> Result<CustomOAuth2ProviderWriteRecord, AdminCustomOAuth2ProviderError> {
        let secret = if self.clear_client_secret {
            CustomOAuth2ProviderSecretUpdate::Clear
        } else if let Some(client_secret) = self.client_secret.as_ref() {
            CustomOAuth2ProviderSecretUpdate::Replace(
                cipher
                    .encrypt_custom_oauth2_client_secret(provider_key, client_secret)
                    .map_err(|_| AdminCustomOAuth2ProviderError::Internal)?,
            )
        } else {
            CustomOAuth2ProviderSecretUpdate::Keep
        };
        Ok(CustomOAuth2ProviderWriteRecord::new(
            self.expected_version,
            self.display_name,
            self.client_id,
            self.authorization_endpoint,
            self.token_endpoint,
            self.userinfo_endpoint,
            self.scope,
            self.subject_field,
            self.enabled,
            secret,
        ))
    }
}

impl fmt::Debug for AdminCustomOAuth2ProviderCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCustomOAuth2ProviderCommand(<已脱敏>)")
    }
}

/// 自定义 OAuth2 管理服务错误；不携带端点、密钥或用户输入。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AdminCustomOAuth2ProviderError {
    #[error("自定义 OAuth2 Provider 输入无效")]
    InvalidInput,
    #[error("自定义 OAuth2 Provider 权限不足")]
    Forbidden,
    #[error("自定义 OAuth2 Provider 不存在")]
    NotFound,
    #[error("自定义 OAuth2 Provider 版本冲突")]
    Conflict,
    #[error("自定义 OAuth2 Provider 服务内部失败")]
    Internal,
}

pub type AdminCustomOAuth2ProviderListFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<AdminCustomOAuth2Provider>, AdminCustomOAuth2ProviderError>>
            + Send
            + 'a,
    >,
>;
pub type AdminCustomOAuth2ProviderGetFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminCustomOAuth2Provider, AdminCustomOAuth2ProviderError>>
            + Send
            + 'a,
    >,
>;
pub type AdminCustomOAuth2ProviderUpdateFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminCustomOAuth2Provider, AdminCustomOAuth2ProviderError>>
            + Send
            + 'a,
    >,
>;

/// 管理员范围的 Provider 配置端口；所有读写均在服务层执行平台权限校验。
pub trait AdminCustomOAuth2ProviderService: Send + Sync {
    fn list(&self, principal: SessionPrincipal) -> AdminCustomOAuth2ProviderListFuture<'_>;
    fn get(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
    ) -> AdminCustomOAuth2ProviderGetFuture<'_>;
    fn save(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
        command: AdminCustomOAuth2ProviderCommand,
    ) -> AdminCustomOAuth2ProviderUpdateFuture<'_>;
}

/// 使用第 91 号迁移仓储和系统密钥封套实现管理员 Provider 管理。
pub struct DatabaseAdminCustomOAuth2ProviderService {
    repository: CustomOAuth2ProviderRepository,
    cipher: SystemSecretCipher,
}

impl DatabaseAdminCustomOAuth2ProviderService {
    #[must_use]
    pub const fn new(
        repository: CustomOAuth2ProviderRepository,
        cipher: SystemSecretCipher,
    ) -> Self {
        Self { repository, cipher }
    }
}

impl AdminCustomOAuth2ProviderService for DatabaseAdminCustomOAuth2ProviderService {
    fn list(&self, principal: SessionPrincipal) -> AdminCustomOAuth2ProviderListFuture<'_> {
        Box::pin(async move {
            require_permission(principal)?;
            self.repository
                .providers()
                .await
                .map(|records| {
                    records
                        .iter()
                        .map(AdminCustomOAuth2Provider::from_record)
                        .collect()
                })
                .map_err(map_repository_error)
        })
    }

    fn get(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
    ) -> AdminCustomOAuth2ProviderGetFuture<'_> {
        Box::pin(async move {
            require_permission(principal)?;
            self.repository
                .provider(&provider_key)
                .await
                .map_err(map_repository_error)?
                .as_ref()
                .map(AdminCustomOAuth2Provider::from_record)
                .ok_or(AdminCustomOAuth2ProviderError::NotFound)
        })
    }

    fn save(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
        command: AdminCustomOAuth2ProviderCommand,
    ) -> AdminCustomOAuth2ProviderUpdateFuture<'_> {
        Box::pin(async move {
            require_permission(principal)?;
            let provider_key = CustomOAuth2ProviderKey::new(provider_key)
                .map_err(|_| AdminCustomOAuth2ProviderError::InvalidInput)?;
            self.repository
                .save_provider(
                    provider_key.as_str(),
                    command.into_write_record(&self.cipher, &provider_key)?,
                )
                .await
                .map(|record| AdminCustomOAuth2Provider::from_record(&record))
                .map_err(map_repository_error)
        })
    }
}

impl fmt::Debug for DatabaseAdminCustomOAuth2ProviderService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminCustomOAuth2ProviderService")
    }
}

fn require_permission(principal: SessionPrincipal) -> Result<(), AdminCustomOAuth2ProviderError> {
    PlatformPolicy::allows(principal, PlatformPermission::CustomOAuth2ProvidersManage)
        .then_some(())
        .ok_or(AdminCustomOAuth2ProviderError::Forbidden)
}

fn map_repository_error(
    error: CustomOAuth2ProviderRepositoryError,
) -> AdminCustomOAuth2ProviderError {
    match error {
        CustomOAuth2ProviderRepositoryError::InvalidInput => {
            AdminCustomOAuth2ProviderError::InvalidInput
        }
        CustomOAuth2ProviderRepositoryError::ConcurrentUpdate => {
            AdminCustomOAuth2ProviderError::Conflict
        }
        CustomOAuth2ProviderRepositoryError::Query
        | CustomOAuth2ProviderRepositoryError::Timeout
        | CustomOAuth2ProviderRepositoryError::Invariant => {
            AdminCustomOAuth2ProviderError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_redacts_secret_and_rejects_ambiguous_updates() {
        let command =
            AdminCustomOAuth2ProviderCommand::new(AdminCustomOAuth2ProviderCommandInput {
                expected_version: 0,
                display_name: "企业登录".to_owned(),
                client_id: "client-id".to_owned(),
                authorization_endpoint: "https://login.example.com/authorize".to_owned(),
                token_endpoint: "https://login.example.com/token".to_owned(),
                userinfo_endpoint: "https://login.example.com/userinfo".to_owned(),
                scope: "openid profile".to_owned(),
                subject_field: "sub".to_owned(),
                enabled: true,
                client_secret: Some("secret-value".to_owned()),
                clear_client_secret: false,
            })
            .unwrap();
        assert!(!format!("{command:?}").contains("secret-value"));
        assert_eq!(
            AdminCustomOAuth2ProviderCommand::new(AdminCustomOAuth2ProviderCommandInput {
                expected_version: 1,
                display_name: "企业登录".to_owned(),
                client_id: "client-id".to_owned(),
                authorization_endpoint: "https://login.example.com/authorize".to_owned(),
                token_endpoint: "https://login.example.com/token".to_owned(),
                userinfo_endpoint: "https://login.example.com/userinfo".to_owned(),
                scope: "openid".to_owned(),
                subject_field: "sub".to_owned(),
                enabled: false,
                client_secret: Some("secret-value".to_owned()),
                clear_client_secret: true,
            },)
            .unwrap_err(),
            AdminCustomOAuth2ProviderError::InvalidInput
        );
    }
}
