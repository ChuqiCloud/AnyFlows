use std::{
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
    time::Instant,
};

use af_domain::{ChannelId, CredentialId, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill as fill_random;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use url::{Host, Url};
use zeroize::Zeroize as _;

const RANDOM_MATERIAL_BYTES: usize = 32;
const MAX_CLIENT_ID_BYTES: usize = 256;
const MAX_SCOPE_BYTES: usize = 2 * 1_024;
const MAX_AUTHORIZATION_PARAMETERS: usize = 16;
const MAX_AUTHORIZATION_PARAMETER_NAME_BYTES: usize = 128;
const MAX_AUTHORIZATION_PARAMETER_VALUE_BYTES: usize = 1_024;
const MAX_CALLBACK_COMPONENT_BYTES: usize = 4 * 1_024;
const MAX_AUTHORIZATION_URL_BYTES: usize = 8 * 1_024;
const MAX_CALLBACK_URL_BYTES: usize = 16 * 1_024;
const OAUTH_AUTHORIZATION_RESPONSE_TYPE: &str = "code";
const OAUTH_PKCE_METHOD: &str = "S256";
const RESERVED_AUTHORIZATION_PARAMETERS: [&str; 7] = [
    "response_type",
    "client_id",
    "redirect_uri",
    "scope",
    "state",
    "code_challenge",
    "code_challenge_method",
];

/// 支持的上游 OAuth 身份提供商；供应商端点和字段由后续适配器配置。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UpstreamOAuthProvider {
    /// Anthropic/Claude Code 账号。
    ClaudeCode,
    /// OpenAI/Codex 账号。
    Codex,
    /// Google/Gemini 账号。
    Gemini,
    /// Antigravity 账号。
    Antigravity,
}

impl UpstreamOAuthProvider {
    /// 返回稳定的配置和审计标识；不包含外部输入。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Antigravity => "antigravity",
        }
    }
}

impl fmt::Display for UpstreamOAuthProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// OAuth 授权要绑定的业务主体；不保存任何密钥或授权码。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OAuthAuthorizationContext {
    user_id: UserId,
    channel_id: Option<ChannelId>,
    credential_id: Option<CredentialId>,
}

impl OAuthAuthorizationContext {
    /// 创建当前用户范围的授权上下文；凭据目标不能脱离渠道单独存在。
    pub fn new(
        user_id: UserId,
        channel_id: Option<ChannelId>,
        credential_id: Option<CredentialId>,
    ) -> Result<Self, OAuthAuthorizationError> {
        if credential_id.is_some() && channel_id.is_none() {
            return Err(OAuthAuthorizationError::InvalidContext);
        }
        Ok(Self {
            user_id,
            channel_id,
            credential_id,
        })
    }

    /// 返回授权发起用户。
    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }

    /// 返回可选渠道目标。
    #[must_use]
    pub const fn channel_id(self) -> Option<ChannelId> {
        self.channel_id
    }

    /// 返回可选凭据目标。
    #[must_use]
    pub const fn credential_id(self) -> Option<CredentialId> {
        self.credential_id
    }
}

/// 启动一次 OAuth 授权所需的已校验输入。
#[derive(Clone)]
pub struct OAuthAuthorizationRequest {
    provider: UpstreamOAuthProvider,
    context: OAuthAuthorizationContext,
    client_id: String,
    authorization_endpoint: Url,
    redirect_uri: Url,
    scope: String,
    additional_parameters: &'static [(&'static str, &'static str)],
}

impl OAuthAuthorizationRequest {
    /// 校验供应商端点、公共客户端标识、scope 和 loopback 回调地址。
    pub fn new(
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
        client_id: String,
        authorization_endpoint: String,
        redirect_uri: String,
        scope: String,
    ) -> Result<Self, OAuthAuthorizationError> {
        let redirect_uri = parse_loopback_redirect_uri(&redirect_uri)?;
        Self::new_with_authorization_parameters(
            provider,
            context,
            client_id,
            authorization_endpoint,
            redirect_uri,
            scope,
            &[],
        )
    }

