use std::{collections::BTreeMap, fmt};

use af_domain::{TopupOrderId, TopupPaymentEventType};
use md5::{Digest as Md5Digest, Md5};
use sha2::Sha256;
use thiserror::Error;
use url::Url;
use zeroize::Zeroize;

use crate::{
    PaymentOrderFuture, PaymentOrderProvider, PaymentOrderProviderError,
    PaymentOrderRecoveryRequest, PaymentOrderRequest, PaymentOrderSession, PaymentProvider,
    PaymentRedirectUrl, PaymentWebhookRequest, PaymentWebhookVerificationError,
    VerifiedTopupPaymentEvent,
};

/// 易支付在订单、回调和持久化记录中使用的稳定标识。
pub const EASYPAY_PAYMENT_PROVIDER: &str = "epay";
const EASYPAY_SIGN_TYPE: &str = "MD5";
const EASYPAY_SUCCESS_STATUS: &str = "TRADE_SUCCESS";
const MAX_EASYPAY_MERCHANT_ID_BYTES: usize = 64;
const MAX_EASYPAY_MERCHANT_KEY_BYTES: usize = 4096;
const MAX_EASYPAY_PRODUCT_NAME_BYTES: usize = 128;
const MAX_EASYPAY_FORM_FIELD_BYTES: usize = 16 * 1024;

/// 易支付首版支持的闭合支付方式。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EasyPayPaymentMethod {
    /// 支付宝。
    Alipay,
    /// 微信支付。
    WechatPay,
}

impl EasyPayPaymentMethod {
    /// 返回易支付协议要求的 `type` 参数。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Alipay => "alipay",
            Self::WechatPay => "wxpay",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "alipay" => Some(Self::Alipay),
            "wxpay" => Some(Self::WechatPay),
            _ => None,
        }
    }
}

/// 易支付配置错误；错误值不携带商户密钥或回调地址。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EasyPayPaymentProviderConfigError {
    /// 网关地址不是无凭据、无查询参数的绝对 HTTP(S) 地址。
    #[error("易支付网关地址无效")]
    InvalidGatewayUrl,
    /// 商户 PID 不满足有界可打印文本约束。
    #[error("易支付商户 PID 无效")]
    InvalidMerchantId,
    /// 商户密钥为空、过长或包含控制字符。
    #[error("易支付商户密钥无效")]
    InvalidMerchantKey,
    /// 异步通知地址不是绝对 HTTP(S) 地址。
    #[error("易支付异步通知地址无效")]
    InvalidNotifyUrl,
    /// 同步返回地址不是绝对 HTTP(S) 地址。
    #[error("易支付同步返回地址无效")]
    InvalidReturnUrl,
    /// 商品名称为空、过长或包含控制字符。
    #[error("易支付商品名称无效")]
    InvalidProductName,
    /// 启用支付方式为空或包含重复项。
    #[error("易支付启用方式无效")]
    InvalidPaymentMethods,
}

/// 易支付签名输入错误；签名结果和密钥均不会进入错误文本。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EasyPaySigningError {
    /// 商户密钥为空或超过安全边界。
    #[error("易支付签名密钥无效")]
    InvalidMerchantKey,
    /// 参数名或参数值违反易支付签名边界。
    #[error("易支付签名参数无效")]
    InvalidParameter,
}

/// 易支付托管收银台 Provider，同时承担下单和回调验签职责。
pub struct EasyPayPaymentProvider {
    submit_url: Url,
    merchant_id: String,
    merchant_key: Vec<u8>,
    secret_fingerprint: [u8; 32],
    notify_url: String,
    return_url: String,
    enabled_payment_methods: Vec<EasyPayPaymentMethod>,
    product_name: String,
}

