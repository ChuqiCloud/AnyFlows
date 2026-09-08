use std::{fmt, future::Future, pin::Pin};

use af_account::{
    CustomOAuth2EndpointBundle, CustomOAuth2ProviderConfig, CustomOAuth2ProviderKey,
    LoginOAuthProvider, OAuthLoginClient, OAuthLoginClientError, PlainSystemSecret,
    SystemSecretCipher, SystemSecretKind,
};
use af_db::{
    CustomOAuth2IdentityCompletion, CustomOAuth2LoginRepository, CustomOAuth2LoginRepositoryError,
    CustomOAuth2ProviderRecord, CustomOAuth2ProviderRepository,
    CustomOAuth2ProviderRepositoryError, EncryptedCredentialEnvelope, OAuthIdentityCompletion,
    OAuthLoginProviderRecord, OAuthLoginProviderWriteRecord, OAuthLoginRepository,
    OAuthLoginRepositoryError, OAuthLoginSecretUpdate, SiteSettingsRepository,
};
use af_domain::UserId;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use url::Url;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{SessionPrincipal, SessionRole};

const STATE_TTL_SECONDS: u64 = 10 * 60;
const TICKET_TTL_SECONDS: u64 = 2 * 60;

/// 游客可见的已配置 OAuth 登录 Provider。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicOAuthLoginProvider {
    id: String,
    display_name: String,
}

impl PublicOAuthLoginProvider {
    /// 由受信 Provider 适配器构造公开摘要。
    #[must_use]
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// 管理员可见的内置 OAuth App 配置，密钥仅投影配置事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminOAuthLoginProviderSettings {
    enabled: bool,
    client_id: Option<String>,
    issuer_url: Option<String>,
    client_secret_configured: bool,
    callback_url: Option<String>,
    version: i64,
}

impl AdminOAuthLoginProviderSettings {
    fn from_record(record: OAuthLoginProviderRecord, callback_url: Option<String>) -> Self {
        Self {
            enabled: record.enabled(),
            client_id: record.client_id().map(str::to_owned),
            issuer_url: record.issuer_url().map(str::to_owned),
            client_secret_configured: record.client_secret_configured(),
            callback_url,
            version: record.version(),
        }
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn client_id(&self) -> Option<&str> {
        self.client_id.as_deref()
    }

    #[must_use]
    pub fn issuer_url(&self) -> Option<&str> {
        self.issuer_url.as_deref()
    }

    #[must_use]
    pub const fn client_secret_configured(&self) -> bool {
        self.client_secret_configured
    }

    #[must_use]
    pub fn callback_url(&self) -> Option<&str> {
        self.callback_url.as_deref()
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 管理员完整保存内置 OAuth App 设置的命令。
pub struct AdminOAuthLoginProviderSettingsCommand {
    expected_version: i64,
    enabled: bool,
    client_id: Option<String>,
    issuer_url: Option<String>,
    client_secret: Option<PlainSystemSecret>,
    clear_client_secret: bool,
}

impl AdminOAuthLoginProviderSettingsCommand {
    pub fn new(
        expected_version: i64,
        enabled: bool,
        client_id: Option<String>,
        issuer_url: Option<String>,
        client_secret: Option<String>,
        clear_client_secret: bool,
    ) -> Result<Self, OAuthLoginError> {
        let client_id = normalize_client_id(client_id)?;
        let issuer_url = normalize_issuer_url(issuer_url)?;
        let client_secret = client_secret
            .and_then(|value| (!value.is_empty()).then_some(value))
            .map(PlainSystemSecret::new)
            .transpose()
            .map_err(|_| OAuthLoginError::InvalidInput)?;
        if expected_version < 1
            || (client_secret.is_some() && clear_client_secret)
            || (enabled && client_id.is_none())
        {
            return Err(OAuthLoginError::InvalidInput);
        }
        Ok(Self {
            expected_version,
            enabled,
            client_id,
            issuer_url,
            client_secret,
            clear_client_secret,
        })
    }

    fn into_record(
        self,
        cipher: &SystemSecretCipher,
        provider: LoginOAuthProvider,
    ) -> Result<OAuthLoginProviderWriteRecord, OAuthLoginError> {
        validate_command_issuer(provider, self.issuer_url.as_deref(), self.enabled)?;
        let secret = if self.clear_client_secret {
            OAuthLoginSecretUpdate::Clear
        } else if let Some(secret) = self.client_secret.as_ref() {
            OAuthLoginSecretUpdate::Replace(
                cipher
                    .encrypt(client_secret_kind(provider), secret)
                    .map_err(|_| OAuthLoginError::Internal)?,
            )
        } else {
            OAuthLoginSecretUpdate::Keep
        };
        Ok(OAuthLoginProviderWriteRecord::new(
            self.expected_version,
            self.enabled,
            self.client_id,
            match provider {
                LoginOAuthProvider::Oidc => self.issuer_url,
                LoginOAuthProvider::LinuxDo => Some("https://connect.linux.do".to_owned()),
                LoginOAuthProvider::Telegram => Some("https://oauth.telegram.org".to_owned()),
                LoginOAuthProvider::Google => Some("https://accounts.google.com".to_owned()),
                LoginOAuthProvider::GitHub
                | LoginOAuthProvider::Discord
                | LoginOAuthProvider::WeChat => None,
            },
            secret,
        ))
    }
}

impl fmt::Debug for AdminOAuthLoginProviderSettingsCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminOAuthLoginProviderSettingsCommand(<已脱敏>)")
    }
}

/// OAuth 授权启动响应，只携带服务端构造的固定 Provider URL。
#[derive(Clone, Eq, PartialEq)]
pub struct OAuthLoginStart {
    authorization_url: String,
}

impl fmt::Debug for OAuthLoginStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuthLoginStart(<已脱敏>)")
    }
}

