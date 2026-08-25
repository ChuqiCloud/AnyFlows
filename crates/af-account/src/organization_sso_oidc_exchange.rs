//! 企业 OIDC token exchange 的受控配置与输入契约。
//!
//! 本模块在协议适配器边界校验配置和一次性材料，并只通过受控 Client 发起
//! 固定授权码交换；不持久化授权码、PKCE verifier 或 Token，也不负责身份绑定。

use std::{fmt, mem, net::IpAddr, time::Duration};

use af_db::EncryptedCredentialEnvelope;
use af_domain::{OrganizationId, OrganizationSsoProtocol, OrganizationSsoProviderKey};
use af_httpclient::{Body, Bytes, HeaderMap, HeaderName, HeaderValue, Method, PooledClient};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::StreamExt as _;
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Value;
use thiserror::Error;
use url::Url;
use zeroize::Zeroizing;

use crate::ORGANIZATION_SSO_OIDC_CALLBACK_PATH;
use crate::system_secret::SystemSecretCipher;

const MAX_ENDPOINT_BYTES: usize = 2_048;
const MAX_CLIENT_ID_BYTES: usize = 255;
const MAX_CODE_BYTES: usize = 4_096;
const MAX_CODE_VERIFIER_BYTES: usize = 512;
const MAX_CLIENT_SECRET_BYTES: usize = 16 * 1_024;
const MAX_TOKEN_REQUEST_BYTES: usize = 32 * 1_024;
const MAX_TOKEN_RESPONSE_BYTES: usize = 64 * 1_024;
const MAX_TOKEN_BYTES: usize = 64 * 1_024;
const MAX_SCOPE_BYTES: usize = 4_096;

/// OIDC Provider 版本绑定的 token exchange 配置快照。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoOidcTokenExchangeConfig {
    provider_version: i64,
    issuer: Url,
    token_endpoint: Url,
    jwks_uri: Url,
    client_id: String,
    audience: String,
    redirect_uri: Url,
}

