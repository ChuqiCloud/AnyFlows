use af_config::AlipayVerificationSettings;
use af_db::{
    AccountVerificationProviderRequest, AccountVerificationProviderResult,
    AccountVerificationProviderStart, SiteSettingsRepository,
};
use af_httpclient::{Body, HeaderMap, HeaderName, HeaderValue, HttpClientProvider, Method};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use futures_util::StreamExt as _;
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey},
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    signature::SignatureEncoding,
};
use serde_json::{Map, Value, json, value::RawValue};
use sha2::Sha256;
use std::{collections::BTreeMap, fmt, sync::Arc, time::Duration};
use thiserror::Error;
use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;

const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const API_VERSION: &str = "1.0";
const DEFAULT_AUTH_URL: &str = "https://openauth.alipay.com/oauth2/publicAppAuthorize.htm";
const DEFAULT_SCOPE: &str = "id_verify";
const CALLBACK_PATH: &str = "/api/account/verifications/alipay/callback";

#[derive(Clone, Default)]
pub struct AlipayAccountVerificationProvider {
    client: Option<Arc<AlipayClient>>,
}

impl fmt::Debug for AlipayAccountVerificationProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AlipayAccountVerificationProvider")
            .field("configured", &self.client.is_some())
            .finish()
    }
}

impl AlipayAccountVerificationProvider {
    pub fn from_config(
        settings: &AlipayVerificationSettings,
        http_clients: HttpClientProvider,
    ) -> Result<Self, AlipayProviderConfigError> {
        Self::from_config_with_site_settings(settings, http_clients, None)
    }

    pub fn from_config_with_site_settings(
        settings: &AlipayVerificationSettings,
        http_clients: HttpClientProvider,
        site_settings: Option<Arc<SiteSettingsRepository>>,
    ) -> Result<Self, AlipayProviderConfigError> {
        Self::from_parts_with_site_settings(
            settings.enabled(),
            settings.app_id().map(|value| value.expose()),
            settings.private_key().map(|value| value.expose()),
            settings.public_key().map(|value| value.expose()),
            settings.gateway_url(),
            settings.biz_code(),
            settings.timeout_secs(),
            http_clients,
            site_settings,
        )
    }