impl OAuthLoginStart {
    /// 由受信 Provider 适配器构造授权跳转结果。
    #[must_use]
    pub fn new(authorization_url: String) -> Self {
        Self { authorization_url }
    }

    #[must_use]
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }
}

/// OAuth 回调完成后跳转到前端单次票据交换页。
#[derive(Clone, Eq, PartialEq)]
pub struct OAuthLoginCallbackResult {
    redirect_url: String,
}

impl fmt::Debug for OAuthLoginCallbackResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuthLoginCallbackResult(<已脱敏>)")
    }
}

impl OAuthLoginCallbackResult {
    /// 由受信回调适配器构造同站前端跳转结果。
    #[must_use]
    pub fn new(redirect_url: String) -> Self {
        Self { redirect_url }
    }

    #[must_use]
    pub fn redirect_url(&self) -> &str {
        &self.redirect_url
    }
}

/// OAuth 登录公开和管理员边界可稳定映射的错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OAuthLoginError {
    #[error("OAuth 登录输入无效")]
    InvalidInput,
    #[error("OAuth 登录管理权限不足")]
    Forbidden,
    #[error("OAuth 登录尚未配置")]
    Unavailable,
    #[error("OAuth 登录凭据已拒绝")]
    Rejected,
    #[error("OAuth 登录设置发生并发更新")]
    ConcurrentUpdate,
    #[error("OAuth Provider 请求失败")]
    ProviderUnavailable,
    #[error("OAuth 登录内部失败")]
    Internal,
}

pub type OAuthLoginServiceFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, OAuthLoginError>> + Send + 'a>>;

/// 用户登录 OAuth 的对象安全应用端口。
pub trait OAuthLoginService: Send + Sync {
    fn public_providers(&self) -> OAuthLoginServiceFuture<'_, Vec<PublicOAuthLoginProvider>>;

    fn begin<'a>(&'a self, provider: &'a str) -> OAuthLoginServiceFuture<'a, OAuthLoginStart>;

    fn complete<'a>(
        &'a self,
        provider: &'a str,
        state: String,
        code: String,
    ) -> OAuthLoginServiceFuture<'a, OAuthLoginCallbackResult>;

    fn cancel<'a>(
        &'a self,
        provider: &'a str,
        state: String,
    ) -> OAuthLoginServiceFuture<'a, OAuthLoginCallbackResult>;

    fn callback_failure(&self) -> OAuthLoginServiceFuture<'_, OAuthLoginCallbackResult>;

    fn exchange_ticket(&self, ticket: String) -> OAuthLoginServiceFuture<'_, UserId>;

    fn admin_settings<'a>(
        &'a self,
        principal: SessionPrincipal,
        provider: &'a str,
    ) -> OAuthLoginServiceFuture<'a, AdminOAuthLoginProviderSettings>;

    fn update_admin_settings<'a>(
        &'a self,
        principal: SessionPrincipal,
        provider: &'a str,
        command: AdminOAuthLoginProviderSettingsCommand,
    ) -> OAuthLoginServiceFuture<'a, AdminOAuthLoginProviderSettings>;
}

/// 使用数据库单次事务、系统密钥和受控 HTTP Client 的用户登录 OAuth 适配器。
pub struct DatabaseOAuthLoginService {
    repository: OAuthLoginRepository,
    custom_provider_repository: CustomOAuth2ProviderRepository,
    custom_login_repository: CustomOAuth2LoginRepository,
    site_settings: SiteSettingsRepository,
    cipher: SystemSecretCipher,
    oauth_client: OAuthLoginClient,
}

impl DatabaseOAuthLoginService {
    #[must_use]
    pub fn new(
        repository: OAuthLoginRepository,
        custom_provider_repository: CustomOAuth2ProviderRepository,
        custom_login_repository: CustomOAuth2LoginRepository,
        site_settings: SiteSettingsRepository,
        cipher: SystemSecretCipher,
        oauth_client: OAuthLoginClient,
    ) -> Self {
        Self {
            repository,
            custom_provider_repository,
            custom_login_repository,
            site_settings,
            cipher,
            oauth_client,
        }
    }