    /// 仅供闭合 provider profile 附加代码内静态白名单参数。
    pub(crate) fn new_with_authorization_parameters(
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
        client_id: String,
        authorization_endpoint: String,
        redirect_uri: Url,
        scope: String,
        additional_parameters: &'static [(&'static str, &'static str)],
    ) -> Result<Self, OAuthAuthorizationError> {
        let authorization_endpoint = validate_authorization_profile(
            &client_id,
            &authorization_endpoint,
            &scope,
            additional_parameters,
        )?;
        Ok(Self {
            provider,
            context,
            client_id,
            authorization_endpoint,
            redirect_uri,
            scope,
            additional_parameters,
        })
    }

    /// 返回目标提供商。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    /// 返回授权上下文。
    #[must_use]
    pub const fn context(&self) -> OAuthAuthorizationContext {
        self.context
    }

    pub(crate) fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }
}

impl fmt::Debug for OAuthAuthorizationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthAuthorizationRequest")
            .field("provider", &self.provider)
            .field("context", &self.context)
            .field("client_id", &"<已脱敏>")
            .field("authorization_endpoint", &"<已脱敏>")
            .field("redirect_uri", &"<已脱敏>")
            .field("scope", &"<已脱敏>")
            .finish()
    }
}

/// 成功创建授权会话后返回的公开材料；不返回 code_verifier。
pub struct OAuthAuthorizationStart {
    provider: UpstreamOAuthProvider,
    context: OAuthAuthorizationContext,
    authorization_url: Url,
    redirect_uri: Url,
    expires_at: Instant,
}

impl OAuthAuthorizationStart {
    pub(crate) fn new(
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
        authorization_url: Url,
        redirect_uri: Url,
        expires_at: Instant,
    ) -> Self {
        Self {
            provider,
            context,
            authorization_url,
            redirect_uri,
            expires_at,
        }
    }

    /// 返回应交给浏览器打开的授权地址。地址中的 state 只用于当前一次会话。
    #[must_use]
    pub fn authorization_url(&self) -> &Url {
        &self.authorization_url
    }

    /// 返回已绑定的 loopback 回调地址。
    #[must_use]
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    /// 返回授权目标提供商。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    /// 返回授权业务上下文。
    #[must_use]
    pub const fn context(&self) -> OAuthAuthorizationContext {
        self.context
    }

    /// 返回单调时钟下的过期时刻，供协调器计算剩余 TTL。
    #[must_use]
    pub const fn expires_at(&self) -> Instant {
        self.expires_at
    }
}

impl fmt::Debug for OAuthAuthorizationStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthAuthorizationStart")
            .field("provider", &self.provider)
            .field("context", &self.context)
            .field("authorization_url", &"<已脱敏>")
            .field("redirect_uri", &"<已脱敏>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// 从 loopback 回调 URL 提取的已校验 OAuth 响应。
pub struct OAuthAuthorizationCallback {
    provider: UpstreamOAuthProvider,
    redirect_uri: Url,
    state: SecretText,
    code: Option<SecretText>,
    provider_error: Option<SecretText>,
}

impl OAuthAuthorizationCallback {
    /// 解析回调查询参数；拒绝重复 state/code/error、fragment 和不完整响应。
    pub fn from_redirect_url(
        provider: UpstreamOAuthProvider,
        mut redirect_uri: Url,
    ) -> Result<Self, OAuthAuthorizationError> {
        validate_callback_redirect_uri(&redirect_uri)?;
        let mut state = None;
        let mut code = None;
        let mut provider_error = None;
        for (key, value) in redirect_uri.query_pairs() {
            match key.as_ref() {
                "state" if state.is_none() => state = Some(SecretText::new(value.into_owned())),
                "code" if code.is_none() => code = Some(SecretText::new(value.into_owned())),
                "error" if provider_error.is_none() => {
                    provider_error = Some(SecretText::new(value.into_owned()));
                }
                "state" | "code" | "error" => {
                    return Err(OAuthAuthorizationError::MalformedCallback);
                }
                _ => {}
            }
        }
        let state = state.ok_or(OAuthAuthorizationError::MalformedCallback)?;
        validate_state(state.expose())?;
        if let Some(code) = &code {
            validate_callback_component(code.expose())?;
        }
        if let Some(provider_error) = &provider_error {
            validate_callback_component(provider_error.expose())?;
        }
        if code.is_some() == provider_error.is_some() {
            return Err(OAuthAuthorizationError::MalformedCallback);
        }

        // 匹配时只保留注册回调部分，避免查询中的授权材料继续附着在 URL 上。
        redirect_uri.set_query(None);
        Ok(Self {
            provider,
            redirect_uri,
            state,
            code,
            provider_error,
        })
    }

    /// 返回回调提供商。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    pub(crate) fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    pub(crate) fn state(&self) -> &str {
        self.state.expose()
    }

    pub(crate) fn take_state(&mut self) -> String {
        std::mem::take(&mut self.state.0)
    }

    pub(crate) fn take_authorization_code(&mut self) -> Option<String> {
        self.code.take().map(SecretText::into_inner)
    }

    pub(crate) const fn is_denied(&self) -> bool {
        self.provider_error.is_some()
    }
}