    pub fn from_parts(
        enabled: bool,
        app_id: Option<&str>,
        private_key: Option<&str>,
        public_key: Option<&str>,
        gateway_url: &str,
        _biz_code: &str,
        timeout_secs: u64,
        http_clients: HttpClientProvider,
    ) -> Result<Self, AlipayProviderConfigError> {
        Self::from_parts_with_site_settings(
            enabled,
            app_id,
            private_key,
            public_key,
            gateway_url,
            _biz_code,
            timeout_secs,
            http_clients,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_parts_with_site_settings(
        enabled: bool,
        app_id: Option<&str>,
        private_key: Option<&str>,
        public_key: Option<&str>,
        gateway_url: &str,
        _biz_code: &str,
        timeout_secs: u64,
        http_clients: HttpClientProvider,
        site_settings: Option<Arc<SiteSettingsRepository>>,
    ) -> Result<Self, AlipayProviderConfigError> {
        if !enabled {
            return Ok(Self::default());
        }
        let app_id = app_id
            .ok_or(AlipayProviderConfigError::MissingField("app_id"))?
            .to_owned();
        let private_key =
            private_key.ok_or(AlipayProviderConfigError::MissingField("private_key"))?;
        let public_key = public_key.ok_or(AlipayProviderConfigError::MissingField("public_key"))?;
        let private_key = parse_private_key(private_key)?;
        let public_key = parse_public_key(public_key)?;
        let gateway_url =
            url::Url::parse(gateway_url).map_err(|_| AlipayProviderConfigError::InvalidGateway)?;
        let auth_url = url::Url::parse(DEFAULT_AUTH_URL)
            .map_err(|_| AlipayProviderConfigError::InvalidAuthUrl)?;
        Ok(Self {
            client: Some(Arc::new(AlipayClient {
                app_id,
                private_key,
                public_key,
                gateway_url,
                auth_url,
                timeout: Duration::from_secs(timeout_secs),
                http_clients,
                site_settings,
            })),
        })
    }

    pub fn validate_keys(
        private_key: &str,
        public_key: &str,
    ) -> Result<(), AlipayProviderConfigError> {
        parse_private_key(private_key)?;
        parse_public_key(public_key)?;
        Ok(())
    }
}

fn parse_private_key(value: &str) -> Result<RsaPrivateKey, AlipayProviderConfigError> {
    let value = value.trim();
    if value.len() > 16 * 1024 {
        return Err(AlipayProviderConfigError::InvalidKey);
    }
    if value.starts_with("-----BEGIN") {
        RsaPrivateKey::from_pkcs8_pem(value)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(value))
            .map_err(|_| AlipayProviderConfigError::InvalidKey)
    } else {
        let der = decode_base64_key(value)?;
        RsaPrivateKey::from_pkcs8_der(&der)
            .or_else(|_| RsaPrivateKey::from_pkcs1_der(&der))
            .map_err(|_| AlipayProviderConfigError::InvalidKey)
    }
}

fn parse_public_key(value: &str) -> Result<RsaPublicKey, AlipayProviderConfigError> {
    let value = value.trim();
    if value.len() > 16 * 1024 {
        return Err(AlipayProviderConfigError::InvalidKey);
    }
    if value.starts_with("-----BEGIN") {
        RsaPublicKey::from_public_key_pem(value)
            .or_else(|_| RsaPublicKey::from_pkcs1_pem(value))
            .map_err(|_| AlipayProviderConfigError::InvalidKey)
    } else {
        let der = decode_base64_key(value)?;
        RsaPublicKey::from_public_key_der(&der)
            .or_else(|_| RsaPublicKey::from_pkcs1_der(&der))
            .map_err(|_| AlipayProviderConfigError::InvalidKey)
    }
}

fn decode_base64_key(value: &str) -> Result<Vec<u8>, AlipayProviderConfigError> {
    let compact = value
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let der = BASE64
        .decode(compact)
        .map_err(|_| AlipayProviderConfigError::InvalidKey)?;
    if der.is_empty() || der.len() > 8 * 1024 {
        return Err(AlipayProviderConfigError::InvalidKey);
    }
    Ok(der)
}

#[async_trait]
impl super::AccountVerificationProvider for AlipayAccountVerificationProvider {
    fn key(&self) -> &'static str {
        "alipay"
    }

    fn configured(&self) -> bool {
        self.client.is_some()
    }

    async fn initialize(
        &self,
        request: &AccountVerificationProviderRequest,
    ) -> Result<AccountVerificationProviderStart, super::AccountVerificationProviderError> {
        self.client
            .as_ref()
            .ok_or(super::AccountVerificationProviderError::Unavailable)?
            .initialize(request)
            .await
            .map_err(|error| {
                tracing::warn!(stage = "initialize", error = %error, "Alipay verification failed");
                super::AccountVerificationProviderError::Unavailable
            })
    }

    async fn query(
        &self,
        reference: &str,
    ) -> Result<AccountVerificationProviderResult, super::AccountVerificationProviderError> {
        self.client
            .as_ref()
            .ok_or(super::AccountVerificationProviderError::Unavailable)?
            .query(reference)
            .await
            .map_err(|_| super::AccountVerificationProviderError::Unavailable)
    }

    async fn complete(
        &self,
        reference: &str,
        authorization_code: &str,
    ) -> Result<AccountVerificationProviderResult, super::AccountVerificationProviderError> {
        self.client
            .as_ref()
            .ok_or(super::AccountVerificationProviderError::Unavailable)?
            .complete(reference, authorization_code)
            .await
            .map_err(|error| {
                tracing::warn!(stage = "complete", error = %error, "Alipay verification failed");
                super::AccountVerificationProviderError::Unavailable
            })
    }
}

#[derive(Clone)]
struct AlipayClient {
    app_id: String,
    private_key: RsaPrivateKey,
    public_key: RsaPublicKey,
    gateway_url: url::Url,
    auth_url: url::Url,
    timeout: Duration,
    http_clients: HttpClientProvider,
    site_settings: Option<Arc<SiteSettingsRepository>>,
}