impl OrganizationSsoOidcTokenExchangeConfig {
    /// 创建不可变配置；回调地址必须指向固定企业 OIDC 回调路径。
    pub fn new(
        provider_version: i64,
        issuer: String,
        token_endpoint: String,
        jwks_uri: String,
        client_id: String,
        audience: String,
        redirect_uri: String,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        if provider_version <= 0 {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        let issuer = parse_https_url(&issuer)?;
        let token_endpoint = parse_https_url(&token_endpoint)?;
        let jwks_uri = parse_https_url(&jwks_uri)?;
        let redirect_uri = parse_https_url(&redirect_uri)?;
        if redirect_uri.path() != ORGANIZATION_SSO_OIDC_CALLBACK_PATH
            || redirect_uri.query().is_some()
            || redirect_uri.fragment().is_some()
        {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        if !valid_ascii(&client_id, MAX_CLIENT_ID_BYTES)
            || !valid_ascii(&audience, MAX_CLIENT_ID_BYTES)
        {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        Ok(Self {
            provider_version,
            issuer,
            token_endpoint,
            jwks_uri,
            client_id,
            audience,
            redirect_uri,
        })
    }

    #[must_use]
    pub const fn provider_version(&self) -> i64 {
        self.provider_version
    }

    #[must_use]
    pub fn issuer(&self) -> &Url {
        &self.issuer
    }

    #[must_use]
    pub fn token_endpoint(&self) -> &Url {
        &self.token_endpoint
    }

    /// 返回与当前 Provider 版本绑定的受控 JWKS 地址。
    #[must_use]
    pub fn jwks_uri(&self) -> &Url {
        &self.jwks_uri
    }

    #[must_use]
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    #[must_use]
    pub fn audience(&self) -> &str {
        &self.audience
    }

    #[must_use]
    pub fn redirect_uri(&self) -> &Url {
        &self.redirect_uri
    }

    /// 从 Provider 脱敏元数据投影构造严格的交换配置。
    pub fn from_metadata_projection(
        provider_version: i64,
        projection: &Value,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        let object = projection
            .as_object()
            .ok_or(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration)?;
        const KEYS: [&str; 7] = [
            "issuer",
            "token_endpoint",
            "jwks_uri",
            "client_id",
            "audience",
            "redirect_uri",
            "protocol",
        ];
        if object.len() != KEYS.len() || object.keys().any(|key| !KEYS.contains(&key.as_str())) {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        if object.get("protocol").and_then(Value::as_str) != Some("oidc") {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
        }
        let string = |key: &str| {
            object
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration)
        };
        Self::new(
            provider_version,
            string("issuer")?,
            string("token_endpoint")?,
            string("jwks_uri")?,
            string("client_id")?,
            string("audience")?,
            string("redirect_uri")?,
        )
    }
}

impl fmt::Debug for OrganizationSsoOidcTokenExchangeConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcTokenExchangeConfig")
            .field("provider_version", &self.provider_version)
            .field("issuer", &"<redacted>")
            .field("token_endpoint", &"<redacted>")
            .field("jwks_uri", &"<redacted>")
            .field("client_id", &"<redacted>")
            .field("audience", &"<redacted>")
            .field("redirect_uri", &"<redacted>")
            .finish()
    }
}

/// 单次 token exchange 所需的授权码和 PKCE verifier。
#[derive(Eq, PartialEq)]
pub struct OrganizationSsoOidcTokenExchangeInput {
    code: Zeroizing<String>,
    code_verifier: Zeroizing<String>,
}

/// 受控企业 OIDC 授权码交换器；明文密钥和 Token 仅在本次调用内存中存在。
pub struct OrganizationSsoOidcTokenExchangeClient {
    config: OrganizationSsoOidcTokenExchangeConfig,
    client_secret: Zeroizing<String>,
}

impl OrganizationSsoOidcTokenExchangeClient {
    /// 使用已经完成版本绑定的配置和明文密钥构造交换器。
    pub fn new(
        config: OrganizationSsoOidcTokenExchangeConfig,
        client_secret: String,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        Self::from_zeroizing(config, Zeroizing::new(client_secret))
    }

    fn from_zeroizing(
        config: OrganizationSsoOidcTokenExchangeConfig,
        client_secret: Zeroizing<String>,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        if !valid_client_secret(&client_secret) {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidInput);
        }
        Ok(Self {
            config,
            client_secret,
        })
    }

    /// 按企业、Provider、协议和版本 AAD 解密 Client Secret 后构造交换器。
    pub fn from_encrypted_secret(
        cipher: &SystemSecretCipher,
        organization_id: OrganizationId,
        provider_key: OrganizationSsoProviderKey,
        config: OrganizationSsoOidcTokenExchangeConfig,
        envelope: &EncryptedCredentialEnvelope,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        let secret = cipher
            .decrypt_organization_sso_provider_secret(
                organization_id,
                provider_key,
                OrganizationSsoProtocol::Oidc,
                config.provider_version(),
                envelope,
            )
            .map_err(|_| OrganizationSsoOidcTokenExchangeError::ProviderSecretUnavailable)?;
        Self::from_zeroizing(config, secret.into_zeroizing())
    }

