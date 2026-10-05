use std::fmt;

use af_domain::{
    OrganizationId, Quota, TopupOrderId, TopupOrderStatus, TopupPaymentEventId,
    TopupPaymentEventType, TopupRequestId, UserId,
};
use thiserror::Error;

/// 支付 Provider 规范标识允许的最大 UTF-8 字节数。
pub const MAX_PAYMENT_PROVIDER_BYTES: usize = 64;
/// 支付方式规范标识允许的最大 UTF-8 字节数。
pub const MAX_PAYMENT_METHOD_BYTES: usize = 32;
/// Provider 订单标识允许的最大 UTF-8 字节数。
pub const MAX_PROVIDER_ORDER_ID_BYTES: usize = 128;
/// Provider webhook 事件标识允许的最大 UTF-8 字节数。
pub const MAX_PROVIDER_EVENT_ID_BYTES: usize = 128;
/// Provider 支付流水号允许的最大 UTF-8 字节数。
pub const MAX_PROVIDER_TRADE_NO_BYTES: usize = 128;

const WALLET_OPENING_NAMESPACE: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];

/// 创建充值订单的不可变业务事实。
pub struct TopupOrderCreate {
    pub(super) order_id: TopupOrderId,
    pub(super) request_id: TopupRequestId,
    pub(super) user_id: UserId,
    pub(super) organization_id: Option<OrganizationId>,
    pub(super) provider: String,
    pub(super) payment_method: String,
    pub(super) amount_minor: i64,
    pub(super) currency: String,
    pub(super) quota_amount: Quota,
    pub(super) created_at: u64,
}

impl TopupOrderCreate {
    /// 校验 Provider、金额、币种、到账额度与时间边界。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与充值订单不可变事实一一对应"
    )]
    pub fn new(
        order_id: TopupOrderId,
        request_id: TopupRequestId,
        user_id: UserId,
        provider: String,
        payment_method: String,
        amount_minor: u64,
        currency: String,
        quota_amount: Quota,
        created_at: u64,
    ) -> Result<Self, TopupInputError> {
        if !valid_provider(&provider) {
            return Err(TopupInputError::InvalidProvider);
        }
        if !valid_payment_method(&payment_method) {
            return Err(TopupInputError::InvalidPaymentMethod);
        }
        let amount_minor =
            i64::try_from(amount_minor).map_err(|_| TopupInputError::InvalidAmount)?;
        if amount_minor == 0 {
            return Err(TopupInputError::InvalidAmount);
        }
        if !valid_currency(&currency) {
            return Err(TopupInputError::InvalidCurrency);
        }
        if quota_amount.is_zero() {
            return Err(TopupInputError::InvalidQuota);
        }
        validate_time(created_at)?;
        Ok(Self {
            order_id,
            request_id,
            user_id,
            organization_id: None,
            provider,
            payment_method,
            amount_minor,
            currency,
            quota_amount,
            created_at,
        })
    }

    /// 创建企业收款主体订单；付款人仍由会话主体提供，到账只写企业钱包。
    #[allow(clippy::too_many_arguments, reason = "企业充值订单事实字段一一对应")]
    pub fn new_for_organization(
        order_id: TopupOrderId,
        request_id: TopupRequestId,
        payer_user_id: UserId,
        organization_id: OrganizationId,
        provider: String,
        payment_method: String,
        amount_minor: u64,
        currency: String,
        quota_amount: Quota,
        created_at: u64,
    ) -> Result<Self, TopupInputError> {
        let mut write = Self::new(
            order_id,
            request_id,
            payer_user_id,
            provider,
            payment_method,
            amount_minor,
            currency,
            quota_amount,
            created_at,
        )?;
        write.organization_id = Some(organization_id);
        Ok(write)
    }
}

impl fmt::Debug for TopupOrderCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TopupOrderCreate(<redacted>)")
    }
}

/// 把本地订单绑定到 Provider 订单的 CAS 命令。
pub struct TopupOrderSubmission {
    pub(super) order_id: TopupOrderId,
    pub(super) expected_version: i64,
    pub(super) provider_order_id: String,
    pub(super) submitted_at: u64,
    pub(super) expires_at: u64,
}

