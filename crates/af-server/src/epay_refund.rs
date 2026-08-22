use std::{fmt, str, time::Duration};

use af_billing::{
    EASYPAY_PAYMENT_PROVIDER, RefundProvider, RefundProviderError, RefundProviderFuture,
    RefundProviderRecoveryRequest, RefundProviderRequest, RefundProviderResult,
};
use af_httpclient::{Body, HeaderMap, HeaderValue, HttpClientProvider, Method, StatusCode};
use serde::Deserialize;
use thiserror::Error;
use url::{Url, form_urlencoded};
use zeroize::Zeroize;

const EASYPAY_REFUND_PATH: &str = "/api.php?act=refund";
const MAX_EASYPAY_MERCHANT_ID_BYTES: usize = 64;
const MAX_EASYPAY_MERCHANT_KEY_BYTES: usize = 4 * 1024;
const MAX_EASYPAY_RESPONSE_BYTES: usize = 64 * 1024;

/// 易支付退款 Provider 的本地配置错误，不包含商户密钥或网关地址。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[allow(
    clippy::enum_variant_names,
    reason = "配置错误需要保留统一的 Invalid 前缀以便运行时分类"
)]
pub(crate) enum EasyPayRefundProviderConfigError {
    #[error("易支付网关地址无效")]
    InvalidGatewayUrl,
    #[error("易支付商户 PID 无效")]
    InvalidMerchantId,
    #[error("易支付商户密钥无效")]
    InvalidMerchantKey,
    #[error("易支付退款请求超时无效")]
    InvalidRequestTimeout,
}

/// 按已确认的 EasyPay `api.php?act=refund` 协议执行原路退款。
pub(crate) struct EasyPayRefundProvider {
    api_base_url: String,
    merchant_id: String,
    merchant_key: Vec<u8>,
    clients: HttpClientProvider,
    request_timeout: Duration,
}

impl EasyPayRefundProvider {
    /// 创建易支付退款 Provider；密钥仅保留在内存并在释放时清零。
    pub(crate) fn new(
        gateway_url: &str,
        merchant_id: &str,
        merchant_key: impl AsRef<[u8]>,
        clients: HttpClientProvider,
        request_timeout: Duration,
    ) -> Result<Self, EasyPayRefundProviderConfigError> {
        let api_base_url = normalize_api_base(gateway_url)?;
        if !valid_visible_text(merchant_id, MAX_EASYPAY_MERCHANT_ID_BYTES) {
            return Err(EasyPayRefundProviderConfigError::InvalidMerchantId);
        }
        let merchant_key = merchant_key.as_ref();
        if !valid_secret(merchant_key) {
            return Err(EasyPayRefundProviderConfigError::InvalidMerchantKey);
        }
        if request_timeout.is_zero() {
            return Err(EasyPayRefundProviderConfigError::InvalidRequestTimeout);
        }
        Ok(Self {
            api_base_url,
            merchant_id: merchant_id.to_owned(),
            merchant_key: merchant_key.to_vec(),
            clients,
            request_timeout,
        })
    }

    async fn submit_refund(
        &self,
        request: RefundProviderRequest,
    ) -> Result<RefundProviderResult, RefundProviderError> {
        validate_request(&request)?;
        let target = format!("{}{EASYPAY_REFUND_PATH}", self.api_base_url);
        let body = refund_form(&request, &self.merchant_id, &self.merchant_key)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        let response = self
            .execute(&target, headers, Some(Body::from(body)), true)
            .await?;
        decode_response(response).await?;
        RefundProviderResult::new(deterministic_refund_id(&request))
            .map_err(|_| RefundProviderError::InvalidResponse)
    }

    async fn recover_refund(
        &self,
        request: RefundProviderRecoveryRequest,
    ) -> Result<RefundProviderResult, RefundProviderError> {
        validate_request(request.request())?;
        let expected = deterministic_refund_id(request.request());
        if request.provider_refund_id() != expected {
            return Err(RefundProviderError::InvalidResponse);
        }
        // EasyPay 兼容协议没有退款查询接口；只有已绑定的成功事实才允许本地恢复。
        RefundProviderResult::new(expected).map_err(|_| RefundProviderError::InvalidResponse)
    }

    async fn execute(
        &self,
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
            .execute(Method::POST, target, headers, body)
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

impl RefundProvider for EasyPayRefundProvider {
    fn provider(&self) -> &str {
        EASYPAY_PAYMENT_PROVIDER
    }

    fn submit<'a>(&'a self, request: RefundProviderRequest) -> RefundProviderFuture<'a> {
        Box::pin(async move { self.submit_refund(request).await })
    }

    fn recover<'a>(&'a self, request: RefundProviderRecoveryRequest) -> RefundProviderFuture<'a> {
        Box::pin(async move { self.recover_refund(request).await })
    }
}

impl Drop for EasyPayRefundProvider {
    fn drop(&mut self) {
        self.merchant_key.zeroize();
    }
}

impl fmt::Debug for EasyPayRefundProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EasyPayRefundProvider")
            .field("request_timeout", &self.request_timeout)
            .field("endpoint", &"<易支付退款端点>")
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct EasyPayRefundResponse {
    code: serde_json::Value,
}

