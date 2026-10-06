use std::fmt;

use af_domain::{
    MAX_REFUND_PROVIDER_REFUND_ID_BYTES, RefundRequestId, RefundRequestStatus, TopupOrderId,
    TopupPaymentEventType,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use zeroize::Zeroize;

use crate::payment_webhook::{
    PaymentProvider, PaymentWebhookRequest, PaymentWebhookVerificationError,
    VerifiedTopupPaymentEvent,
};
use crate::refund::{RefundReceiptVerificationError, RefundReceiptVerifier, VerifiedRefundReceipt};

/// Stripe Provider 在订单和 webhook 中使用的稳定标识。
pub const STRIPE_PAYMENT_PROVIDER: &str = "stripe";
/// Stripe 原始签名请求头名称。
pub const STRIPE_SIGNATURE_HEADER: &str = "stripe-signature";
/// Stripe 官方常用的签名时间容差（五分钟）。
pub const DEFAULT_STRIPE_SIGNATURE_TOLERANCE_SECS: u64 = 300;
/// 防止错误配置把重放窗口放大到不可接受范围。
pub const MAX_STRIPE_SIGNATURE_TOLERANCE_SECS: u64 = 86_400;
const MAX_STRIPE_WEBHOOK_SECRET_BYTES: usize = 4096;
const STRIPE_SIGNATURE_BYTES: usize = 32;

/// Stripe Provider 配置错误；不携带密钥内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum StripePaymentProviderConfigError {
    /// webhook 密钥为空或超过有界长度。
    #[error("Stripe webhook 密钥无效")]
    InvalidWebhookSecret,
    /// 签名时间容差必须为正且不能过大。
    #[error("Stripe webhook 签名时间容差无效")]
    InvalidSignatureTolerance,
}

/// Stripe webhook 验签 Provider。
pub struct StripePaymentProvider {
    webhook_secret: Vec<u8>,
    secret_fingerprint: [u8; 32],
    tolerance_secs: u64,
}

impl StripePaymentProvider {
    /// 创建 Stripe Provider；密钥只保存在内存中并在释放时清零。
    pub fn new(
        webhook_secret: impl AsRef<[u8]>,
        tolerance_secs: u64,
    ) -> Result<Self, StripePaymentProviderConfigError> {
        let webhook_secret = webhook_secret.as_ref();
        let invalid_secret = webhook_secret.is_empty()
            || webhook_secret.len() > MAX_STRIPE_WEBHOOK_SECRET_BYTES
            || webhook_secret
                .iter()
                .any(|byte| *byte < b' ' || *byte == 0x7f);
        if invalid_secret {
            return Err(StripePaymentProviderConfigError::InvalidWebhookSecret);
        }
        if tolerance_secs == 0 || tolerance_secs > MAX_STRIPE_SIGNATURE_TOLERANCE_SECS {
            return Err(StripePaymentProviderConfigError::InvalidSignatureTolerance);
        }

        let secret_fingerprint: [u8; 32] = Sha256::digest(webhook_secret).into();
        Ok(Self {
            webhook_secret: webhook_secret.to_vec(),
            secret_fingerprint,
            tolerance_secs,
        })
    }

    /// 返回当前重放保护窗口（秒）。
    #[must_use]
    pub const fn tolerance_secs(&self) -> u64 {
        self.tolerance_secs
    }
}

impl PaymentProvider for StripePaymentProvider {
    fn provider(&self) -> &str {
        STRIPE_PAYMENT_PROVIDER
    }

    fn verify_webhook(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        self.verify_signature(request)?;
        let payload: Value = serde_json::from_slice(request.payload())
            .map_err(|_| PaymentWebhookVerificationError::InvalidPayload)?;
        self.parse_event(&payload)
    }
}

impl RefundReceiptVerifier for StripePaymentProvider {
    fn provider(&self) -> &str {
        STRIPE_PAYMENT_PROVIDER
    }