impl TopupOrderSubmission {
    /// 校验正版本、Provider 订单标识和严格递增的过期时间。
    pub fn new(
        order_id: TopupOrderId,
        expected_version: u64,
        provider_order_id: String,
        submitted_at: u64,
        expires_at: u64,
    ) -> Result<Self, TopupInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| TopupInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(TopupInputError::InvalidVersion);
        }
        if !valid_provider_value(&provider_order_id, MAX_PROVIDER_ORDER_ID_BYTES) {
            return Err(TopupInputError::InvalidProviderOrderId);
        }
        validate_time(submitted_at)?;
        validate_time(expires_at)?;
        if expires_at <= submitted_at {
            return Err(TopupInputError::InvalidTiming);
        }
        Ok(Self {
            order_id,
            expected_version,
            provider_order_id,
            submitted_at,
            expires_at,
        })
    }
}

impl fmt::Debug for TopupOrderSubmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TopupOrderSubmission(<redacted>)")
    }
}

/// 已完成 Provider 验签和 payload 摘要计算的支付事件写入事实。
pub struct TopupPaymentEventWrite {
    pub(super) event_id: TopupPaymentEventId,
    pub(super) order_id: TopupOrderId,
    pub(super) provider: String,
    pub(super) provider_event_id: String,
    pub(super) trade_no: Option<String>,
    pub(super) amount_minor: i64,
    pub(super) currency: String,
    pub(super) payment_method: String,
    pub(super) event_type: TopupPaymentEventType,
    pub(super) signature_key_fingerprint: [u8; 32],
    pub(super) payload_sha256: [u8; 32],
    pub(super) received_at: u64,
}

impl TopupPaymentEventWrite {
    /// 校验已验证事件的规范标识、时间和成功流水号边界。
    #[allow(clippy::too_many_arguments, reason = "字段与支付事件审计事实一一对应")]
    pub fn new(
        event_id: TopupPaymentEventId,
        order_id: TopupOrderId,
        provider: String,
        provider_event_id: String,
        trade_no: Option<String>,
        amount_minor: u64,
        currency: String,
        payment_method: String,
        event_type: TopupPaymentEventType,
        signature_key_fingerprint: [u8; 32],
        payload_sha256: [u8; 32],
        received_at: u64,
    ) -> Result<Self, TopupInputError> {
        if event_id.bytes()[..WALLET_OPENING_NAMESPACE.len()] == WALLET_OPENING_NAMESPACE {
            return Err(TopupInputError::ReservedEventId);
        }
        if !valid_provider(&provider) {
            return Err(TopupInputError::InvalidProvider);
        }
        if !valid_provider_value(&provider_event_id, MAX_PROVIDER_EVENT_ID_BYTES) {
            return Err(TopupInputError::InvalidProviderEventId);
        }
        if trade_no
            .as_deref()
            .is_some_and(|value| !valid_provider_value(value, MAX_PROVIDER_TRADE_NO_BYTES))
            || (event_type == TopupPaymentEventType::Succeeded && trade_no.is_none())
        {
            return Err(TopupInputError::InvalidTradeNo);
        }
        let amount_minor =
            i64::try_from(amount_minor).map_err(|_| TopupInputError::InvalidAmount)?;
        if amount_minor == 0 {
            return Err(TopupInputError::InvalidAmount);
        }
        if !valid_currency(&currency) {
            return Err(TopupInputError::InvalidCurrency);
        }
        if !valid_payment_method(&payment_method) {
            return Err(TopupInputError::InvalidPaymentMethod);
        }
        validate_time(received_at)?;
        Ok(Self {
            event_id,
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

impl fmt::Debug for TopupPaymentEventWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TopupPaymentEventWrite(<redacted>)")
    }
}

/// 已持久化充值订单的非敏感状态快照。
pub struct TopupOrderRecord {
    pub(super) database_id: i64,
    pub(super) order_id: TopupOrderId,
    pub(super) request_id: TopupRequestId,
    pub(super) user_id: UserId,
    pub(super) organization_id: Option<OrganizationId>,
    pub(super) provider: String,
    pub(super) payment_method: Option<String>,
    pub(super) provider_order_id: Option<String>,
    pub(super) trade_no: Option<String>,
    pub(super) status: TopupOrderStatus,
    pub(super) amount_minor: u64,
    pub(super) currency: String,
    pub(super) quota_amount: Quota,
    pub(super) version: u64,
    pub(super) expires_at: Option<u64>,
    pub(super) paid_at: Option<u64>,
    pub(super) closed_at: Option<u64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl TopupOrderRecord {
    /// 返回本地订单标识。
    #[must_use]
    pub const fn order_id(&self) -> TopupOrderId {
        self.order_id
    }

    /// 返回订单所属用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回收款主体企业；为空表示个人钱包订单。
    #[must_use]
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }

    /// 返回规范 Provider 标识。
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// 返回订单绑定的支付方式；旧迁移订单可能尚无该事实。
    #[must_use]
    pub fn payment_method(&self) -> Option<&str> {
        self.payment_method.as_deref()
    }

    /// 返回当前闭合订单状态。
    #[must_use]
    pub const fn status(&self) -> TopupOrderStatus {
        self.status
    }

    /// 返回当前 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回支付金额的整数最小单位。
    #[must_use]
    pub const fn amount_minor(&self) -> u64 {
        self.amount_minor
    }

    /// 返回三位大写币种代码。
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// 返回支付成功后应增加的钱包额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回 Provider 订单标识；调用方不得写入日志。
    #[must_use]
    pub fn provider_order_id(&self) -> Option<&str> {
        self.provider_order_id.as_deref()
    }

    /// 返回 Provider 支付流水号；调用方不得写入日志。
    #[must_use]
    pub fn trade_no(&self) -> Option<&str> {
        self.trade_no.as_deref()
    }

    /// 返回订单过期时间的 Unix 秒数。
    #[must_use]
    pub const fn expires_at(&self) -> Option<u64> {
        self.expires_at
    }

    /// 返回支付完成时间的 Unix 秒数。
    #[must_use]
    pub const fn paid_at(&self) -> Option<u64> {
        self.paid_at
    }

    /// 返回未支付终态关闭时间的 Unix 秒数。
    #[must_use]
    pub const fn closed_at(&self) -> Option<u64> {
        self.closed_at
    }

    /// 返回订单创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回最近一次状态迁移时间的 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_create(&self, write: &TopupOrderCreate) -> bool {
        self.order_id == write.order_id
            && self.request_id == write.request_id
            && self.user_id == write.user_id
            && self.organization_id == write.organization_id
            && self.provider == write.provider
            && self.payment_method.as_deref() == Some(write.payment_method.as_str())
            && self.amount_minor == write.amount_minor as u64
            && self.currency == write.currency
            && self.quota_amount == write.quota_amount
        // 创建时间是服务端首次接收事实，不参与跨请求幂等比较；重试仍保留数据库原值。
    }

    pub(super) fn matches_submission(&self, write: &TopupOrderSubmission) -> bool {
        self.provider_order_id.as_deref() == Some(write.provider_order_id.as_str())
            && self.expires_at == Some(write.expires_at)
    }
}

impl fmt::Debug for TopupOrderRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TopupOrderRecord(<redacted>)")
    }
}

