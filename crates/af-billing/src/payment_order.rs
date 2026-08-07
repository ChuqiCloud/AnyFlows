use std::{fmt, future::Future, pin::Pin};

use af_db::MAX_PROVIDER_ORDER_ID_BYTES;
use af_domain::TopupOrderId;
use thiserror::Error;
use zeroize::Zeroize;

/// 支付客户端密钥允许的最大 UTF-8 字节数。
pub const MAX_PAYMENT_CLIENT_SECRET_BYTES: usize = 4 * 1024;

/// 创建 Provider 支付订单所需的不可变本地事实。
pub struct PaymentOrderRequest {
    order_id: TopupOrderId,
    amount_minor: u64,
    currency: String,
}

impl PaymentOrderRequest {
    /// 校验正整数最小单位金额和三位大写币种。
    pub fn new(
        order_id: TopupOrderId,
        amount_minor: u64,
        currency: String,
    ) -> Result<Self, PaymentOrderInputError> {
        if amount_minor == 0 || amount_minor > i64::MAX as u64 {
            return Err(PaymentOrderInputError::InvalidAmount);
        }
        if currency.len() != 3 || !currency.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(PaymentOrderInputError::InvalidCurrency);
        }
        Ok(Self {
            order_id,
            amount_minor,
            currency,
        })
    }

    /// 返回不可变本地订单标识。
    #[must_use]
    pub const fn order_id(&self) -> TopupOrderId {
        self.order_id
    }

    /// 返回待收取的整数最小单位金额。
    #[must_use]
    pub const fn amount_minor(&self) -> u64 {
        self.amount_minor
    }

    /// 返回本地规范化的三位大写币种。
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
}

impl fmt::Debug for PaymentOrderRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentOrderRequest(<已脱敏>)")
    }
}

/// 恢复已绑定 Provider 支付订单所需的完整事实。
pub struct PaymentOrderRecoveryRequest {
    order: PaymentOrderRequest,
    provider_order_id: String,
}

impl PaymentOrderRecoveryRequest {
    /// 组合本地订单事实与已持久化的 Provider 订单标识。
    pub fn new(
        order: PaymentOrderRequest,
        provider_order_id: String,
    ) -> Result<Self, PaymentOrderInputError> {
        if !valid_provider_value(&provider_order_id, MAX_PROVIDER_ORDER_ID_BYTES) {
            return Err(PaymentOrderInputError::InvalidProviderOrderId);
        }
        Ok(Self {
            order,
            provider_order_id,
        })
    }

    /// 返回必须与 Provider 响应重新核对的本地订单事实。
    #[must_use]
    pub const fn order(&self) -> &PaymentOrderRequest {
        &self.order
    }

    /// 返回已绑定 Provider 订单标识；调用方不得写入日志。
    #[must_use]
    pub fn provider_order_id(&self) -> &str {
        &self.provider_order_id
    }
}

impl fmt::Debug for PaymentOrderRecoveryRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentOrderRecoveryRequest(<已脱敏>)")
    }
}

/// 只在内存和公开支付响应中短暂存在的客户端密钥。
pub struct PaymentClientSecret(String);

impl PaymentClientSecret {
    /// 校验客户端密钥的有界可打印 ASCII 形式。
    pub fn new(value: String) -> Result<Self, PaymentOrderInputError> {
        if value.is_empty()
            || value.len() > MAX_PAYMENT_CLIENT_SECRET_BYTES
            || !value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
        {
            return Err(PaymentOrderInputError::InvalidClientSecret);
        }
        Ok(Self(value))
    }

    /// 返回仅供 HTTPS 响应序列化使用的客户端密钥。
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for PaymentClientSecret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for PaymentClientSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentClientSecret(<已脱敏>)")
    }
}

/// Provider 创建或恢复后返回的可继续支付会话。
pub struct PaymentOrderSession {
    provider_order_id: String,
    client_secret: PaymentClientSecret,
}