    async fn public_providers_inner(
        &self,
    ) -> Result<Vec<PublicOAuthLoginProvider>, OAuthLoginError> {
        let mut providers = Vec::new();
        for profile in LoginOAuthProvider::ALL {
            let provider = self
                .repository
                .provider(profile.id())
                .await
                .map_err(map_repository_error)?;
            if provider.available() {
                match self.redirect_uri(profile).await {
                    Ok(_) => providers.push(PublicOAuthLoginProvider::new(
                        profile.id(),
                        profile.display_name(),
                    )),
                    Err(OAuthLoginError::Unavailable) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        for record in self
            .custom_provider_repository
            .providers()
            .await
            .map_err(map_custom_provider_repository_error)?
        {
            if record.available() {
                match self.custom_redirect_uri(record.provider_key()).await {
                    Ok(_) => providers.push(PublicOAuthLoginProvider::new(
                        record.provider_key(),
                        record.display_name(),
                    )),
                    Err(OAuthLoginError::Unavailable) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(providers)
    }

    async fn begin_inner(&self, provider: &str) -> Result<OAuthLoginStart, OAuthLoginError> {
        if LoginOAuthProvider::from_id(provider).is_some() {
            return self.begin_builtin(provider).await;
        }
        self.begin_custom(provider).await
    }

    async fn begin_builtin(&self, provider: &str) -> Result<OAuthLoginStart, OAuthLoginError> {
        let profile = require_provider(provider)?;
        let settings = self.available_provider(profile).await?;
        let client_id = settings.client_id().ok_or(OAuthLoginError::Unavailable)?;
        let redirect_uri = self.redirect_uri(profile).await?;
        let verifier = random_token()?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let verifier_secret =
            PlainSystemSecret::new(verifier.to_string()).map_err(|_| OAuthLoginError::Internal)?;
        let envelope = self
            .cipher
            .encrypt(SystemSecretKind::OAuthLoginState, &verifier_secret)
            .map_err(|_| OAuthLoginError::Internal)?;
        let state = encode_state_envelope(&envelope);
        let now = current_timestamp()?;
        let expires_at = now
            .checked_add(STATE_TTL_SECONDS)
            .ok_or(OAuthLoginError::Internal)?;
        self.repository
            .create_state(
                provider,
                &digest("oauth-login-state-v1", &state),
                expires_at,
            )
            .await
            .map_err(map_repository_error)?;

        if profile.is_oidc() {
            let issuer = settings.issuer_url().ok_or(OAuthLoginError::Unavailable)?;
            let authorization_url = self
                .oauth_client
                .oidc_authorization_url(
                    profile,
                    issuer,
                    client_id,
                    &redirect_uri,
                    &state,
                    &challenge,
                )
                .await
                .map_err(map_oauth_client_error)?;
            return Ok(OAuthLoginStart { authorization_url });
        }
        Ok(OAuthLoginStart {
            authorization_url: provider_authorization_url(
                profile,
                client_id,
                &redirect_uri,
                &state,
                &challenge,
            )?,
        })
    }

    async fn begin_custom(&self, provider: &str) -> Result<OAuthLoginStart, OAuthLoginError> {
        let key = custom_provider_key(provider)?;
        let (record, config) = self.available_custom_provider(&key).await?;
        let redirect_uri = self.custom_redirect_uri(key.as_str()).await?;
        let verifier = random_token()?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let verifier_secret =
            PlainSystemSecret::new(verifier.to_string()).map_err(|_| OAuthLoginError::Internal)?;
        let envelope = self
            .cipher
            .encrypt(SystemSecretKind::OAuthLoginState, &verifier_secret)
            .map_err(|_| OAuthLoginError::Internal)?;
        let state = encode_state_envelope(&envelope);
        let now = current_timestamp()?;
        let expires_at = now
            .checked_add(STATE_TTL_SECONDS)
            .ok_or(OAuthLoginError::Internal)?;
        self.custom_login_repository
            .create_state(
                key.as_str(),
                record.version(),
                &digest("custom-oauth2-login-state-v1", &state),
                expires_at,
            )
            .await
            .map_err(map_custom_login_repository_error)?;
        let authorization_url =
            OAuthLoginClient::custom_authorization_url(&config, &redirect_uri, &state, &challenge)
                .map_err(map_oauth_client_error)?;
        Ok(OAuthLoginStart { authorization_url })
    }

    async fn complete_inner(
        &self,
        provider: &str,
        state: String,
        code: String,
    ) -> Result<OAuthLoginCallbackResult, OAuthLoginError> {
        if LoginOAuthProvider::from_id(provider).is_some() {
            return self.complete_builtin(provider, state, code).await;
        }
        self.complete_custom(provider, state, code).await
    }

    async fn complete_builtin(
        &self,
        provider: &str,
        state: String,
        code: String,
    ) -> Result<OAuthLoginCallbackResult, OAuthLoginError> {
        let profile = require_provider(provider)?;
        validate_state(&state)?;
        let code = Zeroizing::new(code);
        validate_code(code.as_str())?;
        let now = current_timestamp()?;
        let claim = self
            .repository
            .claim_state(provider, &digest("oauth-login-state-v1", &state), now)
            .await
            .map_err(map_repository_error)?;
        let envelope = decode_state_envelope(&state)?;
        let verifier = self
            .cipher
            .decrypt(SystemSecretKind::OAuthLoginState, &envelope)
            .map_err(|_| OAuthLoginError::Rejected)?;
        let settings = self.available_provider(profile).await?;
        let client_id = settings.client_id().ok_or(OAuthLoginError::Unavailable)?;
        let client_secret = self.decrypt_client_secret(profile, &settings)?;
        let redirect_uri = self.redirect_uri(profile).await?;
        let identity = if profile.is_oidc() {
            let issuer = settings.issuer_url().ok_or(OAuthLoginError::Unavailable)?;
            self.oauth_client
                .exchange_oidc_identity(
                    profile,
                    issuer,
                    client_id,
                    &client_secret,
                    code.as_str(),
                    verifier.expose_secret(),
                    &redirect_uri,
                )
                .await
                .map_err(map_oauth_client_error)?
        } else {
            self.oauth_client
                .exchange_identity(
                    profile,
                    client_id,
                    &client_secret,
                    code.as_str(),
                    profile.uses_pkce().then_some(verifier.expose_secret()),
                    &redirect_uri,
                )
                .await
                .map_err(map_oauth_client_error)?
        };
        drop(client_secret);
        drop(verifier);
        let ticket = random_token()?;
        let ticket_expires_at = now
            .checked_add(TICKET_TTL_SECONDS)
            .ok_or(OAuthLoginError::Internal)?;
        let local_username = local_username(profile, identity.username(), identity.subject());
        match self
            .repository
            .complete_identity(
                claim,
                provider,
                identity.subject(),
                &local_username,
                &digest("oauth-login-ticket-v1", &ticket),
                ticket_expires_at,
                now,
            )
            .await
            .map_err(map_repository_error)?
        {
            OAuthIdentityCompletion::Issued(_) => {}
            OAuthIdentityCompletion::Rejected => return Err(OAuthLoginError::Rejected),
        }
        Ok(OAuthLoginCallbackResult {
            redirect_url: self.frontend_callback_url(Some(&ticket), None).await?,
        })
    }

    async fn complete_custom(
        &self,
        provider: &str,
        state: String,
        code: String,
    ) -> Result<OAuthLoginCallbackResult, OAuthLoginError> {
        let key = custom_provider_key(provider)?;
        validate_state(&state)?;
        let code = Zeroizing::new(code);
        validate_code(code.as_str())?;
        let now = current_timestamp()?;
        // 先永久领取 state，再读取当前配置并执行任何外部网络请求。
        let claim = self
            .custom_login_repository
            .claim_state(
                key.as_str(),
                &digest("custom-oauth2-login-state-v1", &state),
                now,
            )
            .await
            .map_err(map_custom_login_repository_error)?;
        let envelope = decode_state_envelope(&state)?;
        let verifier = self
            .cipher
            .decrypt(SystemSecretKind::OAuthLoginState, &envelope)
            .map_err(|_| OAuthLoginError::Rejected)?;
        let (record, config) = self.available_custom_provider(&key).await?;
        if record.version() != claim.configuration_version() {
            return Err(OAuthLoginError::Rejected);
        }
        let client_secret = self
            .cipher
            .decrypt_custom_oauth2_client_secret(
                &key,
                record.client_secret().ok_or(OAuthLoginError::Unavailable)?,
            )
            .map_err(|_| OAuthLoginError::Internal)?;
        let redirect_uri = self.custom_redirect_uri(key.as_str()).await?;
        let identity = self
            .oauth_client
            .exchange_custom_identity(
                &config,
                &client_secret,
                code.as_str(),
                verifier.expose_secret(),
                &redirect_uri,
            )
            .await
            .map_err(map_oauth_client_error)?;
        drop(client_secret);
        drop(verifier);
        let ticket = random_token()?;
        let ticket_expires_at = now
            .checked_add(TICKET_TTL_SECONDS)
            .ok_or(OAuthLoginError::Internal)?;
        let local_username = custom_local_username(key.as_str(), identity.subject());
        match self
            .custom_login_repository
            .complete_identity(
                claim,
                key.as_str(),
                identity.subject(),
                &local_username,
                &digest("custom-oauth2-login-ticket-v1", &ticket),
                ticket_expires_at,
                now,
            )
            .await
            .map_err(map_custom_login_repository_error)?
        {
            CustomOAuth2IdentityCompletion::Issued(_) => {}
            CustomOAuth2IdentityCompletion::Rejected => return Err(OAuthLoginError::Rejected),
        }
        Ok(OAuthLoginCallbackResult {
            redirect_url: self.frontend_callback_url(Some(&ticket), None).await?,
        })
    }

    async fn cancel_inner(
        &self,
        provider: &str,
        state: String,
    ) -> Result<OAuthLoginCallbackResult, OAuthLoginError> {
        validate_state(&state)?;
        if LoginOAuthProvider::from_id(provider).is_some() {
            require_provider(provider)?;
            self.repository
                .claim_state(
                    provider,
                    &digest("oauth-login-state-v1", &state),
                    current_timestamp()?,
                )
                .await
                .map_err(map_repository_error)?;
        } else {
            let key = custom_provider_key(provider)?;
            self.custom_login_repository
                .claim_state(
                    key.as_str(),
                    &digest("custom-oauth2-login-state-v1", &state),
                    current_timestamp()?,
                )
                .await
                .map_err(map_custom_login_repository_error)?;
        }
        Ok(OAuthLoginCallbackResult {
            redirect_url: self.frontend_callback_url(None, Some("cancelled")).await?,
        })
    }

    async fn callback_failure_inner(&self) -> Result<OAuthLoginCallbackResult, OAuthLoginError> {
        Ok(OAuthLoginCallbackResult {
            redirect_url: self.frontend_callback_url(None, Some("failed")).await?,
        })
    }

    async fn exchange_ticket_inner(&self, ticket: String) -> Result<UserId, OAuthLoginError> {
        validate_ticket(&ticket)?;
        let builtin = self
            .repository
            .consume_ticket(
                &digest("oauth-login-ticket-v1", &ticket),
                current_timestamp()?,
            )
            .await;
        match builtin {
            Ok(user_id) => Ok(user_id),
            Err(OAuthLoginRepositoryError::Rejected) => self
                .custom_login_repository
                .consume_ticket(
                    &digest("custom-oauth2-login-ticket-v1", &ticket),
                    current_timestamp()?,
                )
                .await
                .map_err(map_custom_login_repository_error),
            Err(error) => Err(map_repository_error(error)),
        }
    }

    async fn admin_settings_inner(
        &self,
        principal: SessionPrincipal,
        provider: &str,
    ) -> Result<AdminOAuthLoginProviderSettings, OAuthLoginError> {
        require_admin(principal)?;
        let profile = require_provider(provider)?;
        let callback_url = match self.redirect_uri(profile).await {
            Ok(callback_url) => Some(callback_url),
            Err(OAuthLoginError::Unavailable) => None,
            Err(error) => return Err(error),
        };
        self.repository
            .provider(profile.id())
            .await
            .map(|record| AdminOAuthLoginProviderSettings::from_record(record, callback_url))
            .map_err(map_repository_error)
    }

    async fn update_admin_settings_inner(
        &self,
        principal: SessionPrincipal,
        provider: &str,
        command: AdminOAuthLoginProviderSettingsCommand,
    ) -> Result<AdminOAuthLoginProviderSettings, OAuthLoginError> {
        require_admin(principal)?;
        let profile = require_provider(provider)?;
        let callback_url = match self.redirect_uri(profile).await {
            Ok(callback_url) => Some(callback_url),
            Err(OAuthLoginError::Unavailable) if !command.enabled => None,
            Err(error) => return Err(error),
        };
        let write = command.into_record(&self.cipher, profile)?;
        self.repository
            .update_provider(profile.id(), write)
            .await
            .map(|record| AdminOAuthLoginProviderSettings::from_record(record, callback_url))
            .map_err(map_repository_error)
    }

    async fn available_provider(
        &self,
        profile: LoginOAuthProvider,
    ) -> Result<OAuthLoginProviderRecord, OAuthLoginError> {
        let provider = self
            .repository
            .provider(profile.id())
            .await
            .map_err(map_repository_error)?;
        if provider.available() {
            Ok(provider)
        } else {
            Err(OAuthLoginError::Unavailable)
        }
    }

    async fn available_custom_provider(
        &self,
        provider_key: &CustomOAuth2ProviderKey,
    ) -> Result<(CustomOAuth2ProviderRecord, CustomOAuth2ProviderConfig), OAuthLoginError> {
        let record = self
            .custom_provider_repository
            .provider(provider_key.as_str())
            .await
            .map_err(map_custom_provider_repository_error)?
            .ok_or(OAuthLoginError::Unavailable)?;
        if !record.available() {
            return Err(OAuthLoginError::Unavailable);
        }
        let endpoints = CustomOAuth2EndpointBundle::new(
            record.authorization_endpoint().to_owned(),
            record.token_endpoint().to_owned(),
            record.userinfo_endpoint().to_owned(),
            record.scope().to_owned(),
            record.subject_field().to_owned(),
        )
        .map_err(|_| OAuthLoginError::Internal)?;
        let config = CustomOAuth2ProviderConfig::new(
            record.provider_key().to_owned(),
            record.display_name().to_owned(),
            record.client_id().to_owned(),
            endpoints,
            record.enabled(),
        )
        .map_err(|_| OAuthLoginError::Internal)?;
        Ok((record, config))
    }

    fn decrypt_client_secret(
        &self,
        profile: LoginOAuthProvider,
        provider: &OAuthLoginProviderRecord,
    ) -> Result<af_account::DecryptedSystemSecret, OAuthLoginError> {
        self.cipher
            .decrypt(
                client_secret_kind(profile),
                provider
                    .client_secret()
                    .ok_or(OAuthLoginError::Unavailable)?,
            )
            .map_err(|_| OAuthLoginError::Internal)
    }

    async fn redirect_uri(&self, profile: LoginOAuthProvider) -> Result<String, OAuthLoginError> {
        let settings = self
            .site_settings
            .settings()
            .await
            .map_err(|_| OAuthLoginError::Internal)?;
        let base = settings
            .public_base_url()
            .ok_or(OAuthLoginError::Unavailable)?;
        let base = Url::parse(base).map_err(|_| OAuthLoginError::Internal)?;
        base.join(&format!("/api/auth/oauth/{}/callback", profile.id()))
            .map(String::from)
            .map_err(|_| OAuthLoginError::Internal)
    }

    async fn custom_redirect_uri(&self, provider_key: &str) -> Result<String, OAuthLoginError> {
        let key = custom_provider_key(provider_key)?;
        let settings = self
            .site_settings
            .settings()
            .await
            .map_err(|_| OAuthLoginError::Internal)?;
        let base = settings
            .public_base_url()
            .ok_or(OAuthLoginError::Unavailable)?;
        let base = Url::parse(base).map_err(|_| OAuthLoginError::Internal)?;
        base.join(&format!("/api/auth/oauth/custom/{}/callback", key.as_str()))
            .map(String::from)
            .map_err(|_| OAuthLoginError::Internal)
    }

    async fn frontend_callback_url(
        &self,
        ticket: Option<&str>,
        error: Option<&str>,
    ) -> Result<String, OAuthLoginError> {
        let settings = self
            .site_settings
            .settings()
            .await
            .map_err(|_| OAuthLoginError::Internal)?;
        let mut base = Url::parse(
            settings
                .public_base_url()
                .ok_or(OAuthLoginError::Unavailable)?,
        )
        .map_err(|_| OAuthLoginError::Internal)?;
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        if let Some(ticket) = ticket {
            query.append_pair("ticket", ticket);
        }
        if let Some(error) = error {
            query.append_pair("error", error);
        }
        let query = query.finish();
        base.set_fragment(Some(&format!("/oauth/callback?{query}")));
        Ok(base.into())
    }
}

impl OAuthLoginService for DatabaseOAuthLoginService {
    fn public_providers(&self) -> OAuthLoginServiceFuture<'_, Vec<PublicOAuthLoginProvider>> {
        Box::pin(self.public_providers_inner())
    }

    fn begin<'a>(&'a self, provider: &'a str) -> OAuthLoginServiceFuture<'a, OAuthLoginStart> {
        Box::pin(self.begin_inner(provider))
    }

    fn complete<'a>(
        &'a self,
        provider: &'a str,
        state: String,
        code: String,
    ) -> OAuthLoginServiceFuture<'a, OAuthLoginCallbackResult> {
        Box::pin(self.complete_inner(provider, state, code))
    }

    fn cancel<'a>(
        &'a self,
        provider: &'a str,
        state: String,
    ) -> OAuthLoginServiceFuture<'a, OAuthLoginCallbackResult> {
        Box::pin(self.cancel_inner(provider, state))
    }

    fn callback_failure(&self) -> OAuthLoginServiceFuture<'_, OAuthLoginCallbackResult> {
        Box::pin(self.callback_failure_inner())
    }