/// 幂等创建订单后的闭合结果。
pub enum TopupOrderCreateOutcome {
    /// 本次调用创建了新订单。
    Created(TopupOrderRecord),
    /// 相同订单键、请求键和不可变事实已经存在。
    Existing(TopupOrderRecord),
    /// 目标用户不存在或已经软删除。
    NotFound,
}

/// Provider 订单绑定后的闭合结果。
pub enum TopupOrderSubmitOutcome {
    /// 本次调用把 Created 订单推进为 Pending。
    Applied(TopupOrderRecord),
    /// 相同 Provider 订单和过期时间已经绑定。
    Existing(TopupOrderRecord),
    /// 本地订单不存在。
    NotFound,
}

/// 已验证事件无法应用到当前订单的可审计原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopupPaymentEventRejection {
    /// 事件来自与订单不一致的 Provider。
    ProviderMismatch,
    /// 回调金额与订单金额不一致。
    AmountMismatch,
    /// 回调币种与订单币种不一致。
    CurrencyMismatch,
    /// 回调支付方式与订单支付方式不一致。
    PaymentMethodMismatch,
    /// 事件接收时间早于订单创建时间。
    TimingConflict,
    /// Provider 流水号与订单已经绑定的流水号冲突。
    TradeNumberConflict,
    /// 当前终态与事件目标终态冲突。
    TerminalConflict,
    /// 到账会超过钱包额度整数上界。
    CreditOverflow,
}

