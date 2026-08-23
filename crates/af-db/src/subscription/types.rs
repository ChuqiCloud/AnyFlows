use std::fmt;

use af_domain::{
    Quota, SubscriptionCycle, SubscriptionOrderId, SubscriptionOrderRequestId,
    SubscriptionOrderStatus, SubscriptionPaymentEventId, SubscriptionPaymentEventType,
    SubscriptionPlanId, SubscriptionPlanStatus, SubscriptionWindow, UserId, UserSubscriptionId,
    UserSubscriptionStatus,
};
use thiserror::Error;

/// 订阅计划名称允许的最大 UTF-8 字节数。
pub const MAX_SUBSCRIPTION_PLAN_NAME_BYTES: usize = 80;
pub const MAX_SUBSCRIPTION_PROVIDER_BYTES: usize = 32;
pub const MAX_SUBSCRIPTION_CURRENCY_BYTES: usize = 3;
pub const MAX_SUBSCRIPTION_IDEMPOTENCY_KEY_BYTES: usize = 32;
pub const MAX_SUBSCRIPTION_PROVIDER_EVENT_ID_BYTES: usize = 128;
pub const MAX_SUBSCRIPTION_TRADE_NO_BYTES: usize = 128;
pub const MAX_SUBSCRIPTION_PAYMENT_METHOD_BYTES: usize = 32;

/// 订阅目录与用户订阅列表允许的最大页大小。
pub const MAX_SUBSCRIPTION_PAGE_SIZE: usize = 100;

/// 创建订阅计划时固化的不可变业务事实。
pub struct SubscriptionPlanWrite {
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) name: String,
    pub(super) created_by_user_id: UserId,
    pub(super) quota_amount: Quota,
    pub(super) cycle: SubscriptionCycle,
    pub(super) price_provider: String,
    pub(super) price_currency: String,
    pub(super) price_amount_minor: i64,
    pub(super) created_at: u64,
}

impl SubscriptionPlanWrite {
    /// 校验计划名称、正整数额度和审计时间边界。
    pub fn new(
        plan_id: SubscriptionPlanId,
        name: String,
        created_by_user_id: UserId,
        quota_amount: Quota,
        cycle: SubscriptionCycle,
        created_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        Self::new_with_price(
            plan_id,
            name,
            created_by_user_id,
            quota_amount,
            cycle,
            "stripe".to_owned(),
            "USD".to_owned(),
            100,
            created_at,
        )
    }

    /// 构造带有明确支付 Provider、币种和最小货币单位金额的计划事实。
    #[allow(
        clippy::too_many_arguments,
        reason = "价格快照字段与订阅计划写入契约一一对应"
    )]
    pub fn new_with_price(
        plan_id: SubscriptionPlanId,
        name: String,
        created_by_user_id: UserId,
        quota_amount: Quota,
        cycle: SubscriptionCycle,
        price_provider: String,
        price_currency: String,
        price_amount_minor: i64,
        created_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        if !valid_name(&name) {
            return Err(SubscriptionInputError::InvalidName);
        }
        if quota_amount.is_zero() {
            return Err(SubscriptionInputError::InvalidQuota);
        }
        if !valid_price_text(&price_provider, MAX_SUBSCRIPTION_PROVIDER_BYTES)
            || !valid_currency(&price_currency)
            || price_amount_minor <= 0
        {
            return Err(SubscriptionInputError::InvalidPrice);
        }
        validate_time(created_at)?;
        Ok(Self {
            plan_id,
            name,
            created_by_user_id,
            quota_amount,
            cycle,
            price_provider,
            price_currency,
            price_amount_minor,
            created_at,
        })
    }

    #[must_use]
    pub fn price_provider(&self) -> &str {
        &self.price_provider
    }

    #[must_use]
    pub fn price_currency(&self) -> &str {
        &self.price_currency
    }

    #[must_use]
    pub const fn price_amount_minor(&self) -> i64 {
        self.price_amount_minor
    }
}

impl fmt::Debug for SubscriptionPlanWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPlanWrite(<redacted>)")
    }
}

/// 通过 CAS 停止订阅计划新增绑定的命令。
pub struct SubscriptionPlanDisable {
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) expected_version: i64,
    pub(super) disabled_at: u64,
}

impl SubscriptionPlanDisable {
    /// 校验可递增正版本和受信服务端停用时间。
    pub fn new(
        plan_id: SubscriptionPlanId,
        expected_version: u64,
        disabled_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| SubscriptionInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(SubscriptionInputError::InvalidVersion);
        }
        validate_time(disabled_at)?;
        Ok(Self {
            plan_id,
            expected_version,
            disabled_at,
        })
    }
}