    fn exchange_ticket(&self, ticket: String) -> OAuthLoginServiceFuture<'_, UserId> {
        Box::pin(self.exchange_ticket_inner(ticket))
    }

    fn admin_settings<'a>(
        &'a self,
        principal: SessionPrincipal,
        provider: &'a str,
    ) -> OAuthLoginServiceFuture<'a, AdminOAuthLoginProviderSettings> {
        Box::pin(self.admin_settings_inner(principal, provider))
    }

    fn update_admin_settings<'a>(
        &'a self,
        principal: SessionPrincipal,
        provider: &'a str,
        command: AdminOAuthLoginProviderSettingsCommand,
    ) -> OAuthLoginServiceFuture<'a, AdminOAuthLoginProviderSettings> {
        Box::pin(self.update_admin_settings_inner(principal, provider, command))
    }
}

impl fmt::Debug for DatabaseOAuthLoginService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOAuthLoginService(<已脱敏>)")
    }
}

fn encode_state_envelope(envelope: &EncryptedCredentialEnvelope) -> String {
    format!(
        "ols1.{}.{}.{}",
        URL_SAFE_NO_PAD.encode(envelope.key_id()),
        URL_SAFE_NO_PAD.encode(envelope.nonce()),
        URL_SAFE_NO_PAD.encode(envelope.ciphertext())
    )
}