/// 已验证支付事件的持久化和处理结果。
pub enum TopupPaymentEventOutcome {
    /// 新事件已审计，订单首次进入对应终态。
    Applied(TopupOrderRecord),
    /// 新事件已审计，订单已处于相同终态且未重复加额。
    Acknowledged(TopupOrderRecord),
    /// 相同 Provider 事件事实已经存在。
    Existing(TopupOrderRecord),
    /// 新事件已审计但未处理，等待人工或后续恢复。
    RecordedUnprocessed {
        /// 事件关联的当前订单快照。
        order: TopupOrderRecord,
        /// 未处理的闭合原因。
        reason: TopupPaymentEventRejection,
    },
    /// 本地订单不存在。
    NotFound,
}

/// 充值输入构造错误；不携带外部标识或支付内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TopupInputError {
    /// Provider 标识不符合规范。
    #[error("支付 Provider 标识无效")]
    InvalidProvider,
    /// 支付方式标识不符合规范。
    #[error("支付方式标识无效")]
    InvalidPaymentMethod,
    /// Provider 订单标识不符合边界。
    #[error("Provider 订单标识无效")]
    InvalidProviderOrderId,
    /// Provider 事件标识不符合边界。
    #[error("Provider 事件标识无效")]
    InvalidProviderEventId,
    /// Provider 流水号缺失或不符合边界。
    #[error("Provider 支付流水号无效")]
    InvalidTradeNo,
    /// 金额不是可表示的正整数最小单位。
    #[error("充值金额无效")]
    InvalidAmount,
    /// 币种不是三位大写 ASCII。
    #[error("充值币种无效")]
    InvalidCurrency,
    /// 到账额度必须为正数。
    #[error("充值到账额度无效")]
    InvalidQuota,
    /// 时间戳或时间先后关系无效。
    #[error("充值时间边界无效")]
    InvalidTiming,
    /// CAS 版本不是可递增的正整数。
    #[error("充值订单版本无效")]
    InvalidVersion,
    /// 支付事件占用了钱包 opening 保留命名空间。
    #[error("充值支付事件标识命名空间无效")]
    ReservedEventId,
}

/// 充值仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TopupRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("充值仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 充值仓储错误；不携带订单、Provider 或支付流水内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TopupRepositoryError {
    /// The selected wallet target is supplied only by a distribution extension.
    #[error("充值收款主体未启用")]
    UnsupportedTarget,
    /// 相同幂等键绑定了不同事实，或 CAS 状态不允许迁移。
    #[error("充值订单或支付事件冲突")]
    Conflict,
    /// 获取连接或执行确定未提交的数据库操作失败。
    #[error("充值数据库操作失败")]
    Query,
    /// 写入超时或提交失败，调用方必须复用 Provider 事件查询或重试。
    #[error("充值操作结果未知")]
    OutcomeUnknown,
    /// 只读或确定未提交操作超过硬截止时间。
    #[error("充值数据库操作超时")]
    Timeout,
    /// 持久化订单、事件、钱包或版本违反不变量。
    #[error("充值持久化状态损坏")]
    Invariant,
}

impl fmt::Debug for TopupOrderCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => formatter.write_str("TopupOrderCreateOutcome::Created(<redacted>)"),
            Self::Existing(_) => {
                formatter.write_str("TopupOrderCreateOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("TopupOrderCreateOutcome::NotFound"),
        }
    }
}

impl fmt::Debug for TopupOrderSubmitOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => formatter.write_str("TopupOrderSubmitOutcome::Applied(<redacted>)"),
            Self::Existing(_) => {
                formatter.write_str("TopupOrderSubmitOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("TopupOrderSubmitOutcome::NotFound"),
        }
    }
}

impl fmt::Debug for TopupPaymentEventOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => {
                formatter.write_str("TopupPaymentEventOutcome::Applied(<redacted>)")
            }
            Self::Acknowledged(_) => {
                formatter.write_str("TopupPaymentEventOutcome::Acknowledged(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("TopupPaymentEventOutcome::Existing(<redacted>)")
            }
            Self::RecordedUnprocessed { reason, .. } => formatter
                .debug_struct("TopupPaymentEventOutcome::RecordedUnprocessed")
                .field("reason", reason)
                .finish_non_exhaustive(),
            Self::NotFound => formatter.write_str("TopupPaymentEventOutcome::NotFound"),
        }
    }
}

pub(super) fn valid_provider(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PAYMENT_PROVIDER_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

pub(super) fn valid_payment_method(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PAYMENT_METHOD_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

pub(super) fn valid_provider_value(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

pub(super) fn validate_time(value: u64) -> Result<(), TopupInputError> {
    if value > i64::MAX as u64 {
        return Err(TopupInputError::InvalidTiming);
    }
    Ok(())
}
