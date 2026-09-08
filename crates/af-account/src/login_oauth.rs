use std::{fmt, net::IpAddr, time::Duration};

use af_httpclient::{Body, Bytes, HeaderMap, HeaderName, HeaderValue, HttpClientProvider, Method};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::{DecryptedSystemSecret, oauth::CustomOAuth2ProviderConfig};

const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_URL: &str = "https://api.github.com/user";
const GITHUB_API_VERSION: &str = "2022-11-28";
const DISCORD_TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
const DISCORD_USER_URL: &str = "https://discord.com/api/v10/users/@me";
const LINUXDO_ISSUER: &str = "https://connect.linux.do";
const LINUXDO_DISCOVERY_ISSUER: &str = "https://connect.linux.do/";
const LINUXDO_AUTHORIZATION_URL: &str = "https://connect.linux.do/oauth2/authorize";
const LINUXDO_TOKEN_URL: &str = "https://connect.linux.do/oauth2/token";
const LINUXDO_USER_URL: &str = "https://connect.linux.do/api/user";
const LINUXDO_JWKS_URL: &str = "https://connect.linux.do/.well-known/jwks.json";
const WECHAT_AUTHORIZATION_URL: &str = "https://open.weixin.qq.com/connect/qrconnect";
const WECHAT_TOKEN_URL: &str = "https://api.weixin.qq.com/sns/oauth2/access_token";
const WECHAT_USER_URL: &str = "https://api.weixin.qq.com/sns/userinfo";
const TELEGRAM_ISSUER: &str = "https://oauth.telegram.org";
const TELEGRAM_AUTHORIZATION_URL: &str = "https://oauth.telegram.org/auth";
const TELEGRAM_TOKEN_URL: &str = "https://oauth.telegram.org/token";
const TELEGRAM_JWKS_URL: &str = "https://oauth.telegram.org/.well-known/jwks.json";
const GOOGLE_ISSUER: &str = "https://accounts.google.com";
const GOOGLE_AUTHORIZATION_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_JWKS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_PROVIDER_RESPONSE_BYTES: usize = 64 * 1_024;
const MAX_ACCESS_TOKEN_BYTES: usize = 4 * 1_024;

/// 用户登录仅允许使用受控 Provider；通用 OIDC 只接受管理员配置的 HTTPS issuer。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginOAuthProvider {
    GitHub,
    Discord,
    Oidc,
    LinuxDo,
    WeChat,
    Telegram,
    Google,
}

impl LoginOAuthProvider {
    pub const ALL: [Self; 7] = [
        Self::GitHub,
        Self::Discord,
        Self::Oidc,
        Self::LinuxDo,
        Self::WeChat,
        Self::Telegram,
        Self::Google,
    ];

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Discord => "discord",
            Self::Oidc => "oidc",
            Self::LinuxDo => "linuxdo",
            Self::WeChat => "wechat",
            Self::Telegram => "telegram",
            Self::Google => "google",
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Discord => "Discord",
            Self::Oidc => "OIDC",
            Self::LinuxDo => "LinuxDO",
            Self::WeChat => "微信",
            Self::Telegram => "Telegram",
            Self::Google => "Google",
        }
    }

    #[must_use]
    pub const fn authorization_url(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/oauth/authorize",
            Self::Discord => "https://discord.com/oauth2/authorize",
            Self::Oidc => "",
            Self::LinuxDo => LINUXDO_AUTHORIZATION_URL,
            Self::WeChat => WECHAT_AUTHORIZATION_URL,
            Self::Telegram => TELEGRAM_AUTHORIZATION_URL,
            Self::Google => GOOGLE_AUTHORIZATION_URL,
        }
    }

    #[must_use]
    pub const fn uses_pkce(self) -> bool {
        matches!(
            self,
            Self::GitHub | Self::Oidc | Self::LinuxDo | Self::Telegram | Self::Google
        )
    }

    #[must_use]
    pub const fn username_prefix(self) -> &'static str {
        match self {
            Self::GitHub => "gh",
            Self::Discord => "dc",
            Self::Oidc => "oidc",
            Self::LinuxDo => "ld",
            Self::WeChat => "wx",
            Self::Telegram => "tg",
            Self::Google => "gg",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.id() == value)
    }

    const fn token_url(self) -> &'static str {
        match self {
            Self::GitHub => GITHUB_TOKEN_URL,
            Self::Discord => DISCORD_TOKEN_URL,
            Self::Oidc => "",
            Self::LinuxDo => LINUXDO_TOKEN_URL,
            Self::WeChat => WECHAT_TOKEN_URL,
            Self::Telegram => TELEGRAM_TOKEN_URL,
            Self::Google => GOOGLE_TOKEN_URL,
        }
    }

    const fn user_url(self) -> &'static str {
        match self {
            Self::GitHub => GITHUB_USER_URL,
            Self::Discord => DISCORD_USER_URL,
            Self::Oidc => "",
            Self::LinuxDo => LINUXDO_USER_URL,
            Self::WeChat => WECHAT_USER_URL,
            Self::Telegram => "",
            Self::Google => "",
        }
    }

    #[must_use]
    pub const fn is_oidc(self) -> bool {
        matches!(
            self,
            Self::Oidc | Self::LinuxDo | Self::Telegram | Self::Google
        )
    }

    #[must_use]
    pub const fn fixed_issuer(self) -> Option<&'static str> {
        match self {
            Self::LinuxDo => Some(LINUXDO_ISSUER),
            Self::Telegram => Some(TELEGRAM_ISSUER),
            Self::Google => Some(GOOGLE_ISSUER),
            _ => None,
        }
    }
}

/// 用户登录适配器解析出的最小稳定外部身份。
#[derive(Clone, Eq, PartialEq)]
pub struct OAuthLoginIdentity {
    subject: String,
    username: String,
}

impl OAuthLoginIdentity {
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }
}

impl fmt::Debug for OAuthLoginIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthLoginIdentity")
            .field("subject", &self.subject)
            .field("username", &self.username)
            .finish()
    }
}

/// 用户登录 OAuth 受控传输的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OAuthLoginClientError {
    #[error("OAuth 登录 HTTP Client 不可用")]
    ClientUnavailable,
    #[error("OAuth 登录 Provider 请求失败")]
    ProviderUnavailable,
}

