use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_domain::{ChannelType, CredentialKind, MAX_MODEL_NAME_BYTES, Operation, Protocol};
use af_httpclient::{Body, HeaderMap, HeaderName, HeaderValue, Method};
use async_trait::async_trait;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use url::form_urlencoded;
use zeroize::{Zeroize, Zeroizing};

use crate::credential::clear_authentication_headers;
use crate::{
    Adaptor, AdaptorError, AdaptorResult, AdaptorTarget, Credential, MAX_CREDENTIAL_SECRET_BYTES,
    MAX_UPSTREAM_REQUEST_TARGET_BYTES, RelayContext, ResponseMode, UpstreamRequest,
};

/// Vertex 项目 ID 或项目编号的本地容量边界。
pub const MAX_VERTEX_PROJECT_ID_BYTES: usize = 30;
/// Vertex location 的本地容量边界。
pub const MAX_VERTEX_LOCATION_BYTES: usize = 63;
/// Google OAuth token endpoint 响应体上限。
pub const MAX_VERTEX_TOKEN_RESPONSE_BYTES: usize = 64 * 1_024;

const MIN_VERTEX_PROJECT_ID_BYTES: usize = 6;
const VERTEX_API_VERSION: &str = "v1";
const VERTEX_PUBLISHER: &str = "google";
const JSON_CONTENT_TYPE: &str = "application/json";
const FORM_CONTENT_TYPE: &str = "application/x-www-form-urlencoded";
const JWT_TTL: Duration = Duration::from_secs(60 * 60);
const JWT_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

/// Google Vertex AI 的 Google Publisher/Gemini 适配器。
///
/// 项目和区域属于渠道配置，Service Account 只提供签名身份。当前直接交换 access
/// token 以闭合协议正确性；缓存、singleflight 与持久化由 M3 账号层统一承担。
#[derive(Clone, Eq, PartialEq)]
pub struct VertexAdaptor {
    project_id: String,
    location: String,
    default_base_url: String,
    supported_models: Vec<String>,
    token_endpoint: String,
}

impl VertexAdaptor {
    /// Google OAuth 2.0 Service Account 固定 token endpoint。
    pub const TOKEN_ENDPOINT: &'static str = "https://oauth2.googleapis.com/token";
    /// Vertex 请求所需的 Google Cloud OAuth scope。
    pub const CLOUD_PLATFORM_SCOPE: &'static str = "https://www.googleapis.com/auth/cloud-platform";

    /// 创建不携带内建模型清单的 Vertex 适配器。
    pub fn new(project_id: impl Into<String>, location: impl Into<String>) -> AdaptorResult<Self> {
        Self::with_supported_models(project_id, location, std::iter::empty::<String>())
    }

