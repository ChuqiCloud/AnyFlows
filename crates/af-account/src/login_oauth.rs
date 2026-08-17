use std::{fmt, time::Duration};

use af_httpclient::{Body, Bytes, HeaderMap, HeaderName, HeaderValue, HttpClientProvider, Method};
use serde::{Deserialize, Deserializer, de::Error as _};
use thiserror::Error;
use zeroize::{Zeroize as _, Zeroizing};

use crate::DecryptedSystemSecret;

const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_URL: &str = "https://api.github.com/user";
const GITHUB_API_VERSION: &str = "2022-11-28";
const PROVIDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_PROVIDER_RESPONSE_BYTES: usize = 64 * 1_024;
const MAX_ACCESS_TOKEN_BYTES: usize = 4 * 1_024;

/// GitHub 用户登录适配器解析出的稳定外部身份。
#[derive(Clone, Eq, PartialEq)]
pub struct GitHubLoginIdentity {
    subject: String,
    login: String,
}

impl GitHubLoginIdentity {
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    #[must_use]
    pub fn login(&self) -> &str {
        &self.login
    }
}

impl fmt::Debug for GitHubLoginIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitHubLoginIdentity")
            .field("subject", &self.subject)
            .field("login", &self.login)
            .finish()
    }
}

/// GitHub 用户登录受控传输的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GitHubLoginClientError {
    #[error("GitHub 登录 HTTP Client 不可用")]
    ClientUnavailable,
    #[error("GitHub 登录 Provider 请求失败")]
    ProviderUnavailable,
}

/// 只访问 GitHub 固定 token/user 端点的用户登录客户端。
#[derive(Clone)]
pub struct GitHubLoginClient {
    http_clients: HttpClientProvider,
}

impl GitHubLoginClient {
    #[must_use]
    pub fn new(http_clients: HttpClientProvider) -> Self {
        Self { http_clients }
    }

    /// 交换一次性授权码，并只保留 GitHub 数字 subject 与登录名。
    pub async fn exchange_identity(
        &self,
        client_id: &str,
        client_secret: &DecryptedSystemSecret,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<GitHubLoginIdentity, GitHubLoginClientError> {
        let client = self
            .http_clients
            .get(Some(PROVIDER_REQUEST_TIMEOUT))
            .map_err(|_| GitHubLoginClientError::ClientUnavailable)?;
        let request_body = Zeroizing::new(
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("client_id", client_id)
                .append_pair("client_secret", client_secret.expose_secret())
                .append_pair("code", code)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("code_verifier", verifier)
                .finish(),
        );
        // Bytes 持有可清零 owner，确保传输共享引用全部释放后再擦除表单材料。
        let request_body = Body::from(Bytes::from_owner(Zeroizing::new(
            request_body.as_bytes().to_vec(),
        )));
        let token_bytes = execute_json(
            &client,
            Method::POST,
            GITHUB_TOKEN_URL,
            github_headers(true),
            Some(request_body),
        )
        .await?;
        let response = serde_json::from_slice::<GithubTokenResponse>(&token_bytes)
            .map_err(|_| GitHubLoginClientError::ProviderUnavailable)?;
        if !response.token_type.eq_ignore_ascii_case("bearer") {
            return Err(GitHubLoginClientError::ProviderUnavailable);
        }
        let access_token = response.access_token.into_zeroizing();
        let mut authorization = Zeroizing::new(String::from("Bearer "));
        authorization.push_str(access_token.as_str());
        let mut headers = github_headers(false);
        let mut authorization = HeaderValue::from_str(authorization.as_str())
            .map_err(|_| GitHubLoginClientError::ProviderUnavailable)?;
        authorization.set_sensitive(true);
        headers.insert(HeaderName::from_static("authorization"), authorization);
        let user_bytes = execute_json(&client, Method::GET, GITHUB_USER_URL, headers, None).await?;
        let user: GithubUser = serde_json::from_slice(&user_bytes)
            .map_err(|_| GitHubLoginClientError::ProviderUnavailable)?;
        validate_github_login(&user.login)?;
        if user.id == 0 {
            return Err(GitHubLoginClientError::ProviderUnavailable);
        }
        Ok(GitHubLoginIdentity {
            subject: user.id.to_string(),
            login: user.login,
        })
    }
}

impl fmt::Debug for GitHubLoginClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GitHubLoginClient(<已脱敏>)")
    }
}

#[derive(Deserialize)]
struct GithubTokenResponse {
    access_token: TokenSecret,
    token_type: String,
}

#[derive(Deserialize)]
struct GithubUser {
    id: u64,
    login: String,
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
            return Err(D::Error::custom("GitHub access token 无效"));
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

async fn execute_json(
    client: &af_httpclient::PooledClient,
    method: Method,
    target: &str,
    headers: HeaderMap,
    body: Option<Body>,
) -> Result<Zeroizing<Vec<u8>>, GitHubLoginClientError> {
    let response = client
        .execute(method, target, headers, body)
        .await
        .map_err(|_| GitHubLoginClientError::ProviderUnavailable)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > MAX_PROVIDER_RESPONSE_BYTES as u64)
    {
        return Err(GitHubLoginClientError::ProviderUnavailable);
    }
    let mut stream = response.into_bytes_stream();
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(|_| GitHubLoginClientError::ProviderUnavailable)?;
        let next_length = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or(GitHubLoginClientError::ProviderUnavailable)?;
        if next_length > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(GitHubLoginClientError::ProviderUnavailable);
        }
        bytes
            .try_reserve(chunk.len())
            .map_err(|_| GitHubLoginClientError::ProviderUnavailable)?;
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn github_headers(form: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        HeaderName::from_static("user-agent"),
        HeaderValue::from_static("AnyFlows-OAuth"),
    );
    headers.insert(
        HeaderName::from_static("x-github-api-version"),
        HeaderValue::from_static(GITHUB_API_VERSION),
    );
    if form {
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
    }
    headers
}

fn validate_github_login(login: &str) -> Result<(), GitHubLoginClientError> {
    if !login.is_empty()
        && login.len() <= 39
        && login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(GitHubLoginClientError::ProviderUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_secret_and_client_debug_are_redacted() {
        let response: GithubTokenResponse = serde_json::from_str(
            r#"{"access_token":"github-sensitive-token","token_type":"bearer"}"#,
        )
        .unwrap();
        assert!(!format!("{:?}", response.access_token).contains("github-sensitive-token"));
        assert!(
            serde_json::from_str::<GithubTokenResponse>(
                r#"{"access_token":"bad\ntoken","token_type":"bearer"}"#,
            )
            .is_err()
        );
    }

    #[test]
    fn github_identity_requires_a_closed_login_shape() {
        assert!(validate_github_login("octocat").is_ok());
        assert!(validate_github_login("bad_login").is_err());
        assert!(validate_github_login(&"a".repeat(40)).is_err());
    }
}