    fn verify(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedRefundReceipt, RefundReceiptVerificationError> {
        self.verify_signature(request)
            .map_err(|_| RefundReceiptVerificationError::InvalidSignature)?;
        let payload: Value = serde_json::from_slice(request.payload())
            .map_err(|_| RefundReceiptVerificationError::InvalidReceipt)?;
        self.parse_refund_event(&payload)
    }
}

impl StripePaymentProvider {
    /// 解析已验签的 Stripe 退款事件，关联键只从服务端写入的 metadata 读取。
    fn parse_refund_event(
        &self,
        payload: &Value,
    ) -> Result<VerifiedRefundReceipt, RefundReceiptVerificationError> {
        let event_id =
            string_field(payload, &["id"]).ok_or(RefundReceiptVerificationError::InvalidReceipt)?;
        let event_type = string_field(payload, &["type"])
            .ok_or(RefundReceiptVerificationError::InvalidReceipt)?;
        if !matches!(
            event_type,
            "refund.created" | "refund.updated" | "refund.failed"
        ) {
            return Err(RefundReceiptVerificationError::InvalidReceipt);
        }
        let object = payload
            .get("data")
            .and_then(|data| data.get("object"))
            .ok_or(RefundReceiptVerificationError::InvalidReceipt)?;
        let request_id = RefundRequestId::from_persistence_key(
            string_field(object, &["metadata", "anyflows_refund_request_id"])
                .ok_or(RefundReceiptVerificationError::InvalidReceipt)?,
        )
        .map_err(|_| RefundReceiptVerificationError::InvalidReceipt)?;
        let provider_refund_id = string_field(object, &["id"])
            .ok_or(RefundReceiptVerificationError::InvalidReceipt)?
            .to_owned();
        if !valid_stripe_refund_id(&provider_refund_id) {
            return Err(RefundReceiptVerificationError::InvalidReceipt);
        }
        let amount_minor = object
            .get("amount")
            .and_then(Value::as_u64)
            .filter(|amount| *amount > 0 && *amount <= i64::MAX as u64)
            .ok_or(RefundReceiptVerificationError::InvalidReceipt)?;
        let currency = string_field(object, &["currency"])
            .filter(|value| value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_lowercase()))
            .map(str::to_ascii_uppercase)
            .ok_or(RefundReceiptVerificationError::InvalidReceipt)?;
        let status = match string_field(object, &["status"]) {
            Some("succeeded") => RefundRequestStatus::Succeeded,
            Some("failed" | "canceled") => RefundRequestStatus::Failed,
            _ => return Err(RefundReceiptVerificationError::InvalidReceipt),
        };
        VerifiedRefundReceipt::new(
            request_id,
            event_id.to_owned(),
            provider_refund_id,
            status,
            i64::try_from(amount_minor)
                .map_err(|_| RefundReceiptVerificationError::InvalidReceipt)?,
            currency,
            self.secret_fingerprint,
        )
    }

    fn verify_signature(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<(), PaymentWebhookVerificationError> {
        let mut matching_headers = request
            .headers()
            .iter()
            .filter(|header| header.name().eq_ignore_ascii_case(STRIPE_SIGNATURE_HEADER));
        let signature = matching_headers
            .next()
            .map(|header| header.value())
            .ok_or(PaymentWebhookVerificationError::InvalidSignature)?;
        if matching_headers.next().is_some() {
            return Err(PaymentWebhookVerificationError::InvalidSignature);
        }
        let (timestamp, signatures) = parse_signature_header(signature)?;
        if timestamp == 0 || request.received_at().abs_diff(timestamp) > self.tolerance_secs {
            return Err(PaymentWebhookVerificationError::InvalidSignature);
        }

        let timestamp_text = timestamp.to_string();
        let mut signed_payload =
            Vec::with_capacity(timestamp_text.len() + 1 + request.payload().len());
        signed_payload.extend_from_slice(timestamp_text.as_bytes());
        signed_payload.push(b'.');
        signed_payload.extend_from_slice(request.payload());
        let expected = hmac_sha256(&self.webhook_secret, &signed_payload);
        if signatures
            .iter()
            .all(|candidate| !constant_time_eq(candidate, &expected))
        {
            return Err(PaymentWebhookVerificationError::InvalidSignature);
        }
        Ok(())
    }

    fn parse_event(
        &self,
        payload: &Value,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        let event_id = string_field(payload, &["id"])
            .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
        let event_type = string_field(payload, &["type"])
            .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
        let object = payload
            .get("data")
            .and_then(|data| data.get("object"))
            .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
        let object_id = string_field(object, &["id"]);

        let (kind, trade_no) = match event_type {
            "checkout.session.completed" => {
                let payment_status = string_field(object, &["payment_status"])
                    .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
                if payment_status != "paid" {
                    return Err(PaymentWebhookVerificationError::InvalidPayload);
                }
                (TopupPaymentEventType::Succeeded, checkout_trade_no(object))
            }
            "checkout.session.async_payment_succeeded" => {
                (TopupPaymentEventType::Succeeded, checkout_trade_no(object))
            }
            "checkout.session.async_payment_failed" => (TopupPaymentEventType::Failed, object_id),
            // 单次支付方式失败后同一个 PaymentIntent 仍可继续确认，不能关闭本地订单。
            "payment_intent.payment_failed" => {
                return Err(PaymentWebhookVerificationError::NonTerminalEvent);
            }
            "checkout.session.expired" => (TopupPaymentEventType::Expired, object_id),
            "payment_intent.succeeded" => (TopupPaymentEventType::Succeeded, object_id),
            "payment_intent.canceled" => (TopupPaymentEventType::Failed, object_id),
            _ => return Err(PaymentWebhookVerificationError::UnsupportedEvent),
        };
        let amount_minor = stripe_amount_minor(object)?;
        let currency = string_field(object, &["currency"])
            .filter(|value| value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_lowercase()))
            .map(str::to_ascii_uppercase)
            .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
        let payment_method = stripe_payment_method(object)?;

        VerifiedTopupPaymentEvent::new(
            TopupOrderId::from_persistence_key(
                string_field(object, &["metadata", "anyflows_order_id"])
                    .ok_or(PaymentWebhookVerificationError::InvalidPayload)?,
            )
            .map_err(|_| PaymentWebhookVerificationError::InvalidPayload)?,
            event_id.to_owned(),
            trade_no.map(str::to_owned),
            kind,
            amount_minor,
            currency,
            payment_method.to_owned(),
            self.secret_fingerprint,
        )
    }
}

