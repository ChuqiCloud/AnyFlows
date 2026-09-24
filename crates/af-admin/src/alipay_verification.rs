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
        if !settings.enabled() {
            return Ok(Self::default());
        }
        let app_id = settings
            .app_id()
            .ok_or(AlipayProviderConfigError::MissingField("app_id"))?
            .expose()
            .to_owned();
        let private_key = settings
            .private_key()
            .ok_or(AlipayProviderConfigError::MissingField("private_key"))?
            .expose();
        let public_key = settings
            .public_key()
            .ok_or(AlipayProviderConfigError::MissingField("public_key"))?
            .expose();
        let private_key = RsaPrivateKey::from_pkcs8_pem(private_key)
            .or_else(|_| RsaPrivateKey::from_pkcs1_pem(private_key))
            .map_err(|_| AlipayProviderConfigError::InvalidKey)?;
        let public_key = RsaPublicKey::from_public_key_pem(public_key)
            .or_else(|_| RsaPublicKey::from_pkcs1_pem(public_key))
            .map_err(|_| AlipayProviderConfigError::InvalidKey)?;
        let gateway_url = url::Url::parse(settings.gateway_url())
            .map_err(|_| AlipayProviderConfigError::InvalidGateway)?;
        Ok(Self {
            client: Some(Arc::new(AlipayClient {
                app_id,
                private_key,
                public_key,
                gateway_url,
                biz_code: settings.biz_code().to_owned(),
                timeout: Duration::from_secs(settings.timeout_secs()),
                http_clients,
            })),
        })
    }
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