impl fmt::Debug for SubscriptionPlanDisable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPlanDisable(<redacted>)")
    }
}

/// 将用户、计划和首个额度周期绑定为一项不可变幂等事实。
pub struct UserSubscriptionBind {
    pub(super) subscription_id: UserSubscriptionId,
    pub(super) user_id: UserId,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) window_started_at: u64,
    pub(super) window_ends_at: u64,
    pub(super) bound_at: u64,
}

impl UserSubscriptionBind {
    /// 校验时间窗非空，并确保绑定发生时窗口尚未结束。
    pub fn new(
        subscription_id: UserSubscriptionId,
        user_id: UserId,
        plan_id: SubscriptionPlanId,
        window_started_at: u64,
        window_ends_at: u64,
        bound_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        validate_time(window_started_at)?;
        validate_time(window_ends_at)?;
        validate_time(bound_at)?;
        if window_ends_at <= window_started_at || bound_at >= window_ends_at {
            return Err(SubscriptionInputError::InvalidWindow);
        }
        Ok(Self {
            subscription_id,
            user_id,
            plan_id,
            window_started_at,
            window_ends_at,
            bound_at,
        })
    }
}

impl fmt::Debug for UserSubscriptionBind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSubscriptionBind(<redacted>)")
    }
}

/// 订阅周期扫描使用的复合游标，按窗口结束时间和数据库主键稳定排序。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionResetDueCursor {
    window_ends_at: i64,
    database_id: i64,
}

impl SubscriptionResetDueCursor {
    /// 校验并构造一个可继续使用的到期扫描游标。
    pub fn new(window_ends_at: u64, database_id: i64) -> Result<Self, SubscriptionInputError> {
        let window_ends_at =
            i64::try_from(window_ends_at).map_err(|_| SubscriptionInputError::InvalidCursor)?;
        if window_ends_at < 0 || database_id <= 0 {
            return Err(SubscriptionInputError::InvalidCursor);
        }
        Ok(Self {
            window_ends_at,
            database_id,
        })
    }

    /// 返回游标保存的窗口结束时间。
    #[must_use]
    pub const fn window_ends_at(self) -> u64 {
        self.window_ends_at as u64
    }

    /// 返回游标保存的数据库主键。
    #[must_use]
    pub const fn database_id(self) -> i64 {
        self.database_id
    }
}

/// 推进单个用户订阅窗口的 CAS 命令。
pub struct UserSubscriptionWindowAdvance {
    pub(super) subscription_id: UserSubscriptionId,
    pub(super) expected_version: i64,
    pub(super) window_started_at: u64,
    pub(super) window_ends_at: u64,
    pub(super) now: u64,
}

impl UserSubscriptionWindowAdvance {
    /// 校验版本、观测窗口和扫描时刻，固化一次可重放的推进事实。
    pub fn new(
        subscription_id: UserSubscriptionId,
        expected_version: u64,
        window_started_at: u64,
        window_ends_at: u64,
        now: u64,
    ) -> Result<Self, SubscriptionInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| SubscriptionInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(SubscriptionInputError::InvalidVersion);
        }
        validate_time(window_started_at)?;
        validate_time(window_ends_at)?;
        validate_time(now)?;
        SubscriptionWindow::new(window_started_at, window_ends_at)
            .map_err(|_| SubscriptionInputError::InvalidWindow)?;
        Ok(Self {
            subscription_id,
            expected_version,
            window_started_at,
            window_ends_at,
            now,
        })
    }

    /// 从已读出的订阅快照构造推进命令，避免调用方拼接窗口事实。
    pub fn from_record(
        record: &UserSubscriptionRecord,
        now: u64,
    ) -> Result<Self, SubscriptionInputError> {
        Self::new(
            record.subscription_id,
            record.version,
            record.window_started_at,
            record.window_ends_at,
            now,
        )
    }
}

impl fmt::Debug for UserSubscriptionWindowAdvance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSubscriptionWindowAdvance(<redacted>)")
    }
}

/// 已持久化订阅计划的不可变价格快照。
pub struct SubscriptionPlanPriceRecord {
    pub(super) database_id: i64,
    pub(super) plan_database_id: i64,
    pub(super) provider: String,
    pub(super) currency: String,
    pub(super) amount_minor: i64,
    pub(super) created_at: u64,
}

impl SubscriptionPlanPriceRecord {
    #[must_use]
    pub const fn database_id(&self) -> i64 {
        self.database_id
    }