#[derive(Debug, Error)]
pub enum AlipayProviderConfigError {
    #[error("支付宝实名认证缺少配置项 {0}")]
    MissingField(&'static str),
    #[error("支付宝实名认证密钥无效")]
    InvalidKey,
    #[error("支付宝实名认证网关地址无效")]
    InvalidGateway,
    #[error("支付宝实名认证 OAuth 地址无效")]
    InvalidAuthUrl,
}

#[derive(Debug, Error)]
enum AlipayRequestError {
    #[error("支付宝实名认证需要在站点设置中配置公开访问地址 public_base_url")]
    MissingPublicBaseUrl,
    #[error("支付宝实名认证网络请求失败")]
    Transport,
    #[error("支付宝实名认证响应无效")]
    Response,
    #[error("支付宝实名认证未通过")]
    Rejected,
}

impl AlipayClient {
    async fn initialize(
        &self,
        request: &AccountVerificationProviderRequest,
    ) -> Result<AccountVerificationProviderStart, AlipayRequestError> {
        if request.document_country != "CN" || request.document_type != "national_id" {
            return Err(AlipayRequestError::Response);
        }
        let redirect_uri = self.redirect_uri().await?;
        let preconsult = self
            .call(
                "alipay.user.certdoc.certverify.preconsult",
                json!({
                    "user_name": request.subject_name,
                    "cert_no": request.document_number,
                    "cert_type": "IDENTITY_CARD",
                }),
                None,
                &[],
            )
            .await?;
        let verify_id = preconsult
            .get("verify_id")
            .and_then(Value::as_str)
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 96
                    && !value.contains('.')
                    && !value.chars().any(char::is_control)
            })
            .ok_or(AlipayRequestError::Response)?
            .to_owned();
        let state = Uuid::new_v4().simple().to_string();
        let reference = format!("{verify_id}.{state}");
        if reference.len() > 128 {
            return Err(AlipayRequestError::Response);
        }
        let action_url = self.authorize_url(&redirect_uri, &state)?;
        Ok(AccountVerificationProviderStart {
            reference,
            action_url,
            status: "pending".to_owned(),
        })
    }

    async fn query(
        &self,
        _reference: &str,
    ) -> Result<AccountVerificationProviderResult, AlipayRequestError> {
        // CertDoc consult requires the OAuth access token returned by the
        // browser authorization flow. A manual provider-sync request cannot
        // safely query it without that token.
        Err(AlipayRequestError::Response)
    }

    async fn complete(
        &self,
        reference: &str,
        authorization_code: &str,
    ) -> Result<AccountVerificationProviderResult, AlipayRequestError> {
        let (verify_id, state) = reference
            .rsplit_once('.')
            .ok_or(AlipayRequestError::Response)?;
        if verify_id.is_empty()
            || verify_id.len() > 96
            || state.len() != 32
            || !state.bytes().all(|byte| byte.is_ascii_hexdigit())
            || authorization_code.trim().is_empty()
            || authorization_code.len() > 4_096
        {
            return Err(AlipayRequestError::Response);
        }
        let access_token = self.exchange_access_token(authorization_code).await?;
        let response = self
            .call(
                "alipay.user.certdoc.certverify.consult",
                json!({ "verify_id": verify_id }),
                Some(&access_token),
                &[],
            )
            .await?;
        let passed = response.get("passed").and_then(|value| match value {
            Value::Bool(value) => Some(*value),
            Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
                "true" | "t" | "y" | "yes" | "1" => Some(true),
                "false" | "f" | "n" | "no" | "0" => Some(false),
                _ => None,
            },
            _ => None,
        });
        match passed {
            Some(true) => Ok(AccountVerificationProviderResult {
                status: "approved".to_owned(),
                terminal_status: Some(4),
                reason: Some("支付宝实名认证通过".to_owned()),
            }),
            Some(false) => Ok(AccountVerificationProviderResult {
                status: "rejected".to_owned(),
                terminal_status: Some(5),
                reason: Some("支付宝实名认证未通过".to_owned()),
            }),
            None => Ok(AccountVerificationProviderResult {
                status: "pending".to_owned(),
                terminal_status: None,
                reason: None,
            }),
        }
    }

    async fn exchange_access_token(
        &self,
        authorization_code: &str,
    ) -> Result<String, AlipayRequestError> {
        let response = self
            .call(
                "alipay.system.oauth.token",
                Value::Null,
                None,
                &[
                    ("grant_type", "authorization_code"),
                    ("code", authorization_code),
                ],
            )
            .await?;
        response
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 4_096)
            .map(str::to_owned)
            .ok_or(AlipayRequestError::Response)
    }

    async fn call(
        &self,
        method: &str,
        biz_content: Value,
        auth_token: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Result<Map<String, Value>, AlipayRequestError> {
        let params = self.signed_params(method, biz_content, auth_token, extra)?;
        let body = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            for (key, value) in params {
                form.append_pair(&key, &value);
            }
            form.finish()
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/x-www-form-urlencoded;charset=UTF-8"),
        );
        let client = self
            .http_clients
            .get(Some(self.timeout))
            .map_err(|_| AlipayRequestError::Transport)?;
        let response = client
            .execute(
                Method::POST,
                self.gateway_url.as_str(),
                headers,
                Some(Body::from(body)),
            )
            .await
            .map_err(|_| AlipayRequestError::Transport)?;
        let status = response.status();
        let body = collect_response(response).await?;
        if !status.is_success() {
            tracing::warn!(
                method,
                http_status = status.as_u16(),
                "Alipay gateway request failed"
            );
            return Err(AlipayRequestError::Rejected);
        }
        parse_gateway_response(&body, method, &self.public_key).inspect_err(|error| {
            tracing::warn!(method, error = %error, "Alipay gateway response failed validation");
        })
    }

    async fn redirect_uri(&self) -> Result<String, AlipayRequestError> {
        let repository = self
            .site_settings
            .as_ref()
            .ok_or(AlipayRequestError::MissingPublicBaseUrl)?;
        let settings = repository
            .settings()
            .await
            .map_err(|_| AlipayRequestError::Transport)?;
        let base = settings
            .public_base_url()
            .ok_or(AlipayRequestError::MissingPublicBaseUrl)?
            .trim_end_matches('/');
        let redirect_uri = format!("{base}{CALLBACK_PATH}");
        let parsed = url::Url::parse(&redirect_uri).map_err(|_| AlipayRequestError::Response)?;
        if redirect_uri.len() > 2_048
            || !matches!(parsed.scheme(), "http" | "https")
            || parsed.host_str().is_none()
        {
            return Err(AlipayRequestError::Response);
        }
        Ok(redirect_uri)
    }

    fn authorize_url(&self, redirect_uri: &str, state: &str) -> Result<String, AlipayRequestError> {
        let mut url = self.auth_url.clone();
        url.query_pairs_mut()
            .append_pair("app_id", &self.app_id)
            .append_pair("scope", DEFAULT_SCOPE)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("state", state);
        let value = url.to_string();
        if value.len() > 2_048 || url.scheme() != "https" {
            return Err(AlipayRequestError::Response);
        }
        Ok(value)
    }

    fn signed_params(
        &self,
        method: &str,
        biz_content: Value,
        auth_token: Option<&str>,
        extra: &[(&str, &str)],
    ) -> Result<BTreeMap<String, String>, AlipayRequestError> {
        let mut params = BTreeMap::new();
        params.insert("app_id".to_owned(), self.app_id.clone());
        params.insert("charset".to_owned(), "UTF-8".to_owned());
        params.insert("format".to_owned(), "JSON".to_owned());
        params.insert("method".to_owned(), method.to_owned());
        params.insert("sign_type".to_owned(), "RSA2".to_owned());
        params.insert("timestamp".to_owned(), timestamp());
        params.insert("version".to_owned(), API_VERSION.to_owned());
        if !biz_content.is_null() {
            params.insert(
                "biz_content".to_owned(),
                serde_json::to_string(&biz_content).map_err(|_| AlipayRequestError::Response)?,
            );
        }
        if let Some(auth_token) = auth_token.filter(|value| !value.is_empty()) {
            params.insert("auth_token".to_owned(), auth_token.to_owned());
        }
        for (key, value) in extra {
            if !key.is_empty() && !value.is_empty() {
                params.insert((*key).to_owned(), (*value).to_owned());
            }
        }
        let sign_content = params
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let signer = SigningKey::<Sha256>::new(self.private_key.clone());
        let signature = rsa::signature::Signer::sign(&signer, sign_content.as_bytes());
        params.insert("sign".to_owned(), BASE64.encode(signature.to_bytes()));
        Ok(params)
    }
}

