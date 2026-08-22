use std::{fmt, time::Duration};

use af_billing::{
    RefundProvider, RefundProviderError, RefundProviderFuture, RefundProviderRecoveryRequest,
    RefundProviderRequest, RefundProviderResult, STRIPE_PAYMENT_PROVIDER,
};
use af_httpclient::{Body, HeaderMap, HeaderValue, HttpClientProvider, Method, StatusCode};
use serde::Deserialize;
use thiserror::Error;
use url::form_urlencoded;
use zeroize::Zeroize;

const STRIPE_API_BASE_URL: &str = "https://api.stripe.com";
const STRIPE_REFUNDS_PATH: &str = "/v1/refunds";
const STRIPE_REFUND_REQUEST_METADATA_KEY: &str = "anyflows_refund_request_id";
const MAX_STRIPE_SECRET_KEY_BYTES: usize = 4 * 1024;
const MAX_STRIPE_RESPONSE_BYTES: usize = 64 * 1024;
const STRIPE_REFUND_IDEMPOTENCY_DOMAIN: &str = "anyflows-refund-v1";

/// Stripe 退款 Provider 的本地配置错误，不包含密钥内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum StripeRefundProviderConfigError {
    #[error("Stripe API 密钥无效")]
    InvalidSecretKey,
    #[error("Stripe 退款请求超时无效")]
    InvalidRequestTimeout,
}

/// 使用受控 HTTP Client 执行 Stripe 原路退款提交与恢复。
pub(crate) struct StripeRefundProvider {
    secret_key: Vec<u8>,
    clients: HttpClientProvider,
    request_timeout: Duration,
    api_base_url: String,
}

impl StripeRefundProvider {
    /// 创建使用固定 Stripe 官方端点的退款 Provider。
    pub(crate) fn new(
        secret_key: impl AsRef<[u8]>,
        clients: HttpClientProvider,
        request_timeout: Duration,
    ) -> Result<Self, StripeRefundProviderConfigError> {
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
    ) -> Result<Self, StripeRefundProviderConfigError> {
        let secret_key = secret_key.as_ref();
        if secret_key.is_empty()
            || secret_key.len() > MAX_STRIPE_SECRET_KEY_BYTES
            || !secret_key.iter().all(|byte| (0x21..=0x7e).contains(byte))
        {
            return Err(StripeRefundProviderConfigError::InvalidSecretKey);
        }
        if request_timeout.is_zero() {
            return Err(StripeRefundProviderConfigError::InvalidRequestTimeout);
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
    ) -> Result<Self, StripeRefundProviderConfigError> {
        Self::with_api_base(secret_key, clients, request_timeout, api_base_url)
    }

    async fn submit_refund(
        &self,
        request: RefundProviderRequest,
    ) -> Result<RefundProviderResult, RefundProviderError> {
        if !valid_payment_intent_id(request.payment_reference()) {
            return Err(RefundProviderError::Rejected);
        }
        let target = format!("{}{}", self.api_base_url, STRIPE_REFUNDS_PATH);
        let body = submit_form(&request);
        let headers = self.headers(Some(&idempotency_key(&request)))?;
        let response = self
            .execute(Method::POST, &target, headers, Some(Body::from(body)), true)
            .await?;
        decode_response(response, &request, None, true).await
    }

    async fn recover_refund(
        &self,
        request: RefundProviderRecoveryRequest,
    ) -> Result<RefundProviderResult, RefundProviderError> {
        if !valid_refund_id(request.provider_refund_id())
            || !valid_payment_intent_id(request.request().payment_reference())
        {
            return Err(RefundProviderError::InvalidResponse);
        }
        let target = format!(
            "{}{}/{}",
            self.api_base_url,
            STRIPE_REFUNDS_PATH,
            request.provider_refund_id()
        );
        let response = self
            .execute(Method::GET, &target, self.headers(None)?, None, false)
            .await?;
        decode_response(
            response,
            request.request(),
            Some(request.provider_refund_id()),
            false,
        )
        .await
    }

    fn headers(&self, idempotency_key: Option<&str>) -> Result<HeaderMap, RefundProviderError> {
        let mut authorization = b"Bearer ".to_vec();
        authorization.extend_from_slice(&self.secret_key);
        let mut authorization_header = HeaderValue::from_bytes(&authorization)
            .map_err(|_| RefundProviderError::InvalidResponse)?;
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
                    .map_err(|_| RefundProviderError::InvalidResponse)?,
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
        submitting: bool,
    ) -> Result<af_httpclient::HttpResponse, RefundProviderError> {
        let client = self.clients.get(Some(self.request_timeout)).map_err(|_| {
            if submitting {
                RefundProviderError::OutcomeUnknown
            } else {
                RefundProviderError::Unavailable
            }
        })?;
        client
            .execute(method, target, headers, body)
            .await
            .map_err(|_| {
                if submitting {
                    RefundProviderError::OutcomeUnknown
                } else {
                    RefundProviderError::Unavailable
                }
            })
    }
}

impl RefundProvider for StripeRefundProvider {
    fn provider(&self) -> &str {
        STRIPE_PAYMENT_PROVIDER
    }

