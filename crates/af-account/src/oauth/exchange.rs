use std::{fmt, mem, time::Duration};

use af_httpclient::{Body, Bytes, HeaderMap, HeaderName, HeaderValue, Method, PooledClient};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use thiserror::Error;
use url::{Url, form_urlencoded};
use zeroize::{Zeroize as _, Zeroizing};

use super::authorization::{
    OAuthAuthorizationContext, OAuthAuthorizationGrant, UpstreamOAuthProvider, validate_client_id,
    validate_scope,
};
use crate::credential_decryption::MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES;

/// OAuth token endpoint 响应体硬上限。
pub const MAX_OAUTH_TOKEN_RESPONSE_BYTES: usize = 64 * 1_024;
const MAX_OAUTH_TOKEN_ENDPOINT_BYTES: usize = 4 * 1_024;
const MAX_OAUTH_TOKEN_REQUEST_BYTES: usize = 32 * 1_024;
const MAX_OAUTH_CLIENT_SECRET_BYTES: usize = 16 * 1_024;
const AUTHORIZATION_CODE_GRANT_TYPE: &str = "authorization_code";
const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";
const JSON_CONTENT_TYPE: &str = "application/json";

/// OAuth 2.0 客户端在 token endpoint 使用的闭合认证方式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthClientAuthenticationMethod {
    /// RFC 8252 原生应用使用的公共客户端，不发送 client_secret。
    Public,
    /// RFC 6749 HTTP Basic 客户端密码认证。
    ClientSecretBasic,
    /// RFC 6749 请求体客户端密码认证；仅用于明确要求该方式的供应商。
    ClientSecretPost,
}

/// OAuth token endpoint 请求体的闭合编码方式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthTokenRequestEncoding {
    /// RFC 6749 常用的表单编码。
    Form,
    /// 明确要求 JSON 与原始 state 的供应商扩展。
    Json,
}

/// OAuth token endpoint 返回的稳定错误码；未知扩展不会保留原始文本。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OAuthEndpointErrorCode {
    /// 请求缺少参数、包含重复参数或格式无效。
    InvalidRequest,
    /// 客户端认证失败。
    InvalidClient,
    /// 授权码无效、过期、已消费或与 redirect URI 不匹配。
    InvalidGrant,
    /// 客户端不允许使用授权码流程。
    UnauthorizedClient,
    /// token endpoint 不支持授权码 grant。
    UnsupportedGrantType,
    /// 请求的 scope 无效或超出授权范围。
    InvalidScope,
    /// 供应商扩展错误；原始文本不会离开解析边界。
    #[serde(other)]
    Other,
}

/// 受控 OAuth 授权码交换器；配置、请求和响应均不开放任意字段注入。
pub struct OAuthTokenExchange {
    provider: UpstreamOAuthProvider,
    client_id: String,
    token_endpoint: Url,
    authentication_method: OAuthClientAuthenticationMethod,
    request_encoding: OAuthTokenRequestEncoding,
    client_secret: Option<TokenSecret>,
}

impl OAuthTokenExchange {
    /// 校验 HTTPS token endpoint、公共客户端标识及客户端认证材料。
    pub fn new(
        provider: UpstreamOAuthProvider,
        client_id: String,
        token_endpoint: String,
        authentication_method: OAuthClientAuthenticationMethod,
        client_secret: Option<String>,
    ) -> Result<Self, OAuthTokenExchangeError> {
        Self::new_with_request_encoding(
            provider,
            client_id,
            token_endpoint,
            authentication_method,
            client_secret,
            OAuthTokenRequestEncoding::Form,
        )
    }