fn decode_state_envelope(state: &str) -> Result<EncryptedCredentialEnvelope, OAuthLoginError> {
    let mut parts = state.split('.');
    if parts.next() != Some("ols1") {
        return Err(OAuthLoginError::Rejected);
    }
    let key_id = parts.next().ok_or(OAuthLoginError::Rejected)?;
    let nonce = parts.next().ok_or(OAuthLoginError::Rejected)?;
    let ciphertext = parts.next().ok_or(OAuthLoginError::Rejected)?;
    if parts.next().is_some() {
        return Err(OAuthLoginError::Rejected);
    }
    let mut key_id = URL_SAFE_NO_PAD
        .decode(key_id)
        .map_err(|_| OAuthLoginError::Rejected)?;
    let key_id = String::from_utf8(std::mem::take(&mut key_id)).map_err(|error| {
        let mut bytes = error.into_bytes();
        bytes.zeroize();
        OAuthLoginError::Rejected
    })?;
    let mut nonce_bytes = [0_u8; af_db::CREDENTIAL_ENVELOPE_NONCE_BYTES];
    let nonce_length = URL_SAFE_NO_PAD
        .decode_slice(nonce, &mut nonce_bytes)
        .map_err(|_| OAuthLoginError::Rejected)?;
    if nonce_length != nonce_bytes.len() {
        return Err(OAuthLoginError::Rejected);
    }
    let ciphertext = URL_SAFE_NO_PAD
        .decode(ciphertext)
        .map_err(|_| OAuthLoginError::Rejected)?;
    EncryptedCredentialEnvelope::new(key_id, nonce_bytes, ciphertext)
        .map_err(|_| OAuthLoginError::Rejected)
}