/// 只访问内置 Provider 固定 token/user 端点的用户登录客户端。
#[derive(Clone)]
pub struct OAuthLoginClient {
    http_clients: HttpClientProvider,
}

impl OAuthLoginClient {
    #[must_use]
    pub fn new(http_clients: HttpClientProvider) -> Self {
        Self { http_clients }
    }

    /// 交换一次性授权码，并只保留 Provider 的稳定 subject 与受控用户名。
    pub async fn exchange_identity(
        &self,
        provider: LoginOAuthProvider,
        client_id: &str,
        client_secret: &DecryptedSystemSecret,
        code: &str,
        verifier: Option<&str>,
        redirect_uri: &str,
    ) -> Result<OAuthLoginIdentity, OAuthLoginClientError> {
        if provider.uses_pkce() != verifier.is_some() {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        if provider == LoginOAuthProvider::WeChat {
            return self
                .exchange_wechat_identity(client_id, client_secret, code)
                .await;
        }
        let client = self
            .http_clients
            .get(Some(PROVIDER_REQUEST_TIMEOUT))
            .map_err(|_| OAuthLoginClientError::ClientUnavailable)?;
        let request_body = token_request_body(
            provider,
            client_id,
            client_secret.expose_secret(),
            code,
            verifier,
            redirect_uri,
            true,
        );
        let token_bytes = execute_json(
            &client,
            Method::POST,
            provider.token_url(),
            provider_headers(provider, true),
            Some(request_body),
        )
        .await?;
        let response = serde_json::from_slice::<OAuthTokenResponse>(&token_bytes)
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        if !response.token_type.eq_ignore_ascii_case("bearer") {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let access_token = response.access_token.into_zeroizing();
        let mut authorization = Zeroizing::new(String::from("Bearer "));
        authorization.push_str(access_token.as_str());
        let mut headers = provider_headers(provider, false);
        let mut authorization = HeaderValue::from_str(authorization.as_str())
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        authorization.set_sensitive(true);
        headers.insert(HeaderName::from_static("authorization"), authorization);
        let user_bytes =
            execute_json(&client, Method::GET, provider.user_url(), headers, None).await?;
        parse_identity(provider, &user_bytes)
    }

    /// 使用管理员已校验的静态端点交换自定义 Provider 身份；Token 与 UserInfo 只存在于内存。
    pub async fn exchange_custom_identity(
        &self,
        config: &CustomOAuth2ProviderConfig,
        client_secret: &DecryptedSystemSecret,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthLoginIdentity, OAuthLoginClientError> {
        if !config.enabled()
            || !valid_custom_oauth_parameter(code)
            || !valid_custom_oauth_parameter(verifier)
            || !valid_custom_redirect_uri(redirect_uri)
        {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let client = self
            .http_clients
            .get(Some(PROVIDER_REQUEST_TIMEOUT))
            .map_err(|_| OAuthLoginClientError::ClientUnavailable)?;
        let token_bytes = execute_json(
            &client,
            Method::POST,
            config.endpoints().token_endpoint().as_str(),
            custom_oauth_headers(true),
            Some(custom_token_request_body(
                config,
                client_secret,
                code,
                verifier,
                redirect_uri,
            )),
        )
        .await?;
        let response = serde_json::from_slice::<OAuthTokenResponse>(&token_bytes)
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        if !response.token_type.eq_ignore_ascii_case("bearer") {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let access_token = response.access_token.into_zeroizing();
        let mut authorization = Zeroizing::new(String::from("Bearer "));
        authorization.push_str(access_token.as_str());
        let mut headers = custom_oauth_headers(false);
        let mut authorization = HeaderValue::from_str(authorization.as_str())
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        authorization.set_sensitive(true);
        headers.insert(HeaderName::from_static("authorization"), authorization);
        let user_bytes = execute_json(
            &client,
            Method::GET,
            config.endpoints().userinfo_endpoint().as_str(),
            headers,
            None,
        )
        .await?;
        parse_custom_identity(config, &user_bytes)
    }

    /// 构造固定参数的自定义 OAuth2 授权地址，不接受请求方覆盖端点、scope 或回调。
    pub fn custom_authorization_url(
        config: &CustomOAuth2ProviderConfig,
        redirect_uri: &str,
        state: &str,
        challenge: &str,
    ) -> Result<String, OAuthLoginClientError> {
        if !config.enabled()
            || !valid_custom_oauth_parameter(state)
            || !valid_custom_oauth_parameter(challenge)
            || !valid_custom_redirect_uri(redirect_uri)
        {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let mut url = config.endpoints().authorization_endpoint().clone();
        url.query_pairs_mut()
            .append_pair("client_id", config.client_id())
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", config.endpoints().scope())
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        if url.as_str().len() > 8 * 1_024 {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        Ok(url.into())
    }

    /// 微信协议要求 AppSecret 与 access token 位于固定 HTTPS 查询参数中；临时 URL 用后立即清零。
    async fn exchange_wechat_identity(
        &self,
        client_id: &str,
        client_secret: &DecryptedSystemSecret,
        code: &str,
    ) -> Result<OAuthLoginIdentity, OAuthLoginClientError> {
        let client = self
            .http_clients
            .get(Some(PROVIDER_REQUEST_TIMEOUT))
            .map_err(|_| OAuthLoginClientError::ClientUnavailable)?;
        let token_url = sensitive_query_url(
            WECHAT_TOKEN_URL,
            &[
                ("appid", client_id),
                ("secret", client_secret.expose_secret()),
                ("code", code),
                ("grant_type", "authorization_code"),
            ],
        )?;
        let token_bytes = execute_json(
            &client,
            Method::GET,
            token_url.as_str(),
            provider_headers(LoginOAuthProvider::WeChat, false),
            None,
        )
        .await?;
        drop(token_url);
        let token = parse_wechat_token(&token_bytes)?;
        let access_token = token.access_token.into_zeroizing();
        let user_url = sensitive_query_url(
            WECHAT_USER_URL,
            &[
                ("access_token", access_token.as_str()),
                ("openid", &token.openid),
                ("lang", "zh_CN"),
            ],
        )?;
        let user_bytes = execute_json(
            &client,
            Method::GET,
            user_url.as_str(),
            provider_headers(LoginOAuthProvider::WeChat, false),
            None,
        )
        .await?;
        drop(user_url);
        drop(access_token);
        parse_wechat_identity(&token.openid, &user_bytes)
    }

    /// 通过受控 discovery 交换 OIDC 授权码，并验证 RS256 ID Token 的关键声明。
    pub async fn exchange_oidc_identity(
        &self,
        provider: LoginOAuthProvider,
        issuer: &str,
        client_id: &str,
        client_secret: &DecryptedSystemSecret,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<OAuthLoginIdentity, OAuthLoginClientError> {
        if !provider.is_oidc() || issuer.is_empty() {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let discovery = self.oidc_discovery(provider, issuer).await?;
        validate_issuer(issuer)?;
        if !valid_discovery(&discovery, issuer) {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let client = self
            .http_clients
            .get(Some(PROVIDER_REQUEST_TIMEOUT))
            .map_err(|_| OAuthLoginClientError::ClientUnavailable)?;
        let client_authentication = discovery.client_authentication()?;
        let request_body = token_request_body(
            provider,
            client_id,
            client_secret.expose_secret(),
            code,
            Some(verifier),
            redirect_uri,
            client_authentication == OidcClientAuthentication::Post,
        );
        let mut headers = provider_headers(provider, true);
        if client_authentication == OidcClientAuthentication::Basic {
            insert_basic_client_auth(&mut headers, client_id, client_secret.expose_secret())?;
        }
        let token_bytes = execute_json(
            &client,
            Method::POST,
            &discovery.token_endpoint,
            headers,
            Some(request_body),
        )
        .await?;
        let response: OidcTokenResponse = serde_json::from_slice(&token_bytes)
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        if !response.token_type.eq_ignore_ascii_case("bearer") {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let access_token = response.access_token.into_zeroizing();
        let id_token = Zeroizing::new(
            response
                .id_token
                .ok_or(OAuthLoginClientError::ProviderUnavailable)?,
        );
        let jwks = self.fetch_jwks(&client, &discovery.jwks_uri).await?;
        let header = jsonwebtoken::decode_header(&id_token)
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        if header.alg != Algorithm::RS256 {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let kid = header
            .kid
            .ok_or(OAuthLoginClientError::ProviderUnavailable)?;
        let jwk = jwks
            .find(&kid)
            .ok_or(OAuthLoginClientError::ProviderUnavailable)?;
        let mut validation = Validation::new(Algorithm::RS256);
        // JWT 的 iss 必须匹配已经通过 discovery 校验的规范值，不能直接信任配置中的尾斜杠形式。
        validation.set_issuer(std::slice::from_ref(&discovery.issuer));
        validation.set_audience(&[client_id]);
        let claims = decode::<OidcClaims>(
            &id_token,
            &DecodingKey::from_jwk(jwk).map_err(|_| OAuthLoginClientError::ProviderUnavailable)?,
            &validation,
        )
        .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?
        .claims;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?
            .as_secs();
        if claims.exp <= now {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let expected_nonce = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        if claims.nonce.as_deref() != Some(expected_nonce.as_str()) {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let subject = claims.sub;
        validate_oidc_subject(&subject)?;
        let username = match provider {
            LoginOAuthProvider::Telegram => {
                validate_telegram_subject(&subject)?;
                "telegram".to_owned()
            }
            LoginOAuthProvider::Google => {
                validate_google_subject(&subject)?;
                "google".to_owned()
            }
            _ => {
                let username = claims
                    .preferred_username
                    .or(claims.login)
                    .or(claims.name)
                    .unwrap_or_else(|| subject.clone());
                validate_oidc_username(&username)?;
                username
            }
        };
        let subject = oidc_subject_namespace(discovery.issuer.trim_end_matches('/'), &subject);
        drop(access_token);
        Ok(OAuthLoginIdentity { subject, username })
    }

    /// 构造 OIDC 授权地址；仅使用已校验的 discovery 元数据。
    pub async fn oidc_authorization_url(
        &self,
        provider: LoginOAuthProvider,
        issuer: &str,
        client_id: &str,
        redirect_uri: &str,
        state: &str,
        challenge: &str,
    ) -> Result<String, OAuthLoginClientError> {
        if !provider.is_oidc() {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let discovery = self.oidc_discovery(provider, issuer).await?;
        validate_issuer(issuer)?;
        if discovery.issuer.trim_end_matches('/') != issuer.trim_end_matches('/')
            || !valid_endpoint(&discovery.authorization_endpoint)
        {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        let mut url = url::Url::parse(&discovery.authorization_endpoint)
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair(
                "scope",
                if matches!(
                    provider,
                    LoginOAuthProvider::Telegram | LoginOAuthProvider::Google
                ) {
                    "openid"
                } else {
                    "openid profile email"
                },
            )
            .append_pair("state", state)
            .append_pair("nonce", challenge)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    async fn oidc_discovery(
        &self,
        provider: LoginOAuthProvider,
        issuer: &str,
    ) -> Result<DiscoveryDocument, OAuthLoginClientError> {
        if !provider.is_oidc() {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        if provider == LoginOAuthProvider::LinuxDo {
            return (issuer == LINUXDO_ISSUER)
                .then(DiscoveryDocument::linuxdo)
                .ok_or(OAuthLoginClientError::ProviderUnavailable);
        }
        match provider {
            LoginOAuthProvider::Telegram if issuer != TELEGRAM_ISSUER => {
                return Err(OAuthLoginClientError::ProviderUnavailable);
            }
            LoginOAuthProvider::Google if issuer != GOOGLE_ISSUER => {
                return Err(OAuthLoginClientError::ProviderUnavailable);
            }
            _ => {}
        }
        let discovery = self.fetch_discovery(issuer).await?;
        if provider == LoginOAuthProvider::Telegram && !valid_telegram_discovery(&discovery) {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        if provider == LoginOAuthProvider::Google && !valid_google_discovery(&discovery) {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        Ok(discovery)
    }

    async fn fetch_discovery(
        &self,
        issuer: &str,
    ) -> Result<DiscoveryDocument, OAuthLoginClientError> {
        validate_issuer(issuer)?;
        let target = format!(
            "{}/.well-known/openid-configuration",
            issuer.trim_end_matches('/')
        );
        let client = self
            .http_clients
            .get(Some(PROVIDER_REQUEST_TIMEOUT))
            .map_err(|_| OAuthLoginClientError::ClientUnavailable)?;
        let bytes = execute_json(
            &client,
            Method::GET,
            &target,
            provider_headers(LoginOAuthProvider::Oidc, false),
            None,
        )
        .await?;
        let discovery: DiscoveryDocument = serde_json::from_slice(&bytes)
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        if !valid_discovery(&discovery, issuer) {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        Ok(discovery)
    }

    async fn fetch_jwks(
        &self,
        client: &af_httpclient::PooledClient,
        target: &str,
    ) -> Result<jsonwebtoken::jwk::JwkSet, OAuthLoginClientError> {
        let bytes = execute_json(
            client,
            Method::GET,
            target,
            provider_headers(LoginOAuthProvider::Oidc, false),
            None,
        )
        .await?;
        serde_json::from_slice(&bytes).map_err(|_| OAuthLoginClientError::ProviderUnavailable)
    }
}

/// 在异步边界外构造表单，避免 URL 编码器的非 `Sync` 类型进入请求 Future。
fn token_request_body(
    provider: LoginOAuthProvider,
    client_id: &str,
    client_secret: &str,
    code: &str,
    verifier: Option<&str>,
    redirect_uri: &str,
    include_client_credentials: bool,
) -> Body {
    let request_body = token_request_form(
        provider,
        client_id,
        client_secret,
        code,
        verifier,
        redirect_uri,
        include_client_credentials,
    );
    // Bytes 持有可清零 owner，确保传输共享引用全部释放后再擦除表单材料。
    Body::from(Bytes::from_owner(Zeroizing::new(
        request_body.as_bytes().to_vec(),
    )))
}

fn token_request_form(
    provider: LoginOAuthProvider,
    client_id: &str,
    client_secret: &str,
    code: &str,
    verifier: Option<&str>,
    redirect_uri: &str,
    include_client_credentials: bool,
) -> Zeroizing<String> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri);
    if include_client_credentials {
        serializer
            .append_pair("client_id", client_id)
            .append_pair("client_secret", client_secret);
    }
    if matches!(
        provider,
        LoginOAuthProvider::Discord
            | LoginOAuthProvider::Oidc
            | LoginOAuthProvider::LinuxDo
            | LoginOAuthProvider::Telegram
            | LoginOAuthProvider::Google
    ) {
        serializer.append_pair("grant_type", "authorization_code");
    }
    if let Some(verifier) = verifier {
        serializer.append_pair("code_verifier", verifier);
    }
    Zeroizing::new(serializer.finish())
}

fn custom_token_request_body(
    config: &CustomOAuth2ProviderConfig,
    client_secret: &DecryptedSystemSecret,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Body {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("client_id", config.client_id())
        .append_pair("client_secret", client_secret.expose_secret())
        .append_pair("code_verifier", verifier);
    let form = Zeroizing::new(serializer.finish());
    Body::from(Bytes::from_owner(Zeroizing::new(form.as_bytes().to_vec())))
}

fn custom_oauth_headers(form: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        HeaderName::from_static("user-agent"),
        HeaderValue::from_static("AnyFlows-OAuth"),
    );
    if form {
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
    }
    headers
}

fn parse_custom_identity(
    config: &CustomOAuth2ProviderConfig,
    bytes: &[u8],
) -> Result<OAuthLoginIdentity, OAuthLoginClientError> {
    let user = serde_json::from_slice::<Value>(bytes)
        .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
    let subject = user
        .as_object()
        .and_then(|object| object.get(config.endpoints().subject_field()))
        .and_then(Value::as_str)
        .ok_or(OAuthLoginClientError::ProviderUnavailable)?;
    validate_oidc_subject(subject)?;
    Ok(OAuthLoginIdentity {
        subject: subject.to_owned(),
        username: subject.to_owned(),
    })
}

fn valid_custom_oauth_parameter(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_custom_redirect_uri(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    value.len() <= 2_048
        && matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && !value.chars().any(char::is_control)
}

impl fmt::Debug for OAuthLoginClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuthLoginClient(<已脱敏>)")
    }
}

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: TokenSecret,
    token_type: String,
}

#[derive(Deserialize)]
struct OidcTokenResponse {
    access_token: TokenSecret,
    token_type: String,
    id_token: Option<String>,
}

#[derive(Deserialize)]
struct OidcClaims {
    sub: String,
    exp: u64,
    nonce: Option<String>,
    preferred_username: Option<String>,
    login: Option<String>,
    name: Option<String>,
}

#[derive(Clone, Deserialize)]
struct DiscoveryDocument {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    code_challenge_methods_supported: Option<Vec<String>>,
    response_types_supported: Vec<String>,
    id_token_signing_alg_values_supported: Vec<String>,
    token_endpoint_auth_methods_supported: Option<Vec<String>>,
}

impl DiscoveryDocument {
    fn linuxdo() -> Self {
        Self {
            issuer: LINUXDO_DISCOVERY_ISSUER.to_owned(),
            authorization_endpoint: LINUXDO_AUTHORIZATION_URL.to_owned(),
            token_endpoint: LINUXDO_TOKEN_URL.to_owned(),
            jwks_uri: LINUXDO_JWKS_URL.to_owned(),
            code_challenge_methods_supported: Some(vec!["S256".to_owned()]),
            response_types_supported: vec!["code".to_owned()],
            id_token_signing_alg_values_supported: vec!["RS256".to_owned()],
            token_endpoint_auth_methods_supported: Some(vec![
                "client_secret_basic".to_owned(),
                "client_secret_post".to_owned(),
            ]),
        }
    }

    fn client_authentication(&self) -> Result<OidcClientAuthentication, OAuthLoginClientError> {
        let Some(methods) = self.token_endpoint_auth_methods_supported.as_deref() else {
            // OIDC Discovery 省略该字段时默认使用 client_secret_basic。
            return Ok(OidcClientAuthentication::Basic);
        };
        if methods.iter().any(|method| method == "client_secret_basic") {
            Ok(OidcClientAuthentication::Basic)
        } else if methods.iter().any(|method| method == "client_secret_post") {
            Ok(OidcClientAuthentication::Post)
        } else {
            Err(OAuthLoginClientError::ProviderUnavailable)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OidcClientAuthentication {
    Basic,
    Post,
}

#[derive(Deserialize)]
struct GithubUser {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
}

#[derive(Deserialize)]
struct WechatTokenResponse {
    access_token: TokenSecret,
    openid: String,
}

#[derive(Deserialize)]
struct WechatUser {
    openid: String,
}

struct TokenSecret(String);

impl TokenSecret {
    fn into_zeroizing(mut self) -> Zeroizing<String> {
        Zeroizing::new(std::mem::take(&mut self.0))
    }
}

impl<'de> Deserialize<'de> for TokenSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value = String::deserialize(deserializer)?;
        if value.is_empty()
            || value.len() > MAX_ACCESS_TOKEN_BYTES
            || value.chars().any(char::is_control)
        {
            value.zeroize();
            return Err(D::Error::custom("OAuth access token 无效"));
        }
        Ok(Self(value))
    }
}

impl Drop for TokenSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for TokenSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<已脱敏>")
    }
}

fn parse_identity(
    provider: LoginOAuthProvider,
    bytes: &[u8],
) -> Result<OAuthLoginIdentity, OAuthLoginClientError> {
    match provider {
        LoginOAuthProvider::GitHub => {
            let user: GithubUser = serde_json::from_slice(bytes)
                .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
            validate_github_login(&user.login)?;
            if user.id == 0 {
                return Err(OAuthLoginClientError::ProviderUnavailable);
            }
            Ok(OAuthLoginIdentity {
                subject: user.id.to_string(),
                username: user.login,
            })
        }
        LoginOAuthProvider::Discord => {
            let user: DiscordUser = serde_json::from_slice(bytes)
                .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
            validate_discord_subject(&user.id)?;
            validate_discord_username(&user.username)?;
            Ok(OAuthLoginIdentity {
                subject: user.id,
                username: user.username,
            })
        }
        LoginOAuthProvider::Oidc
        | LoginOAuthProvider::LinuxDo
        | LoginOAuthProvider::WeChat
        | LoginOAuthProvider::Telegram
        | LoginOAuthProvider::Google => Err(OAuthLoginClientError::ProviderUnavailable),
    }
}

fn validate_issuer(value: &str) -> Result<url::Url, OAuthLoginClientError> {
    let url = url::Url::parse(value).map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some()
        || value.len() > 2_048
        || value != value.trim_end_matches('/')
        || value.chars().any(char::is_control)
        || is_blocked_host(url.host_str().unwrap_or_default())
    {
        return Err(OAuthLoginClientError::ProviderUnavailable);
    }
    Ok(url)
}

fn valid_endpoint(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username() == ""
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.port().is_none()
        && !is_blocked_host(url.host_str().unwrap_or_default())
}

fn valid_discovery(discovery: &DiscoveryDocument, configured_issuer: &str) -> bool {
    discovery.issuer.trim_end_matches('/') == configured_issuer.trim_end_matches('/')
        && valid_endpoint(&discovery.authorization_endpoint)
        && valid_endpoint(&discovery.token_endpoint)
        && valid_endpoint(&discovery.jwks_uri)
        && discovery
            .code_challenge_methods_supported
            .as_deref()
            .is_some_and(|methods| methods.iter().any(|method| method == "S256"))
        && discovery
            .response_types_supported
            .iter()
            .any(|response_type| response_type == "code")
        && discovery
            .id_token_signing_alg_values_supported
            .iter()
            .any(|algorithm| algorithm == "RS256")
}

fn valid_telegram_discovery(discovery: &DiscoveryDocument) -> bool {
    valid_discovery(discovery, TELEGRAM_ISSUER)
        && discovery.issuer == TELEGRAM_ISSUER
        && discovery.authorization_endpoint == TELEGRAM_AUTHORIZATION_URL
        && discovery.token_endpoint == TELEGRAM_TOKEN_URL
        && discovery.jwks_uri == TELEGRAM_JWKS_URL
}

fn valid_google_discovery(discovery: &DiscoveryDocument) -> bool {
    valid_discovery(discovery, GOOGLE_ISSUER)
        && discovery.issuer == GOOGLE_ISSUER
        && discovery.authorization_endpoint == GOOGLE_AUTHORIZATION_URL
        && discovery.token_endpoint == GOOGLE_TOKEN_URL
        && discovery.jwks_uri == GOOGLE_JWKS_URL
}

fn is_blocked_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized == "localhost.localdomain"
        || normalized.parse::<IpAddr>().is_ok()
}

async fn execute_json(
    client: &af_httpclient::PooledClient,
    method: Method,
    target: &str,
    headers: HeaderMap,
    body: Option<Body>,
) -> Result<Zeroizing<Vec<u8>>, OAuthLoginClientError> {
    let response = client
        .execute(method, target, headers, body)
        .await
        .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > MAX_PROVIDER_RESPONSE_BYTES as u64)
    {
        return Err(OAuthLoginClientError::ProviderUnavailable);
    }
    let mut stream = response.into_bytes_stream();
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        let next_length = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or(OAuthLoginClientError::ProviderUnavailable)?;
        if next_length > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(OAuthLoginClientError::ProviderUnavailable);
        }
        bytes
            .try_reserve(chunk.len())
            .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn validate_oidc_subject(subject: &str) -> Result<(), OAuthLoginClientError> {
    if !subject.is_empty()
        && subject.len() <= 255
        && subject.trim() == subject
        && !subject.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

fn validate_wechat_openid(openid: &str) -> Result<(), OAuthLoginClientError> {
    if !openid.is_empty()
        && openid.len() <= 128
        && openid
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

fn validate_telegram_subject(subject: &str) -> Result<(), OAuthLoginClientError> {
    if !subject.is_empty()
        && subject.len() <= 20
        && subject.bytes().all(|byte| byte.is_ascii_digit())
        && subject.bytes().any(|byte| byte != b'0')
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

fn validate_google_subject(subject: &str) -> Result<(), OAuthLoginClientError> {
    if !subject.is_empty()
        && subject.len() <= 255
        && subject.is_ascii()
        && subject.trim() == subject
        && !subject.bytes().any(|byte| byte.is_ascii_control())
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

fn parse_wechat_token(bytes: &[u8]) -> Result<WechatTokenResponse, OAuthLoginClientError> {
    let token: WechatTokenResponse =
        serde_json::from_slice(bytes).map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
    validate_wechat_openid(&token.openid)?;
    Ok(token)
}

fn parse_wechat_identity(
    token_openid: &str,
    bytes: &[u8],
) -> Result<OAuthLoginIdentity, OAuthLoginClientError> {
    validate_wechat_openid(token_openid)?;
    let user: WechatUser =
        serde_json::from_slice(bytes).map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
    validate_wechat_openid(&user.openid)?;
    if user.openid != token_openid {
        return Err(OAuthLoginClientError::ProviderUnavailable);
    }
    Ok(OAuthLoginIdentity {
        subject: user.openid,
        username: "wechat".to_owned(),
    })
}

/// 将外部 subject 绑定到 issuer 命名空间，避免不同企业 IdP 的相同 sub 错误合并。
fn oidc_subject_namespace(issuer: &str, subject: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"oidc-subject-v1");
    hasher.update([0]);
    hasher.update(issuer.as_bytes());
    hasher.update([0]);
    hasher.update(subject.as_bytes());
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn validate_oidc_username(username: &str) -> Result<(), OAuthLoginClientError> {
    if !username.is_empty()
        && username.len() <= 128
        && username.trim() == username
        && !username.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

fn insert_basic_client_auth(
    headers: &mut HeaderMap,
    client_id: &str,
    client_secret: &str,
) -> Result<(), OAuthLoginClientError> {
    let client_id = encode_form_component(client_id);
    let client_secret = encode_form_component(client_secret);
    let mut credentials = Zeroizing::new(String::with_capacity(
        client_id.len() + client_secret.len() + 1,
    ));
    credentials.push_str(client_id.as_str());
    credentials.push(':');
    credentials.push_str(client_secret.as_str());
    let encoded = Zeroizing::new(STANDARD.encode(credentials.as_bytes()));
    let mut authorization = Zeroizing::new(String::with_capacity(encoded.len() + 6));
    authorization.push_str("Basic ");
    authorization.push_str(encoded.as_str());
    let mut value = HeaderValue::from_str(authorization.as_str())
        .map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
    value.set_sensitive(true);
    headers.insert(HeaderName::from_static("authorization"), value);
    Ok(())
}

fn encode_form_component(value: &str) -> Zeroizing<String> {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("", value);
    let mut encoded = Zeroizing::new(serializer.finish());
    debug_assert!(encoded.starts_with('='));
    encoded.remove(0);
    encoded
}

fn sensitive_query_url(
    base: &str,
    values: &[(&str, &str)],
) -> Result<Zeroizing<String>, OAuthLoginClientError> {
    let mut url = url::Url::parse(base).map_err(|_| OAuthLoginClientError::ProviderUnavailable)?;
    url.query_pairs_mut().extend_pairs(values.iter().copied());
    Ok(Zeroizing::new(url.into()))
}

fn provider_headers(provider: LoginOAuthProvider, form: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        HeaderName::from_static("user-agent"),
        HeaderValue::from_static("AnyFlows-OAuth"),
    );
    if matches!(provider, LoginOAuthProvider::GitHub) && !form {
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            HeaderName::from_static("x-github-api-version"),
            HeaderValue::from_static(GITHUB_API_VERSION),
        );
    }
    if form {
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
    }
    headers
}

fn validate_github_login(login: &str) -> Result<(), OAuthLoginClientError> {
    if !login.is_empty()
        && login.len() <= 39
        && login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

fn validate_discord_subject(subject: &str) -> Result<(), OAuthLoginClientError> {
    if !subject.is_empty()
        && subject.len() <= 20
        && subject.bytes().all(|byte| byte.is_ascii_digit())
        && subject.bytes().any(|byte| byte != b'0')
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

fn validate_discord_username(username: &str) -> Result<(), OAuthLoginClientError> {
    if !username.is_empty()
        && username.len() <= 64
        && username.trim() == username
        && !username.chars().any(char::is_control)
    {
        Ok(())
    } else {
        Err(OAuthLoginClientError::ProviderUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oauth::CustomOAuth2EndpointBundle;

    fn custom_config(enabled: bool) -> CustomOAuth2ProviderConfig {
        let endpoints = CustomOAuth2EndpointBundle::new(
            "https://login.example.test/oauth/authorize".to_owned(),
            "https://login.example.test/oauth/token".to_owned(),
            "https://login.example.test/oauth/userinfo".to_owned(),
            "openid profile".to_owned(),
            "sub".to_owned(),
        )
        .unwrap();
        CustomOAuth2ProviderConfig::new(
            "custom_enterprise".to_owned(),
            "企业登录".to_owned(),
            "client-id".to_owned(),
            endpoints,
            enabled,
        )
        .unwrap()
    }

    #[test]
    fn custom_authorization_url_uses_only_fixed_pkce_parameters() {
        let config = custom_config(true);
        let authorization = OAuthLoginClient::custom_authorization_url(
            &config,
            "https://anyflows.example.test/oauth/custom/callback",
            "state-value",
            "challenge-value",
        )
        .unwrap();
        let parsed = url::Url::parse(&authorization).unwrap();
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(params.get("client_id"), Some(&"client-id".to_owned()));
        assert_eq!(params.get("response_type"), Some(&"code".to_owned()));
        assert_eq!(params.get("scope"), Some(&"openid profile".to_owned()));
        assert_eq!(
            params.get("code_challenge_method"),
            Some(&"S256".to_owned())
        );
        assert_eq!(params.get("state"), Some(&"state-value".to_owned()));
        assert!(!authorization.contains("client_secret"));
        assert!(
            OAuthLoginClient::custom_authorization_url(
                &config,
                "https://anyflows.example.test/oauth/custom/callback?unsafe=1",
                "state-value",
                "challenge-value",
            )
            .is_err()
        );
        assert!(
            OAuthLoginClient::custom_authorization_url(
                &custom_config(false),
                "https://anyflows.example.test/oauth/custom/callback",
                "state-value",
                "challenge-value",
            )
            .is_err()
        );
    }

    #[test]
    fn custom_identity_keeps_only_configured_top_level_subject() {
        let config = custom_config(true);
        let identity = parse_custom_identity(
            &config,
            br#"{"sub":"external-123","email":"private@example.test","nested":{"sub":"wrong"}}"#,
        )
        .unwrap();
        assert_eq!(identity.subject(), "external-123");
        assert_eq!(identity.username(), "external-123");
        assert!(parse_custom_identity(&config, br#"{"sub":123}"#).is_err());
        assert!(parse_custom_identity(&config, br#"{"nested":{"sub":"external-123"}}"#).is_err());
    }

    #[test]
    fn token_secret_and_client_debug_are_redacted() {
        let response: OAuthTokenResponse = serde_json::from_str(
            r#"{"access_token":"oauth-sensitive-token","token_type":"bearer"}"#,
        )
        .unwrap();
        assert!(!format!("{:?}", response.access_token).contains("oauth-sensitive-token"));
        assert!(
            serde_json::from_str::<OAuthTokenResponse>(
                r#"{"access_token":"bad\ntoken","token_type":"bearer"}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn provider_identities_require_closed_subject_and_username_shapes() {
        assert!(validate_github_login("octocat").is_ok());
        assert!(validate_github_login("bad_login").is_err());
        assert!(validate_discord_subject("80351110224678912").is_ok());
        assert!(validate_discord_subject("discord-user").is_err());
        assert!(validate_discord_subject("0000").is_err());
        assert!(validate_discord_username("discord_user").is_ok());
        assert!(validate_discord_username("bad\nuser").is_err());
    }

    #[test]
    fn discord_user_response_keeps_only_stable_identity_fields() {
        let identity = parse_identity(
            LoginOAuthProvider::Discord,
            br#"{"id":"80351110224678912","username":"discord_user","global_name":"Visible Name","email":"private@example.com"}"#,
        )
        .unwrap();
        assert_eq!(identity.subject(), "80351110224678912");
        assert_eq!(identity.username(), "discord_user");
        assert_eq!(
            LoginOAuthProvider::from_id("discord"),
            Some(LoginOAuthProvider::Discord)
        );
        assert_eq!(LoginOAuthProvider::from_id("custom"), None);
    }

    #[test]
    fn wechat_provider_uses_only_fixed_official_endpoints() {
        let provider = LoginOAuthProvider::from_id("wechat").unwrap();
        assert_eq!(provider, LoginOAuthProvider::WeChat);
        assert_eq!(
            provider.authorization_url(),
            "https://open.weixin.qq.com/connect/qrconnect"
        );
        assert_eq!(
            provider.token_url(),
            "https://api.weixin.qq.com/sns/oauth2/access_token"
        );
        assert_eq!(
            provider.user_url(),
            "https://api.weixin.qq.com/sns/userinfo"
        );
        assert!(!provider.uses_pkce());
        assert!(!provider.is_oidc());
    }

    #[test]
    fn wechat_responses_require_matching_closed_openids() {
        let token = parse_wechat_token(
            br#"{"access_token":"wechat-sensitive-token","openid":"oAbC_123-def"}"#,
        )
        .unwrap();
        assert!(!format!("{:?}", token.access_token).contains("wechat-sensitive-token"));

        let identity = parse_wechat_identity(
            &token.openid,
            r#"{"openid":"oAbC_123-def","nickname":"不可信昵称","unionid":"not-used"}"#.as_bytes(),
        )
        .unwrap();
        assert_eq!(identity.subject(), "oAbC_123-def");
        assert_eq!(identity.username(), "wechat");

        assert_eq!(
            parse_wechat_identity(&token.openid, br#"{"openid":"other-openid"}"#),
            Err(OAuthLoginClientError::ProviderUnavailable)
        );
        assert_eq!(
            parse_wechat_token(br#"{"access_token":"token","openid":"bad openid"}"#).err(),
            Some(OAuthLoginClientError::ProviderUnavailable)
        );
        assert_eq!(
            parse_wechat_token(br#"{"errcode":40029,"errmsg":"invalid code"}"#).err(),
            Some(OAuthLoginClientError::ProviderUnavailable)
        );
    }

    #[test]
    fn telegram_oidc_keeps_fixed_official_metadata_and_closed_subjects() {
        let provider = LoginOAuthProvider::from_id("telegram").unwrap();
        assert_eq!(provider, LoginOAuthProvider::Telegram);
        assert_eq!(provider.fixed_issuer(), Some(TELEGRAM_ISSUER));
        assert_eq!(provider.authorization_url(), TELEGRAM_AUTHORIZATION_URL);
        assert_eq!(provider.token_url(), TELEGRAM_TOKEN_URL);
        assert!(provider.uses_pkce());
        assert!(provider.is_oidc());

        let discovery = DiscoveryDocument {
            issuer: TELEGRAM_ISSUER.to_owned(),
            authorization_endpoint: TELEGRAM_AUTHORIZATION_URL.to_owned(),
            token_endpoint: TELEGRAM_TOKEN_URL.to_owned(),
            jwks_uri: TELEGRAM_JWKS_URL.to_owned(),
            code_challenge_methods_supported: Some(vec!["plain".to_owned(), "S256".to_owned()]),
            response_types_supported: vec!["code".to_owned()],
            id_token_signing_alg_values_supported: vec!["RS256".to_owned(), "ES256".to_owned()],
            token_endpoint_auth_methods_supported: Some(vec!["client_secret_basic".to_owned()]),
        };
        assert!(valid_telegram_discovery(&discovery));
        let mut tampered = discovery.clone();
        tampered.token_endpoint = "https://attacker.example.com/token".to_owned();
        assert!(!valid_telegram_discovery(&tampered));

        assert!(validate_telegram_subject("80351110224678912").is_ok());
        assert!(validate_telegram_subject("0000").is_err());
        assert!(validate_telegram_subject("telegram-user").is_err());
    }

    #[test]
    fn google_oidc_keeps_fixed_official_metadata_and_stable_subjects() {
        let provider = LoginOAuthProvider::from_id("google").unwrap();
        assert_eq!(provider, LoginOAuthProvider::Google);
        assert_eq!(provider.fixed_issuer(), Some(GOOGLE_ISSUER));
        assert_eq!(provider.authorization_url(), GOOGLE_AUTHORIZATION_URL);
        assert_eq!(provider.token_url(), GOOGLE_TOKEN_URL);
        assert!(provider.uses_pkce());
        assert!(provider.is_oidc());

        let discovery = DiscoveryDocument {
            issuer: GOOGLE_ISSUER.to_owned(),
            authorization_endpoint: GOOGLE_AUTHORIZATION_URL.to_owned(),
            token_endpoint: GOOGLE_TOKEN_URL.to_owned(),
            jwks_uri: GOOGLE_JWKS_URL.to_owned(),
            code_challenge_methods_supported: Some(vec!["plain".to_owned(), "S256".to_owned()]),
            response_types_supported: vec!["code".to_owned()],
            id_token_signing_alg_values_supported: vec!["RS256".to_owned()],
            token_endpoint_auth_methods_supported: Some(vec![
                "client_secret_post".to_owned(),
                "client_secret_basic".to_owned(),
            ]),
        };
        assert!(valid_google_discovery(&discovery));
        let mut tampered = discovery.clone();
        tampered.jwks_uri = "https://attacker.example.com/certs".to_owned();
        assert!(!valid_google_discovery(&tampered));

        assert!(validate_google_subject("110169484474386276334").is_ok());
        assert!(validate_google_subject("").is_err());
        assert!(validate_google_subject("google user").is_ok());
        assert!(validate_google_subject("google\nuser").is_err());
        assert!(validate_google_subject("谷歌用户").is_err());
    }

    #[test]
    fn oidc_subject_is_namespaced_by_issuer() {
        let first = oidc_subject_namespace("https://idp.example.com", "same-subject");
        let second = oidc_subject_namespace("https://other.example.com", "same-subject");
        assert_eq!(first.len(), 64);
        assert_ne!(first, second);
        assert_eq!(
            first,
            oidc_subject_namespace("https://idp.example.com", "same-subject")
        );
        assert_eq!(
            first,
            oidc_subject_namespace(
                "https://idp.example.com/".trim_end_matches('/'),
                "same-subject"
            )
        );
    }

    #[test]
    fn oidc_issuer_rejects_local_or_ambiguous_targets() {
        assert!(validate_issuer("https://idp.example.com").is_ok());
        assert!(validate_issuer("https://localhost").is_err());
        assert!(validate_issuer("https://127.0.0.1").is_err());
        assert!(validate_issuer("https://user@idp.example.com").is_err());
        assert!(validate_issuer("https://idp.example.com?tenant=one").is_err());
        assert!(validate_issuer("https://idp.example.com/").is_err());
    }

    #[test]
    fn oidc_discovery_selects_supported_client_authentication() {
        let mut discovery = DiscoveryDocument::linuxdo();
        discovery.token_endpoint_auth_methods_supported = None;
        assert_eq!(
            discovery.client_authentication().unwrap(),
            OidcClientAuthentication::Basic
        );

        discovery.token_endpoint_auth_methods_supported = Some(vec![
            "client_secret_post".to_owned(),
            "client_secret_basic".to_owned(),
        ]);
        assert_eq!(
            discovery.client_authentication().unwrap(),
            OidcClientAuthentication::Basic
        );

        discovery.token_endpoint_auth_methods_supported =
            Some(vec!["client_secret_post".to_owned()]);
        assert_eq!(
            discovery.client_authentication().unwrap(),
            OidcClientAuthentication::Post
        );

        discovery.token_endpoint_auth_methods_supported = Some(vec!["private_key_jwt".to_owned()]);
        assert_eq!(
            discovery.client_authentication(),
            Err(OAuthLoginClientError::ProviderUnavailable)
        );
    }

    #[test]
    fn oidc_discovery_requires_pkce_code_flow_and_rs256() {
        let mut discovery = DiscoveryDocument::linuxdo();
        assert!(valid_discovery(&discovery, LINUXDO_ISSUER));

        discovery.code_challenge_methods_supported = None;
        assert!(!valid_discovery(&discovery, LINUXDO_ISSUER));
        discovery.code_challenge_methods_supported = Some(vec!["S256".to_owned()]);

        discovery.response_types_supported = vec!["id_token".to_owned()];
        assert!(!valid_discovery(&discovery, LINUXDO_ISSUER));
        discovery.response_types_supported = vec!["code".to_owned()];

        discovery.id_token_signing_alg_values_supported = vec!["ES256".to_owned()];
        assert!(!valid_discovery(&discovery, LINUXDO_ISSUER));
    }

    #[test]
    fn authorization_code_forms_keep_provider_contracts() {
        for provider in [
            LoginOAuthProvider::Discord,
            LoginOAuthProvider::Oidc,
            LoginOAuthProvider::LinuxDo,
            LoginOAuthProvider::Telegram,
            LoginOAuthProvider::Google,
        ] {
            let form = token_request_form(
                provider,
                "client-id",
                "client-secret",
                "authorization-code",
                Some("pkce-verifier"),
                "https://app.example.com/callback",
                true,
            );
            let values = url::form_urlencoded::parse(form.as_bytes())
                .into_owned()
                .collect::<std::collections::HashMap<_, _>>();
            assert_eq!(
                values.get("grant_type").map(String::as_str),
                Some("authorization_code")
            );
            assert_eq!(
                values.get("client_id").map(String::as_str),
                Some("client-id")
            );
            assert_eq!(
                values.get("client_secret").map(String::as_str),
                Some("client-secret")
            );
        }

        let form = token_request_form(
            LoginOAuthProvider::Oidc,
            "client-id",
            "client-secret",
            "authorization-code",
            Some("pkce-verifier"),
            "https://app.example.com/callback",
            false,
        );
        let values = url::form_urlencoded::parse(form.as_bytes())
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();
        assert!(!values.contains_key("client_id"));
        assert!(!values.contains_key("client_secret"));
    }

    #[test]
    fn oidc_basic_client_auth_encodes_credentials_and_marks_header_sensitive() {
        let mut headers = HeaderMap::new();
        insert_basic_client_auth(&mut headers, "client id", "secret/value").unwrap();
        let authorization = headers.get("authorization").unwrap();
        assert!(authorization.is_sensitive());
        assert_eq!(
            authorization.to_str().unwrap(),
            format!(
                "Basic {}",
                STANDARD.encode("client+id:secret%2Fvalue".as_bytes())
            )
        );
    }
}