    #[must_use]
    pub const fn plan_database_id(&self) -> i64 {
        self.plan_database_id
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
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
}

impl fmt::Debug for SubscriptionPlanPriceRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPlanPriceRecord(<redacted>)")
    }
}

/// 创建订阅购买订单时固化的请求事实。
pub struct SubscriptionOrderCreate {
    pub(super) order_id: SubscriptionOrderId,
    pub(super) request_id: SubscriptionOrderRequestId,
    pub(super) user_id: UserId,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) plan_version: u64,
    pub(super) provider: String,
    pub(super) currency: String,
    pub(super) amount_minor: i64,
    pub(super) created_at: u64,
    pub(super) expires_at: Option<u64>,
}

impl SubscriptionOrderCreate {
    /// 校验订单 ID、价格快照和有效时间后构造幂等建单命令。
    #[allow(clippy::too_many_arguments, reason = "字段与订单建单契约一一对应")]
    pub fn new(
        order_id: SubscriptionOrderId,
        request_id: SubscriptionOrderRequestId,
        user_id: UserId,
        plan_id: SubscriptionPlanId,
        plan_version: u64,
        provider: String,
        currency: String,
        amount_minor: i64,
        created_at: u64,
        expires_at: Option<u64>,
    ) -> Result<Self, SubscriptionInputError> {
        let plan_version =
            i64::try_from(plan_version).map_err(|_| SubscriptionInputError::InvalidVersion)?;
        if plan_version <= 0
            || !valid_price_text(&provider, MAX_SUBSCRIPTION_PROVIDER_BYTES)
            || !valid_currency(&currency)
            || amount_minor <= 0
        {
            return Err(SubscriptionInputError::InvalidPrice);
        }
        validate_time(created_at)?;
        if expires_at.is_some_and(|value| value <= created_at) {
            return Err(SubscriptionInputError::InvalidTiming);
        }
        Ok(Self {
            order_id,
            request_id,
            user_id,
            plan_id,
            plan_version: plan_version as u64,
            provider,
            currency,
            amount_minor,
            created_at,
            expires_at,
        })
    }
}

impl fmt::Debug for SubscriptionOrderCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionOrderCreate(<redacted>)")
    }
}

/// 将订阅订单绑定到 Provider 订单的版本 CAS 命令。
#[derive(Clone)]
pub struct SubscriptionOrderSubmission {
    pub(super) order_id: SubscriptionOrderId,
    pub(super) expected_version: i64,
    pub(super) provider_order_id: String,
    pub(super) payment_method: String,
    pub(super) submitted_at: u64,
    pub(super) expires_at: u64,
}

impl SubscriptionOrderSubmission {
    pub fn new(
        order_id: SubscriptionOrderId,
        expected_version: u64,
        provider_order_id: String,
        payment_method: String,
        submitted_at: u64,
        expires_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| SubscriptionInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(SubscriptionInputError::InvalidVersion);
        }
        if !valid_price_text(&provider_order_id, MAX_SUBSCRIPTION_PROVIDER_EVENT_ID_BYTES)
            || !valid_price_text(&payment_method, MAX_SUBSCRIPTION_PAYMENT_METHOD_BYTES)
        {
            return Err(SubscriptionInputError::InvalidPrice);
        }
        validate_time(submitted_at)?;
        validate_time(expires_at)?;
        if expires_at <= submitted_at {
            return Err(SubscriptionInputError::InvalidTiming);
        }
        Ok(Self {
            order_id,
            expected_version,
            provider_order_id,
            payment_method,
            submitted_at,
            expires_at,
        })
    }
}

impl fmt::Debug for SubscriptionOrderSubmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionOrderSubmission(<redacted>)")
    }
}

/// 已持久化的订阅购买订单快照。
pub struct SubscriptionOrderRecord {
    #[allow(dead_code, reason = "支付确认事务切片将使用持久化订单主键")]
    pub(super) database_id: i64,
    pub(super) order_id: SubscriptionOrderId,
    pub(super) request_id: SubscriptionOrderRequestId,
    pub(super) user_id: UserId,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) plan_version: u64,
    pub(super) provider: String,
    pub(super) currency: String,
    pub(super) amount_minor: i64,
    pub(super) quota_amount: Quota,
    pub(super) status: SubscriptionOrderStatus,
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