    fn submit<'a>(&'a self, request: RefundProviderRequest) -> RefundProviderFuture<'a> {
        Box::pin(async move { self.submit_refund(request).await })
    }

    fn recover<'a>(&'a self, request: RefundProviderRecoveryRequest) -> RefundProviderFuture<'a> {
        Box::pin(async move { self.recover_refund(request).await })
    }
}

impl Drop for StripeRefundProvider {
    fn drop(&mut self) {
        self.secret_key.zeroize();
    }
}

impl fmt::Debug for StripeRefundProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StripeRefundProvider")
            .field("request_timeout", &self.request_timeout)
            .field("endpoint", &"<固定 Stripe 端点>")
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct StripeRefundResponse {
    id: String,
    payment_intent: Option<String>,
    amount: u64,
    currency: String,
    status: String,
}

async fn decode_response(
    response: af_httpclient::HttpResponse,
    request: &RefundProviderRequest,
    expected_provider_refund_id: Option<&str>,
    submitting: bool,
) -> Result<RefundProviderResult, RefundProviderError> {
    let status = response.status();
    if !status.is_success() {
        return Err(classify_status(status, submitting));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_STRIPE_RESPONSE_BYTES as u64)
    {
        return Err(RefundProviderError::InvalidResponse);
    }
    let body = response.bytes().await.map_err(|_| {
        if submitting {
            RefundProviderError::OutcomeUnknown
        } else {
            RefundProviderError::Unavailable
        }
    })?;
    if body.len() > MAX_STRIPE_RESPONSE_BYTES {
        return Err(RefundProviderError::InvalidResponse);
    }
    parse_response(&body, request, expected_provider_refund_id)
}

fn parse_response(
    body: &[u8],
    request: &RefundProviderRequest,
    expected_provider_refund_id: Option<&str>,
) -> Result<RefundProviderResult, RefundProviderError> {
    let response: StripeRefundResponse =
        serde_json::from_slice(body).map_err(|_| RefundProviderError::InvalidResponse)?;
    let amount =
        u64::try_from(request.amount_minor()).map_err(|_| RefundProviderError::InvalidResponse)?;
    if !valid_refund_id(&response.id)
        || expected_provider_refund_id.is_some_and(|expected| expected != response.id)
        || response.payment_intent.as_deref() != Some(request.payment_reference())
        || response.amount != amount
        || response.currency != request.currency().to_ascii_lowercase()
    {
        return Err(RefundProviderError::InvalidResponse);
    }
    match response.status.as_str() {
        "pending" | "succeeded" => {
            RefundProviderResult::new(response.id).map_err(|_| RefundProviderError::InvalidResponse)
        }
        "failed" | "canceled" => Err(RefundProviderError::Rejected),
        _ => Err(RefundProviderError::InvalidResponse),
    }
}

fn classify_status(status: StatusCode, submitting: bool) -> RefundProviderError {
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        if submitting {
            RefundProviderError::OutcomeUnknown
        } else {
            RefundProviderError::Unavailable
        }
    } else if submitting {
        RefundProviderError::Rejected
    } else if status == StatusCode::NOT_FOUND {
        RefundProviderError::InvalidResponse
    } else {
        RefundProviderError::Rejected
    }
}