    /// 消费一次性授权码并返回仅供后续验证使用的短生命周期 Token 集合。
    pub async fn exchange(
        &self,
        http_client: &PooledClient,
        input: OrganizationSsoOidcTokenExchangeInput,
    ) -> Result<OrganizationSsoOidcTokenSet, OrganizationSsoOidcTokenExchangeError> {
        let body = self.build_request_body(&input)?;
        drop(input);
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        let client_id = Zeroizing::<String>::new(
            url::form_urlencoded::byte_serialize(self.config.client_id().as_bytes()).collect(),
        );
        let client_secret = Zeroizing::<String>::new(
            url::form_urlencoded::byte_serialize(self.client_secret.as_bytes()).collect(),
        );
        let credentials =
            Zeroizing::new(format!("{}:{}", client_id.as_str(), client_secret.as_str()));
        let encoded = Zeroizing::new(STANDARD.encode(credentials.as_bytes()));
        let authorization_value = Zeroizing::new(format!("Basic {}", encoded.as_str()));
        let mut authorization = HeaderValue::from_str(authorization_value.as_str())
            .map_err(|_| OrganizationSsoOidcTokenExchangeError::InvalidInput)?;
        authorization.set_sensitive(true);
        headers.insert(HeaderName::from_static("authorization"), authorization);

        let response = http_client
            .execute(
                Method::POST,
                self.config.token_endpoint().as_str(),
                headers,
                Some(Body::from(Bytes::from_owner(body))),
            )
            .await
            .map_err(|_| OrganizationSsoOidcTokenExchangeError::Transport)?;
        let status = response.status();
        let body = collect_token_response(response).await?;
        if !status.is_success() {
            return Err(OrganizationSsoOidcTokenExchangeError::EndpointRejected {
                status: status.as_u16(),
            });
        }
        let wire = serde_json::from_slice::<OrganizationSsoOidcTokenResponseWire>(&body)
            .map_err(|_| OrganizationSsoOidcTokenExchangeError::MalformedResponse)?;
        OrganizationSsoOidcTokenSet::from_wire(wire)
    }

    fn build_request_body(
        &self,
        input: &OrganizationSsoOidcTokenExchangeInput,
    ) -> Result<Zeroizing<Vec<u8>>, OrganizationSsoOidcTokenExchangeError> {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        serializer
            .append_pair("grant_type", "authorization_code")
            .append_pair("code", input.code())
            .append_pair("redirect_uri", self.config.redirect_uri().as_str())
            .append_pair("client_id", self.config.client_id())
            .append_pair("code_verifier", input.code_verifier());
        let body = Zeroizing::new(serializer.finish().into_bytes());
        if body.len() > MAX_TOKEN_REQUEST_BYTES {
            return Err(OrganizationSsoOidcTokenExchangeError::RequestTooLarge);
        }
        Ok(body)
    }
}

impl fmt::Debug for OrganizationSsoOidcTokenExchangeClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcTokenExchangeClient")
            .field("config", &self.config)
            .field("client_secret", &"<redacted>")
            .finish()
    }
}

/// OIDC token endpoint 返回的短生命周期结果；不实现序列化，避免误持久化。
pub struct OrganizationSsoOidcTokenSet {
    access_token: Zeroizing<String>,
    refresh_token: Option<Zeroizing<String>>,
    id_token: Zeroizing<String>,
    scope: Option<Zeroizing<String>>,
    expires_in: Option<Duration>,
}

impl OrganizationSsoOidcTokenSet {
    fn from_wire(
        wire: OrganizationSsoOidcTokenResponseWire,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        if !wire.token_type.eq_ignore_ascii_case("bearer")
            || !valid_token(&wire.access_token)
            || wire.expires_in.is_some_and(|expires_in| expires_in == 0)
            || wire.scope.as_ref().is_some_and(|scope| !valid_scope(scope))
        {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidTokenResponse);
        }
        Ok(Self {
            access_token: wire.access_token.into_zeroizing(),
            refresh_token: wire.refresh_token.map(SsoTokenSecret::into_zeroizing),
            id_token: wire.id_token.into_zeroizing(),
            scope: wire.scope.map(SsoTokenSecret::into_zeroizing),
            expires_in: wire.expires_in.map(Duration::from_secs),
        })
    }

    #[must_use]
    pub fn access_token(&self) -> &str {
        self.access_token.as_str()
    }

    #[must_use]
    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref().map(String::as_str)
    }

    #[must_use]
    pub fn id_token(&self) -> &str {
        self.id_token.as_str()
    }

    #[must_use]
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref().map(String::as_str)
    }

    #[must_use]
    pub const fn expires_in(&self) -> Option<Duration> {
        self.expires_in
    }
}