impl SubscriptionOrderRecord {
    #[allow(dead_code, reason = "支付确认事务切片将读取持久化订单主键")]
    pub(crate) const fn database_id(&self) -> i64 {
        self.database_id
    }
    #[must_use]
    pub const fn order_id(&self) -> SubscriptionOrderId {
        self.order_id
    }
    #[must_use]
    pub const fn request_id(&self) -> SubscriptionOrderRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub const fn plan_id(&self) -> SubscriptionPlanId {
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
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }
    #[must_use]
    pub const fn status(&self) -> SubscriptionOrderStatus {
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

    pub(super) fn matches_create(
        &self,
        create: &SubscriptionOrderCreate,
        quota: Option<Quota>,
    ) -> bool {
        self.request_id == create.request_id
            && self.user_id == create.user_id
            && self.plan_id == create.plan_id
            && self.plan_version == create.plan_version
            && self.provider == create.provider
            && self.currency == create.currency
            && self.amount_minor == create.amount_minor
            && quota.is_none_or(|value| self.quota_amount == value)
    }

    pub(super) fn matches_submission(&self, write: &SubscriptionOrderSubmission) -> bool {
        self.order_id == write.order_id
            && self.status == SubscriptionOrderStatus::Pending
            && self.version == write.expected_version as u64 + 1
            && self.provider_order_id.as_deref() == Some(write.provider_order_id.as_str())
            && self.payment_method.as_deref() == Some(write.payment_method.as_str())
            && self.expires_at == Some(write.expires_at)
    }
}

impl fmt::Debug for SubscriptionOrderRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionOrderRecord(<redacted>)")
    }
}

/// 已完成验签的订阅支付事件写入事实；原始 payload 不进入订阅仓储。
pub struct SubscriptionPaymentEventWrite {
    pub(super) event_id: SubscriptionPaymentEventId,
    pub(super) order_id: SubscriptionOrderId,
    pub(super) provider: String,
    pub(super) provider_event_id: String,
    pub(super) trade_no: Option<String>,
    pub(super) amount_minor: Option<i64>,
    pub(super) currency: Option<String>,
    pub(super) payment_method: Option<String>,
    pub(super) event_type: SubscriptionPaymentEventType,
    pub(super) signature_key_fingerprint: [u8; 32],
    pub(super) payload_sha256: [u8; 32],
    pub(super) received_at: u64,
}

impl SubscriptionPaymentEventWrite {
    /// 校验 Provider 事件边界；成功事件必须携带可核对的金额、币种和流水号。
    #[allow(clippy::too_many_arguments, reason = "支付事件字段需要逐项固化并核对")]
    pub fn new(
        event_id: SubscriptionPaymentEventId,
        order_id: SubscriptionOrderId,
        provider: String,
        provider_event_id: String,
        trade_no: Option<String>,
        amount_minor: Option<u64>,
        currency: Option<String>,
        payment_method: Option<String>,
        event_type: SubscriptionPaymentEventType,
        signature_key_fingerprint: [u8; 32],
        payload_sha256: [u8; 32],
        received_at: u64,
    ) -> Result<Self, SubscriptionInputError> {
        if !valid_price_text(&provider, MAX_SUBSCRIPTION_PROVIDER_BYTES)
            || !valid_price_text(&provider_event_id, MAX_SUBSCRIPTION_PROVIDER_EVENT_ID_BYTES)
        {
            return Err(SubscriptionInputError::InvalidPrice);
        }
        if trade_no
            .as_deref()
            .is_some_and(|value| !valid_price_text(value, MAX_SUBSCRIPTION_TRADE_NO_BYTES))
            || (event_type == SubscriptionPaymentEventType::Succeeded && trade_no.is_none())
        {
            return Err(SubscriptionInputError::InvalidPrice);
        }
        let amount_minor = amount_minor
            .map(|value| i64::try_from(value).map_err(|_| SubscriptionInputError::InvalidPrice))
            .transpose()?;
        if event_type == SubscriptionPaymentEventType::Succeeded
            && (amount_minor.is_none_or(|value| value <= 0) || currency.is_none())
        {
            return Err(SubscriptionInputError::InvalidPrice);
        }
        if currency
            .as_deref()
            .is_some_and(|value| !valid_currency(value))
            || payment_method.as_deref().is_some_and(|value| {
                !valid_price_text(value, MAX_SUBSCRIPTION_PAYMENT_METHOD_BYTES)
            })
        {
            return Err(SubscriptionInputError::InvalidPrice);
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

impl fmt::Debug for SubscriptionPaymentEventWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPaymentEventWrite(<redacted>)")
    }
}

/// 已验签支付事件在订单与订阅事务中的闭合拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubscriptionPaymentEventRejection {
    ProviderMismatch,
    AmountMismatch,
    CurrencyMismatch,
    PaymentMethodMismatch,
    TimingConflict,
    TradeNumberConflict,
    TerminalConflict,
}