impl Drop for StripePaymentProvider {
    fn drop(&mut self) {
        self.webhook_secret.zeroize();
        self.secret_fingerprint.zeroize();
    }
}

impl fmt::Debug for StripePaymentProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StripePaymentProvider")
            .field("tolerance_secs", &self.tolerance_secs)
            .finish_non_exhaustive()
    }
}

fn parse_signature_header(
    value: &str,
) -> Result<(u64, Vec<[u8; STRIPE_SIGNATURE_BYTES]>), PaymentWebhookVerificationError> {
    let mut timestamp = None;
    let mut signatures = Vec::new();
    for item in value.split(',') {
        let (key, raw_value) = item
            .trim()
            .split_once('=')
            .ok_or(PaymentWebhookVerificationError::InvalidSignature)?;
        let raw_value = raw_value.trim();
        if raw_value.is_empty() {
            return Err(PaymentWebhookVerificationError::InvalidSignature);
        }
        match key {
            "t" => {
                let parsed = raw_value
                    .parse()
                    .map_err(|_| PaymentWebhookVerificationError::InvalidSignature)?;
                if timestamp.replace(parsed).is_some() {
                    return Err(PaymentWebhookVerificationError::InvalidSignature);
                }
            }
            "v1" => signatures.push(decode_signature(raw_value)?),
            _ => {}
        }
    }
    let timestamp = timestamp.ok_or(PaymentWebhookVerificationError::InvalidSignature)?;
    if signatures.is_empty() {
        return Err(PaymentWebhookVerificationError::InvalidSignature);
    }
    Ok((timestamp, signatures))
}