impl fmt::Debug for OrganizationSsoOidcTokenSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcTokenSet")
            .field("access_token", &"<redacted>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("id_token", &"<redacted>")
            .field("scope", &self.scope.as_ref().map(|_| "<redacted>"))
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

impl OrganizationSsoOidcTokenExchangeInput {
    /// 校验并接管只在本次交换中使用的敏感材料。
    pub fn new(
        code: String,
        code_verifier: String,
    ) -> Result<Self, OrganizationSsoOidcTokenExchangeError> {
        if !valid_ascii(&code, MAX_CODE_BYTES)
            || !valid_ascii(&code_verifier, MAX_CODE_VERIFIER_BYTES)
        {
            return Err(OrganizationSsoOidcTokenExchangeError::InvalidInput);
        }
        Ok(Self {
            code: Zeroizing::new(code),
            code_verifier: Zeroizing::new(code_verifier),
        })
    }

    #[must_use]
    pub fn code(&self) -> &str {
        self.code.as_str()
    }

    #[must_use]
    pub fn code_verifier(&self) -> &str {
        self.code_verifier.as_str()
    }
}

impl fmt::Debug for OrganizationSsoOidcTokenExchangeInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcTokenExchangeInput")
            .field("code", &"<redacted>")
            .field("code_verifier", &"<redacted>")
            .finish()
    }
}

/// 配置与交换输入的稳定拒绝分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoOidcTokenExchangeError {
    #[error("企业 OIDC token exchange 配置无效")]
    InvalidConfiguration,
    #[error("企业 OIDC token exchange 输入无效")]
    InvalidInput,
    #[error("企业 OIDC Provider 密钥不可用")]
    ProviderSecretUnavailable,
    #[error("企业 OIDC token exchange 请求超过长度上限")]
    RequestTooLarge,
    #[error("企业 OIDC token exchange 传输失败")]
    Transport,
    #[error("企业 OIDC token endpoint 拒绝请求（HTTP {status}）")]
    EndpointRejected { status: u16 },
    #[error("企业 OIDC token 响应超过长度上限")]
    ResponseTooLarge,
    #[error("企业 OIDC token 响应不是有效 JSON")]
    MalformedResponse,
    #[error("企业 OIDC token 响应语义无效")]
    InvalidTokenResponse,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrganizationSsoOidcTokenResponseWire {
    access_token: SsoTokenSecret,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<SsoTokenSecret>,
    id_token: SsoTokenSecret,
    #[serde(default)]
    scope: Option<SsoTokenSecret>,
}

struct SsoTokenSecret(String);

impl SsoTokenSecret {
    fn into_zeroizing(mut self) -> Zeroizing<String> {
        Zeroizing::new(mem::take(&mut self.0))
    }
}

impl<'de> Deserialize<'de> for SsoTokenSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value = String::deserialize(deserializer)?;
        if value.is_empty() || value.len() > MAX_TOKEN_BYTES {
            zeroize::Zeroize::zeroize(&mut value);
            return Err(D::Error::custom("企业 OIDC token 无效"));
        }
        Ok(Self(value))
    }
}

impl Drop for SsoTokenSecret {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.0);
    }
}