/// 订阅支付事件的幂等处理结果；通知投递状态不改变购买状态。
pub enum SubscriptionPaymentEventOutcome {
    Applied {
        order: SubscriptionOrderRecord,
        subscription: Option<UserSubscriptionRecord>,
    },
    Acknowledged {
        order: SubscriptionOrderRecord,
        subscription: Option<UserSubscriptionRecord>,
    },
    Existing {
        order: SubscriptionOrderRecord,
        subscription: Option<UserSubscriptionRecord>,
    },
    RecordedUnprocessed {
        order: SubscriptionOrderRecord,
        reason: SubscriptionPaymentEventRejection,
    },
    NotFound,
}

impl fmt::Debug for SubscriptionPaymentEventOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied { .. } => {
                formatter.write_str("SubscriptionPaymentEventOutcome::Applied(<redacted>)")
            }
            Self::Acknowledged { .. } => {
                formatter.write_str("SubscriptionPaymentEventOutcome::Acknowledged(<redacted>)")
            }
            Self::Existing { .. } => {
                formatter.write_str("SubscriptionPaymentEventOutcome::Existing(<redacted>)")
            }
            Self::RecordedUnprocessed { reason, .. } => formatter
                .debug_struct("SubscriptionPaymentEventOutcome::RecordedUnprocessed")
                .field("reason", reason)
                .finish_non_exhaustive(),
            Self::NotFound => formatter.write_str("SubscriptionPaymentEventOutcome::NotFound"),
        }
    }
}

/// 已持久化订阅计划的非敏感状态快照。
pub struct SubscriptionPlanRecord {
    pub(super) database_id: i64,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) name: String,
    pub(super) created_by_user_id: UserId,
    pub(super) status: SubscriptionPlanStatus,
    pub(super) quota_amount: Quota,
    pub(super) cycle: SubscriptionCycle,
    pub(super) version: u64,
    pub(super) disabled_at: Option<u64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl SubscriptionPlanRecord {
    #[must_use]
    pub(crate) const fn database_id(&self) -> i64 {
        self.database_id
    }
    /// 返回稳定计划标识。
    #[must_use]
    pub const fn plan_id(&self) -> SubscriptionPlanId {
        self.plan_id
    }

    /// 返回计划名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回创建该计划的用户。
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }

    /// 返回当前计划状态。
    #[must_use]
    pub const fn status(&self) -> SubscriptionPlanStatus {
        self.status
    }

    /// 返回每个周期可消费的整数额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回计划额度周期。
    #[must_use]
    pub const fn cycle(&self) -> SubscriptionCycle {
        self.cycle
    }

    /// 返回当前 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回可选停用时间的 Unix 秒数。
    #[must_use]
    pub const fn disabled_at(&self) -> Option<u64> {
        self.disabled_at
    }

    /// 返回计划创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回保持单调的计划审计更新时间 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_write(&self, write: &SubscriptionPlanWrite) -> bool {
        self.plan_id == write.plan_id
            && self.name == write.name
            && self.created_by_user_id == write.created_by_user_id
            && self.quota_amount == write.quota_amount
            && self.cycle == write.cycle
            && self.created_at == write.created_at
    }
}

impl fmt::Debug for SubscriptionPlanRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SubscriptionPlanRecord(<redacted>)")
    }
}