impl EasyPayPaymentProvider {
    /// 创建易支付 Provider；商户密钥只驻留内存，并在释放时清零。
    #[allow(clippy::too_many_arguments, reason = "字段与易支付商户快照一一对应")]
    pub fn new(
        gateway_url: String,
        merchant_id: String,
        merchant_key: impl AsRef<[u8]>,
        notify_url: String,
        return_url: String,
        mut enabled_payment_methods: Vec<EasyPayPaymentMethod>,
        product_name: String,
    ) -> Result<Self, EasyPayPaymentProviderConfigError> {
        let submit_url = normalize_submit_url(&gateway_url)?;
        if !valid_visible_text(&merchant_id, MAX_EASYPAY_MERCHANT_ID_BYTES) {
            return Err(EasyPayPaymentProviderConfigError::InvalidMerchantId);
        }
        let merchant_key = merchant_key.as_ref();
        if !valid_secret(merchant_key) {
            return Err(EasyPayPaymentProviderConfigError::InvalidMerchantKey);
        }
        validate_callback_url(&notify_url, false)
            .map_err(|_| EasyPayPaymentProviderConfigError::InvalidNotifyUrl)?;
        // 同步返回可以使用 SPA hash 路由；它只参与浏览器导航，不承担到账语义。
        validate_callback_url(&return_url, true)
            .map_err(|_| EasyPayPaymentProviderConfigError::InvalidReturnUrl)?;
        if !valid_visible_text(&product_name, MAX_EASYPAY_PRODUCT_NAME_BYTES) {
            return Err(EasyPayPaymentProviderConfigError::InvalidProductName);
        }
        enabled_payment_methods.sort_unstable();
        let original_len = enabled_payment_methods.len();
        enabled_payment_methods.dedup();
        if enabled_payment_methods.is_empty() || enabled_payment_methods.len() != original_len {
            return Err(EasyPayPaymentProviderConfigError::InvalidPaymentMethods);
        }

        Ok(Self {
            submit_url,
            merchant_id,
            merchant_key: merchant_key.to_vec(),
            secret_fingerprint: Sha256::digest(merchant_key).into(),
            notify_url,
            return_url,
            enabled_payment_methods,
            product_name,
        })
    }

    fn build_session(
        &self,
        request: &PaymentOrderRequest,
    ) -> Result<PaymentOrderSession, PaymentOrderProviderError> {
        if request.currency() != "CNY" {
            return Err(PaymentOrderProviderError::Rejected);
        }
        let payment_method = EasyPayPaymentMethod::parse(request.payment_method())
            .filter(|method| self.enabled_payment_methods.contains(method))
            .ok_or(PaymentOrderProviderError::Rejected)?;
        let provider_order_id = request.order_id().persistence_key();
        let parameters = self.checkout_parameters(request, payment_method);
        let signature = easy_pay_signature(&parameters, &self.merchant_key)
            .map_err(|_| PaymentOrderProviderError::InvalidResponse)?;
        let mut redirect_url = self.submit_url.clone();
        {
            let mut query = redirect_url.query_pairs_mut();
            for (key, value) in &parameters {
                query.append_pair(key, value);
            }
            query.append_pair("sign", &signature);
            query.append_pair("sign_type", EASYPAY_SIGN_TYPE);
        }
        let redirect_url = PaymentRedirectUrl::new(redirect_url.into())
            .map_err(|_| PaymentOrderProviderError::InvalidResponse)?;
        PaymentOrderSession::with_redirect_url(provider_order_id, redirect_url)
            .map_err(|_| PaymentOrderProviderError::InvalidResponse)
    }