    /// 仅供闭合 provider profile 选择表单或 JSON 请求编码。
    pub(crate) fn new_with_request_encoding(
        provider: UpstreamOAuthProvider,
        client_id: String,
        token_endpoint: String,
        authentication_method: OAuthClientAuthenticationMethod,
        client_secret: Option<String>,
        request_encoding: OAuthTokenRequestEncoding,
    ) -> Result<Self, OAuthTokenExchangeError> {
        // 先接管客户端密钥，确保任何后续配置错误都走清零 Drop。
        let client_secret = client_secret.map(TokenSecret::new_unchecked);
        validate_client_id(&client_id).map_err(|_| OAuthTokenExchangeError::InvalidClientId)?;
        let token_endpoint = parse_token_endpoint(&token_endpoint)?;
        let secret_required = authentication_method != OAuthClientAuthenticationMethod::Public;
        if secret_required != client_secret.is_some()
            || client_secret
                .as_ref()
                .is_some_and(|value| !is_valid_client_secret(value.expose()))
        {
            return Err(OAuthTokenExchangeError::InvalidClientAuthentication);
        }
        Ok(Self {
            provider,
            client_id,
            token_endpoint,
            authentication_method,
            request_encoding,
            client_secret,
        })
    }

    /// 返回该交换器绑定的上游提供商。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    /// 返回已校验的客户端认证方式。
    #[must_use]
    pub const fn authentication_method(&self) -> OAuthClientAuthenticationMethod {
        self.authentication_method
    }

    /// 返回 provider profile 固化的 token 请求编码。
    #[must_use]
    pub const fn request_encoding(&self) -> OAuthTokenRequestEncoding {
        self.request_encoding
    }

    #[cfg(test)]
    pub(crate) fn set_token_endpoint_for_test(&mut self, endpoint: Url) {
        // 测试只替换传输目标，生产构造器仍强制 HTTPS。
        self.token_endpoint = endpoint;
    }

    #[cfg(test)]
    pub(crate) fn token_endpoint_for_test(&self) -> &Url {
        &self.token_endpoint
    }

    /// 消费一次性授权 grant，并通过受控 Client 交换 Bearer token。
    pub async fn exchange(
        &self,
        http_client: &PooledClient,
        grant: OAuthAuthorizationGrant,
    ) -> Result<OAuthTokenSet, OAuthTokenExchangeError> {
        if grant.provider() != self.provider {
            return Err(OAuthTokenExchangeError::ProviderMismatch);
        }
        let request_body = self.build_request_body(&grant)?;
        let headers = self.build_headers()?;
        let context = grant.context();
        // Bytes 直接持有 Zeroizing owner，传输层的共享引用全部释放后才清零底层分配。
        let request_body = Body::from(Bytes::from_owner(request_body.into_zeroizing_bytes()));
        let response = http_client
            .execute(
                Method::POST,
                self.token_endpoint.as_str(),
                headers,
                Some(request_body),
            )
            .await;
        // 网络请求完成后立即清零 grant 中的 state、授权码与 verifier，不延长到响应解析结束。
        drop(grant);
        let response = response.map_err(|_| OAuthTokenExchangeError::Transport)?;
        let status = response.status();
        let body = collect_token_response(response).await?;
        if !status.is_success() {
            let code = serde_json::from_slice::<OAuthErrorWire>(&body)
                .ok()
                .map(|wire| wire.error);
            return Err(OAuthTokenExchangeError::EndpointRejected {
                status: status.as_u16(),
                code,
            });
        }
        let wire = serde_json::from_slice::<TokenResponseWire>(&body)
            .map_err(|_| OAuthTokenExchangeError::MalformedResponse)?;
        OAuthTokenSet::from_wire(self.provider, context, wire)
    }

    fn build_form(
        &self,
        grant: &OAuthAuthorizationGrant,
    ) -> Result<Zeroizing<String>, OAuthTokenExchangeError> {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        serializer
            .append_pair("grant_type", AUTHORIZATION_CODE_GRANT_TYPE)
            .append_pair("code", grant.authorization_code())
            .append_pair("redirect_uri", grant.redirect_uri().as_str())
            .append_pair("code_verifier", grant.code_verifier());
        match self.authentication_method {
            OAuthClientAuthenticationMethod::Public => {
                serializer.append_pair("client_id", &self.client_id);
            }
            OAuthClientAuthenticationMethod::ClientSecretBasic => {}
            OAuthClientAuthenticationMethod::ClientSecretPost => {
                serializer
                    .append_pair("client_id", &self.client_id)
                    .append_pair(
                        "client_secret",
                        self.client_secret
                            .as_ref()
                            .expect("客户端认证配置已完成闭合校验")
                            .expose(),
                    );
            }
        }
        let form = Zeroizing::new(serializer.finish());
        if form.len() > MAX_OAUTH_TOKEN_REQUEST_BYTES {
            return Err(OAuthTokenExchangeError::RequestTooLarge);
        }
        Ok(form)
    }