/// 已持久化用户订阅的当前窗口快照。
pub struct UserSubscriptionRecord {
    pub(super) subscription_id: UserSubscriptionId,
    pub(super) user_id: UserId,
    pub(super) plan_id: SubscriptionPlanId,
    pub(super) plan_name: String,
    pub(super) plan_version: u64,
    pub(super) status: UserSubscriptionStatus,
    pub(super) quota_amount: Quota,
    pub(super) quota_used: Quota,
    pub(super) cycle: SubscriptionCycle,
    pub(super) window_started_at: u64,
    pub(super) window_ends_at: u64,
    pub(super) version: u64,
    pub(super) bound_at: u64,
    pub(super) status_changed_at: u64,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl UserSubscriptionRecord {
    /// 返回稳定用户订阅标识。
    #[must_use]
    pub const fn subscription_id(&self) -> UserSubscriptionId {
        self.subscription_id
    }

    /// 返回订阅所属用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回订阅绑定的稳定计划标识。
    #[must_use]
    pub const fn plan_id(&self) -> SubscriptionPlanId {
        self.plan_id
    }

    /// 返回绑定计划的不可变名称。
    #[must_use]
    pub fn plan_name(&self) -> &str {
        &self.plan_name
    }

    /// 返回绑定时快照的计划版本。
    #[must_use]
    pub const fn plan_version(&self) -> u64 {
        self.plan_version
    }

    /// 返回当前订阅状态。
    #[must_use]
    pub const fn status(&self) -> UserSubscriptionStatus {
        self.status
    }

    /// 返回本周期固化的额度上限。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回本周期已消费额度。
    #[must_use]
    pub const fn quota_used(&self) -> Quota {
        self.quota_used
    }

    /// 返回绑定时快照的日历周期。
    #[must_use]
    pub const fn cycle(&self) -> SubscriptionCycle {
        self.cycle
    }

    /// 返回当前额度窗口起点 Unix 秒数。
    #[must_use]
    pub const fn window_started_at(&self) -> u64 {
        self.window_started_at
    }

    /// 返回当前额度窗口终点 Unix 秒数。
    #[must_use]
    pub const fn window_ends_at(&self) -> u64 {
        self.window_ends_at
    }

    /// 返回当前 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回首次绑定时间 Unix 秒数。
    #[must_use]
    pub const fn bound_at(&self) -> u64 {
        self.bound_at
    }

    /// 返回最近一次生命周期状态变更时间 Unix 秒数。
    #[must_use]
    pub const fn status_changed_at(&self) -> u64 {
        self.status_changed_at
    }

    /// 返回首次持久化时间 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回保持单调的用户订阅审计更新时间 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_bind(&self, bind: &UserSubscriptionBind) -> bool {
        self.subscription_id == bind.subscription_id
            && self.user_id == bind.user_id
            && self.plan_id == bind.plan_id
            && self.window_started_at == bind.window_started_at
            && self.window_ends_at == bind.window_ends_at
            && self.bound_at == bind.bound_at
    }
}

impl fmt::Debug for UserSubscriptionRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserSubscriptionRecord(<redacted>)")
    }
}

/// 订阅计划分页查询结果；游标使用内部主键但不会进入响应正文之外的业务事实。
pub struct SubscriptionPlanPageRecord {
    plans: Vec<SubscriptionPlanRecord>,
    next_cursor: Option<i64>,
}

impl SubscriptionPlanPageRecord {
    pub(super) fn new(plans: Vec<SubscriptionPlanRecord>, next_cursor: Option<i64>) -> Self {
        Self { plans, next_cursor }
    }