    fn checkout_parameters(
        &self,
        request: &PaymentOrderRequest,
        payment_method: EasyPayPaymentMethod,
    ) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("money".to_owned(), format_cny_minor(request.amount_minor())),
            ("name".to_owned(), self.product_name.clone()),
            ("notify_url".to_owned(), self.notify_url.clone()),
            (
                "out_trade_no".to_owned(),
                request.order_id().persistence_key(),
            ),
            ("pid".to_owned(), self.merchant_id.clone()),
            ("return_url".to_owned(), self.return_url.clone()),
            ("type".to_owned(), payment_method.as_str().to_owned()),
        ])
    }

    fn parse_verified_event(
        &self,
        payload: &[u8],
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        // 表单只在这里严格解码一次，后续验签与业务读取共享同一份参数，避免参数污染。
        let parameters = parse_form_once(payload)?;
        self.verify_signature(&parameters)?;

        let merchant_id = required_parameter(&parameters, "pid")?;
        if merchant_id != self.merchant_id {
            return Err(PaymentWebhookVerificationError::InvalidPayload);
        }
        if required_parameter(&parameters, "sign_type")? != EASYPAY_SIGN_TYPE {
            return Err(PaymentWebhookVerificationError::InvalidSignature);
        }
        let payment_method = EasyPayPaymentMethod::parse(required_parameter(&parameters, "type")?)
            .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
        if !self.enabled_payment_methods.contains(&payment_method) {
            return Err(PaymentWebhookVerificationError::InvalidPayload);
        }

        let order_id =
            TopupOrderId::from_persistence_key(required_parameter(&parameters, "out_trade_no")?)
                .map_err(|_| PaymentWebhookVerificationError::InvalidPayload)?;
        let trade_no = required_parameter(&parameters, "trade_no")?;
        if !valid_visible_text(trade_no, af_db::MAX_PROVIDER_TRADE_NO_BYTES) {
            return Err(PaymentWebhookVerificationError::InvalidPayload);
        }
        let trade_status = required_parameter(&parameters, "trade_status")?;
        if !valid_visible_text(trade_status, 64) {
            return Err(PaymentWebhookVerificationError::InvalidPayload);
        }
        let amount_minor = parse_cny_minor(required_parameter(&parameters, "money")?)?;

        if trade_status != EASYPAY_SUCCESS_STATUS {
            return Err(PaymentWebhookVerificationError::NonTerminalEvent);
        }

        let provider_event_id = deterministic_event_id(
            merchant_id,
            payment_method.as_str(),
            &order_id.persistence_key(),
            trade_no,
            trade_status,
        );
        VerifiedTopupPaymentEvent::new(
            order_id,
            provider_event_id,
            Some(trade_no.to_owned()),
            TopupPaymentEventType::Succeeded,
            amount_minor,
            "CNY".to_owned(),
            payment_method.as_str().to_owned(),
            self.secret_fingerprint,
        )
    }

    fn verify_signature(
        &self,
        parameters: &BTreeMap<String, String>,
    ) -> Result<(), PaymentWebhookVerificationError> {
        let provided = decode_md5_hex(required_parameter(parameters, "sign")?)?;
        let expected = easy_pay_signature_bytes(parameters, &self.merchant_key)
            .map_err(|_| PaymentWebhookVerificationError::InvalidSignature)?;
        if !constant_time_eq(&provided, &expected) {
            return Err(PaymentWebhookVerificationError::InvalidSignature);
        }
        Ok(())
    }
}

impl PaymentOrderProvider for EasyPayPaymentProvider {
    fn provider(&self) -> &str {
        EASYPAY_PAYMENT_PROVIDER
    }

    fn create<'a>(&'a self, request: PaymentOrderRequest) -> PaymentOrderFuture<'a> {
        Box::pin(async move { self.build_session(&request) })
    }

    fn recover<'a>(&'a self, request: PaymentOrderRecoveryRequest) -> PaymentOrderFuture<'a> {
        Box::pin(async move {
            if request.provider_order_id() != request.order().order_id().persistence_key() {
                return Err(PaymentOrderProviderError::InvalidResponse);
            }
            self.build_session(request.order())
        })
    }
}

impl PaymentProvider for EasyPayPaymentProvider {
    fn provider(&self) -> &str {
        EASYPAY_PAYMENT_PROVIDER
    }

    fn verify_webhook(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        self.parse_verified_event(request.payload())
    }
}

