use af_config::AlipayVerificationSettings;
use af_db::{
    AccountVerificationProviderRequest, AccountVerificationProviderResult,
    AccountVerificationProviderStart,
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
use serde_json::{Map, Value, json};
use sha2::Sha256;
use std::{collections::BTreeMap, fmt, sync::Arc, time::Duration};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const API_VERSION: &str = "1.0";

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
        Self::from_parts(
            settings.enabled(),
            settings.app_id().map(|value| value.expose()),
            settings.private_key().map(|value| value.expose()),
            settings.public_key().map(|value| value.expose()),
            settings.gateway_url(),
            settings.biz_code(),
            settings.timeout_secs(),
            http_clients,
        )
    }

    pub fn from_parts(
        enabled: bool,
        app_id: Option<&str>,
        private_key: Option<&str>,
        public_key: Option<&str>,
        gateway_url: &str,
        biz_code: &str,
        timeout_secs: u64,
        http_clients: HttpClientProvider,
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
        Ok(Self {
            client: Some(Arc::new(AlipayClient {
                app_id,
                private_key,
                public_key,
                gateway_url,
                biz_code: biz_code.to_owned(),
                timeout: Duration::from_secs(timeout_secs),
                http_clients,
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
            .map_err(|_| super::AccountVerificationProviderError::Unavailable)
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
}

#[derive(Clone)]
struct AlipayClient {
    app_id: String,
    private_key: RsaPrivateKey,
    public_key: RsaPublicKey,
    gateway_url: url::Url,
    biz_code: String,
    timeout: Duration,
    http_clients: HttpClientProvider,
}

#[derive(Debug, Error)]
pub enum AlipayProviderConfigError {
    #[error("支付宝实名认证缺少配置项 {0}")]
    MissingField(&'static str),
    #[error("支付宝实名认证密钥无效")]
    InvalidKey,
    #[error("支付宝实名认证网关地址无效")]
    InvalidGateway,
}

#[derive(Debug, Error)]
enum AlipayRequestError {
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
        let outer_order_no = format!("anyflows-{}", Uuid::new_v4().simple());
        // Alipay's certdoc flow currently accepts only a mainland Chinese ID card.
        if request.document_country != "CN" || request.document_type != "national_id" {
            return Err(AlipayRequestError::Response);
        }
        let identity_param = json!({
            "identity_type": "CERT_INFO",
            "cert_type": "IDENTITY_CARD",
            "cert_name": request.subject_name,
            "cert_no": request.document_number,
        });
        let init = self
            .call(
                "alipay.user.certify.open.initialize",
                json!({
                    "outer_order_no": outer_order_no,
                    "biz_code": self.biz_code,
                    "identity_param": serde_json::to_string(&identity_param).map_err(|_| AlipayRequestError::Response)?,
                    "merchant_config": "{}",
                }),
            )
            .await?;
        let certify_id = init
            .get("certify_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 128)
            .ok_or(AlipayRequestError::Response)?
            .to_owned();
        let certify = self
            .call(
                "alipay.user.certify.open.certify",
                json!({ "certify_id": certify_id }),
            )
            .await?;
        let action_url = certify
            .get("certify_url")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("https://") && value.len() <= 2_048)
            .ok_or(AlipayRequestError::Response)?
            .to_owned();
        Ok(AccountVerificationProviderStart {
            reference: certify_id,
            action_url,
            status: "pending".to_owned(),
        })
    }

    async fn query(
        &self,
        reference: &str,
    ) -> Result<AccountVerificationProviderResult, AlipayRequestError> {
        if reference.is_empty() || reference.len() > 128 {
            return Err(AlipayRequestError::Response);
        }
        let response = self
            .call(
                "alipay.user.certify.open.query",
                json!({ "certify_id": reference }),
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

    async fn call(
        &self,
        method: &str,
        biz_content: Value,
    ) -> Result<Map<String, Value>, AlipayRequestError> {
        let mut params = BTreeMap::new();
        params.insert("app_id".to_owned(), self.app_id.clone());
        params.insert("charset".to_owned(), "utf-8".to_owned());
        params.insert("format".to_owned(), "json".to_owned());
        params.insert("method".to_owned(), method.to_owned());
        params.insert("sign_type".to_owned(), "RSA2".to_owned());
        params.insert("timestamp".to_owned(), timestamp());
        params.insert("version".to_owned(), API_VERSION.to_owned());
        params.insert(
            "biz_content".to_owned(),
            serde_json::to_string(&biz_content).map_err(|_| AlipayRequestError::Response)?,
        );
        let sign_content = params
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&");
        let signer = SigningKey::<Sha256>::new(self.private_key.clone());
        let signature = rsa::signature::Signer::sign(&signer, sign_content.as_bytes());
        params.insert("sign".to_owned(), BASE64.encode(signature.to_bytes()));
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
            HeaderValue::from_static("application/x-www-form-urlencoded"),
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
            return Err(AlipayRequestError::Rejected);
        }
        let value: Value =
            serde_json::from_slice(&body).map_err(|_| AlipayRequestError::Response)?;
        let response_key = format!("{method}_response");
        let object = value
            .get(response_key)
            .and_then(Value::as_object)
            .ok_or(AlipayRequestError::Response)?;
        if object.get("code").and_then(Value::as_str) != Some("10000") {
            return Err(AlipayRequestError::Rejected);
        }
        let encoded_signature = value
            .get("sign")
            .and_then(Value::as_str)
            .ok_or(AlipayRequestError::Response)?;
        let signature = BASE64
            .decode(encoded_signature)
            .map_err(|_| AlipayRequestError::Response)?;
        let signature =
            Signature::try_from(signature.as_slice()).map_err(|_| AlipayRequestError::Response)?;
        let signed_response =
            serde_json::to_string(object).map_err(|_| AlipayRequestError::Response)?;
        let verifier = VerifyingKey::<Sha256>::new(self.public_key.clone());
        rsa::signature::Verifier::verify(&verifier, signed_response.as_bytes(), &signature)
            .map_err(|_| AlipayRequestError::Response)?;
        Ok(object.clone())
    }
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
    let now = OffsetDateTime::now_utc();
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

    const TEST_PRIVATE_KEY: &str =
        include_str!("../../af-account/src/fixtures/organization_sso_oidc_private.pem");

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
}