impl fmt::Debug for OAuthAuthorizationCallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthAuthorizationCallback")
            .field("provider", &self.provider)
            .field("redirect_uri", &"<已脱敏>")
            .field("state", &"<已脱敏>")
            .field("code", &self.code.as_ref().map(|_| "<已脱敏>"))
            .field(
                "provider_error",
                &self.provider_error.as_ref().map(|_| "<已脱敏>"),
            )
            .finish()
    }
}

/// 一次性回调消费后的授权码材料，供后续 token endpoint 交换。
pub struct OAuthAuthorizationGrant {
    provider: UpstreamOAuthProvider,
    context: OAuthAuthorizationContext,
    redirect_uri: Url,
    state: SecretText,
    authorization_code: SecretText,
    code_verifier: SecretText,
}

impl OAuthAuthorizationGrant {
    pub(crate) fn new(
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
        redirect_uri: Url,
        state: String,
        authorization_code: String,
        code_verifier: String,
    ) -> Self {
        Self {
            provider,
            context,
            redirect_uri,
            state: SecretText::new(state),
            authorization_code: SecretText::new(authorization_code),
            code_verifier: SecretText::new(code_verifier),
        }
    }

    /// 返回提供商。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    /// 返回业务上下文。
    #[must_use]
    pub const fn context(&self) -> OAuthAuthorizationContext {
        self.context
    }

    /// 返回精确回调地址，token endpoint 应使用同一值。
    #[must_use]
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    /// 暴露原始 state 给明确要求该字段的受控 token 交换器；调用方不得记录或复制。
    #[must_use]
    pub fn state(&self) -> &str {
        self.state.expose()
    }

    /// 暴露授权码给受控 token 交换器；调用方不得复制或记录。
    #[must_use]
    pub fn authorization_code(&self) -> &str {
        self.authorization_code.expose()
    }

    /// 暴露 PKCE verifier 给受控 token 交换器；调用方不得复制或记录。
    #[must_use]
    pub fn code_verifier(&self) -> &str {
        self.code_verifier.expose()
    }
}

impl fmt::Debug for OAuthAuthorizationGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthAuthorizationGrant")
            .field("provider", &self.provider)
            .field("context", &self.context)
            .field("redirect_uri", &"<已脱敏>")
            .field("state", &"<已脱敏>")
            .field("authorization_code", &"<已脱敏>")
            .field("code_verifier", &"<已脱敏>")
            .finish()
    }
}

/// OAuth 授权会话错误；不回显 state、授权码、URL 或供应商错误正文。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthAuthorizationError {
    #[error("OAuth 授权上下文无效")]
    InvalidContext,
    #[error("OAuth 客户端标识无效")]
    InvalidClientId,
    #[error("OAuth 授权端点无效")]
    InvalidAuthorizationEndpoint,
    #[error("OAuth 授权地址超过长度上限")]
    AuthorizationUrlTooLong,
    #[error("OAuth 回调地址必须是显式 loopback 地址")]
    InvalidRedirectUri,
    #[error("OAuth scope 无效")]
    InvalidScope,
    #[error("OAuth 授权参数已被预置覆盖")]
    ReservedParameter,
    #[error("OAuth 授权附加参数无效")]
    InvalidAuthorizationParameter,
    #[error("OAuth state 无效")]
    InvalidState,
    #[error("OAuth 回调参数无效")]
    MalformedCallback,
    #[error("OAuth 授权会话不存在或已失效")]
    SessionNotFound,
    #[error("OAuth 授权会话容量已满")]
    CapacityExceeded,
    #[error("OAuth 授权会话已过期")]
    SessionExpired,
    #[error("OAuth 回调提供商不匹配")]
    ProviderMismatch,
    #[error("OAuth 回调地址不匹配")]
    RedirectUriMismatch,
    #[error("OAuth 回调用户与授权发起用户不匹配")]
    PrincipalMismatch,
    #[error("OAuth 提供商拒绝了授权")]
    ProviderDenied,
    #[error("OAuth 安全随机数不可用")]
    Entropy,
    #[error("OAuth 授权会话存储不可用")]
    StoreUnavailable,
    #[error("OAuth 授权码无效")]
    InvalidAuthorizationCode,
    #[error("OAuth 授权会话 TTL 无效")]
    InvalidSessionTtl,
    #[error("OAuth 授权会话容量配置无效")]
    InvalidSessionCapacity,
}