fn parse_gateway_response(
    body: &[u8],
    method: &str,
    public_key: &RsaPublicKey,
) -> Result<Map<String, Value>, AlipayRequestError> {
    let envelope: BTreeMap<String, Box<RawValue>> =
        serde_json::from_slice(body).map_err(|_| AlipayRequestError::Response)?;
    if let Some(error) = envelope.get("error_response") {
        let error: Map<String, Value> =
            serde_json::from_str(error.get()).map_err(|_| AlipayRequestError::Response)?;
        log_gateway_rejection(method, &error);
        return Err(AlipayRequestError::Rejected);
    }
    let response_key = format!("{}_response", method.replace('.', "_"));
    let raw_response = envelope
        .get(&response_key)
        .ok_or(AlipayRequestError::Response)?;
    let object: Map<String, Value> =
        serde_json::from_str(raw_response.get()).map_err(|_| AlipayRequestError::Response)?;
    let code = object.get("code").and_then(Value::as_str);
    // OAuth token success responses may omit code/msg. CertDoc methods still
    // require an explicit success code; token fields and signatures are checked below.
    if code != Some("10000")
        && !(method == "alipay.system.oauth.token" && !object.contains_key("code"))
    {
        log_gateway_rejection(method, &object);
        return Err(AlipayRequestError::Rejected);
    }
    let encoded_signature: String = serde_json::from_str(
        envelope
            .get("sign")
            .ok_or(AlipayRequestError::Response)?
            .get(),
    )
    .map_err(|_| AlipayRequestError::Response)?;
    let signature = BASE64
        .decode(encoded_signature)
        .map_err(|_| AlipayRequestError::Response)?;
    let signature =
        Signature::try_from(signature.as_slice()).map_err(|_| AlipayRequestError::Response)?;
    let verifier = VerifyingKey::<Sha256>::new(public_key.clone());
    rsa::signature::Verifier::verify(&verifier, raw_response.get().as_bytes(), &signature)
        .map_err(|_| AlipayRequestError::Response)?;
    Ok(object)
}

