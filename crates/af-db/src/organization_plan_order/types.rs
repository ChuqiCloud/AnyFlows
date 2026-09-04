use std::fmt;

use af_domain::{
    OrganizationId, OrganizationPlanId, OrganizationPlanOrderId, OrganizationPlanOrderRequestId,
    OrganizationPlanOrderStatus, OrganizationPlanPaymentEventId, OrganizationPlanPaymentEventType,
    UserId,
};
use thiserror::Error;

pub const MAX_ORGANIZATION_PLAN_ORDER_PROVIDER_BYTES: usize = 32;
pub const MAX_ORGANIZATION_PLAN_ORDER_CURRENCY_BYTES: usize = 3;
pub const MAX_ORGANIZATION_PLAN_ORDER_IDEMPOTENCY_KEY_BYTES: usize = 32;
pub const MAX_ORGANIZATION_PLAN_ORDER_PROVIDER_EVENT_ID_BYTES: usize = 128;
pub const MAX_ORGANIZATION_PLAN_ORDER_TRADE_NO_BYTES: usize = 128;
pub const MAX_ORGANIZATION_PLAN_ORDER_PAYMENT_METHOD_BYTES: usize = 32;
pub const MAX_ORGANIZATION_PLAN_ORDER_PAGE_SIZE: usize = 100;

/// 创建企业授权订单时固化的企业、付款人和价格事实。
pub struct OrganizationPlanOrderCreate {
    pub(super) order_id: OrganizationPlanOrderId,
    pub(super) request_id: OrganizationPlanOrderRequestId,
    pub(super) organization_id: OrganizationId,
    pub(super) payer_user_id: UserId,
    pub(super) plan_id: OrganizationPlanId,
    pub(super) plan_version: u64,
    pub(super) provider: String,
    pub(super) currency: String,
    pub(super) amount_minor: i64,
    pub(super) created_at: u64,
    pub(super) expires_at: Option<u64>,
}

impl OrganizationPlanOrderCreate {
    /// 校验企业订单的幂等键、价格快照和时间边界。
    #[allow(clippy::too_many_arguments, reason = "订单事实必须逐项固化")]
    pub fn new(
        order_id: OrganizationPlanOrderId,
        request_id: OrganizationPlanOrderRequestId,
        organization_id: OrganizationId,
        payer_user_id: UserId,
        plan_id: OrganizationPlanId,
        plan_version: u64,
        provider: String,
        currency: String,
        amount_minor: i64,
        created_at: u64,
        expires_at: Option<u64>,
    ) -> Result<Self, OrganizationPlanOrderRepositoryError> {
        if plan_version == 0
            || plan_version > i64::MAX as u64
            || !valid_token(&provider, MAX_ORGANIZATION_PLAN_ORDER_PROVIDER_BYTES)
            || !valid_currency(&currency)
            || amount_minor <= 0
            || created_at > i64::MAX as u64
            || expires_at.is_some_and(|value| value <= created_at || value > i64::MAX as u64)
        {
            return Err(OrganizationPlanOrderRepositoryError::InvalidInput);
        }
        Ok(Self {
            order_id,
            request_id,
            organization_id,
            payer_user_id,
            plan_id,
            plan_version,
            provider,
            currency,
            amount_minor,
            created_at,
            expires_at,
        })
    }
}

impl fmt::Debug for OrganizationPlanOrderCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanOrderCreate(<redacted>)")
    }
}