pub(crate) struct PendingAuthorization {
    pub(crate) provider: UpstreamOAuthProvider,
    pub(crate) context: OAuthAuthorizationContext,
    pub(crate) redirect_uri: Url,
    pub(crate) code_verifier: SecretText,
    pub(crate) expires_at: Instant,
}

impl PendingAuthorization {
    pub(crate) fn is_expired(&self, now: Instant) -> bool {
        self.expires_at <= now
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct StateDigest([u8; 32]);

impl StateDigest {
    pub(crate) fn from_state(state: &str) -> Self {
        Self(Sha256::digest(state.as_bytes()).into())
    }
}

impl fmt::Debug for StateDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StateDigest(<已脱敏>)")
    }
}

pub(crate) struct SecretText(String);

impl SecretText {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_inner(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

impl fmt::Debug for SecretText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

impl Drop for SecretText {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub(crate) fn generate_random_token() -> Result<String, OAuthAuthorizationError> {
    let mut bytes = [0_u8; RANDOM_MATERIAL_BYTES];
    fill_random(&mut bytes).map_err(|_| OAuthAuthorizationError::Entropy)?;
    let token = URL_SAFE_NO_PAD.encode(bytes);
    bytes.zeroize();
    Ok(token)
}

pub(crate) fn build_authorization_url(
    request: &OAuthAuthorizationRequest,
    state: &str,
    code_challenge: &str,
) -> Result<Url, OAuthAuthorizationError> {
    let mut url = request.authorization_endpoint.clone();
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("response_type", OAUTH_AUTHORIZATION_RESPONSE_TYPE)
            .append_pair("client_id", &request.client_id)
            .append_pair("redirect_uri", request.redirect_uri.as_str())
            .append_pair("scope", &request.scope)
            .append_pair("state", state)
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", OAUTH_PKCE_METHOD);
        for (name, value) in request.additional_parameters {
            query.append_pair(name, value);
        }
    }
    if url.as_str().len() > MAX_AUTHORIZATION_URL_BYTES {
        return Err(OAuthAuthorizationError::AuthorizationUrlTooLong);
    }
    Ok(url)
}

pub(crate) fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(crate) fn validate_callback_redirect_uri(url: &Url) -> Result<(), OAuthAuthorizationError> {
    if url.as_str().len() > MAX_CALLBACK_URL_BYTES {
        return Err(OAuthAuthorizationError::InvalidRedirectUri);
    }
    validate_loopback_components(url, true, true)
}

fn parse_authorization_endpoint(value: &str) -> Result<Url, OAuthAuthorizationError> {
    let url =
        Url::parse(value).map_err(|_| OAuthAuthorizationError::InvalidAuthorizationEndpoint)?;
    if url.scheme() != "https"
        || url.host().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(OAuthAuthorizationError::InvalidAuthorizationEndpoint);
    }
    Ok(url)
}

fn parse_loopback_redirect_uri(value: &str) -> Result<Url, OAuthAuthorizationError> {
    let url = Url::parse(value).map_err(|_| OAuthAuthorizationError::InvalidRedirectUri)?;
    validate_loopback_components(&url, false, false)?;
    Ok(url)
}

pub(crate) fn parse_profile_loopback_redirect_uri(
    value: &str,
) -> Result<Url, OAuthAuthorizationError> {
    let url = Url::parse(value).map_err(|_| OAuthAuthorizationError::InvalidRedirectUri)?;
    validate_loopback_components(&url, false, true)?;
    Ok(url)
}

fn validate_loopback_components(
    url: &Url,
    allow_query: bool,
    allow_localhost: bool,
) -> Result<(), OAuthAuthorizationError> {
    let is_loopback = matches!(
        url.host(),
        Some(Host::Ipv4(address)) if address == Ipv4Addr::LOCALHOST
    ) || matches!(
        url.host(),
        Some(Host::Ipv6(address)) if address == Ipv6Addr::LOCALHOST
    ) || (allow_localhost
        && matches!(url.host(), Some(Host::Domain("localhost"))));
    if url.scheme() != "http"
        || !is_loopback
        || url.port().is_none()
        || url.port() == Some(0)
        || url.username() != ""
        || url.password().is_some()
        || url.fragment().is_some()
        || (!allow_query && url.query().is_some())
    {
        return Err(OAuthAuthorizationError::InvalidRedirectUri);
    }
    Ok(())
}

pub(crate) fn validate_authorization_profile(
    client_id: &str,
    authorization_endpoint: &str,
    scope: &str,
    additional_parameters: &[(&str, &str)],
) -> Result<Url, OAuthAuthorizationError> {
    validate_client_id(client_id)?;
    let endpoint = parse_authorization_endpoint(authorization_endpoint)?;
    validate_scope(scope)?;
    reject_reserved_query_parameters(&endpoint)?;
    validate_authorization_parameters(&endpoint, additional_parameters)?;
    Ok(endpoint)
}

fn reject_reserved_query_parameters(url: &Url) -> Result<(), OAuthAuthorizationError> {
    if url
        .query_pairs()
        .any(|(key, _)| RESERVED_AUTHORIZATION_PARAMETERS.contains(&key.as_ref()))
    {
        return Err(OAuthAuthorizationError::ReservedParameter);
    }
    Ok(())
}

fn validate_authorization_parameters(
    endpoint: &Url,
    parameters: &[(&str, &str)],
) -> Result<(), OAuthAuthorizationError> {
    if parameters.len() > MAX_AUTHORIZATION_PARAMETERS {
        return Err(OAuthAuthorizationError::InvalidAuthorizationParameter);
    }
    for (index, (name, value)) in parameters.iter().enumerate() {
        let valid_name = !name.is_empty()
            && name.len() <= MAX_AUTHORIZATION_PARAMETER_NAME_BYTES
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
        let valid_value = !value.is_empty()
            && value.len() <= MAX_AUTHORIZATION_PARAMETER_VALUE_BYTES
            && value.is_ascii()
            && !value.bytes().any(|byte| byte.is_ascii_control());
        if !valid_name || !valid_value {
            return Err(OAuthAuthorizationError::InvalidAuthorizationParameter);
        }
        if RESERVED_AUTHORIZATION_PARAMETERS.contains(name)
            || parameters[..index]
                .iter()
                .any(|(existing, _)| existing == name)
            || endpoint
                .query_pairs()
                .any(|(existing, _)| existing == *name)
        {
            return Err(OAuthAuthorizationError::ReservedParameter);
        }
    }
    Ok(())
}

pub(crate) fn validate_client_id(value: &str) -> Result<(), OAuthAuthorizationError> {
    if value.is_empty()
        || value.len() > MAX_CLIENT_ID_BYTES
        || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return Err(OAuthAuthorizationError::InvalidClientId);
    }
    Ok(())
}

pub(crate) fn validate_scope(value: &str) -> Result<(), OAuthAuthorizationError> {
    if value.is_empty()
        || value.len() > MAX_SCOPE_BYTES
        || value.trim() != value
        || !value.is_ascii()
        || value.split(' ').any(|token| {
            token.is_empty()
                || !token.bytes().all(|byte| {
                    byte == 0x21 || (0x23..=0x5b).contains(&byte) || (0x5d..=0x7e).contains(&byte)
                })
        })
    {
        return Err(OAuthAuthorizationError::InvalidScope);
    }
    Ok(())
}

pub(crate) fn validate_state(value: &str) -> Result<(), OAuthAuthorizationError> {
    if value.len() != 43
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(OAuthAuthorizationError::InvalidState);
    }
    Ok(())
}

fn validate_callback_component(value: &str) -> Result<(), OAuthAuthorizationError> {
    if value.is_empty()
        || value.len() > MAX_CALLBACK_COMPONENT_BYTES
        || value.chars().any(char::is_control)
        || !value.is_ascii()
    {
        return Err(OAuthAuthorizationError::MalformedCallback);
    }
    Ok(())
}
