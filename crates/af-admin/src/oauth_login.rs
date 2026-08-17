use std::{fmt, future::Future, pin::Pin};

use af_account::{
    GitHubLoginClient, GitHubLoginClientError, PlainSystemSecret, SystemSecretCipher,
    SystemSecretKind,
};
use af_db::{
    EncryptedCredentialEnvelope, OAUTH_LOGIN_GITHUB_PROVIDER, OAuthIdentityCompletion,
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

const GITHUB_AUTHORIZATION_URL: &str = "https://github.com/login/oauth/authorize";
const STATE_TTL_SECONDS: u64 = 10 * 60;
const TICKET_TTL_SECONDS: u64 = 2 * 60;

/// 游客可见的已配置 OAuth 登录 Provider。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicOAuthLoginProvider {
    id: &'static str,
    display_name: &'static str,
}

impl PublicOAuthLoginProvider {
    /// 由受信 Provider 适配器构造公开摘要。
    #[must_use]
    pub const fn new(id: &'static str, display_name: &'static str) -> Self {
        Self { id, display_name }
    }

    #[must_use]
    pub const fn id(&self) -> &'static str {
        self.id
    }

    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        self.display_name
    }
}

/// 管理员可见的 GitHub OAuth App 配置，密钥仅投影配置事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminOAuthLoginProviderSettings {
    enabled: bool,
    client_id: Option<String>,
    client_secret_configured: bool,
    callback_url: Option<String>,
    version: i64,
}