impl PaymentOrderSession {
    /// 校验 Provider 订单标识并绑定短生命周期客户端密钥。
    pub fn new(
        provider_order_id: String,
        client_secret: PaymentClientSecret,
    ) -> Result<Self, PaymentOrderInputError> {
        if !valid_provider_value(&provider_order_id, MAX_PROVIDER_ORDER_ID_BYTES) {
            return Err(PaymentOrderInputError::InvalidProviderOrderId);
        }
        Ok(Self {
            provider_order_id,
            client_secret,
        })
    }

    /// 返回 Provider 订单标识；调用方不得写入日志。
    #[must_use]
    pub fn provider_order_id(&self) -> &str {
        &self.provider_order_id
    }

    /// 返回仅供公开支付响应使用的客户端密钥。
    #[must_use]
    pub const fn client_secret(&self) -> &PaymentClientSecret {
        &self.client_secret
    }
}

impl fmt::Debug for PaymentOrderSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentOrderSession(<已脱敏>)")
    }
}

/// 支付订单边界输入错误；不携带任何外部标识或密钥。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentOrderInputError {
    /// 金额不是可表示的正整数最小单位。
    #[error("支付订单金额无效")]
    InvalidAmount,
    /// 币种不是三位大写 ASCII。
    #[error("支付订单币种无效")]
    InvalidCurrency,
    /// Provider 订单标识不满足有界文本约束。
    #[error("支付 Provider 订单标识无效")]
    InvalidProviderOrderId,
    /// 客户端密钥不满足有界可打印文本约束。
    #[error("支付客户端密钥无效")]
    InvalidClientSecret,
}

/// Provider 支付订单操作的稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentOrderProviderError {
    /// Provider 确定拒绝了服务端构造的支付请求。
    #[error("支付 Provider 拒绝请求")]
    Rejected,
    /// 创建请求可能已被 Provider 接收，只能使用相同本地订单重试。
    #[error("支付 Provider 创建结果未知")]
    OutcomeUnknown,
    /// Provider 或受控网络路径当前不可用。
    #[error("支付 Provider 暂不可用")]
    Unavailable,
    /// Provider 响应与本地不可变事实不一致或无法安全解析。
    #[error("支付 Provider 响应无效")]
    InvalidResponse,
}

/// 单次 Provider 支付订单调用的对象安全 Future。
pub type PaymentOrderFuture<'a> = Pin<
    Box<dyn Future<Output = Result<PaymentOrderSession, PaymentOrderProviderError>> + Send + 'a>,
>;

/// Provider 支付订单创建与恢复端口。
pub trait PaymentOrderProvider: Send + Sync + 'static {
    /// 返回与本地充值订单 Provider 字段一致的稳定标识。
    fn provider(&self) -> &str;

    /// 使用 Provider 幂等键创建或重放同一支付订单。
    fn create<'a>(&'a self, request: PaymentOrderRequest) -> PaymentOrderFuture<'a>;

    /// 按已绑定 Provider 订单标识恢复支付会话并复核完整事实。
    fn recover<'a>(&'a self, request: PaymentOrderRecoveryRequest) -> PaymentOrderFuture<'a>;
}

fn valid_provider_value(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_and_session_boundaries_fail_closed_and_redact_secrets() {
        let order_id = TopupOrderId::new([0x42; 16]).unwrap();
        assert_eq!(
            PaymentOrderRequest::new(order_id, 0, "USD".to_owned()).unwrap_err(),
            PaymentOrderInputError::InvalidAmount
        );
        assert_eq!(
            PaymentOrderRequest::new(order_id, 50, "usd".to_owned()).unwrap_err(),
            PaymentOrderInputError::InvalidCurrency
        );

        let secret = PaymentClientSecret::new("pi_test_secret_value".to_owned()).unwrap();
        assert!(!format!("{secret:?}").contains("secret_value"));
        let session = PaymentOrderSession::new("pi_test".to_owned(), secret).unwrap();
        assert!(!format!("{session:?}").contains("pi_test"));
    }
}