/// 已持久化的企业授权订单事实；敏感 Provider 引用不在 Debug 中输出。
pub struct OrganizationPlanOrderRecord {
    pub(super) database_id: i64,
    pub(super) order_id: OrganizationPlanOrderId,
    pub(super) request_id: OrganizationPlanOrderRequestId,
    pub(super) organization_id: OrganizationId,
    pub(super) payer_user_id: UserId,
    pub(super) plan_id: OrganizationPlanId,
    pub(super) plan_version: u64,
    pub(super) provider: String,
    pub(super) currency: String,
    pub(super) amount_minor: i64,
    pub(super) status: OrganizationPlanOrderStatus,
    pub(super) version: u64,
    pub(super) provider_order_id: Option<String>,
    pub(super) trade_no: Option<String>,
    pub(super) payment_method: Option<String>,
    pub(super) expires_at: Option<u64>,
    pub(super) paid_at: Option<u64>,
    pub(super) closed_at: Option<u64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl OrganizationPlanOrderRecord {
    #[must_use]
    pub(crate) const fn database_id(&self) -> i64 {
        self.database_id
    }
    #[must_use]
    pub const fn order_id(&self) -> OrganizationPlanOrderId {
        self.order_id
    }
    #[must_use]
    pub const fn request_id(&self) -> OrganizationPlanOrderRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn payer_user_id(&self) -> UserId {
        self.payer_user_id
    }
    #[must_use]
    pub const fn plan_id(&self) -> OrganizationPlanId {
        self.plan_id
    }
    #[must_use]
    pub const fn plan_version(&self) -> u64 {
        self.plan_version
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    #[must_use]
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationPlanOrderStatus {
        self.status
    }
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    #[must_use]
    pub fn provider_order_id(&self) -> Option<&str> {
        self.provider_order_id.as_deref()
    }
    #[must_use]
    pub fn trade_no(&self) -> Option<&str> {
        self.trade_no.as_deref()
    }
    #[must_use]
    pub fn payment_method(&self) -> Option<&str> {
        self.payment_method.as_deref()
    }
    #[must_use]
    pub const fn expires_at(&self) -> Option<u64> {
        self.expires_at
    }
    #[must_use]
    pub const fn paid_at(&self) -> Option<u64> {
        self.paid_at
    }
    #[must_use]
    pub const fn closed_at(&self) -> Option<u64> {
        self.closed_at
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_create(&self, create: &OrganizationPlanOrderCreate) -> bool {
        self.order_id == create.order_id
            && self.request_id == create.request_id
            && self.organization_id == create.organization_id
            && self.payer_user_id == create.payer_user_id
            && self.plan_id == create.plan_id
            && self.plan_version == create.plan_version
            && self.provider == create.provider
            && self.currency == create.currency
            && self.amount_minor == create.amount_minor
            && self.created_at == create.created_at
            && self.expires_at == create.expires_at
    }
}

impl fmt::Debug for OrganizationPlanOrderRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanOrderRecord(<redacted>)")
    }
}

/// 订单创建幂等结果；不存在的企业范围统一返回 NotFound，避免跨租户探测。
pub enum OrganizationPlanOrderCreateOutcome {
    Created(OrganizationPlanOrderRecord),
    Existing(OrganizationPlanOrderRecord),
    NotFound,
}

/// 已验签支付事件的追加结果；本阶段不推进订单状态或企业授权。
pub enum OrganizationPlanPaymentEventOutcome {
    Recorded(OrganizationPlanPaymentEventRecord),
    Existing(OrganizationPlanPaymentEventRecord),
    NotFound,
}

impl fmt::Debug for OrganizationPlanOrderCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("OrganizationPlanOrderCreateOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("OrganizationPlanOrderCreateOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("OrganizationPlanOrderCreateOutcome::NotFound"),
        }
    }
}

impl fmt::Debug for OrganizationPlanPaymentEventOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recorded(_) => {
                formatter.write_str("OrganizationPlanPaymentEventOutcome::Recorded(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("OrganizationPlanPaymentEventOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("OrganizationPlanPaymentEventOutcome::NotFound"),
        }
    }
}

/// 已验签支付事件事实；原始 payload 只以哈希形式保存。
pub struct OrganizationPlanPaymentEventWrite {
    pub(super) event_id: OrganizationPlanPaymentEventId,
    pub(super) organization_id: OrganizationId,
    pub(super) order_id: OrganizationPlanOrderId,
    pub(super) provider: String,
    pub(super) provider_event_id: String,
    pub(super) trade_no: Option<String>,
    pub(super) amount_minor: Option<i64>,
    pub(super) currency: Option<String>,
    pub(super) payment_method: Option<String>,
    pub(super) event_type: OrganizationPlanPaymentEventType,
    pub(super) signature_key_fingerprint: [u8; 32],
    pub(super) payload_sha256: [u8; 32],
    pub(super) received_at: u64,
}