async fn collect_token_response(
    response: af_httpclient::HttpResponse,
) -> Result<Zeroizing<Vec<u8>>, OrganizationSsoOidcTokenExchangeError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TOKEN_RESPONSE_BYTES as u64)
    {
        return Err(OrganizationSsoOidcTokenExchangeError::ResponseTooLarge);
    }
    let mut stream = response.into_bytes_stream();
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| OrganizationSsoOidcTokenExchangeError::Transport)?;
        let next_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or(OrganizationSsoOidcTokenExchangeError::ResponseTooLarge)?;
        if next_len > MAX_TOKEN_RESPONSE_BYTES {
            return Err(OrganizationSsoOidcTokenExchangeError::ResponseTooLarge);
        }
        body.try_reserve(chunk.len())
            .map_err(|_| OrganizationSsoOidcTokenExchangeError::ResponseTooLarge)?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn valid_client_secret(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CLIENT_SECRET_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn valid_token(value: &SsoTokenSecret) -> bool {
    !value.0.is_empty()
        && value.0.len() <= MAX_TOKEN_BYTES
        && value.0.is_ascii()
        && !value
            .0
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

fn valid_scope(value: &SsoTokenSecret) -> bool {
    !value.0.is_empty()
        && value.0.len() <= MAX_SCOPE_BYTES
        && value.0.is_ascii()
        && value.0.split(' ').all(|segment| {
            !segment.is_empty() && segment.bytes().all(|byte| byte.is_ascii_graphic())
        })
}

fn parse_https_url(value: &str) -> Result<Url, OrganizationSsoOidcTokenExchangeError> {
    if value.is_empty() || value.len() > MAX_ENDPOINT_BYTES || value.trim() != value {
        return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
    }
    let url = Url::parse(value)
        .map_err(|_| OrganizationSsoOidcTokenExchangeError::InvalidConfiguration)?;
    if url.as_str() != value
        || url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || blocked_endpoint_host(url.host_str().unwrap_or_default())
    {
        return Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration);
    }
    Ok(url)
}

fn blocked_endpoint_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized == "localhost.localdomain"
        || normalized.parse::<IpAddr>().is_ok()
}

