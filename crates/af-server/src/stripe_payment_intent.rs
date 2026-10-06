use std::{collections::HashMap, fmt, time::Duration};

use af_httpclient::{Body, HeaderMap, HeaderValue, HttpClientProvider, Method, StatusCode};
use serde::Deserialize;
use thiserror::Error;
use url::form_urlencoded;
use zeroize::Zeroize;

use af_billing::{
    PaymentClientSecret, PaymentOrderFuture, PaymentOrderProvider, PaymentOrderProviderError,
    PaymentOrderRecoveryRequest, PaymentOrderRequest, PaymentOrderSession, STRIPE_PAYMENT_PROVIDER,
};

const STRIPE_API_BASE_URL: &str = "https://api.stripe.com";
const STRIPE_PAYMENT_INTENTS_PATH: &str = "/v1/payment_intents";
const MAX_STRIPE_SECRET_KEY_BYTES: usize = 4 * 1024;
const MAX_STRIPE_RESPONSE_BYTES: usize = 64 * 1024;
const STRIPE_IDEMPOTENCY_DOMAIN: &str = "anyflows-pi-v1";

/// Stripe PaymentIntent 出站配置错误；不携带 API 密钥内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum StripePaymentIntentConfigError {
    /// API 密钥为空、过长或包含不可打印字符。
    #[error("Stripe API 密钥无效")]
    InvalidSecretKey,
    /// Provider 请求硬超时必须大于零。
    #[error("Stripe 请求超时无效")]
    InvalidRequestTimeout,
}

/// 使用共享受控 HTTP Client 创建与恢复 Stripe PaymentIntent。
pub(crate) struct StripePaymentIntentProvider {
    secret_key: Vec<u8>,
    clients: HttpClientProvider,
    request_timeout: Duration,
    api_base_url: String,
}

impl StripePaymentIntentProvider {
    /// 使用固定 Stripe 官方端点和全局网络基线创建 Provider。
    pub(crate) fn new(
        secret_key: impl AsRef<[u8]>,
        clients: HttpClientProvider,
        request_timeout: Duration,
    ) -> Result<Self, StripePaymentIntentConfigError> {
        Self::with_api_base(
            secret_key,
            clients,
            request_timeout,
            STRIPE_API_BASE_URL.to_owned(),
        )
    }

    fn with_api_base(
        secret_key: impl AsRef<[u8]>,
        clients: HttpClientProvider,
        request_timeout: Duration,
        api_base_url: String,
    ) -> Result<Self, StripePaymentIntentConfigError> {
        let secret_key = secret_key.as_ref();
        if secret_key.is_empty()
            || secret_key.len() > MAX_STRIPE_SECRET_KEY_BYTES
            || !secret_key.iter().all(|byte| (0x21..=0x7e).contains(byte))
        {
            return Err(StripePaymentIntentConfigError::InvalidSecretKey);
        }
        if request_timeout.is_zero() {
            return Err(StripePaymentIntentConfigError::InvalidRequestTimeout);
        }
        Ok(Self {
            secret_key: secret_key.to_vec(),
            clients,
            request_timeout,
            api_base_url,
        })
    }

    #[cfg(test)]
    fn new_for_test(
        secret_key: impl AsRef<[u8]>,
        clients: HttpClientProvider,
        request_timeout: Duration,
        api_base_url: String,
    ) -> Result<Self, StripePaymentIntentConfigError> {
        Self::with_api_base(secret_key, clients, request_timeout, api_base_url)
    }

    async fn create_intent(
        &self,
        request: PaymentOrderRequest,
    ) -> Result<PaymentOrderSession, PaymentOrderProviderError> {
        let target = format!("{}{}", self.api_base_url, STRIPE_PAYMENT_INTENTS_PATH);
        let body = create_form(&request);
        let headers = self.headers(Some(&idempotency_key(&request)))?;
        let response = self
            .execute(Method::POST, &target, headers, Some(Body::from(body)), true)
            .await?;
        decode_response(response, &request, None, true).await
    }

    async fn recover_intent(
        &self,
        request: PaymentOrderRecoveryRequest,
    ) -> Result<PaymentOrderSession, PaymentOrderProviderError> {
        if !valid_payment_intent_id(request.provider_order_id()) {
            return Err(PaymentOrderProviderError::InvalidResponse);
        }
        let target = format!(
            "{}{}/{}",
            self.api_base_url,
            STRIPE_PAYMENT_INTENTS_PATH,
            request.provider_order_id()
        );
        let response = self
            .execute(Method::GET, &target, self.headers(None)?, None, false)
            .await?;
        decode_response(
            response,
            request.order(),
            Some(request.provider_order_id()),
            false,
        )
        .await
    }