    /// 创建带渠道模型清单的 Vertex 适配器。
    pub fn with_supported_models<I, S>(
        project_id: impl Into<String>,
        location: impl Into<String>,
        models: I,
    ) -> AdaptorResult<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let project_id = project_id.into();
        if !is_valid_project_id(&project_id) {
            return Err(AdaptorError::InvalidGoogleProject);
        }
        let location = location.into();
        if !is_valid_location(&location) {
            return Err(AdaptorError::InvalidGoogleLocation);
        }
        let default_base_url = if location == "global" {
            "https://aiplatform.googleapis.com".to_owned()
        } else {
            format!("https://{location}-aiplatform.googleapis.com")
        };
        Ok(Self {
            project_id,
            location,
            default_base_url,
            supported_models: models.into_iter().map(Into::into).collect(),
            token_endpoint: Self::TOKEN_ENDPOINT.to_owned(),
        })
    }

    fn build_generate_content_url(
        &self,
        context: &RelayContext,
        model: &str,
        response_mode: ResponseMode,
    ) -> AdaptorResult<String> {
        if !is_valid_model(model) {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        let mut target = context.resolve_base_url(&self.default_base_url)?;
        let has_version_suffix = target
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == VERTEX_API_VERSION);
        let action = match response_mode {
            ResponseMode::Full => "generateContent",
            ResponseMode::Stream => "streamGenerateContent",
        };
        let model_action = format!("{model}:{action}");
        {
            // 每个资源标识都作为独立路径段编码，禁止项目、区域或模型注入路径结构。
            let mut segments = target
                .path_segments_mut()
                .map_err(|_| AdaptorError::InvalidBaseUrl)?;
            segments.pop_if_empty();
            if !has_version_suffix {
                segments.push(VERTEX_API_VERSION);
            }
            segments.push("projects");
            segments.push(&self.project_id);
            segments.push("locations");
            segments.push(&self.location);
            segments.push("publishers");
            segments.push(VERTEX_PUBLISHER);
            segments.push("models");
            segments.push(&model_action);
        }
        if response_mode == ResponseMode::Stream {
            target.query_pairs_mut().append_pair("alt", "sse");
        }
        let target = String::from(target);
        if target.len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        Ok(target)
    }

    fn build_assertion_at(
        &self,
        credential: &Credential,
        signing_time: SystemTime,
    ) -> AdaptorResult<String> {
        let Some((client_email, private_key_id, private_key)) = credential.service_account_parts()
        else {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        };
        let issued_at = signing_time
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AdaptorError::CredentialSigning)?
            .as_secs();
        let expires_at = issued_at
            .checked_add(JWT_TTL.as_secs())
            .ok_or(AdaptorError::CredentialSigning)?;
        let claims = ServiceAccountClaims {
            iss: client_email,
            scope: Self::CLOUD_PLATFORM_SCOPE,
            aud: Self::TOKEN_ENDPOINT,
            iat: issued_at,
            exp: expires_at,
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = private_key_id.map(str::to_owned);
        let key = EncodingKey::from_rsa_pem(private_key.as_bytes())
            .map_err(|_| AdaptorError::CredentialSigning)?;
        encode(&header, &claims, &key).map_err(|_| AdaptorError::CredentialSigning)
    }

    async fn exchange_service_account_token(
        &self,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<Zeroizing<String>> {
        let assertion = Zeroizing::new(self.build_assertion_at(credential, SystemTime::now())?);
        let form = {
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            serializer.append_pair("grant_type", JWT_GRANT_TYPE);
            serializer.append_pair("assertion", assertion.as_str());
            Zeroizing::new(serializer.finish())
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static(FORM_CONTENT_TYPE),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static(JSON_CONTENT_TYPE),
        );
        let response = context
            .http_client()
            .execute(
                Method::POST,
                &self.token_endpoint,
                headers,
                Some(Body::from(form.as_bytes().to_vec())),
            )
            .await?;
        let status = response.status();
        let body = collect_token_response(response).await?;
        if !status.is_success() {
            return Err(AdaptorError::CredentialExchange);
        }
        let parsed = serde_json::from_slice::<TokenResponse>(&body)
            .map_err(|_| AdaptorError::CredentialExchange)?;
        if !parsed.token_type.eq_ignore_ascii_case("bearer")
            || parsed.expires_in == 0
            || !is_valid_access_token(&parsed.access_token)
        {
            return Err(AdaptorError::CredentialExchange);
        }
        Ok(Zeroizing::new(parsed.access_token.clone()))
    }

    fn apply_json_headers(headers: &mut HeaderMap) {
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static(JSON_CONTENT_TYPE),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static(JSON_CONTENT_TYPE),
        );
    }
}

#[async_trait]
impl Adaptor for VertexAdaptor {
    fn channel_type(&self) -> ChannelType {
        ChannelType::Vertex
    }

    fn default_protocol(&self) -> Protocol {
        Protocol::Gemini
    }

    fn default_base_url(&self) -> &str {
        &self.default_base_url
    }

    fn supported_models(&self) -> Vec<String> {
        self.supported_models.clone()
    }

    fn build_url(
        &self,
        context: &RelayContext,
        target: AdaptorTarget<'_>,
    ) -> AdaptorResult<String> {
        if target.operation() != Operation::Chat {
            return Err(AdaptorError::UnsupportedOperation {
                operation: target.operation(),
            });
        }
        self.build_generate_content_url(context, target.model(), target.response_mode())
    }