    fn build_request_body(
        &self,
        grant: &OAuthAuthorizationGrant,
    ) -> Result<OAuthTokenRequestBody, OAuthTokenExchangeError> {
        match self.request_encoding {
            OAuthTokenRequestEncoding::Form => {
                Ok(OAuthTokenRequestBody::Form(self.build_form(grant)?))
            }
            OAuthTokenRequestEncoding::Json => {
                let (client_id, client_secret) = match self.authentication_method {
                    OAuthClientAuthenticationMethod::Public => {
                        (Some(self.client_id.as_str()), None)
                    }
                    OAuthClientAuthenticationMethod::ClientSecretBasic => (None, None),
                    OAuthClientAuthenticationMethod::ClientSecretPost => (
                        Some(self.client_id.as_str()),
                        Some(
                            self.client_secret
                                .as_ref()
                                .expect("客户端认证配置已完成闭合校验")
                                .expose(),
                        ),
                    ),
                };
                let request = JsonAuthorizationCodeRequest {
                    grant_type: AUTHORIZATION_CODE_GRANT_TYPE,
                    code: grant.authorization_code(),
                    state: grant.state(),
                    client_id,
                    client_secret,
                    redirect_uri: grant.redirect_uri().as_str(),
                    code_verifier: grant.code_verifier(),
                };
                let body = Zeroizing::new(
                    serde_json::to_vec(&request)
                        .map_err(|_| OAuthTokenExchangeError::RequestEncoding)?,
                );
                if body.len() > MAX_OAUTH_TOKEN_REQUEST_BYTES {
                    return Err(OAuthTokenExchangeError::RequestTooLarge);
                }
                Ok(OAuthTokenRequestBody::Json(body))
            }
        }
    }

    fn build_headers(&self) -> Result<HeaderMap, OAuthTokenExchangeError> {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static(match self.request_encoding {
                OAuthTokenRequestEncoding::Form => FORM_CONTENT_TYPE,
                OAuthTokenRequestEncoding::Json => JSON_CONTENT_TYPE,
            }),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static(JSON_CONTENT_TYPE),
        );
        if self.authentication_method == OAuthClientAuthenticationMethod::ClientSecretBasic {
            let client_id = encode_form_component(&self.client_id);
            let client_secret = encode_form_component(
                self.client_secret
                    .as_ref()
                    .expect("客户端认证配置已完成闭合校验")
                    .expose(),
            );
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
                .map_err(|_| OAuthTokenExchangeError::InvalidClientAuthentication)?;
            value.set_sensitive(true);
            headers.insert(HeaderName::from_static("authorization"), value);
        }
        Ok(headers)
    }
}

impl fmt::Debug for OAuthTokenExchange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthTokenExchange")
            .field("provider", &self.provider)
            .field("client_id", &"<已脱敏>")
            .field("token_endpoint", &"<已脱敏>")
            .field("authentication_method", &self.authentication_method)
            .field("request_encoding", &self.request_encoding)
            .field(
                "client_secret",
                &self.client_secret.as_ref().map(|_| "<已脱敏>"),
            )
            .finish()
    }
}

/// 已校验且默认脱敏的 OAuth Bearer token 集合。
pub struct OAuthTokenSet {
    provider: UpstreamOAuthProvider,
    context: OAuthAuthorizationContext,
    access_token: TokenSecret,
    refresh_token: Option<TokenSecret>,
    expires_in: Option<Duration>,
    scope: Option<TokenSecret>,
}