impl OrganizationPlanPaymentEventWrite {
    /// 校验事件边界；支付结果与订单事实的核对留给后续原子确认切片。
    #[allow(clippy::too_many_arguments, reason = "支付审计字段需要逐项固化")]
    pub fn new(
        event_id: OrganizationPlanPaymentEventId,
        organization_id: OrganizationId,
        order_id: OrganizationPlanOrderId,
        provider: String,
        provider_event_id: String,
        trade_no: Option<String>,
        amount_minor: Option<u64>,
        currency: Option<String>,
        payment_method: Option<String>,
        event_type: OrganizationPlanPaymentEventType,
        signature_key_fingerprint: [u8; 32],
        payload_sha256: [u8; 32],
        received_at: u64,
    ) -> Result<Self, OrganizationPlanOrderRepositoryError> {
        let amount_minor = amount_minor
            .map(|value| {
                i64::try_from(value).map_err(|_| OrganizationPlanOrderRepositoryError::InvalidInput)
            })
            .transpose()?;
        if !valid_token(&provider, MAX_ORGANIZATION_PLAN_ORDER_PROVIDER_BYTES)
            || !valid_text(
                &provider_event_id,
                MAX_ORGANIZATION_PLAN_ORDER_PROVIDER_EVENT_ID_BYTES,
            )
            || trade_no
                .as_deref()
                .is_some_and(|value| !valid_text(value, MAX_ORGANIZATION_PLAN_ORDER_TRADE_NO_BYTES))
            || (event_type == OrganizationPlanPaymentEventType::Succeeded
                && (trade_no.is_none()
                    || amount_minor.is_none_or(|value| value <= 0)
                    || currency.is_none()))
            || amount_minor.is_some_and(|value| value <= 0)
            || currency
                .as_deref()
                .is_some_and(|value| !valid_currency(value))
            || payment_method.as_deref().is_some_and(|value| {
                !valid_text(value, MAX_ORGANIZATION_PLAN_ORDER_PAYMENT_METHOD_BYTES)
            })
            || received_at > i64::MAX as u64
        {
            return Err(OrganizationPlanOrderRepositoryError::InvalidInput);
        }
        Ok(Self {
            event_id,
            organization_id,
            order_id,
            provider,
            provider_event_id,
            trade_no,
            amount_minor,
            currency,
            payment_method,
            event_type,
            signature_key_fingerprint,
            payload_sha256,
            received_at,
        })
    }
}

impl fmt::Debug for OrganizationPlanPaymentEventWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanPaymentEventWrite(<redacted>)")
    }
}

/// 已持久化支付事件的脱敏快照。
pub struct OrganizationPlanPaymentEventRecord {
    pub(super) database_id: i64,
    pub(super) event_id: OrganizationPlanPaymentEventId,
    pub(super) order_id: OrganizationPlanOrderId,
    pub(super) provider: String,
    pub(super) provider_event_id: String,
    pub(super) trade_no: Option<String>,
    pub(super) amount_minor: Option<i64>,
    pub(super) currency: Option<String>,
    pub(super) payment_method: Option<String>,
    pub(super) event_type: OrganizationPlanPaymentEventType,
    pub(super) received_at: u64,
    pub(super) processed_at: Option<u64>,
}

impl OrganizationPlanPaymentEventRecord {
    #[must_use]
    pub(crate) const fn database_id(&self) -> i64 {
        self.database_id
    }
    #[must_use]
    pub const fn event_id(&self) -> OrganizationPlanPaymentEventId {
        self.event_id
    }
    #[must_use]
    pub const fn order_id(&self) -> OrganizationPlanOrderId {
        self.order_id
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn provider_event_id(&self) -> &str {
        &self.provider_event_id
    }
    #[must_use]
    pub fn trade_no(&self) -> Option<&str> {
        self.trade_no.as_deref()
    }
    #[must_use]
    pub const fn amount_minor(&self) -> Option<i64> {
        self.amount_minor
    }
    #[must_use]
    pub fn currency(&self) -> Option<&str> {
        self.currency.as_deref()
    }
    #[must_use]
    pub fn payment_method(&self) -> Option<&str> {
        self.payment_method.as_deref()
    }
    #[must_use]
    pub const fn event_type(&self) -> OrganizationPlanPaymentEventType {
        self.event_type
    }
    #[must_use]
    pub const fn received_at(&self) -> u64 {
        self.received_at
    }
    #[must_use]
    pub const fn processed_at(&self) -> Option<u64> {
        self.processed_at
    }
}

impl fmt::Debug for OrganizationPlanPaymentEventRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanPaymentEventRecord(<redacted>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanOrderRepositoryError {
    #[error("企业授权订单输入无效")]
    InvalidInput,
    #[error("企业授权订单事实冲突")]
    Conflict,
    #[error("企业授权订单数据库操作失败")]
    Query,
    #[error("企业授权订单操作结果未知")]
    OutcomeUnknown,
    #[error("企业授权订单查询超时")]
    Timeout,
    #[error("企业授权订单持久化状态损坏")]
    Invariant,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanOrderRepositoryConfigError {
    #[error("企业授权订单仓储超时必须大于零")]
    ZeroOperationTimeout,
}

fn valid_token(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_currency(value: &str) -> bool {
    value.len() == MAX_ORGANIZATION_PLAN_ORDER_CURRENCY_BYTES
        && value.bytes().all(|byte| byte.is_ascii_uppercase())
}