fn log_gateway_rejection(method: &str, object: &Map<String, Value>) {
    // Descriptions and complete bodies can contain identity or token data.
    let code = object.get("code").and_then(Value::as_str).unwrap_or("");
    let sub_code = object.get("sub_code").and_then(Value::as_str).unwrap_or("");
    tracing::warn!(
        method,
        code,
        sub_code,
        "Alipay gateway rejected verification request"
    );
}

async fn collect_response(
    response: af_httpclient::HttpResponse,
) -> Result<Vec<u8>, AlipayRequestError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(AlipayRequestError::Response);
    }
    let mut stream = response.into_bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AlipayRequestError::Transport)?;
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(AlipayRequestError::Response);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn timestamp() -> String {
    format_timestamp(OffsetDateTime::now_utc())
}

fn format_timestamp(now: OffsetDateTime) -> String {
    let now = now.to_offset(UtcOffset::from_hms(8, 0, 0).expect("valid Beijing offset"));
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::{EncodePublicKey, LineEnding};

    const TEST_PRIVATE_KEY: &str = include_str!("fixtures/alipay_verification_test_private.pem");

    #[test]
    fn accepts_pem_and_alipay_base64_der_keys() {
        let private = RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY).unwrap();
        let public = RsaPublicKey::from(&private);
        let public_pem = public.to_public_key_pem(LineEnding::LF).unwrap();
        let private_base64 = TEST_PRIVATE_KEY
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect::<String>();
        let public_base64 = BASE64.encode(public.to_public_key_der().unwrap().as_bytes());

        AlipayAccountVerificationProvider::validate_keys(TEST_PRIVATE_KEY, &public_pem).unwrap();
        AlipayAccountVerificationProvider::validate_keys(
            &format!("\n{private_base64}\n"),
            &format!("  {public_base64}  "),
        )
        .unwrap();
        assert!(
            AlipayAccountVerificationProvider::from_parts(
                true,
                Some("test-app"),
                Some(&private_base64),
                Some(&public_base64),
                "https://openapi.alipay.com/gateway.do",
                "FACE",
                10,
                HttpClientProvider::new(af_httpclient::HttpClientConfig::default(), 1).unwrap(),
            )
            .unwrap()
            .client
            .is_some()
        );
    }

    #[test]
    fn rejects_invalid_or_oversized_keys() {
        assert!(parse_private_key("not a key").is_err());
        assert!(parse_public_key("not a key").is_err());
        assert!(parse_private_key(&"A".repeat(16 * 1024 + 1)).is_err());
        assert!(parse_public_key(&"A".repeat(16 * 1024 + 1)).is_err());
    }

    #[test]
    fn verifies_original_alipay_response_bytes_and_method_key() {
        let private_key = RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY).unwrap();
        let public_key = RsaPublicKey::from(&private_key);
        let response = r#"{"msg":"Success", "verify_id":"test-verify-id", "code":"10000"}"#;
        let signature = rsa::signature::Signer::sign(
            &SigningKey::<Sha256>::new(private_key),
            response.as_bytes(),
        );
        let body = format!(
            r#"{{"alipay_user_certdoc_certverify_preconsult_response":{response},"sign":"{}"}}"#,
            BASE64.encode(signature.to_bytes())
        );
        let parsed = parse_gateway_response(
            body.as_bytes(),
            "alipay.user.certdoc.certverify.preconsult",
            &public_key,
        )
        .unwrap();
        assert_eq!(
            parsed.get("verify_id").and_then(Value::as_str),
            Some("test-verify-id")
        );

        let tampered = body.replace("test-verify-id", "other-verify-id");
        assert!(matches!(
            parse_gateway_response(
                tampered.as_bytes(),
                "alipay.user.certdoc.certverify.preconsult",
                &public_key
            ),
            Err(AlipayRequestError::Response)
        ));
    }

    #[test]
    fn authorize_link_contains_oauth_scope_and_callback_state() {
        let private_key = RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY).unwrap();
        let public_key = RsaPublicKey::from(&private_key);
        let public_pem = public_key.to_public_key_pem(LineEnding::LF).unwrap();
        let provider = AlipayAccountVerificationProvider::from_parts(
            true,
            Some("test-app"),
            Some(TEST_PRIVATE_KEY),
            Some(&public_pem),
            "https://openapi.alipay.com/gateway.do",
            "FACE",
            10,
            HttpClientProvider::new(af_httpclient::HttpClientConfig::default(), 1).unwrap(),
        )
        .unwrap();
        let link = provider
            .client
            .unwrap()
            .authorize_url(
                "https://example.com/api/account/verifications/alipay/callback",
                "0123456789abcdef0123456789abcdef",
            )
            .unwrap();
        let url = url::Url::parse(&link).unwrap();
        assert_eq!(url.host_str(), Some("openauth.alipay.com"));
        assert_eq!(
            url.query_pairs()
                .find(|(key, _)| key == "app_id")
                .map(|(_, value)| value.into_owned()),
            Some("test-app".to_owned())
        );
        assert_eq!(
            url.query_pairs()
                .find(|(key, _)| key == "scope")
                .map(|(_, value)| value.into_owned()),
            Some("id_verify".to_owned())
        );
        assert_eq!(
            url.query_pairs()
                .find(|(key, _)| key == "state")
                .map(|(_, value)| value.into_owned()),
            Some("0123456789abcdef0123456789abcdef".to_owned())
        );
        assert_eq!(
            url.query_pairs()
                .find(|(key, _)| key == "redirect_uri")
                .map(|(_, value)| value.into_owned()),
            Some("https://example.com/api/account/verifications/alipay/callback".to_owned())
        );
    }

    #[test]
    fn oauth_token_and_consult_parameters_are_signed_as_gateway_fields() {
        let private_key = RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY).unwrap();
        let public_key = RsaPublicKey::from(&private_key);
        let public_pem = public_key.to_public_key_pem(LineEnding::LF).unwrap();
        let provider = AlipayAccountVerificationProvider::from_parts(
            true,
            Some("test-app"),
            Some(TEST_PRIVATE_KEY),
            Some(&public_pem),
            "https://openapi.alipay.com/gateway.do",
            "FACE",
            10,
            HttpClientProvider::new(af_httpclient::HttpClientConfig::default(), 1).unwrap(),
        )
        .unwrap();
        let client = provider.client.unwrap();
        let token_params = client
            .signed_params(
                "alipay.system.oauth.token",
                Value::Null,
                None,
                &[("grant_type", "authorization_code"), ("code", "oauth-code")],
            )
            .unwrap();
        assert_eq!(
            token_params.get("grant_type"),
            Some(&"authorization_code".to_owned())
        );
        assert_eq!(token_params.get("code"), Some(&"oauth-code".to_owned()));
        assert!(!token_params.contains_key("biz_content"));

        let consult_params = client
            .signed_params(
                "alipay.user.certdoc.certverify.consult",
                json!({ "verify_id": "verify-1" }),
                Some("access-token"),
                &[],
            )
            .unwrap();
        assert_eq!(
            consult_params.get("auth_token"),
            Some(&"access-token".to_owned())
        );
        assert_eq!(
            serde_json::from_str::<Value>(consult_params.get("biz_content").unwrap()).unwrap(),
            json!({ "verify_id": "verify-1" })
        );
    }

    #[test]
    fn token_responses_allow_missing_code_but_still_require_valid_signatures() {
        let private = RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY).unwrap();
        let public = RsaPublicKey::from(&private);
        let response = r#"{"access_token":"test-token","expires_in":3600}"#;
        let signature =
            rsa::signature::Signer::sign(&SigningKey::<Sha256>::new(private), response.as_bytes());
        let body = format!(
            r#"{{"alipay_system_oauth_token_response":{response},"sign":"{}"}}"#,
            BASE64.encode(signature.to_bytes())
        );
        assert!(
            parse_gateway_response(body.as_bytes(), "alipay.system.oauth.token", &public).is_ok()
        );
        assert!(
            parse_gateway_response(
                body.replace("test-token", "other-token").as_bytes(),
                "alipay.system.oauth.token",
                &public
            )
            .is_err()
        );
        assert!(
            parse_gateway_response(
                format!(r#"{{"alipay_system_oauth_token_response":{response}}}"#).as_bytes(),
                "alipay.system.oauth.token",
                &public
            )
            .is_err()
        );
        for body in [
            r#"{"error_response":{"code":"40002","sub_code":"isv.code-invalid"}}"#,
            r#"{"alipay_system_oauth_token_response":{"code":"40004","access_token":"test-token"}}"#,
            r#"{"alipay_system_oauth_token_response":{"code":null,"access_token":"test-token"}}"#,
        ] {
            assert!(matches!(
                parse_gateway_response(body.as_bytes(), "alipay.system.oauth.token", &public),
                Err(AlipayRequestError::Rejected)
            ));
        }
        let certdoc = body.replace(
            "alipay_system_oauth_token_response",
            "alipay_user_certdoc_certverify_consult_response",
        );
        assert!(matches!(
            parse_gateway_response(
                certdoc.as_bytes(),
                "alipay.user.certdoc.certverify.consult",
                &public
            ),
            Err(AlipayRequestError::Rejected)
        ));
    }

    #[test]
    fn alipay_timestamp_uses_beijing_time() {
        let instant = OffsetDateTime::from_unix_timestamp(0).unwrap();
        assert_eq!(format_timestamp(instant), "1970-01-01 08:00:00");
    }
}