async fn decode_response(response: af_httpclient::HttpResponse) -> Result<(), RefundProviderError> {
    let status = response.status();
    if !status.is_success() {
        return Err(classify_status(status));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_EASYPAY_RESPONSE_BYTES as u64)
    {
        return Err(RefundProviderError::InvalidResponse);
    }
    let body = response
        .bytes()
        .await
        .map_err(|_| RefundProviderError::OutcomeUnknown)?;
    if body.len() > MAX_EASYPAY_RESPONSE_BYTES {
        return Err(RefundProviderError::InvalidResponse);
    }
    let response: EasyPayRefundResponse =
        serde_json::from_slice(&body).map_err(|_| RefundProviderError::InvalidResponse)?;
    if response_code_is_success(&response.code) {
        Ok(())
    } else {
        Err(RefundProviderError::Rejected)
    }
}

fn classify_status(status: StatusCode) -> RefundProviderError {
    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        RefundProviderError::OutcomeUnknown
    } else {
        RefundProviderError::Rejected
    }
}

fn refund_form(
    request: &RefundProviderRequest,
    merchant_id: &str,
    merchant_key: &[u8],
) -> Result<String, RefundProviderError> {
    let key = str::from_utf8(merchant_key).map_err(|_| RefundProviderError::InvalidResponse)?;
    let amount = format_cny_minor(request.amount_minor())?;
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("pid", merchant_id);
    serializer.append_pair("key", key);
    serializer.append_pair("money", &amount);
    serializer.append_pair("out_trade_no", request.payment_reference());
    Ok(serializer.finish())
}

fn validate_request(request: &RefundProviderRequest) -> Result<(), RefundProviderError> {
    if request.currency() != "CNY" || request.amount_minor() <= 0 {
        return Err(RefundProviderError::Rejected);
    }
    if request.payment_reference().is_empty()
        || request.payment_reference().len() > af_db::MAX_PROVIDER_ORDER_ID_BYTES
        || request.payment_reference().chars().any(char::is_control)
    {
        return Err(RefundProviderError::Rejected);
    }
    Ok(())
}

fn format_cny_minor(amount_minor: i64) -> Result<String, RefundProviderError> {
    let amount_minor = u64::try_from(amount_minor).map_err(|_| RefundProviderError::Rejected)?;
    Ok(format!("{}.{:02}", amount_minor / 100, amount_minor % 100))
}

fn response_code_is_success(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(number) => number.as_i64() == Some(1),
        serde_json::Value::String(value) => value.trim() == "1",
        _ => false,
    }
}

fn deterministic_refund_id(request: &RefundProviderRequest) -> String {
    format!("epay_{}", request.request_id().persistence_key())
}

fn normalize_api_base(value: &str) -> Result<String, EasyPayRefundProviderConfigError> {
    let mut url =
        Url::parse(value).map_err(|_| EasyPayRefundProviderConfigError::InvalidGatewayUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(EasyPayRefundProviderConfigError::InvalidGatewayUrl);
    }
    let mut path = url.path().trim_end_matches('/').to_owned();
    let lower = path.to_ascii_lowercase();
    for suffix in ["/submit.php", "/mapi.php", "/api.php"] {
        if lower.ends_with(suffix) {
            path.truncate(path.len() - suffix.len());
            break;
        }
    }
    url.set_path(path.trim_end_matches('/'));
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn valid_visible_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_secret(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= MAX_EASYPAY_MERCHANT_KEY_BYTES
        && value.iter().all(|byte| (0x20..=0x7e).contains(byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::{
        RefundOrderKind, RefundRequestId, RefundRequestKey, RefundRequestRecord,
        RefundRequestStatus, UserId,
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
            "epay".to_owned(),
            "CNY".to_owned(),
            100,
            40,
            None,
            RefundRequestStatus::Requested,
            1,
            1_900_000_000,
            1_900_000_000,
        )
        .unwrap();
        RefundProviderRequest::new(&record, "order-123".to_owned()).unwrap()
    }

    #[test]
    fn normalizes_known_easypay_endpoints() {
        assert_eq!(
            normalize_api_base("https://pay.example/api").unwrap(),
            "https://pay.example/api"
        );
        assert_eq!(
            normalize_api_base("https://pay.example/api.php").unwrap(),
            "https://pay.example"
        );
        assert!(normalize_api_base("https://user:secret@pay.example").is_err());
    }

    #[test]
    fn builds_refund_form_without_secret_in_debug() {
        let body = refund_form(&request(), "1001", b"secret-key").unwrap();
        assert_eq!(
            body,
            "pid=1001&key=secret-key&money=0.40&out_trade_no=order-123"
        );
        let provider = EasyPayRefundProvider::new(
            "https://pay.example/api",
            "1001",
            b"secret-key",
            HttpClientProvider::new(HttpClientConfig::default(), 2).unwrap(),
            Duration::from_secs(10),
        )
        .unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains("secret-key"));
        assert!(!debug.contains("pay.example"));
    }

    #[test]
    fn accepts_numeric_and_string_success_codes_only() {
        assert!(response_code_is_success(&serde_json::json!(1)));
        assert!(response_code_is_success(&serde_json::json!("1")));
        assert!(!response_code_is_success(&serde_json::json!(0)));
        assert!(!response_code_is_success(&serde_json::json!(true)));
    }

    #[test]
    fn local_recovery_identifier_is_stable() {
        assert_eq!(
            deterministic_refund_id(&request()),
            "epay_01010101010101010101010101010101"
        );
    }
}