impl Drop for EasyPayPaymentProvider {
    fn drop(&mut self) {
        self.merchant_key.zeroize();
        self.secret_fingerprint.zeroize();
    }
}

impl fmt::Debug for EasyPayPaymentProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EasyPayPaymentProvider")
            .field("enabled_method_count", &self.enabled_payment_methods.len())
            .finish_non_exhaustive()
    }
}

/// 按易支付规则生成 32 位小写 MD5 签名。
///
/// `sign`、`sign_type` 和空值不会进入签名，剩余参数按 ASCII 参数名升序连接。
pub fn easy_pay_signature(
    parameters: &BTreeMap<String, String>,
    merchant_key: impl AsRef<[u8]>,
) -> Result<String, EasyPaySigningError> {
    let digest = easy_pay_signature_bytes(parameters, merchant_key.as_ref())?;
    Ok(encode_lower_hex(&digest))
}

fn easy_pay_signature_bytes(
    parameters: &BTreeMap<String, String>,
    merchant_key: &[u8],
) -> Result<[u8; 16], EasyPaySigningError> {
    if !valid_secret(merchant_key) {
        return Err(EasyPaySigningError::InvalidMerchantKey);
    }
    let mut signing_text = Vec::new();
    let mut first = true;
    for (key, value) in parameters {
        if matches!(key.as_str(), "sign" | "sign_type") || value.is_empty() {
            continue;
        }
        if !valid_signing_parameter(key) || !valid_signing_parameter(value) {
            signing_text.zeroize();
            return Err(EasyPaySigningError::InvalidParameter);
        }
        if !first {
            signing_text.push(b'&');
        }
        first = false;
        signing_text.extend_from_slice(key.as_bytes());
        signing_text.push(b'=');
        signing_text.extend_from_slice(value.as_bytes());
    }
    signing_text.extend_from_slice(merchant_key);
    let digest: [u8; 16] = Md5::digest(&signing_text).into();
    signing_text.zeroize();
    Ok(digest)
}

fn normalize_submit_url(value: &str) -> Result<Url, EasyPayPaymentProviderConfigError> {
    let mut url =
        Url::parse(value).map_err(|_| EasyPayPaymentProviderConfigError::InvalidGatewayUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(EasyPayPaymentProviderConfigError::InvalidGatewayUrl);
    }
    let path = url.path().trim_end_matches('/');
    if !path.ends_with("/submit.php") && path != "submit.php" {
        let path = if path.is_empty() {
            "/submit.php".to_owned()
        } else {
            format!("{path}/submit.php")
        };
        url.set_path(&path);
    }
    Ok(url)
}

fn validate_callback_url(value: &str, allow_fragment: bool) -> Result<(), ()> {
    let url = Url::parse(value).map_err(|_| ())?;
    if matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && (allow_fragment || (url.query().is_none() && url.fragment().is_none()))
    {
        Ok(())
    } else {
        Err(())
    }
}

fn parse_form_once(
    payload: &[u8],
) -> Result<BTreeMap<String, String>, PaymentWebhookVerificationError> {
    let mut parameters = BTreeMap::new();
    for pair in payload.split(|byte| *byte == b'&') {
        if pair.is_empty() {
            return Err(PaymentWebhookVerificationError::InvalidPayload);
        }
        let separator = pair
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
        let (key, value_with_separator) = pair.split_at(separator);
        let value = &value_with_separator[1..];
        let key = decode_form_component(key)?;
        let value = decode_form_component(value)?;
        if key.is_empty()
            || key.len() > MAX_EASYPAY_FORM_FIELD_BYTES
            || value.len() > MAX_EASYPAY_FORM_FIELD_BYTES
            || parameters.insert(key, value).is_some()
        {
            return Err(PaymentWebhookVerificationError::InvalidPayload);
        }
    }
    Ok(parameters)
}