    fn headers(
        &self,
        idempotency_key: Option<&str>,
    ) -> Result<HeaderMap, PaymentOrderProviderError> {
        let mut authorization = b"Bearer ".to_vec();
        authorization.extend_from_slice(&self.secret_key);
        let mut authorization_header = HeaderValue::from_bytes(&authorization)
            .map_err(|_| PaymentOrderProviderError::InvalidResponse)?;
        authorization.zeroize();
        authorization_header.set_sensitive(true);

        let mut headers = HeaderMap::new();
        headers.insert("authorization", authorization_header);
        headers.insert("accept", HeaderValue::from_static("application/json"));
        if let Some(idempotency_key) = idempotency_key {
            headers.insert(
                "content-type",
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
            headers.insert(
                "idempotency-key",
                HeaderValue::from_str(idempotency_key)
                    .map_err(|_| PaymentOrderProviderError::InvalidResponse)?,
            );
        }
        Ok(headers)
    }

    async fn execute(
        &self,
        method: Method,
        target: &str,
        headers: HeaderMap,
        body: Option<Body>,
        creating: bool,
    ) -> Result<af_httpclient::HttpResponse, PaymentOrderProviderError> {
        let client = self
            .clients
            .get(Some(self.request_timeout))
            .map_err(|_| PaymentOrderProviderError::Unavailable)?;
        client
            .execute(method, target, headers, body)
            .await
            .map_err(|_| {
                if creating {
                    PaymentOrderProviderError::OutcomeUnknown
                } else {
                    PaymentOrderProviderError::Unavailable
                }
            })
    }
}

impl PaymentOrderProvider for StripePaymentIntentProvider {
    fn provider(&self) -> &str {
        STRIPE_PAYMENT_PROVIDER
    }

    fn create<'a>(&'a self, request: PaymentOrderRequest) -> PaymentOrderFuture<'a> {
        Box::pin(async move { self.create_intent(request).await })
    }

    fn recover<'a>(&'a self, request: PaymentOrderRecoveryRequest) -> PaymentOrderFuture<'a> {
        Box::pin(async move { self.recover_intent(request).await })
    }
}

impl Drop for StripePaymentIntentProvider {
    fn drop(&mut self) {
        self.secret_key.zeroize();
    }
}

impl fmt::Debug for StripePaymentIntentProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StripePaymentIntentProvider")
            .field("request_timeout", &self.request_timeout)
            .field("endpoint", &"<固定 Stripe 端点>")
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct StripePaymentIntentResponse {
    id: String,
    amount: u64,
    currency: String,
    client_secret: Option<String>,
    metadata: HashMap<String, String>,
}

async fn decode_response(
    response: af_httpclient::HttpResponse,
    request: &PaymentOrderRequest,
    expected_provider_order_id: Option<&str>,
    creating: bool,
) -> Result<PaymentOrderSession, PaymentOrderProviderError> {
    let status = response.status();
    if !status.is_success() {
        return Err(classify_status(status, creating));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_STRIPE_RESPONSE_BYTES as u64)
    {
        return Err(PaymentOrderProviderError::InvalidResponse);
    }
    let body = response.bytes().await.map_err(|_| {
        if creating {
            PaymentOrderProviderError::OutcomeUnknown
        } else {
            PaymentOrderProviderError::Unavailable
        }
    })?;
    if body.len() > MAX_STRIPE_RESPONSE_BYTES {
        return Err(PaymentOrderProviderError::InvalidResponse);
    }
    parse_response(&body, request, expected_provider_order_id)
}

fn parse_response(
    body: &[u8],
    request: &PaymentOrderRequest,
    expected_provider_order_id: Option<&str>,
) -> Result<PaymentOrderSession, PaymentOrderProviderError> {
    let response: StripePaymentIntentResponse =
        serde_json::from_slice(body).map_err(|_| PaymentOrderProviderError::InvalidResponse)?;
    if !valid_payment_intent_id(&response.id)
        || expected_provider_order_id.is_some_and(|expected| expected != response.id)
        || response.amount != request.amount_minor()
        || response.currency != request.currency().to_ascii_lowercase()
        || response
            .metadata
            .get("anyflows_order_id")
            .is_none_or(|value| value != &request.order_id().persistence_key())
    {
        return Err(PaymentOrderProviderError::InvalidResponse);
    }
    let client_secret = response
        .client_secret
        .ok_or(PaymentOrderProviderError::InvalidResponse)
        .and_then(|value| {
            PaymentClientSecret::new(value).map_err(|_| PaymentOrderProviderError::InvalidResponse)
        })?;
    PaymentOrderSession::new(response.id, client_secret)
        .map_err(|_| PaymentOrderProviderError::InvalidResponse)
}