fn digest(domain: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    let bytes = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn random_token() -> Result<Zeroizing<String>, OAuthLoginError> {
    let mut entropy = [0_u8; 32];
    getrandom::fill(&mut entropy).map_err(|_| OAuthLoginError::Internal)?;
    let token = Zeroizing::new(URL_SAFE_NO_PAD.encode(entropy));
    entropy.zeroize();
    Ok(token)
}

fn local_username(profile: LoginOAuthProvider, login: &str, subject: &str) -> String {
    let normalized: String = login
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(39)
        .collect();
    let suffix = digest("oauth-login-username-v1", subject);
    format!(
        "{}_{}_{suffix}",
        profile.username_prefix(),
        if normalized.is_empty() {
            "user"
        } else {
            &normalized
        }
    )
    .chars()
    .take(64)
    .collect()
}

fn custom_local_username(provider_key: &str, subject: &str) -> String {
    let prefix: String = provider_key
        .trim_start_matches("custom_")
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(20)
        .collect();
    let suffix = digest(
        "custom-oauth2-login-username-v1",
        &format!("{provider_key}\0{subject}"),
    );
    format!(
        "custom_{}_{}",
        if prefix.is_empty() { "user" } else { &prefix },
        &suffix[..32]
    )
}

fn validate_state(state: &str) -> Result<(), OAuthLoginError> {
    if state.len() <= 4_096
        && state.starts_with("ols1.")
        && state
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(OAuthLoginError::Rejected)
    }
}

fn validate_code(code: &str) -> Result<(), OAuthLoginError> {
    if !code.is_empty()
        && code.len() <= 4_096
        && !code.chars().any(|character| character.is_control())
    {
        Ok(())
    } else {
        Err(OAuthLoginError::Rejected)
    }
}

fn validate_ticket(ticket: &str) -> Result<(), OAuthLoginError> {
    if ticket.len() == 43
        && ticket
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(OAuthLoginError::Rejected)
    }
}

fn normalize_client_id(client_id: Option<String>) -> Result<Option<String>, OAuthLoginError> {
    let client_id = client_id.and_then(|value| {
        let trimmed = value.trim().to_owned();
        (!trimmed.is_empty()).then_some(trimmed)
    });
    if client_id.as_deref().is_some_and(|value| {
        value.len() > 255
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    }) {
        Err(OAuthLoginError::InvalidInput)
    } else {
        Ok(client_id)
    }
}

fn require_provider(provider: &str) -> Result<LoginOAuthProvider, OAuthLoginError> {
    LoginOAuthProvider::from_id(provider).ok_or(OAuthLoginError::InvalidInput)
}

fn custom_provider_key(provider: &str) -> Result<CustomOAuth2ProviderKey, OAuthLoginError> {
    CustomOAuth2ProviderKey::new(provider.to_owned()).map_err(|_| OAuthLoginError::InvalidInput)
}

fn client_secret_kind(provider: LoginOAuthProvider) -> SystemSecretKind {
    match provider {
        LoginOAuthProvider::GitHub => SystemSecretKind::OAuthLoginGithubClientSecret,
        LoginOAuthProvider::Discord => SystemSecretKind::OAuthLoginDiscordClientSecret,
        LoginOAuthProvider::Oidc => SystemSecretKind::OAuthLoginOidcClientSecret,
        LoginOAuthProvider::LinuxDo => SystemSecretKind::OAuthLoginLinuxDoClientSecret,
        LoginOAuthProvider::WeChat => SystemSecretKind::OAuthLoginWechatClientSecret,
        LoginOAuthProvider::Telegram => SystemSecretKind::OAuthLoginTelegramClientSecret,
        LoginOAuthProvider::Google => SystemSecretKind::OAuthLoginGoogleClientSecret,
    }
}