    fn setup_headers(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        if !matches!(
            credential.kind(),
            CredentialKind::Oauth | CredentialKind::ServiceAccount
        ) {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        }
        clear_authentication_headers(headers);
        Self::apply_json_headers(headers);
        if credential.kind() == CredentialKind::Oauth {
            insert_bearer_header(headers, credential.expose_secret())?;
        }
        if let Some(request_id) = context.request_id() {
            let value =
                HeaderValue::from_str(request_id).map_err(|_| AdaptorError::InvalidHeader)?;
            headers.insert(HeaderName::from_static("x-request-id"), value);
        }
        Ok(())
    }

    async fn finalize_request(
        &self,
        request: UpstreamRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        let (method, target, mut headers, body, response_mode, response_body_limit) =
            request.into_parts();
        if method != Method::POST {
            return Err(AdaptorError::UnsupportedRequestMethod);
        }
        let access_token = match credential.kind() {
            CredentialKind::Oauth => Zeroizing::new(credential.expose_secret().to_owned()),
            CredentialKind::ServiceAccount => {
                self.exchange_service_account_token(credential, context)
                    .await?
            }
            kind => return Err(AdaptorError::UnsupportedCredential { kind }),
        };

        // Header 覆盖已完成；最终认证必须覆盖所有旧载体，避免渠道配置替换 access token。
        clear_authentication_headers(&mut headers);
        Self::apply_json_headers(&mut headers);
        insert_bearer_header(&mut headers, access_token.as_str())?;
        UpstreamRequest::new(method, target, headers, body)
            .and_then(|request| request.with_response_body_limit(response_body_limit))
            .map(|request| request.with_response_mode(response_mode))
    }
}

impl fmt::Debug for VertexAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VertexAdaptor")
            .field("project_id", &"<已脱敏>")
            .field("location", &"<已脱敏>")
            .field("supported_model_count", &self.supported_models.len())
            .finish()
    }
}

#[derive(Serialize)]
struct ServiceAccountClaims<'a> {
    iss: &'a str,
    scope: &'static str,
    aud: &'static str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

impl Drop for TokenResponse {
    fn drop(&mut self) {
        self.access_token.zeroize();
    }
}

async fn collect_token_response(
    response: af_httpclient::HttpResponse,
) -> AdaptorResult<Zeroizing<Vec<u8>>> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_VERTEX_TOKEN_RESPONSE_BYTES as u64)
    {
        return Err(AdaptorError::CredentialExchange);
    }
    let mut stream = response.into_bytes_stream();
    let mut body = Zeroizing::new(Vec::new());
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk?;
        let next_len = body
            .len()
            .checked_add(chunk.len())
            .ok_or(AdaptorError::CredentialExchange)?;
        if next_len > MAX_VERTEX_TOKEN_RESPONSE_BYTES {
            return Err(AdaptorError::CredentialExchange);
        }
        body.try_reserve(chunk.len())
            .map_err(|_| AdaptorError::CredentialExchange)?;
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn insert_bearer_header(headers: &mut HeaderMap, access_token: &str) -> AdaptorResult<()> {
    let mut bearer = Zeroizing::new(String::with_capacity(7 + access_token.len()));
    bearer.push_str("Bearer ");
    bearer.push_str(access_token);
    let mut value =
        HeaderValue::from_str(bearer.as_str()).map_err(|_| AdaptorError::InvalidHeader)?;
    value.set_sensitive(true);
    headers.insert(HeaderName::from_static("authorization"), value);
    Ok(())
}

fn is_valid_project_id(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_VERTEX_PROJECT_ID_BYTES
        || value.trim() != value
        || !value.is_ascii()
    {
        return false;
    }
    if value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value.len() >= MIN_VERTEX_PROJECT_ID_BYTES;
    }
    let bytes = value.as_bytes();
    value.len() >= MIN_VERTEX_PROJECT_ID_BYTES
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn is_valid_location(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_VERTEX_LOCATION_BYTES
        || value.trim() != value
        || !value.is_ascii()
    {
        return false;
    }
    let bytes = value.as_bytes();
    bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.contains('/')
        && !model.chars().any(char::is_control)
}

fn is_valid_access_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_CREDENTIAL_SECRET_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

#[cfg(test)]
mod tests;