fn valid_ascii(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        io::{Read as _, Write as _},
        net::{SocketAddr, TcpListener, TcpStream},
        thread::{self, JoinHandle},
        time::Duration,
    };

    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use af_httpclient::{
        HttpClientConfig, HttpClientPool, HttpTimeouts, PooledClient, ProxyConfig, RemoteDnsPolicy,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde_json::json;

    use super::*;
    use crate::PlainSystemSecret;

    fn config() -> OrganizationSsoOidcTokenExchangeConfig {
        OrganizationSsoOidcTokenExchangeConfig::new(
            7,
            "https://idp.example.test/".to_owned(),
            "https://idp.example.test/oauth2/token".to_owned(),
            "https://idp.example.test/oauth2/jwks".to_owned(),
            "client-id".to_owned(),
            "client-id".to_owned(),
            "https://app.example.test/api/auth/organization-sso/oidc/callback".to_owned(),
        )
        .unwrap()
    }

    #[test]
    fn config_binds_version_and_fixed_callback_path() {
        let value = config();
        assert_eq!(value.provider_version(), 7);
        assert_eq!(value.client_id(), "client-id");
        assert!(
            OrganizationSsoOidcTokenExchangeConfig::new(
                7,
                "https://idp.example.test".to_owned(),
                "https://idp.example.test/oauth2/token?unsafe=1".to_owned(),
                "https://idp.example.test/oauth2/jwks".to_owned(),
                "client-id".to_owned(),
                "client-id".to_owned(),
                "https://app.example.test/api/auth/organization-sso/oidc/callback".to_owned(),
            )
            .is_err()
        );
        assert!(
            OrganizationSsoOidcTokenExchangeConfig::new(
                7,
                "https://idp.example.test".to_owned(),
                "https://127.0.0.1/oauth2/token".to_owned(),
                "https://idp.example.test/oauth2/jwks".to_owned(),
                "client-id".to_owned(),
                "client-id".to_owned(),
                "https://app.example.test/api/auth/organization-sso/oidc/callback".to_owned(),
            )
            .is_err()
        );
        assert!(
            OrganizationSsoOidcTokenExchangeConfig::new(
                7,
                "https://idp.example.test".to_owned(),
                "https://idp.example.test/oauth2/token".to_owned(),
                "https://idp.example.test/oauth2/jwks".to_owned(),
                "client-id".to_owned(),
                "client-id".to_owned(),
                "https://app.example.test/unsafe-callback".to_owned(),
            )
            .is_err()
        );
    }

    #[test]
    fn input_redacts_sensitive_material() {
        let input = OrganizationSsoOidcTokenExchangeInput::new(
            "authorization-code".to_owned(),
            "pkce-verifier".to_owned(),
        )
        .unwrap();
        assert_eq!(input.code(), "authorization-code");
        assert_eq!(input.code_verifier(), "pkce-verifier");
        let debug = format!("{input:?}");
        assert!(!debug.contains("authorization-code"));
        assert!(!debug.contains("pkce-verifier"));
    }

    #[test]
    fn metadata_projection_is_closed_and_requires_oidc_shape() {
        let projection = json!({
            "issuer": "https://idp.example.test/",
            "token_endpoint": "https://idp.example.test/oauth2/token",
            "jwks_uri": "https://idp.example.test/oauth2/jwks",
            "client_id": "client-id",
            "audience": "client-id",
            "redirect_uri": "https://app.example.test/api/auth/organization-sso/oidc/callback",
            "protocol": "oidc"
        });
        assert_eq!(
            OrganizationSsoOidcTokenExchangeConfig::from_metadata_projection(7, &projection)
                .unwrap(),
            config()
        );
        for invalid in [
            json!({
                "issuer": "https://idp.example.test/",
                "token_endpoint": "https://idp.example.test/oauth2/token",
                "jwks_uri": "https://idp.example.test/oauth2/jwks",
                "client_id": "client-id",
                "audience": "client-id",
                "redirect_uri": "https://app.example.test/api/auth/organization-sso/oidc/callback",
                "protocol": "saml"
            }),
            json!({
                "issuer": "https://idp.example.test/",
                "token_endpoint": "https://idp.example.test/oauth2/token",
                "client_id": "client-id",
                "audience": "client-id",
                "redirect_uri": "https://app.example.test/api/auth/organization-sso/oidc/callback",
                "protocol": "oidc",
                "unsafe_override": "https://127.0.0.1/"
            }),
        ] {
            assert_eq!(
                OrganizationSsoOidcTokenExchangeConfig::from_metadata_projection(7, &invalid),
                Err(OrganizationSsoOidcTokenExchangeError::InvalidConfiguration)
            );
        }
    }

    #[test]
    fn encrypted_secret_is_bound_to_org_provider_protocol_and_version() {
        let cipher = SystemSecretCipher::new(&settings()).unwrap();
        let organization = OrganizationId::new(7).unwrap();
        let other_organization = OrganizationId::new(8).unwrap();
        let provider = OrganizationSsoProviderKey::new([0x31; 16]).unwrap();
        let other_provider = OrganizationSsoProviderKey::new([0x32; 16]).unwrap();
        let secret = PlainSystemSecret::new("client-secret-marker".to_owned()).unwrap();
        let envelope = cipher
            .encrypt_organization_sso_provider_secret(
                organization,
                provider,
                OrganizationSsoProtocol::Oidc,
                7,
                &secret,
            )
            .unwrap();

        let client = OrganizationSsoOidcTokenExchangeClient::from_encrypted_secret(
            &cipher,
            organization,
            provider,
            config(),
            &envelope,
        )
        .unwrap();
        let rendered = format!("{client:?}{secret:?}{envelope:?}");
        assert!(!rendered.contains("client-secret-marker"));
        for (organization, provider, version) in [
            (other_organization, provider, 7),
            (organization, other_provider, 7),
            (organization, provider, 8),
        ] {
            let mut replacement = config();
            replacement.provider_version = version;
            assert_eq!(
                OrganizationSsoOidcTokenExchangeClient::from_encrypted_secret(
                    &cipher,
                    organization,
                    provider,
                    replacement,
                    &envelope,
                )
                .unwrap_err(),
                OrganizationSsoOidcTokenExchangeError::ProviderSecretUnavailable
            );
        }
    }

    #[tokio::test]
    async fn exchange_sends_closed_form_and_redacts_tokens() {
        let (address, server) = spawn_json_server(
            200,
            br#"{"access_token":"access-token-marker","token_type":"Bearer","expires_in":3600,"refresh_token":"refresh-token-marker","id_token":"id-token-marker","scope":"openid profile"}"#,
            |request| {
                assert!(
                    request.starts_with(
                        "POST http://token.example.test/oauth2/token HTTP/1.1"
                    )
                );
                assert_eq!(
                    header_value(request, "content-type"),
                    Some("application/x-www-form-urlencoded")
                );
                assert_eq!(header_value(request, "accept"), Some("application/json"));
                let authorization = header_value(request, "authorization").unwrap();
                let encoded = authorization.strip_prefix("Basic ").unwrap();
                assert_eq!(
                    STANDARD.decode(encoded).unwrap(),
                    b"client-id:client-secret%2Fmarker"
                );
                let values = form_values(request);
                assert_eq!(values.len(), 5);
                assert_eq!(
                    values.get("grant_type").map(String::as_str),
                    Some("authorization_code")
                );
                assert_eq!(
                    values.get("code").map(String::as_str),
                    Some("authorization-code-marker")
                );
                assert_eq!(
                    values.get("redirect_uri").map(String::as_str),
                    Some("https://app.example.test/api/auth/organization-sso/oidc/callback")
                );
                assert_eq!(
                    values.get("client_id").map(String::as_str),
                    Some("client-id")
                );
                assert_eq!(
                    values.get("code_verifier").map(String::as_str),
                    Some("pkce-verifier-marker")
                );
                for forbidden in ["client_secret", "audience", "nonce", "state"] {
                    assert!(!values.contains_key(forbidden));
                }
            },
        );
        let mut config = config();
        config.token_endpoint = Url::parse("http://token.example.test/oauth2/token").unwrap();
        let result =
            OrganizationSsoOidcTokenExchangeClient::new(config, "client-secret/marker".to_owned())
                .unwrap()
                .exchange(
                    &proxied_client(address),
                    OrganizationSsoOidcTokenExchangeInput::new(
                        "authorization-code-marker".to_owned(),
                        "pkce-verifier-marker".to_owned(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
        server.join().unwrap();

        assert_eq!(result.access_token(), "access-token-marker");
        assert_eq!(result.refresh_token(), Some("refresh-token-marker"));
        assert_eq!(result.id_token(), "id-token-marker");
        assert_eq!(result.scope(), Some("openid profile"));
        assert_eq!(result.expires_in(), Some(Duration::from_secs(3_600)));
        let rendered = format!("{result:?}");
        for secret in [
            "access-token-marker",
            "refresh-token-marker",
            "id-token-marker",
            "openid profile",
        ] {
            assert!(!rendered.contains(secret));
        }
    }

    #[test]
    fn token_response_rejects_unknown_fields_and_missing_identity_token() {
        for body in [
            br#"{"access_token":"access-token","token_type":"bearer"}"#.as_slice(),
            br#"{"access_token":"access-token","token_type":"bearer","id_token":"id-token","unsafe":"value"}"#.as_slice(),
            br#"{"access_token":"access-token","token_type":"mac","id_token":"id-token"}"#.as_slice(),
            br#"{"access_token":"access token","token_type":"bearer","id_token":"id-token"}"#.as_slice(),
            br#"{"access_token":"access-token","token_type":"bearer","id_token":"id-token","scope":"openid  profile"}"#.as_slice(),
        ] {
            let error = serde_json::from_slice::<OrganizationSsoOidcTokenResponseWire>(body)
                .map_err(|_| OrganizationSsoOidcTokenExchangeError::MalformedResponse)
                .and_then(OrganizationSsoOidcTokenSet::from_wire)
                .unwrap_err();
            assert!(
                matches!(
                    error,
                    OrganizationSsoOidcTokenExchangeError::MalformedResponse
                        | OrganizationSsoOidcTokenExchangeError::InvalidTokenResponse
                ),
                "unexpected error: {error:?}"
            );
        }
    }

    #[tokio::test]
    async fn endpoint_rejection_exposes_only_status() {
        let (address, server) = spawn_json_server(
            400,
            br#"{"error":"invalid_grant","error_description":"authorization-code-marker"}"#,
            |_| {},
        );
        let mut config = config();
        config.token_endpoint = Url::parse("http://token.example.test/oauth2/token").unwrap();
        let error =
            OrganizationSsoOidcTokenExchangeClient::new(config, "client-secret-marker".to_owned())
                .unwrap()
                .exchange(
                    &proxied_client(address),
                    OrganizationSsoOidcTokenExchangeInput::new(
                        "authorization-code-marker".to_owned(),
                        "pkce-verifier-marker".to_owned(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap_err();
        server.join().unwrap();
        assert_eq!(
            error,
            OrganizationSsoOidcTokenExchangeError::EndpointRejected { status: 400 }
        );
        let rendered = format!("{error:?}{error}");
        assert!(!rendered.contains("authorization-code-marker"));
        assert!(!rendered.contains("invalid_grant"));
    }

    #[tokio::test]
    async fn declared_oversized_response_is_rejected_before_parsing() {
        let (address, server) = spawn_raw_server(
            |stream| {
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    MAX_TOKEN_RESPONSE_BYTES + 1
                );
                stream.write_all(response.as_bytes()).unwrap();
            },
            |_| {},
        );
        let mut config = config();
        config.token_endpoint = Url::parse("http://token.example.test/oauth2/token").unwrap();
        let error =
            OrganizationSsoOidcTokenExchangeClient::new(config, "client-secret-marker".to_owned())
                .unwrap()
                .exchange(
                    &proxied_client(address),
                    OrganizationSsoOidcTokenExchangeInput::new(
                        "authorization-code-marker".to_owned(),
                        "pkce-verifier-marker".to_owned(),
                    )
                    .unwrap(),
                )
                .await
                .unwrap_err();
        server.join().unwrap();
        assert_eq!(
            error,
            OrganizationSsoOidcTokenExchangeError::ResponseTooLarge
        );
    }

    fn settings() -> CredentialEncryptionSettings {
        serde_json::from_value(json!({
            "key_id": "primary-key",
            "key": base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
        }))
        .unwrap()
    }

    fn proxied_client(address: SocketAddr) -> PooledClient {
        let config = HttpClientConfig::new(
            ProxyConfig::parse(format!("http://{address}")).unwrap(),
            HttpTimeouts::default(),
        )
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
        HttpClientPool::default().get(&config).unwrap()
    }

    fn spawn_json_server<F>(
        status: u16,
        body: &'static [u8],
        inspect: F,
    ) -> (SocketAddr, JoinHandle<()>)
    where
        F: FnOnce(&str) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let request = read_request(&mut stream);
            inspect(&request);
            let reason = if status == 200 { "OK" } else { "Bad Request" };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        });
        (address, handle)
    }

    fn spawn_raw_server<R, F>(respond: R, inspect: F) -> (SocketAddr, JoinHandle<()>)
    where
        R: FnOnce(&mut TcpStream) + Send + 'static,
        F: FnOnce(&str) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let request = read_request(&mut stream);
            inspect(&request);
            respond(&mut stream);
        });
        (address, handle)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4_096];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0, "请求在正文完整前结束");
            bytes.extend_from_slice(&buffer[..count]);
            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let header_end = header_end + 4;
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap_or(0);
            if bytes.len() >= header_end + content_length {
                return String::from_utf8(bytes).unwrap();
            }
        }
    }

    fn form_values(request: &str) -> HashMap<String, String> {
        let (_, body) = request.split_once("\r\n\r\n").unwrap();
        url::form_urlencoded::parse(body.as_bytes())
            .into_owned()
            .collect()
    }

    fn header_value<'a>(request: &'a str, expected_name: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case(expected_name)
                    .then(|| value.trim())
            })
        })
    }
}