    /// 返回当前页计划。
    #[must_use]
    pub fn plans(&self) -> &[SubscriptionPlanRecord] {
        &self.plans
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for SubscriptionPlanPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SubscriptionPlanPageRecord")
            .field("plan_count", &self.plans.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 指定用户订阅分页查询结果。
pub struct UserSubscriptionPageRecord {
    subscriptions: Vec<UserSubscriptionRecord>,
    next_cursor: Option<i64>,
}

impl UserSubscriptionPageRecord {
    pub(super) fn new(
        subscriptions: Vec<UserSubscriptionRecord>,
        next_cursor: Option<i64>,
    ) -> Self {
        Self {
            subscriptions,
            next_cursor,
        }
    }

    /// 返回当前页订阅。
    #[must_use]
    pub fn subscriptions(&self) -> &[UserSubscriptionRecord] {
        &self.subscriptions
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for UserSubscriptionPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSubscriptionPageRecord")
            .field("subscription_count", &self.subscriptions.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 到期 Active 订阅的稳定分页结果。
pub struct UserSubscriptionResetDuePageRecord {
    subscriptions: Vec<UserSubscriptionRecord>,
    next_cursor: Option<SubscriptionResetDueCursor>,
}

impl UserSubscriptionResetDuePageRecord {
    pub(super) fn new(
        subscriptions: Vec<UserSubscriptionRecord>,
        next_cursor: Option<SubscriptionResetDueCursor>,
    ) -> Self {
        Self {
            subscriptions,
            next_cursor,
        }
    }

    /// 返回当前页的到期 Active 订阅。
    #[must_use]
    pub fn subscriptions(&self) -> &[UserSubscriptionRecord] {
        &self.subscriptions
    }

    /// 返回下一页复合游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<SubscriptionResetDueCursor> {
        self.next_cursor
    }
}

impl fmt::Debug for UserSubscriptionResetDuePageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSubscriptionResetDuePageRecord")
            .field("subscription_count", &self.subscriptions.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 单订阅窗口推进的闭合结果。
pub struct UserSubscriptionWindowAdvanceRecord {
    subscription: UserSubscriptionRecord,
    periods_elapsed: u32,
}

impl UserSubscriptionWindowAdvanceRecord {
    pub(super) const fn new(subscription: UserSubscriptionRecord, periods_elapsed: u32) -> Self {
        Self {
            subscription,
            periods_elapsed,
        }
    }

    /// 返回推进后的订阅快照。
    #[must_use]
    pub const fn subscription(&self) -> &UserSubscriptionRecord {
        &self.subscription
    }

    /// 返回本次跨越的完整周期数。
    #[must_use]
    pub const fn periods_elapsed(&self) -> u32 {
        self.periods_elapsed
    }
}

impl fmt::Debug for UserSubscriptionWindowAdvanceRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserSubscriptionWindowAdvanceRecord")
            .field("periods_elapsed", &self.periods_elapsed)
            .finish_non_exhaustive()
    }
}

/// 幂等创建订阅计划后的闭合结果。
pub enum SubscriptionPlanCreateOutcome {
    /// 本次调用创建了新计划。
    Created(SubscriptionPlanRecord),
    /// 相同计划标识和完整不可变事实已经存在。
    Existing(SubscriptionPlanRecord),
    /// 计划创建者不存在或已经软删除。
    CreatorNotFound,
}

/// 幂等创建订阅购买订单后的闭合结果。
pub enum SubscriptionOrderCreateOutcome {
    /// 本次调用创建了新的本地待支付订单。
    Created(SubscriptionOrderRecord),
    /// 相同用户和幂等请求已经创建过完全相同的订单。
    Existing(SubscriptionOrderRecord),
    /// 当前用户不存在或已被软删除。
    UserNotFound,
    /// 计划不存在。
    PlanNotFound,
    /// 计划已停止销售。
    PlanDisabled,
}

pub enum SubscriptionOrderSubmitOutcome {
    Applied(SubscriptionOrderRecord),
    Existing(SubscriptionOrderRecord),
    NotFound,
}

/// CAS 停用订阅计划后的闭合结果。
pub enum SubscriptionPlanDisableOutcome {
    /// 本次调用把 Active 计划推进为 Disabled。
    Applied(SubscriptionPlanRecord),
    /// 相同版本迁移事实已经提交。
    Existing(SubscriptionPlanRecord),
    /// 计划不存在。
    NotFound,
}

/// 用户订阅幂等绑定后的闭合结果。
pub enum UserSubscriptionBindOutcome {
    /// 本次调用首次创建用户订阅。
    Created(UserSubscriptionRecord),
    /// 相同绑定标识和完整事实已经存在。
    Existing(UserSubscriptionRecord),
    /// 目标用户不存在或已经软删除。
    UserNotFound,
    /// 目标计划不存在。
    PlanNotFound,
    /// 目标计划已停止新增绑定。
    PlanDisabled,
}

/// 单订阅窗口推进的状态结果，不把非 Active 订阅误当作可重置对象。
pub enum UserSubscriptionWindowAdvanceOutcome {
    /// 本次调用完成了窗口推进和用量归零。
    Applied(UserSubscriptionWindowAdvanceRecord),
    /// 相同命令已经完成过，返回当前相同窗口快照。
    Existing(UserSubscriptionWindowAdvanceRecord),
    /// 订阅标识不存在。
    NotFound,
    /// 当前窗口尚未到期。
    NotDue(UserSubscriptionRecord),
    /// 当前窗口仍有请求处于预留或待结算状态，必须等待原请求终态。
    InUse(UserSubscriptionRecord),
    /// 订阅处于暂停、取消或过期状态，留给生命周期切片处理。
    Inactive(UserSubscriptionRecord),
}

/// 订阅输入构造错误；不携带名称、标识或用户信息。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionInputError {
    /// 计划名称不符合长度、空白或控制字符边界。
    #[error("订阅计划名称无效")]
    InvalidName,
    /// 计划周期额度必须为正整数。
    #[error("订阅计划额度无效")]
    InvalidQuota,
    #[error("订阅价格无效")]
    InvalidPrice,
    /// 用户订阅时间窗为空或绑定时已经结束。
    #[error("用户订阅时间窗无效")]
    InvalidWindow,
    /// 时间戳无法持久化。
    #[error("订阅时间边界无效")]
    InvalidTiming,
    /// CAS 版本不是可递增的正整数。
    #[error("订阅计划版本无效")]
    InvalidVersion,
    /// 到期扫描游标的时间或数据库主键无效。
    #[error("订阅扫描游标无效")]
    InvalidCursor,
    /// 生命周期源状态与目标状态不属于闭合迁移图。
    #[error("订阅生命周期迁移无效")]
    InvalidLifecycleTransition,
}

/// 订阅仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("订阅仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 订阅仓储错误；不携带计划、用户或绑定标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionRepositoryError {
    /// 相同计划、绑定键或 CAS 版本绑定了不同事实。
    #[error("订阅计划或用户绑定冲突")]
    Conflict,
    /// 支付成功后无法将订单绑定到唯一的用户订阅。
    #[error("订阅支付绑定冲突")]
    BindingConflict,
    /// 获取连接或执行确定未提交的数据库操作失败。
    #[error("订阅数据库操作失败")]
    Query,
    /// 只读查询超过仓储截止时间。
    #[error("订阅数据库查询超时")]
    Timeout,
    /// 写入超时或提交失败，调用方必须复用原标识查询或重试。
    #[error("订阅操作结果未知")]
    OutcomeUnknown,
    /// 持久化计划、用户订阅或时间窗违反不变量。
    #[error("订阅持久化状态损坏")]
    Invariant,
}

impl fmt::Debug for SubscriptionPlanCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("SubscriptionPlanCreateOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("SubscriptionPlanCreateOutcome::Existing(<redacted>)")
            }
            Self::CreatorNotFound => {
                formatter.write_str("SubscriptionPlanCreateOutcome::CreatorNotFound")
            }
        }
    }
}