fn decode_form_component(input: &[u8]) -> Result<String, PaymentWebhookVerificationError> {
    let mut decoded = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= input.len() {
                    return Err(PaymentWebhookVerificationError::InvalidPayload);
                }
                let high = hex_digit(input[index + 1])
                    .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
                let low = hex_digit(input[index + 2])
                    .ok_or(PaymentWebhookVerificationError::InvalidPayload)?;
                decoded.push((high << 4) | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| PaymentWebhookVerificationError::InvalidPayload)
}

fn parse_cny_minor(value: &str) -> Result<u64, PaymentWebhookVerificationError> {
    let (major, fraction) = match value.split_once('.') {
        Some((major, fraction)) => (major, Some(fraction)),
        None => (value, None),
    };
    if major.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || (major.len() > 1 && major.starts_with('0'))
    {
        return Err(PaymentWebhookVerificationError::InvalidPayload);
    }
    let fractional_minor = match fraction {
        None => 0,
        Some(value) if value.len() == 1 && value.bytes().all(|byte| byte.is_ascii_digit()) => {
            u64::from(value.as_bytes()[0] - b'0') * 10
        }
        Some(value) if value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_digit()) => {
            u64::from(value.as_bytes()[0] - b'0') * 10 + u64::from(value.as_bytes()[1] - b'0')
        }
        _ => return Err(PaymentWebhookVerificationError::InvalidPayload),
    };
    major
        .parse::<u64>()
        .ok()
        .and_then(|value| value.checked_mul(100))
        .and_then(|value| value.checked_add(fractional_minor))
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or(PaymentWebhookVerificationError::InvalidPayload)
}

fn format_cny_minor(value: u64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}

fn deterministic_event_id(
    merchant_id: &str,
    payment_method: &str,
    order_id: &str,
    trade_no: &str,
    trade_status: &str,
) -> String {
    let mut hasher = Sha256::new();
    for component in [
        merchant_id,
        payment_method,
        order_id,
        trade_no,
        trade_status,
    ] {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component.as_bytes());
    }
    format!("epay_{}", encode_lower_hex(&hasher.finalize()))
}

fn required_parameter<'a>(
    parameters: &'a BTreeMap<String, String>,
    name: &str,
) -> Result<&'a str, PaymentWebhookVerificationError> {
    parameters
        .get(name)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .ok_or(PaymentWebhookVerificationError::InvalidPayload)
}

fn decode_md5_hex(value: &str) -> Result<[u8; 16], PaymentWebhookVerificationError> {
    if value.len() != 32 {
        return Err(PaymentWebhookVerificationError::InvalidSignature);
    }
    let mut decoded = [0_u8; 16];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or(PaymentWebhookVerificationError::InvalidSignature)?;
        let low = hex_digit(pair[1]).ok_or(PaymentWebhookVerificationError::InvalidSignature)?;
        decoded[index] = (high << 4) | low;
    }
    Ok(decoded)
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
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

fn valid_visible_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_signing_parameter(value: &str) -> bool {
    value.len() <= MAX_EASYPAY_FORM_FIELD_BYTES
        && !value.chars().any(|character| character.is_control())
}