fn decode_signature(
    value: &str,
) -> Result<[u8; STRIPE_SIGNATURE_BYTES], PaymentWebhookVerificationError> {
    if value.len() != STRIPE_SIGNATURE_BYTES * 2 {
        return Err(PaymentWebhookVerificationError::InvalidSignature);
    }
    let mut decoded = [0_u8; STRIPE_SIGNATURE_BYTES];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or(PaymentWebhookVerificationError::InvalidSignature)?;
        let low = hex_digit(pair[1]).ok_or(PaymentWebhookVerificationError::InvalidSignature)?;
        decoded[index] = (high << 4) | low;
    }
    Ok(decoded)
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block_key = [0_u8; 64];
    if key.len() > block_key.len() {
        block_key[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for index in 0..block_key.len() {
        inner_pad[index] ^= block_key[index];
        outer_pad[index] ^= block_key[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    let result = outer.finalize().into();
    block_key.zeroize();
    inner_pad.zeroize();
    outer_pad.zeroize();
    result
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

fn string_field<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
        .and_then(Value::as_str)
}

fn checkout_trade_no(value: &Value) -> Option<&str> {
    value
        .get("payment_intent")
        .and_then(Value::as_str)
        .or_else(|| value.get("id").and_then(Value::as_str))
}

fn stripe_amount_minor(value: &Value) -> Result<u64, PaymentWebhookVerificationError> {
    ["amount_received", "amount_total", "amount"]
        .into_iter()
        .find_map(|field| value.get(field).and_then(Value::as_u64))
        .filter(|amount| *amount <= i64::MAX as u64)
        .ok_or(PaymentWebhookVerificationError::InvalidPayload)
}

fn stripe_payment_method(value: &Value) -> Result<&str, PaymentWebhookVerificationError> {
    value
        .get("payment_method_types")
        .and_then(Value::as_array)
        .and_then(|methods| methods.first())
        .and_then(Value::as_str)
        .filter(|method| {
            !method.is_empty()
                && method.len() <= 32
                && method.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
        })
        .ok_or(PaymentWebhookVerificationError::InvalidPayload)
}

fn valid_stripe_refund_id(value: &str) -> bool {
    value.starts_with("re_")
        && value.len() <= MAX_REFUND_PROVIDER_REFUND_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

const fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PaymentWebhookHeader, PaymentWebhookRequest};

    const RECEIVED_AT: u64 = 1_900_000_000;
    const ORDER_ID: &str = "11111111111111111111111111111111";

    #[test]
    fn verifies_signature_and_payment_intent_event() {
        let payload = r#"{"id":"evt_123","type":"payment_intent.succeeded","data":{"object":{"id":"pi_123","amount_received":1234,"currency":"usd","payment_method_types":["card"],"metadata":{"anyflows_order_id":"ORDER_ID"}}}}"#
            .replace("ORDER_ID", ORDER_ID);
        let provider = StripePaymentProvider::new("whsec_test", 300).unwrap();
        let signature = signature_header(&provider, payload.as_bytes(), RECEIVED_AT);
        let header = PaymentWebhookHeader::new("Stripe-Signature", &signature).unwrap();
        let headers = [header];
        let request =
            PaymentWebhookRequest::new(payload.as_bytes(), &headers, RECEIVED_AT).unwrap();

        assert_eq!(
            PaymentProvider::provider(&provider),
            STRIPE_PAYMENT_PROVIDER
        );
        assert!(provider.verify_webhook(&request).is_ok());
    }

    #[test]
    fn rejects_tampering_and_replayed_timestamps() {
        let payload = r#"{"id":"evt_123","type":"payment_intent.succeeded","data":{"object":{"id":"pi_123","amount_received":1234,"currency":"usd","payment_method_types":["card"],"metadata":{"anyflows_order_id":"ORDER_ID"}}}}"#
            .replace("ORDER_ID", ORDER_ID);
        let provider = StripePaymentProvider::new("whsec_test", 300).unwrap();
        let signature = signature_header(&provider, payload.as_bytes(), RECEIVED_AT + 301);
        let header = PaymentWebhookHeader::new(STRIPE_SIGNATURE_HEADER, &signature).unwrap();
        let headers = [header];
        let request =
            PaymentWebhookRequest::new(payload.as_bytes(), &headers, RECEIVED_AT).unwrap();
        assert!(matches!(
            provider.verify_webhook(&request),
            Err(PaymentWebhookVerificationError::InvalidSignature)
        ));
    }

    #[test]
    fn rejects_unpaid_checkout_and_unknown_events() {
        let provider = StripePaymentProvider::new("whsec_test", 300).unwrap();
        let unpaid = r#"{"id":"evt_123","type":"checkout.session.completed","data":{"object":{"id":"cs_123","payment_status":"unpaid","metadata":{"anyflows_order_id":"ORDER_ID"}}}}"#
            .replace("ORDER_ID", ORDER_ID);
        let unpaid_signature = signature_header(&provider, unpaid.as_bytes(), RECEIVED_AT);
        let unpaid_header =
            PaymentWebhookHeader::new(STRIPE_SIGNATURE_HEADER, &unpaid_signature).unwrap();
        let unpaid_headers = [unpaid_header];
        let unpaid_request =
            PaymentWebhookRequest::new(unpaid.as_bytes(), &unpaid_headers, RECEIVED_AT).unwrap();
        assert!(matches!(
            provider.verify_webhook(&unpaid_request),
            Err(PaymentWebhookVerificationError::InvalidPayload)
        ));

        let unknown = r#"{"id":"evt_123","type":"customer.created","data":{"object":{"metadata":{"anyflows_order_id":"ORDER_ID"}}}}"#
            .replace("ORDER_ID", ORDER_ID);
        let unknown_signature = signature_header(&provider, unknown.as_bytes(), RECEIVED_AT);
        let unknown_header =
            PaymentWebhookHeader::new(STRIPE_SIGNATURE_HEADER, &unknown_signature).unwrap();
        let unknown_headers = [unknown_header];
        let unknown_request =
            PaymentWebhookRequest::new(unknown.as_bytes(), &unknown_headers, RECEIVED_AT).unwrap();
        assert!(matches!(
            provider.verify_webhook(&unknown_request),
            Err(PaymentWebhookVerificationError::UnsupportedEvent)
        ));
    }

    #[test]
    fn treats_payment_method_failure_as_a_retryable_non_terminal_event() {
        let provider = StripePaymentProvider::new("whsec_test", 300).unwrap();
        let payload = r#"{"id":"evt_failed","type":"payment_intent.payment_failed","data":{"object":{"id":"pi_123","status":"requires_payment_method","metadata":{"anyflows_order_id":"ORDER_ID"}}}}"#
            .replace("ORDER_ID", ORDER_ID);
        let signature = signature_header(&provider, payload.as_bytes(), RECEIVED_AT);
        let header = PaymentWebhookHeader::new(STRIPE_SIGNATURE_HEADER, &signature).unwrap();
        let headers = [header];
        let request =
            PaymentWebhookRequest::new(payload.as_bytes(), &headers, RECEIVED_AT).unwrap();

        assert_eq!(
            provider.verify_webhook(&request).unwrap_err(),
            PaymentWebhookVerificationError::NonTerminalEvent
        );
    }

    #[test]
    fn verifies_terminal_refund_event_from_server_metadata() {
        let payload = r#"{"id":"evt_refund_123","type":"refund.updated","data":{"object":{"id":"re_123","amount":40,"currency":"usd","status":"succeeded","metadata":{"anyflows_refund_request_id":"01010101010101010101010101010101"}}}}"#;
        let provider = StripePaymentProvider::new("whsec_test", 300).unwrap();
        let signature = signature_header(&provider, payload.as_bytes(), RECEIVED_AT);
        let header = PaymentWebhookHeader::new(STRIPE_SIGNATURE_HEADER, &signature).unwrap();
        let headers = [header];
        let request =
            PaymentWebhookRequest::new(payload.as_bytes(), &headers, RECEIVED_AT).unwrap();

        assert!(RefundReceiptVerifier::verify(&provider, &request).is_ok());

        let pending = payload.replace("succeeded", "pending");
        let pending_signature = signature_header(&provider, pending.as_bytes(), RECEIVED_AT);
        let pending_header =
            PaymentWebhookHeader::new(STRIPE_SIGNATURE_HEADER, &pending_signature).unwrap();
        let pending_headers = [pending_header];
        let pending_request =
            PaymentWebhookRequest::new(pending.as_bytes(), &pending_headers, RECEIVED_AT).unwrap();
        assert_eq!(
            RefundReceiptVerifier::verify(&provider, &pending_request).unwrap_err(),
            RefundReceiptVerificationError::InvalidReceipt
        );
    }

    #[test]
    fn configuration_and_debug_output_do_not_disclose_secret() {
        assert_eq!(
            StripePaymentProvider::new("", 300).unwrap_err(),
            StripePaymentProviderConfigError::InvalidWebhookSecret
        );
        assert_eq!(
            StripePaymentProvider::new("secret", 0).unwrap_err(),
            StripePaymentProviderConfigError::InvalidSignatureTolerance
        );
        let provider = StripePaymentProvider::new("whsec_secret", 300).unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains("whsec_secret"));
    }

    #[test]
    fn hmac_matches_rfc_reference_vector() {
        let digest = hmac_sha256(b"key", b"The quick brown fox jumps over the lazy dog");
        let expected = "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8";
        let mut actual = String::new();
        for byte in digest {
            actual.push_str(&format!("{byte:02x}"));
        }
        assert_eq!(actual, expected);
    }

    fn signature_header(
        provider: &StripePaymentProvider,
        payload: &[u8],
        timestamp: u64,
    ) -> String {
        let mut signed_payload = timestamp.to_string().into_bytes();
        signed_payload.push(b'.');
        signed_payload.extend_from_slice(payload);
        let digest = hmac_sha256(&provider.webhook_secret, &signed_payload);
        let mut encoded = String::from("t=");
        encoded.push_str(&timestamp.to_string());
        encoded.push_str(",v1=");
        for byte in digest {
            encoded.push_str(&format!("{byte:02x}"));
        }
        encoded
    }
}