impl fmt::Debug for SubscriptionOrderCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("SubscriptionOrderCreateOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("SubscriptionOrderCreateOutcome::Existing(<redacted>)")
            }
            Self::UserNotFound => {
                formatter.write_str("SubscriptionOrderCreateOutcome::UserNotFound")
            }
            Self::PlanNotFound => {
                formatter.write_str("SubscriptionOrderCreateOutcome::PlanNotFound")
            }
            Self::PlanDisabled => {
                formatter.write_str("SubscriptionOrderCreateOutcome::PlanDisabled")
            }
        }
    }
}

impl fmt::Debug for SubscriptionOrderSubmitOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => {
                formatter.write_str("SubscriptionOrderSubmitOutcome::Applied(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("SubscriptionOrderSubmitOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("SubscriptionOrderSubmitOutcome::NotFound"),
        }
    }
}

impl fmt::Debug for SubscriptionPlanDisableOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => {
                formatter.write_str("SubscriptionPlanDisableOutcome::Applied(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("SubscriptionPlanDisableOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("SubscriptionPlanDisableOutcome::NotFound"),
        }
    }
}

impl fmt::Debug for UserSubscriptionBindOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("UserSubscriptionBindOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("UserSubscriptionBindOutcome::Existing(<redacted>)")
            }
            Self::UserNotFound => formatter.write_str("UserSubscriptionBindOutcome::UserNotFound"),
            Self::PlanNotFound => formatter.write_str("UserSubscriptionBindOutcome::PlanNotFound"),
            Self::PlanDisabled => formatter.write_str("UserSubscriptionBindOutcome::PlanDisabled"),
        }
    }
}

impl fmt::Debug for UserSubscriptionWindowAdvanceOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => {
                formatter.write_str("UserSubscriptionWindowAdvanceOutcome::Applied(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("UserSubscriptionWindowAdvanceOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("UserSubscriptionWindowAdvanceOutcome::NotFound"),
            Self::NotDue(_) => {
                formatter.write_str("UserSubscriptionWindowAdvanceOutcome::NotDue(<redacted>)")
            }
            Self::InUse(_) => {
                formatter.write_str("UserSubscriptionWindowAdvanceOutcome::InUse(<redacted>)")
            }
            Self::Inactive(_) => {
                formatter.write_str("UserSubscriptionWindowAdvanceOutcome::Inactive(<redacted>)")
            }
        }
    }
}

pub(super) fn validate_time(value: u64) -> Result<(), SubscriptionInputError> {
    if value > i64::MAX as u64 {
        return Err(SubscriptionInputError::InvalidTiming);
    }
    Ok(())
}

pub(super) fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SUBSCRIPTION_PLAN_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_price_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(super) fn valid_currency(value: &str) -> bool {
    value.len() == MAX_SUBSCRIPTION_CURRENCY_BYTES
        && value.bytes().all(|byte| byte.is_ascii_uppercase())
}