fn valid_secret(value: &[u8]) -> bool {
    !value.is_empty()
        && value.len() <= MAX_EASYPAY_MERCHANT_KEY_BYTES
        && !value.iter().any(|byte| *byte < b' ' || *byte == 0x7f)
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
    use crate::{PaymentWebhookRequest, PaymentWebhookVerifier};

    const ORDER_ID: &str = "11111111111111111111111111111111";
    const KEY: &str = "test-secret-key";

    #[test]
    fn signature_is_sorted_and_ignores_protocol_signature_fields() {
        let parameters = BTreeMap::from([
            ("type".to_owned(), "alipay".to_owned()),
            ("pid".to_owned(), "1001".to_owned()),
            ("empty".to_owned(), String::new()),
            ("sign".to_owned(), "ignored".to_owned()),
            ("sign_type".to_owned(), "MD5".to_owned()),
        ]);
        assert_eq!(
            easy_pay_signature(&parameters, KEY).unwrap(),
            "131c9acfcdf5de1a27a43b9d5987f764"
        );
    }

    #[tokio::test]
    async fn hosted_checkout_is_deterministic_and_recoverable() {
        let provider = provider(EasyPayPaymentMethod::Alipay);
        let request = PaymentOrderRequest::new(
            TopupOrderId::from_persistence_key(ORDER_ID).unwrap(),
            1234,
            "CNY".to_owned(),
            "alipay".to_owned(),
        )
        .unwrap();
        let first = provider.create(request).await.unwrap();
        let first_url = first.redirect_url().unwrap().expose().to_owned();
        assert!(first.client_secret().is_none());
        assert!(first_url.starts_with("https://pay.example.test/api/submit.php?"));
        assert!(first_url.contains("money=12.34"));

        let request = PaymentOrderRequest::new(
            TopupOrderId::from_persistence_key(ORDER_ID).unwrap(),
            1234,
            "CNY".to_owned(),
            "alipay".to_owned(),
        )
        .unwrap();
        let recovery = PaymentOrderRecoveryRequest::new(request, ORDER_ID.to_owned()).unwrap();
        let recovered = provider.recover(recovery).await.unwrap();
        assert_eq!(recovered.redirect_url().unwrap().expose(), first_url);
    }

    #[test]
    fn verifies_success_and_exposes_exact_settlement_facts() {
        let provider = provider(EasyPayPaymentMethod::Alipay);
        let payload = signed_payload(base_callback_parameters(), KEY);
        let request = PaymentWebhookRequest::new(payload.as_bytes(), &[], 1_900_000_000).unwrap();
        let event = provider.verify(&request).unwrap();

        assert_eq!(event.amount_minor(), 1234);
        assert_eq!(event.currency(), "CNY");
        assert_eq!(event.payment_method(), "alipay");
        assert!(!format!("{event:?}").contains("trade-secret"));
    }

    #[test]
    fn rejects_tampering_duplicate_keys_and_wrong_merchant() {
        let provider = provider(EasyPayPaymentMethod::Alipay);
        let mut payload = signed_payload(base_callback_parameters(), KEY);
        payload = payload.replace("money=12.34", "money=99.99");
        assert_eq!(
            verify_payload(&provider, &payload).unwrap_err(),
            PaymentWebhookVerificationError::InvalidSignature
        );

        let duplicate = format!(
            "{}&pid=1001",
            signed_payload(base_callback_parameters(), KEY)
        );
        assert_eq!(
            verify_payload(&provider, &duplicate).unwrap_err(),
            PaymentWebhookVerificationError::InvalidPayload
        );

        let mut wrong_merchant = base_callback_parameters();
        wrong_merchant.insert("pid".to_owned(), "1002".to_owned());
        let wrong_merchant = signed_payload(wrong_merchant, KEY);
        assert_eq!(
            verify_payload(&provider, &wrong_merchant).unwrap_err(),
            PaymentWebhookVerificationError::InvalidPayload
        );
    }

    #[test]
    fn rejects_wrong_payment_method_and_treats_pending_as_non_terminal() {
        let provider = provider(EasyPayPaymentMethod::Alipay);
        let mut wrong_method = base_callback_parameters();
        wrong_method.insert("type".to_owned(), "wxpay".to_owned());
        let wrong_method = signed_payload(wrong_method, KEY);
        assert_eq!(
            verify_payload(&provider, &wrong_method).unwrap_err(),
            PaymentWebhookVerificationError::InvalidPayload
        );

        let mut pending = base_callback_parameters();
        pending.insert("trade_status".to_owned(), "WAIT_BUYER_PAY".to_owned());
        let pending = signed_payload(pending, KEY);
        assert_eq!(
            verify_payload(&provider, &pending).unwrap_err(),
            PaymentWebhookVerificationError::NonTerminalEvent
        );
    }

    #[test]
    fn one_provider_can_verify_every_enabled_payment_method() {
        let provider = provider_with_methods(vec![
            EasyPayPaymentMethod::Alipay,
            EasyPayPaymentMethod::WechatPay,
        ]);
        let mut parameters = base_callback_parameters();
        parameters.insert("type".to_owned(), "wxpay".to_owned());
        let payload = signed_payload(parameters, KEY);
        let event = verify_payload(&provider, &payload).unwrap();
        assert_eq!(event.payment_method(), "wxpay");
    }

    #[test]
    fn parses_cny_without_floating_point_and_rejects_extra_decimals() {
        assert_eq!(parse_cny_minor("12").unwrap(), 1200);
        assert_eq!(parse_cny_minor("12.3").unwrap(), 1230);
        assert_eq!(parse_cny_minor("12.34").unwrap(), 1234);
        for invalid in ["", ".12", "12.", "01.00", "12.345", "-1.00", "1e2"] {
            assert_eq!(
                parse_cny_minor(invalid).unwrap_err(),
                PaymentWebhookVerificationError::InvalidPayload
            );
        }
    }

    #[test]
    fn provider_debug_never_discloses_merchant_credentials_or_urls() {
        let provider = provider(EasyPayPaymentMethod::Alipay);
        let debug = format!("{provider:?}");
        for secret in [KEY, "1001", "pay.example.test", "notify.example.test"] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn callback_urls_distinguish_server_notify_from_spa_return() {
        assert!(validate_callback_url("https://api.example.test/payment/epay", false).is_ok());
        assert!(
            validate_callback_url(
                "https://api.example.test/payment/epay?source=browser",
                false
            )
            .is_err()
        );
        assert!(
            validate_callback_url(
                "https://app.example.test/#/console/wallet?epay_return=1",
                true
            )
            .is_ok()
        );
        assert!(
            validate_callback_url("https://api.example.test/payment/epay#unexpected", false)
                .is_err()
        );
    }

    fn provider(payment_method: EasyPayPaymentMethod) -> EasyPayPaymentProvider {
        provider_with_methods(vec![payment_method])
    }

    fn provider_with_methods(payment_methods: Vec<EasyPayPaymentMethod>) -> EasyPayPaymentProvider {
        EasyPayPaymentProvider::new(
            "https://pay.example.test/api".to_owned(),
            "1001".to_owned(),
            KEY,
            "https://notify.example.test/api/payment-webhooks/epay".to_owned(),
            "https://app.example.test/#/console/wallet?epay_return=1".to_owned(),
            payment_methods,
            "AnyFlows 余额充值".to_owned(),
        )
        .unwrap()
    }

    fn base_callback_parameters() -> BTreeMap<String, String> {
        BTreeMap::from([
            ("money".to_owned(), "12.34".to_owned()),
            ("out_trade_no".to_owned(), ORDER_ID.to_owned()),
            ("pid".to_owned(), "1001".to_owned()),
            ("trade_no".to_owned(), "trade-secret".to_owned()),
            ("trade_status".to_owned(), EASYPAY_SUCCESS_STATUS.to_owned()),
            ("type".to_owned(), "alipay".to_owned()),
        ])
    }

    fn signed_payload(mut parameters: BTreeMap<String, String>, key: &str) -> String {
        let signature = easy_pay_signature(&parameters, key).unwrap();
        parameters.insert("sign".to_owned(), signature);
        parameters.insert("sign_type".to_owned(), EASYPAY_SIGN_TYPE.to_owned());
        parameters
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn verify_payload(
        provider: &EasyPayPaymentProvider,
        payload: &str,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        let request = PaymentWebhookRequest::new(payload.as_bytes(), &[], 1_900_000_000).unwrap();
        provider.verify(&request)
    }
}