fn submit_form(request: &RefundProviderRequest) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("payment_intent", request.payment_reference());
    serializer.append_pair("amount", &request.amount_minor().to_string());
    serializer.append_pair(
        &format!("metadata[{STRIPE_REFUND_REQUEST_METADATA_KEY}]"),
        &request.request_id().persistence_key(),
    );
    serializer.finish()
}

fn idempotency_key(request: &RefundProviderRequest) -> String {
    format!(
        "{STRIPE_REFUND_IDEMPOTENCY_DOMAIN}-{}",
        request.request_id().persistence_key()
    )
}

fn valid_refund_id(value: &str) -> bool {
    value.starts_with("re_")
        && value.len() <= af_domain::MAX_REFUND_PROVIDER_REFUND_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
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
    use af_domain::{
        RefundApprovalStatus, RefundOrderKind, RefundRequestId, RefundRequestKey,
        RefundRequestRecord, RefundRequestStatus, UserId,
    };
    use af_httpclient::HttpClientConfig;

    fn request() -> RefundProviderRequest {
        let record = RefundRequestRecord::from_persistence(
            1,
            RefundRequestId::new([1; 16]).unwrap(),
            RefundRequestKey::new([2; 16]).unwrap(),
            UserId::new(7).unwrap(),
            RefundOrderKind::Topup,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            "stripe".to_owned(),
            Some("pi_original".to_owned()),
            "USD".to_owned(),
            100,
            40,
            None,
            RefundRequestStatus::Requested,
            RefundApprovalStatus::Pending,
            None,
            None,
            1,
            1_900_000_000,
            1_900_000_000,
        )
        .unwrap();
        RefundProviderRequest::new(&record, "pi_test_123".to_owned()).unwrap()
    }

    #[test]
    fn form_and_idempotency_key_are_stable() {
        let request = request();
        assert_eq!(
            submit_form(&request),
            "payment_intent=pi_test_123&amount=40&metadata%5Banyflows_refund_request_id%5D=01010101010101010101010101010101"
        );
        assert_eq!(
            idempotency_key(&request),
            "anyflows-refund-v1-01010101010101010101010101010101"
        );
    }

    #[test]
    fn response_must_match_refund_facts_and_accept_only_pending_or_succeeded() {
        let request = request();
        let valid = br#"{
            "id":"re_test_123",
            "payment_intent":"pi_test_123",
            "amount":40,
            "currency":"usd",
            "status":"succeeded"
        }"#;
        let result = parse_response(valid, &request, Some("re_test_123")).unwrap();
        assert_eq!(result.provider_refund_id(), "re_test_123");

        let wrong_amount = String::from_utf8(valid.to_vec())
            .unwrap()
            .replace("\"amount\":40", "\"amount\":41");
        assert_eq!(
            parse_response(wrong_amount.as_bytes(), &request, None).unwrap_err(),
            RefundProviderError::InvalidResponse
        );
        let failed = String::from_utf8(valid.to_vec())
            .unwrap()
            .replace("succeeded", "failed");
        assert_eq!(
            parse_response(failed.as_bytes(), &request, None).unwrap_err(),
            RefundProviderError::Rejected
        );
    }

    #[test]
    fn configuration_and_debug_output_redact_secret_and_endpoint() {
        let clients = HttpClientProvider::new(HttpClientConfig::default(), 2).unwrap();
        assert_eq!(
            StripeRefundProvider::new("", clients.clone(), Duration::from_secs(10)).unwrap_err(),
            StripeRefundProviderConfigError::InvalidSecretKey
        );
        let provider = StripeRefundProvider::new_for_test(
            "sk_test_secret",
            clients,
            Duration::from_secs(10),
            "http://stripe.test".to_owned(),
        )
        .unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains("sk_test_secret"));
        assert!(!debug.contains("stripe.test"));
        let headers = provider.headers(Some("anyflows-refund-v1-test")).unwrap();
        assert!(!format!("{headers:?}").contains("sk_test_secret"));
    }

    #[test]
    fn identifiers_are_provider_specific() {
        assert!(valid_payment_intent_id("pi_123"));
        assert!(!valid_payment_intent_id("ch_123"));
        assert!(valid_refund_id("re_123"));
        assert!(!valid_refund_id("refund_123"));
    }
}