impl OAuthTokenSet {
    fn from_wire(
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
        wire: TokenResponseWire,
    ) -> Result<Self, OAuthTokenExchangeError> {
        if !wire.token_type.eq_ignore_ascii_case("bearer")
            || wire.expires_in.is_some_and(|seconds| seconds == 0)
            || !is_valid_bearer_secret(wire.access_token.expose())
            || wire
                .refresh_token
                .as_ref()
                .is_some_and(|token| !is_valid_bearer_secret(token.expose()))
            || wire
                .scope
                .as_ref()
                .is_some_and(|scope| validate_scope(scope.expose()).is_err())
        {
            return Err(OAuthTokenExchangeError::InvalidTokenResponse);
        }
        Ok(Self {
            provider,
            context,
            access_token: wire.access_token,
            refresh_token: wire.refresh_token,
            expires_in: wire.expires_in.map(Duration::from_secs),
            scope: wire.scope,
        })
    }

    /// 返回 token 所属提供商。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    /// 返回授权发起时绑定的业务上下文。
    #[must_use]
    pub const fn context(&self) -> OAuthAuthorizationContext {
        self.context
    }

    /// 暴露 access token 给后续加密持久化边界；调用方不得记录或复制。
    #[must_use]
    pub fn access_token(&self) -> &str {
        self.access_token.expose()
    }

    /// 暴露可选 refresh token 给后续加密持久化边界；调用方不得记录或复制。
    #[must_use]
    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_ref().map(TokenSecret::expose)
    }

    /// 返回 access token 的相对有效期；供应商省略时保持未知。
    #[must_use]
    pub const fn expires_in(&self) -> Option<Duration> {
        self.expires_in
    }

    /// 返回供应商确认的 scope；响应省略时保持未知。
    #[must_use]
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_ref().map(TokenSecret::expose)
    }

    pub(super) fn into_parts(self) -> OAuthTokenSetParts {
        let Self {
            provider,
            context: _,
            access_token,
            refresh_token,
            expires_in,
            scope,
        } = self;
        OAuthTokenSetParts {
            provider,
            access_token: access_token.into_zeroizing(),
            refresh_token: refresh_token.map(TokenSecret::into_zeroizing),
            expires_in,
            scope: scope.map(TokenSecret::into_zeroizing),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(
        provider: UpstreamOAuthProvider,
        context: OAuthAuthorizationContext,
        access_token: String,
        refresh_token: Option<String>,
        expires_in: Option<Duration>,
        scope: Option<String>,
    ) -> Self {
        Self {
            provider,
            context,
            access_token: TokenSecret::new_unchecked(access_token),
            refresh_token: refresh_token.map(TokenSecret::new_unchecked),
            expires_in,
            scope: scope.map(TokenSecret::new_unchecked),
        }
    }
}

pub(super) struct OAuthTokenSetParts {
    pub(super) provider: UpstreamOAuthProvider,
    pub(super) access_token: Zeroizing<String>,
    pub(super) refresh_token: Option<Zeroizing<String>>,
    pub(super) expires_in: Option<Duration>,
    pub(super) scope: Option<Zeroizing<String>>,
}

impl fmt::Debug for OAuthTokenSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthTokenSet")
            .field("provider", &self.provider)
            .field("context", &self.context)
            .field("access_token", &"<已脱敏>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<已脱敏>"),
            )
            .field("expires_in", &self.expires_in)
            .field("scope", &self.scope.as_ref().map(|_| "<已脱敏>"))
            .finish()
    }
}