impl AdminOAuthLoginProviderSettings {
    fn from_record(record: OAuthLoginProviderRecord, callback_url: Option<String>) -> Self {
        Self {
            enabled: record.enabled(),
            client_id: record.client_id().map(str::to_owned),
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

/// 管理员完整保存 GitHub OAuth App 设置的命令。
pub struct AdminOAuthLoginProviderSettingsCommand {
    expected_version: i64,
    enabled: bool,
    client_id: Option<String>,
    client_secret: Option<PlainSystemSecret>,
    clear_client_secret: bool,
}

impl AdminOAuthLoginProviderSettingsCommand {
    pub fn new(
        expected_version: i64,
        enabled: bool,
        client_id: Option<String>,
        client_secret: Option<String>,
        clear_client_secret: bool,
    ) -> Result<Self, OAuthLoginError> {
        let client_id = normalize_client_id(client_id)?;
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
            client_secret,
            clear_client_secret,
        })
    }

    fn into_record(
        self,
        cipher: &SystemSecretCipher,
    ) -> Result<OAuthLoginProviderWriteRecord, OAuthLoginError> {
        let secret = if self.clear_client_secret {
            OAuthLoginSecretUpdate::Clear
        } else if let Some(secret) = self.client_secret.as_ref() {
            OAuthLoginSecretUpdate::Replace(
                cipher
                    .encrypt(SystemSecretKind::OAuthLoginGithubClientSecret, secret)
                    .map_err(|_| OAuthLoginError::Internal)?,
            )
        } else {
            OAuthLoginSecretUpdate::Keep
        };
        Ok(OAuthLoginProviderWriteRecord::new(
            self.expected_version,
            self.enabled,
            self.client_id,
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

    fn admin_settings(
        &self,
        principal: SessionPrincipal,
    ) -> OAuthLoginServiceFuture<'_, AdminOAuthLoginProviderSettings>;

    fn update_admin_settings(
        &self,
        principal: SessionPrincipal,
        command: AdminOAuthLoginProviderSettingsCommand,
    ) -> OAuthLoginServiceFuture<'_, AdminOAuthLoginProviderSettings>;
}

/// 使用数据库单次事务、系统密钥和受控 HTTP Client 的 GitHub 首个适配器。
pub struct DatabaseOAuthLoginService {
    repository: OAuthLoginRepository,
    site_settings: SiteSettingsRepository,
    cipher: SystemSecretCipher,
    github_client: GitHubLoginClient,
}

impl DatabaseOAuthLoginService {
    #[must_use]
    pub fn new(
        repository: OAuthLoginRepository,
        site_settings: SiteSettingsRepository,
        cipher: SystemSecretCipher,
        github_client: GitHubLoginClient,
    ) -> Self {
        Self {
            repository,
            site_settings,
            cipher,
            github_client,
        }
    }

    async fn public_providers_inner(
        &self,
    ) -> Result<Vec<PublicOAuthLoginProvider>, OAuthLoginError> {
        let provider = self
            .repository
            .provider(OAUTH_LOGIN_GITHUB_PROVIDER)
            .await
            .map_err(map_repository_error)?;
        if !provider.available() {
            return Ok(Vec::new());
        }
        match self.redirect_uri().await {
            Ok(_) => Ok(vec![PublicOAuthLoginProvider {
                id: OAUTH_LOGIN_GITHUB_PROVIDER,
                display_name: "GitHub",
            }]),
            Err(OAuthLoginError::Unavailable) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }

    async fn begin_inner(&self, provider: &str) -> Result<OAuthLoginStart, OAuthLoginError> {
        require_github(provider)?;
        let settings = self.available_provider().await?;
        let client_id = settings.client_id().ok_or(OAuthLoginError::Unavailable)?;
        let redirect_uri = self.redirect_uri().await?;
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

        let mut url =
            Url::parse(GITHUB_AUTHORIZATION_URL).map_err(|_| OAuthLoginError::Internal)?;
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(OAuthLoginStart {
            authorization_url: url.into(),
        })
    }

    async fn complete_inner(
        &self,
        provider: &str,
        state: String,
        code: String,
    ) -> Result<OAuthLoginCallbackResult, OAuthLoginError> {
        require_github(provider)?;
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
        let settings = self.available_provider().await?;
        let client_id = settings.client_id().ok_or(OAuthLoginError::Unavailable)?;
        let client_secret = self.decrypt_client_secret(&settings)?;
        let redirect_uri = self.redirect_uri().await?;
        let identity = self
            .github_client
            .exchange_identity(
                client_id,
                &client_secret,
                code.as_str(),
                verifier.expose_secret(),
                &redirect_uri,
            )
            .await
            .map_err(map_github_client_error)?;
        drop(client_secret);
        drop(verifier);
        let ticket = random_token()?;
        let ticket_expires_at = now
            .checked_add(TICKET_TTL_SECONDS)
            .ok_or(OAuthLoginError::Internal)?;
        let local_username = local_username(identity.login(), identity.subject());
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

    async fn cancel_inner(
        &self,
        provider: &str,
        state: String,
    ) -> Result<OAuthLoginCallbackResult, OAuthLoginError> {
        require_github(provider)?;
        validate_state(&state)?;
        self.repository
            .claim_state(
                provider,
                &digest("oauth-login-state-v1", &state),
                current_timestamp()?,
            )
            .await
            .map_err(map_repository_error)?;
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
        self.repository
            .consume_ticket(
                &digest("oauth-login-ticket-v1", &ticket),
                current_timestamp()?,
            )
            .await
            .map_err(map_repository_error)
    }

    async fn admin_settings_inner(
        &self,
        principal: SessionPrincipal,
    ) -> Result<AdminOAuthLoginProviderSettings, OAuthLoginError> {
        require_admin(principal)?;
        let callback_url = match self.redirect_uri().await {
            Ok(callback_url) => Some(callback_url),
            Err(OAuthLoginError::Unavailable) => None,
            Err(error) => return Err(error),
        };
        self.repository
            .provider(OAUTH_LOGIN_GITHUB_PROVIDER)
            .await
            .map(|record| AdminOAuthLoginProviderSettings::from_record(record, callback_url))
            .map_err(map_repository_error)
    }

    async fn update_admin_settings_inner(
        &self,
        principal: SessionPrincipal,
        command: AdminOAuthLoginProviderSettingsCommand,
    ) -> Result<AdminOAuthLoginProviderSettings, OAuthLoginError> {
        require_admin(principal)?;
        let callback_url = match self.redirect_uri().await {
            Ok(callback_url) => Some(callback_url),
            Err(OAuthLoginError::Unavailable) if !command.enabled => None,
            Err(error) => return Err(error),
        };
        let write = command.into_record(&self.cipher)?;
        self.repository
            .update_provider(OAUTH_LOGIN_GITHUB_PROVIDER, write)
            .await
            .map(|record| AdminOAuthLoginProviderSettings::from_record(record, callback_url))
            .map_err(map_repository_error)
    }

    async fn available_provider(&self) -> Result<OAuthLoginProviderRecord, OAuthLoginError> {
        let provider = self
            .repository
            .provider(OAUTH_LOGIN_GITHUB_PROVIDER)
            .await
            .map_err(map_repository_error)?;
        if provider.available() {
            Ok(provider)
        } else {
            Err(OAuthLoginError::Unavailable)
        }
    }

    fn decrypt_client_secret(
        &self,
        provider: &OAuthLoginProviderRecord,
    ) -> Result<af_account::DecryptedSystemSecret, OAuthLoginError> {
        self.cipher
            .decrypt(
                SystemSecretKind::OAuthLoginGithubClientSecret,
                provider
                    .client_secret()
                    .ok_or(OAuthLoginError::Unavailable)?,
            )
            .map_err(|_| OAuthLoginError::Internal)
    }

    async fn redirect_uri(&self) -> Result<String, OAuthLoginError> {
        let settings = self
            .site_settings
            .settings()
            .await
            .map_err(|_| OAuthLoginError::Internal)?;
        let base = settings
            .public_base_url()
            .ok_or(OAuthLoginError::Unavailable)?;
        let base = Url::parse(base).map_err(|_| OAuthLoginError::Internal)?;
        base.join("/api/auth/oauth/github/callback")
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

    fn admin_settings(
        &self,
        principal: SessionPrincipal,
    ) -> OAuthLoginServiceFuture<'_, AdminOAuthLoginProviderSettings> {
        Box::pin(self.admin_settings_inner(principal))
    }

    fn update_admin_settings(
        &self,
        principal: SessionPrincipal,
        command: AdminOAuthLoginProviderSettingsCommand,
    ) -> OAuthLoginServiceFuture<'_, AdminOAuthLoginProviderSettings> {
        Box::pin(self.update_admin_settings_inner(principal, command))
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

fn local_username(login: &str, subject: &str) -> String {
    let normalized: String = login
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(39)
        .collect();
    let suffix = digest("oauth-login-username-v1", subject);
    format!(
        "gh_{}_{suffix}",
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

fn require_github(provider: &str) -> Result<(), OAuthLoginError> {
    if provider == OAUTH_LOGIN_GITHUB_PROVIDER {
        Ok(())
    } else {
        Err(OAuthLoginError::InvalidInput)
    }
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

fn map_github_client_error(error: GitHubLoginClientError) -> OAuthLoginError {
    match error {
        GitHubLoginClientError::ClientUnavailable => OAuthLoginError::Internal,
        GitHubLoginClientError::ProviderUnavailable => OAuthLoginError::ProviderUnavailable,
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
                Some("secret-value".to_owned()),
                true,
            )
            .unwrap_err(),
            OAuthLoginError::InvalidInput
        );
        assert_eq!(
            AdminOAuthLoginProviderSettingsCommand::new(1, true, None, None, false).unwrap_err(),
            OAuthLoginError::InvalidInput
        );
        let command = AdminOAuthLoginProviderSettingsCommand::new(
            1,
            false,
            Some("client-id".to_owned()),
            Some("secret-value".to_owned()),
            false,
        )
        .unwrap();
        let debug = format!("{command:?}");
        assert!(!debug.contains("client-id"));
        assert!(!debug.contains("secret-value"));
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
        let username = local_username("octocat", "583231");
        assert!(username.starts_with("gh_octocat_"));
        assert!(username.len() <= 64);
        assert!(!username.contains("583231"));
    }

    #[test]
    fn provider_and_secret_responses_do_not_expose_sensitive_values() {
        let provider = PublicOAuthLoginProvider::new("github", "GitHub");
        assert_eq!(provider.id(), "github");
        assert_eq!(provider.display_name(), "GitHub");
        let settings = AdminOAuthLoginProviderSettings {
            enabled: true,
            client_id: Some("client-id".to_owned()),
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