fn normalize_issuer_url(value: Option<String>) -> Result<Option<String>, OAuthLoginError> {
    let value = value.and_then(|value| {
        let value = value.trim().trim_end_matches('/').to_owned();
        (!value.is_empty()).then_some(value)
    });
    if value.as_deref().is_some_and(|value| {
        let Ok(url) = Url::parse(value) else {
            return true;
        };
        value.len() > 2048
            || url.scheme() != "https"
            || url.host_str().is_none()
            || url.username() != ""
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.port().is_some()
            || value.chars().any(char::is_control)
            || is_blocked_issuer_host(url.host_str().unwrap_or_default())
    }) {
        return Err(OAuthLoginError::InvalidInput);
    }
    Ok(value)
}

fn validate_command_issuer(
    provider: LoginOAuthProvider,
    issuer: Option<&str>,
    enabled: bool,
) -> Result<(), OAuthLoginError> {
    let valid = match provider {
        LoginOAuthProvider::Oidc => !enabled || issuer.is_some(),
        LoginOAuthProvider::LinuxDo => {
            issuer.is_none() || issuer == Some("https://connect.linux.do")
        }
        LoginOAuthProvider::Telegram => {
            issuer.is_none() || issuer == Some("https://oauth.telegram.org")
        }
        LoginOAuthProvider::Google => {
            issuer.is_none() || issuer == Some("https://accounts.google.com")
        }
        LoginOAuthProvider::GitHub | LoginOAuthProvider::Discord | LoginOAuthProvider::WeChat => {
            issuer.is_none()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(OAuthLoginError::InvalidInput)
    }
}

fn provider_authorization_url(
    profile: LoginOAuthProvider,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Result<String, OAuthLoginError> {
    let mut url = Url::parse(profile.authorization_url()).map_err(|_| OAuthLoginError::Internal)?;
    if profile == LoginOAuthProvider::WeChat {
        url.query_pairs_mut()
            .append_pair("appid", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", "snsapi_login")
            .append_pair("state", state);
        url.set_fragment(Some("wechat_redirect"));
    } else {
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state);
        if matches!(profile, LoginOAuthProvider::Discord) {
            url.query_pairs_mut()
                .append_pair("response_type", "code")
                .append_pair("scope", "identify");
        }
    }
    // Discord 官方授权码文档未声明 PKCE；GitHub 继续强制 S256。
    if profile.uses_pkce() {
        url.query_pairs_mut()
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
    }
    Ok(url.into())
}

fn is_blocked_issuer_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized == "localhost.localdomain"
        || normalized.parse::<std::net::IpAddr>().is_ok()
}

fn require_admin(principal: SessionPrincipal) -> Result<(), OAuthLoginError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(OAuthLoginError::Forbidden)
    }
}

fn current_timestamp() -> Result<u64, OAuthLoginError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| OAuthLoginError::Internal)
}

fn map_repository_error(error: OAuthLoginRepositoryError) -> OAuthLoginError {
    match error {
        OAuthLoginRepositoryError::InvalidInput => OAuthLoginError::InvalidInput,
        OAuthLoginRepositoryError::ConcurrentUpdate => OAuthLoginError::ConcurrentUpdate,
        OAuthLoginRepositoryError::Rejected | OAuthLoginRepositoryError::Conflict => {
            OAuthLoginError::Rejected
        }
        OAuthLoginRepositoryError::Query
        | OAuthLoginRepositoryError::Timeout
        | OAuthLoginRepositoryError::Invariant => OAuthLoginError::Internal,
    }
}

fn map_custom_provider_repository_error(
    error: CustomOAuth2ProviderRepositoryError,
) -> OAuthLoginError {
    match error {
        CustomOAuth2ProviderRepositoryError::InvalidInput => OAuthLoginError::InvalidInput,
        CustomOAuth2ProviderRepositoryError::ConcurrentUpdate
        | CustomOAuth2ProviderRepositoryError::Query
        | CustomOAuth2ProviderRepositoryError::Timeout
        | CustomOAuth2ProviderRepositoryError::Invariant => OAuthLoginError::Internal,
    }
}

fn map_custom_login_repository_error(error: CustomOAuth2LoginRepositoryError) -> OAuthLoginError {
    match error {
        CustomOAuth2LoginRepositoryError::InvalidInput => OAuthLoginError::InvalidInput,
        CustomOAuth2LoginRepositoryError::Rejected | CustomOAuth2LoginRepositoryError::Conflict => {
            OAuthLoginError::Rejected
        }
        CustomOAuth2LoginRepositoryError::Query
        | CustomOAuth2LoginRepositoryError::Timeout
        | CustomOAuth2LoginRepositoryError::Invariant => OAuthLoginError::Internal,
    }
}