/// OAuth 授权码交换错误；不包含端点、授权码、verifier、token 或供应商错误正文。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthTokenExchangeError {
    #[error("OAuth 客户端标识无效")]
    InvalidClientId,
    #[error("OAuth token endpoint 无效")]
    InvalidTokenEndpoint,
    #[error("OAuth 客户端认证配置无效")]
    InvalidClientAuthentication,
    #[error("OAuth 授权提供商不匹配")]
    ProviderMismatch,
    #[error("OAuth token 请求超过长度上限")]
    RequestTooLarge,
    #[error("OAuth token 请求编码失败")]
    RequestEncoding,
    #[error("OAuth token endpoint 传输失败")]
    Transport,
    #[error("OAuth token endpoint 拒绝请求（HTTP {status}）")]
    EndpointRejected {
        status: u16,
        code: Option<OAuthEndpointErrorCode>,
    },
    #[error("OAuth token 响应超过长度上限")]
    ResponseTooLarge,
    #[error("OAuth token 响应不是有效 JSON")]
    MalformedResponse,
    #[error("OAuth token 响应语义无效")]
    InvalidTokenResponse,
}

#[derive(Deserialize)]
struct OAuthErrorWire {
    error: OAuthEndpointErrorCode,
}

#[derive(Serialize)]
struct JsonAuthorizationCodeRequest<'a> {
    grant_type: &'static str,
    code: &'a str,
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<&'a str>,
    redirect_uri: &'a str,
    code_verifier: &'a str,
}

enum OAuthTokenRequestBody {
    Form(Zeroizing<String>),
    Json(Zeroizing<Vec<u8>>),
}

impl OAuthTokenRequestBody {
    fn into_zeroizing_bytes(self) -> Zeroizing<Vec<u8>> {
        match self {
            Self::Form(body) => Zeroizing::new(body.as_bytes().to_vec()),
            Self::Json(body) => body,
        }
    }
}

#[derive(Deserialize)]
struct TokenResponseWire {
    access_token: TokenSecret,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<TokenSecret>,
    #[serde(default)]
    scope: Option<TokenSecret>,
}

struct TokenSecret(String);

impl TokenSecret {
    fn new_unchecked(value: String) -> Self {
        Self(value)
    }

    fn expose(&self) -> &str {
        &self.0
    }

    fn into_zeroizing(mut self) -> Zeroizing<String> {
        Zeroizing::new(mem::take(&mut self.0))
    }
}

impl<'de> Deserialize<'de> for TokenSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut value = String::deserialize(deserializer)?;
        if value.is_empty() || value.len() > MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES {
            value.zeroize();
            return Err(D::Error::custom("OAuth token 无效"));
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

async fn collect_token_response(
    response: af_httpclient::HttpResponse,
) -> Result<Zeroizing<Vec<u8>>, OAuthTokenExchangeError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_OAUTH_TOKEN_RESPONSE_BYTES as u64)
    {
        return Err(OAuthTokenExchangeError::ResponseTooLarge);
    }
    let mut stream = response.into_bytes_stream();
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(|_| OAuthTokenExchangeError::Transport)?;
        let next_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or(OAuthTokenExchangeError::ResponseTooLarge)?;
        if next_len > MAX_OAUTH_TOKEN_RESPONSE_BYTES {
            return Err(OAuthTokenExchangeError::ResponseTooLarge);
        }
        body.try_reserve(chunk.len())
            .map_err(|_| OAuthTokenExchangeError::ResponseTooLarge)?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn parse_token_endpoint(value: &str) -> Result<Url, OAuthTokenExchangeError> {
    if value.len() > MAX_OAUTH_TOKEN_ENDPOINT_BYTES {
        return Err(OAuthTokenExchangeError::InvalidTokenEndpoint);
    }
    let endpoint = Url::parse(value).map_err(|_| OAuthTokenExchangeError::InvalidTokenEndpoint)?;
    if endpoint.scheme() != "https"
        || endpoint.host().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.fragment().is_some()
        || endpoint.port() == Some(0)
    {
        return Err(OAuthTokenExchangeError::InvalidTokenEndpoint);
    }
    Ok(endpoint)
}

fn is_valid_client_secret(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OAUTH_CLIENT_SECRET_BYTES
        && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

fn is_valid_bearer_secret(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

fn encode_form_component(value: &str) -> Zeroizing<String> {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("", value);
    let mut encoded = Zeroizing::new(serializer.finish());
    debug_assert!(encoded.starts_with('='));
    encoded.remove(0);
    encoded
}

#[cfg(test)]
mod tests;