fn classify_status(status: StatusCode, creating: bool) -> PaymentOrderProviderError {
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        if creating {
            PaymentOrderProviderError::OutcomeUnknown
        } else {
            PaymentOrderProviderError::Unavailable
        }
    } else if creating || status != StatusCode::NOT_FOUND {
        PaymentOrderProviderError::Rejected
    } else {
        PaymentOrderProviderError::InvalidResponse
    }
}

fn create_form(request: &PaymentOrderRequest) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("amount", &request.amount_minor().to_string());
    serializer.append_pair("currency", &request.currency().to_ascii_lowercase());
    serializer.append_pair("automatic_payment_methods[enabled]", "true");
    serializer.append_pair(
        "metadata[anyflows_order_id]",
        &request.order_id().persistence_key(),
    );
    serializer.finish()
}

fn idempotency_key(request: &PaymentOrderRequest) -> String {
    format!(
        "{STRIPE_IDEMPOTENCY_DOMAIN}-{}",
        request.order_id().persistence_key()
    )
}

fn valid_payment_intent_id(value: &str) -> bool {
    value.starts_with("pi_")
        && value.len() <= af_db::MAX_PROVIDER_ORDER_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::TopupOrderId;
    use af_httpclient::HttpClientConfig;

    #[test]
    fn create_form_and_idempotency_key_are_stable_and_scoped_to_order() {
        let request = request();
        let form = create_form(&request);
        assert!(form.contains("amount=500"));
        assert!(form.contains("currency=usd"));
        assert!(form.contains("automatic_payment_methods%5Benabled%5D=true"));
        assert!(form.contains("metadata%5Banyflows_order_id%5D=42424242424242424242424242424242"));
        assert_eq!(
            idempotency_key(&request),
            "anyflows-pi-v1-42424242424242424242424242424242"
        );
    }

    #[test]
    fn configuration_and_debug_output_do_not_disclose_api_key() {
        let clients = HttpClientProvider::new(HttpClientConfig::default(), 2).unwrap();
        assert_eq!(
            StripePaymentIntentProvider::new("", clients.clone(), Duration::from_secs(10))
                .unwrap_err(),
            StripePaymentIntentConfigError::InvalidSecretKey
        );
        let provider = StripePaymentIntentProvider::new_for_test(
            "sk_test_secret",
            clients,
            Duration::from_secs(10),
            "http://api.stripe.test".to_owned(),
        )
        .unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains("sk_test_secret"));
        assert!(!debug.contains("api.stripe.test"));
        let headers = provider.headers(Some("anyflows-pi-v1-test")).unwrap();
        assert!(!format!("{headers:?}").contains("sk_test_secret"));
    }

    #[test]
    fn response_must_match_amount_currency_metadata_and_provider_order() {
        let request = request();
        let valid = br#"{
            "id":"pi_test_123",
            "amount":500,
            "currency":"usd",
            "client_secret":"pi_test_123_secret_client",
            "metadata":{"anyflows_order_id":"42424242424242424242424242424242"}
        }"#;
        let session = parse_response(valid, &request, Some("pi_test_123")).unwrap();
        assert_eq!(session.provider_order_id(), "pi_test_123");
        assert_eq!(
            session
                .client_secret()
                .expect("Stripe 支付会话必须返回客户端密钥")
                .expose(),
            "pi_test_123_secret_client"
        );

        for invalid in [
            String::from_utf8(valid.to_vec())
                .unwrap()
                .replace("\"amount\":500", "\"amount\":501"),
            String::from_utf8(valid.to_vec())
                .unwrap()
                .replace("\"currency\":\"usd\"", "\"currency\":\"eur\""),
            String::from_utf8(valid.to_vec()).unwrap().replace(
                "42424242424242424242424242424242",
                "43434343434343434343434343434343",
            ),
        ] {
            assert_eq!(
                parse_response(invalid.as_bytes(), &request, Some("pi_test_123")).unwrap_err(),
                PaymentOrderProviderError::InvalidResponse
            );
        }
    }

    fn request() -> PaymentOrderRequest {
        PaymentOrderRequest::new(
            TopupOrderId::new([0x42; 16]).unwrap(),
            500,
            "USD".to_owned(),
            "card".to_owned(),
        )
        .unwrap()
    }
}