fn map_oauth_client_error(error: OAuthLoginClientError) -> OAuthLoginError {
    match error {
        OAuthLoginClientError::ClientUnavailable => OAuthLoginError::Internal,
        OAuthLoginClientError::ProviderUnavailable => OAuthLoginError::ProviderUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_command_rejects_ambiguous_secret_updates_and_redacts_debug() {
        assert_eq!(
            AdminOAuthLoginProviderSettingsCommand::new(
                1,
                true,
                Some("client-id".to_owned()),
                None,
                Some("secret-value".to_owned()),
                true,
            )
            .unwrap_err(),
            OAuthLoginError::InvalidInput
        );
        assert_eq!(
            AdminOAuthLoginProviderSettingsCommand::new(1, true, None, None, None, false)
                .unwrap_err(),
            OAuthLoginError::InvalidInput
        );
        let command = AdminOAuthLoginProviderSettingsCommand::new(
            1,
            false,
            Some("client-id".to_owned()),
            None,
            Some("secret-value".to_owned()),
            false,
        )
        .unwrap();
        let debug = format!("{command:?}");
        assert!(!debug.contains("client-id"));
        assert!(!debug.contains("secret-value"));

        assert_eq!(
            validate_command_issuer(
                LoginOAuthProvider::LinuxDo,
                Some("https://attacker.example.com"),
                false,
            ),
            Err(OAuthLoginError::InvalidInput)
        );
        assert_eq!(
            validate_command_issuer(LoginOAuthProvider::Oidc, None, true),
            Err(OAuthLoginError::InvalidInput)
        );
        assert_eq!(
            validate_command_issuer(
                LoginOAuthProvider::WeChat,
                Some("https://attacker.example.com"),
                false,
            ),
            Err(OAuthLoginError::InvalidInput)
        );
    }

    #[test]
    fn callback_material_and_local_username_keep_closed_formats() {
        assert!(validate_ticket(&"A".repeat(43)).is_ok());
        assert_eq!(
            validate_ticket(&"A".repeat(42)),
            Err(OAuthLoginError::Rejected)
        );
        assert_eq!(
            validate_state("https://attacker.invalid"),
            Err(OAuthLoginError::Rejected)
        );
        let username = local_username(LoginOAuthProvider::GitHub, "octocat", "583231");
        assert!(username.starts_with("gh_octocat_"));
        assert!(username.len() <= 64);
        assert!(!username.contains("583231"));
        let discord_username = local_username(
            LoginOAuthProvider::Discord,
            "discord_user",
            "80351110224678912",
        );
        assert!(discord_username.starts_with("dc_discord_user_"));
        assert!(discord_username.len() <= 64);
        let custom_username = custom_local_username("custom_enterprise", "external-subject-1");
        assert!(custom_username.starts_with("custom_enterprise_"));
        assert!(custom_username.len() <= 64);
        assert!(!custom_username.contains("external-subject-1"));
        assert_eq!(
            custom_username,
            custom_local_username("custom_enterprise", "external-subject-1")
        );
        assert_ne!(
            custom_username,
            custom_local_username("custom_partner", "external-subject-1")
        );
        assert_eq!(require_provider("discord"), Ok(LoginOAuthProvider::Discord));
        assert_eq!(require_provider("wechat"), Ok(LoginOAuthProvider::WeChat));
        assert_eq!(
            require_provider("custom"),
            Err(OAuthLoginError::InvalidInput)
        );
        assert_eq!(
            custom_provider_key("github"),
            Err(OAuthLoginError::InvalidInput)
        );
    }

    #[test]
    fn custom_repository_errors_keep_public_security_classification() {
        assert_eq!(
            map_custom_provider_repository_error(CustomOAuth2ProviderRepositoryError::InvalidInput),
            OAuthLoginError::InvalidInput
        );
        assert_eq!(
            map_custom_login_repository_error(CustomOAuth2LoginRepositoryError::Rejected),
            OAuthLoginError::Rejected
        );
        assert_eq!(
            map_custom_login_repository_error(CustomOAuth2LoginRepositoryError::Conflict),
            OAuthLoginError::Rejected
        );
        assert_eq!(
            map_custom_login_repository_error(CustomOAuth2LoginRepositoryError::Query),
            OAuthLoginError::Internal
        );
    }

    #[test]
    fn wechat_authorization_url_uses_qr_login_contract() {
        let authorization_url = provider_authorization_url(
            LoginOAuthProvider::WeChat,
            "wx-app-id",
            "https://app.example.com/api/auth/oauth/wechat/callback",
            "opaque-state",
            "unused-challenge",
        )
        .unwrap();
        let url = Url::parse(&authorization_url).unwrap();
        let query = url
            .query_pairs()
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("open.weixin.qq.com"));
        assert_eq!(url.path(), "/connect/qrconnect");
        assert_eq!(url.fragment(), Some("wechat_redirect"));
        assert_eq!(query.len(), 5);
        assert_eq!(query.get("appid").map(String::as_str), Some("wx-app-id"));
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some("https://app.example.com/api/auth/oauth/wechat/callback")
        );
        assert_eq!(query.get("response_type").map(String::as_str), Some("code"));
        assert_eq!(query.get("scope").map(String::as_str), Some("snsapi_login"));
        assert_eq!(query.get("state").map(String::as_str), Some("opaque-state"));
        assert!(!query.contains_key("code_challenge"));
    }

    #[test]
    fn provider_and_secret_responses_do_not_expose_sensitive_values() {
        let provider = PublicOAuthLoginProvider::new("github", "GitHub");
        assert_eq!(provider.id(), "github");
        assert_eq!(provider.display_name(), "GitHub");
        let settings = AdminOAuthLoginProviderSettings {
            enabled: true,
            client_id: Some("client-id".to_owned()),
            issuer_url: None,
            client_secret_configured: true,
            callback_url: Some("https://example.com/api/auth/oauth/github/callback".to_owned()),
            version: 2,
        };
        let debug = format!("{settings:?}");
        assert!(!debug.contains("secret-value"));
        assert!(settings.client_secret_configured());
    }

    #[test]
    fn browser_redirect_results_redact_state_and_ticket() {
        let start = OAuthLoginStart::new(
            "https://github.com/login/oauth/authorize?state=sensitive-state".to_owned(),
        );
        let callback = OAuthLoginCallbackResult::new(
            "https://example.com/#/auth/oauth/callback?ticket=sensitive-ticket".to_owned(),
        );

        let start_debug = format!("{start:?}");
        let callback_debug = format!("{callback:?}");
        assert!(!start_debug.contains("sensitive-state"));
        assert!(!callback_debug.contains("sensitive-ticket"));
    }
}
