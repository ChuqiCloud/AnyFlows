use std::{fmt, time::Duration};

use af_httpclient::{Body, Bytes, HeaderMap, HeaderName, HeaderValue, HttpClientProvider, Method};
use serde::{Deserialize, Deserializer, de::Error as _};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::DecryptedSystemSecret;

const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_URL: &str = "https://api.github.com/user";
const GITHUB_API_VERSION: &str = "2022-11-28";
const DISCORD_TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
const DISCORD_USER_URL: &str = "https://discord.com/api/v10/users/@me";
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_PROVIDER_RESPONSE_BYTES: usize = 64 * 1_024;
const MAX_ACCESS_TOKEN_BYTES: usize = 4 * 1_024;

/// 用户登录仅允许使用这些内置 OAuth Provider，不接受运行时端点覆盖。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginOAuthProvider {
    GitHub,
    Discord,
}

impl LoginOAuthProvider {
    pub const ALL: [Self; 2] = [Self::GitHub, Self::Discord];

    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Discord => "discord",
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::GitHub => "GitHub",
            Self::Discord => "Discord",
        }
    }

    #[must_use]
    pub const fn authorization_url(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/login/oauth/authorize",
            Self::Discord => "https://discord.com/oauth2/authorize",
        }
    }

    #[must_use]
    pub const fn uses_pkce(self) -> bool {
        matches!(self, Self::GitHub)
    }

    #[must_use]
    pub const fn username_prefix(self) -> &'static str {
        match self {
            Self::GitHub => "gh",
            Self::Discord => "dc",
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
        }
    }

    const fn user_url(self) -> &'static str {
        match self {
            Self::GitHub => GITHUB_USER_URL,
            Self::Discord => DISCORD_USER_URL,
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
        let client = self
            .http_clients
            .get(Some(PROVIDER_REQUEST_TIMEOUT))
            .map_err(|_| OAuthLoginClientError::ClientUnavailable)?;
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer
            .append_pair("client_id", client_id)
            .append_pair("client_secret", client_secret.expose_secret())
            .append_pair("code", code)
            .append_pair("redirect_uri", redirect_uri);
        if matches!(provider, LoginOAuthProvider::Discord) {
            serializer.append_pair("grant_type", "authorization_code");
        }
        if let Some(verifier) = verifier {
            serializer.append_pair("code_verifier", verifier);
        }
        let request_body = Zeroizing::new(serializer.finish());
        // Bytes 持有可清零 owner，确保传输共享引用全部释放后再擦除表单材料。
        let request_body = Body::from(Bytes::from_owner(Zeroizing::new(
            request_body.as_bytes().to_vec(),
        )));
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
struct GithubUser {
    id: u64,
    login: String,
}

#[derive(Deserialize)]
struct DiscordUser {
    id: String,
    username: String,
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
    }
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
}
