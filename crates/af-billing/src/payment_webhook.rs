use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    SubscriptionPaymentEventOutcome, SubscriptionPaymentEventWrite, SubscriptionRepository,
    SubscriptionRepositoryError, TopupPaymentEventOutcome, TopupPaymentEventRejection,
    TopupPaymentEventWrite, TopupRepository, TopupRepositoryError,
};
use af_domain::{
    SubscriptionOrderId, SubscriptionPaymentEventId, SubscriptionPaymentEventType, TopupOrderId,
    TopupOrderStatus, TopupPaymentEventId, TopupPaymentEventType,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// 单个 webhook 原始 payload 的最大字节数。
pub const MAX_PAYMENT_WEBHOOK_PAYLOAD_BYTES: usize = 1024 * 1024;
/// 交给 Provider 验签器的最大签名相关请求头数量。
pub const MAX_PAYMENT_WEBHOOK_HEADERS: usize = 32;
/// 单个签名请求头名称的最大字节数。
pub const MAX_PAYMENT_WEBHOOK_HEADER_NAME_BYTES: usize = 128;
/// 单个签名请求头值的最大字节数。
pub const MAX_PAYMENT_WEBHOOK_HEADER_VALUE_BYTES: usize = 4 * 1024;

/// 保留原始字节语义的单个 webhook 请求头。
#[derive(Clone, Copy)]
pub struct PaymentWebhookHeader<'a> {
    name: &'a str,
    value: &'a str,
}

impl<'a> PaymentWebhookHeader<'a> {
    /// 校验 HTTP 头名称和值的有界可表示形式。
    pub fn new(name: &'a str, value: &'a str) -> Result<Self, PaymentWebhookInputError> {
        if name.is_empty()
            || name.len() > MAX_PAYMENT_WEBHOOK_HEADER_NAME_BYTES
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value.len() > MAX_PAYMENT_WEBHOOK_HEADER_VALUE_BYTES
            || value
                .bytes()
                .any(|byte| (byte < b' ' && byte != b'\t') || byte == 0x7f)
        {
            return Err(PaymentWebhookInputError::InvalidHeader);
        }
        Ok(Self { name, value })
    }

    /// 返回原始请求头名称。
    #[must_use]
    pub const fn name(self) -> &'a str {
        self.name
    }

    /// 返回原始请求头值；调用方不得写入日志。
    #[must_use]
    pub const fn value(self) -> &'a str {
        self.value
    }
}

impl fmt::Debug for PaymentWebhookHeader<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentWebhookHeader(<redacted>)")
    }
}

/// 未解析、未验签的支付 webhook 输入。
pub struct PaymentWebhookRequest<'a> {
    payload: &'a [u8],
    headers: &'a [PaymentWebhookHeader<'a>],
    received_at: u64,
}

impl<'a> PaymentWebhookRequest<'a> {
    /// 校验 payload、头数量和受信服务端接收时间。
    pub fn new(
        payload: &'a [u8],
        headers: &'a [PaymentWebhookHeader<'a>],
        received_at: u64,
    ) -> Result<Self, PaymentWebhookInputError> {
        if payload.is_empty() {
            return Err(PaymentWebhookInputError::EmptyPayload);
        }
        if payload.len() > MAX_PAYMENT_WEBHOOK_PAYLOAD_BYTES {
            return Err(PaymentWebhookInputError::PayloadTooLarge);
        }
        if headers.len() > MAX_PAYMENT_WEBHOOK_HEADERS {
            return Err(PaymentWebhookInputError::TooManyHeaders);
        }
        if received_at > i64::MAX as u64 {
            return Err(PaymentWebhookInputError::InvalidTiming);
        }
        Ok(Self {
            payload,
            headers,
            received_at,
        })
    }

    /// 返回必须按原始字节参与验签的 payload。
    #[must_use]
    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }

    /// 返回 Provider 验签所需的原始请求头。
    #[must_use]
    pub const fn headers(&self) -> &'a [PaymentWebhookHeader<'a>] {
        self.headers
    }

    /// 返回受信服务端记录的接收时间。
    #[must_use]
    pub const fn received_at(&self) -> u64 {
        self.received_at
    }
}

impl fmt::Debug for PaymentWebhookRequest<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentWebhookRequest(<redacted>)")
    }
}

/// Provider 验签成功后返回的规范支付事实。
pub struct VerifiedTopupPaymentEvent {
    order_id: TopupOrderId,
    provider_event_id: String,
    trade_no: Option<String>,
    event_type: TopupPaymentEventType,
    amount_minor: u64,
    currency: String,
    payment_method: String,
    signature_key_fingerprint: [u8; 32],
}

impl VerifiedTopupPaymentEvent {
    /// 构造已验签事件；调用方必须是经过审查的 Provider Verifier 实现。
    pub fn new(
        order_id: TopupOrderId,
        provider_event_id: String,
        trade_no: Option<String>,
        event_type: TopupPaymentEventType,
        amount_minor: u64,
        currency: String,
        payment_method: String,
        signature_key_fingerprint: [u8; 32],
    ) -> Result<Self, PaymentWebhookVerificationError> {
        if provider_event_id.is_empty()
            || provider_event_id.len() > af_db::MAX_PROVIDER_EVENT_ID_BYTES
            || provider_event_id.trim() != provider_event_id
            || provider_event_id.chars().any(char::is_control)
            || trade_no.as_deref().is_some_and(|value| {
                value.is_empty()
                    || value.len() > af_db::MAX_PROVIDER_TRADE_NO_BYTES
                    || value.trim() != value
                    || value.chars().any(char::is_control)
            })
            || (event_type == TopupPaymentEventType::Succeeded && trade_no.is_none())
            || amount_minor > i64::MAX as u64
            || (event_type == TopupPaymentEventType::Succeeded && amount_minor == 0)
            || currency.len() != 3
            || !currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || !valid_payment_method(&payment_method)
        {
            return Err(PaymentWebhookVerificationError::InvalidEvent);
        }
        Ok(Self {
            order_id,
            provider_event_id,
            trade_no,
            event_type,
            amount_minor,
            currency,
            payment_method,
            signature_key_fingerprint,
        })
    }

    /// 返回 Provider 已验签的支付金额，单位为币种最小单位。
    #[must_use]
    pub const fn amount_minor(&self) -> u64 {
        self.amount_minor
    }

    /// 返回 Provider 已验签并规范化为大写的三位币种代码。
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// 返回 Provider 已验签并规范化的支付方式标识。
    #[must_use]
    pub fn payment_method(&self) -> &str {
        &self.payment_method
    }
}

impl fmt::Debug for VerifiedTopupPaymentEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VerifiedTopupPaymentEvent(<redacted>)")
    }
}

fn valid_payment_method(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

/// Provider 专属 webhook 验签与解析边界。
///
/// 实现必须先用原始 payload 和 Provider 规定的原始请求头完成密码学验证，再解析事件；
/// 禁止先规范化 JSON、表单或头值后验签。
pub trait PaymentProvider: Send + Sync + 'static {
    /// 返回与充值订单持久化值一致的规范 Provider 标识。
    fn provider(&self) -> &str;

    /// 验证原始 webhook 并返回不含原始 payload 的规范事件。
    fn verify_webhook(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError>;
}

/// 兼容旧调用方的 webhook 验签视图；新 Provider 应直接实现 [`PaymentProvider`]。
pub trait PaymentWebhookVerifier: Send + Sync + 'static {
    /// 返回与充值订单中保存值一致的规范 Provider 标识。
    fn provider(&self) -> &str;

    /// 验证签名并返回不含原始 payload 的规范事件。
    fn verify(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError>;
}

impl<T> PaymentWebhookVerifier for T
where
    T: PaymentProvider + ?Sized,
{
    fn provider(&self) -> &str {
        PaymentProvider::provider(self)
    }

    fn verify(
        &self,
        request: &PaymentWebhookRequest<'_>,
    ) -> Result<VerifiedTopupPaymentEvent, PaymentWebhookVerificationError> {
        PaymentProvider::verify_webhook(self, request)
    }
}

/// 持久化已验证事件的一次异步调用结果。
pub type TopupPaymentEventFuture<'a> = Pin<
    Box<dyn Future<Output = Result<TopupWebhookOutcome, TopupPaymentEventPortError>> + Send + 'a>,
>;

/// 只接受已由处理器封装的规范支付事实。
pub trait TopupPaymentEventPort: Send + Sync + 'static {
    /// 审计事件并原子推进订单及钱包账本。
    fn accept<'a>(&'a self, write: TopupPaymentEventWrite) -> TopupPaymentEventFuture<'a>;
}

impl TopupPaymentEventPort for TopupRepository {
    fn accept<'a>(&'a self, write: TopupPaymentEventWrite) -> TopupPaymentEventFuture<'a> {
        Box::pin(async move {
            TopupRepository::accept_verified_event(self, write)
                .await
                .map(map_repository_outcome)
                .map_err(map_repository_error)
        })
    }
}

/// 验签成功后的支付事件处理结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TopupWebhookOutcome {
    /// 新事件首次推进订单终态。
    Applied(TopupOrderStatus),
    /// 新事件与现有相同终态一致，已处理但未重复加额。
    Acknowledged(TopupOrderStatus),
    /// 相同 Provider 事件事实已经存在。
    Existing(TopupOrderStatus),
    /// 新事件已审计但未处理。
    RecordedUnprocessed {
        /// 当前订单状态。
        status: TopupOrderStatus,
        /// 未处理的闭合原因。
        reason: TopupPaymentEventRejection,
    },
    /// 已验签事件只描述 Provider 可重试中间态，无需持久化或推进订单。
    IgnoredNonTerminal,
    /// 事件引用的本地订单不存在。
    NotFound,
}

/// 验签并持久化支付事件的应用服务。
#[derive(Clone)]
pub struct TopupWebhookProcessor {
    verifier: Arc<dyn PaymentWebhookVerifier>,
    event_port: Arc<dyn TopupPaymentEventPort>,
}

impl TopupWebhookProcessor {
    /// 组合单个 Provider 的验签器与持久化端口。
    #[must_use]
    pub fn new(
        verifier: Arc<dyn PaymentWebhookVerifier>,
        event_port: Arc<dyn TopupPaymentEventPort>,
    ) -> Self {
        Self {
            verifier,
            event_port,
        }
    }

    /// 先验签、再计算原始 payload 摘要，最后才进入幂等仓储。
    pub async fn process(
        &self,
        request: PaymentWebhookRequest<'_>,
    ) -> Result<TopupWebhookOutcome, TopupWebhookProcessorError> {
        let verified = match self.verifier.verify(&request) {
            Ok(verified) => verified,
            Err(PaymentWebhookVerificationError::NonTerminalEvent) => {
                return Ok(TopupWebhookOutcome::IgnoredNonTerminal);
            }
            Err(_) => return Err(TopupWebhookProcessorError::VerificationRejected),
        };
        let event_id = TopupPaymentEventId::new(*Uuid::new_v4().as_bytes())
            .map_err(|_| TopupWebhookProcessorError::Invariant)?;
        let payload_sha256: [u8; 32] = Sha256::digest(request.payload()).into();
        let write = TopupPaymentEventWrite::new(
            event_id,
            verified.order_id,
            self.verifier.provider().to_owned(),
            verified.provider_event_id,
            verified.trade_no,
            verified.amount_minor,
            verified.currency,
            verified.payment_method,
            verified.event_type,
            verified.signature_key_fingerprint,
            payload_sha256,
            request.received_at(),
        )
        .map_err(|_| TopupWebhookProcessorError::Invariant)?;
        self.event_port.accept(write).await.map_err(Into::into)
    }
}

impl fmt::Debug for TopupWebhookProcessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TopupWebhookProcessor(<redacted>)")
    }
}

/// 原始 webhook 输入校验错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentWebhookInputError {
    /// 空 payload 无法形成支付事件。
    #[error("支付 webhook payload 不能为空")]
    EmptyPayload,
    /// payload 超过硬上限。
    #[error("支付 webhook payload 过大")]
    PayloadTooLarge,
    /// 签名相关请求头数量超过硬上限。
    #[error("支付 webhook 请求头过多")]
    TooManyHeaders,
    /// 请求头名称或值不符合受控边界。
    #[error("支付 webhook 请求头无效")]
    InvalidHeader,
    /// 服务端接收时间无法表示。
    #[error("支付 webhook 接收时间无效")]
    InvalidTiming,
}

/// Provider 验签器的闭合拒绝分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentWebhookVerificationError {
    /// 签名不存在、错误、过期或使用了非受信密钥。
    #[error("支付 webhook 验签失败")]
    InvalidSignature,
    /// 已验签 payload 无法按 Provider 协议解析。
    #[error("支付 webhook payload 无效")]
    InvalidPayload,
    /// Provider 事件类型不在当前闭合集合中。
    #[error("支付 webhook 事件不受支持")]
    UnsupportedEvent,
    /// 事件已经验签，但只描述可重试的中间状态，不应关闭本地订单。
    #[error("支付 webhook 事件属于非终态")]
    NonTerminalEvent,
    /// 解析后的规范事件违反字段边界。
    #[error("支付 webhook 事件字段无效")]
    InvalidEvent,
}

/// 已验证事件持久化端口错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TopupPaymentEventPortError {
    /// Provider 事件或订单状态发生幂等冲突。
    #[error("充值支付事件冲突")]
    Conflict,
    /// 提交结果未知，只能重放同一 Provider 事件。
    #[error("充值支付事件结果未知")]
    OutcomeUnknown,
    /// 持久化当前不可用。
    #[error("充值支付事件暂不可用")]
    Unavailable,
    /// 持久化状态违反充值不变量。
    #[error("充值支付事件状态损坏")]
    Invariant,
}

/// webhook 处理器错误；不携带签名、payload 或 Provider 标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TopupWebhookProcessorError {
    /// Provider 验签或解析拒绝，公开层应返回统一拒绝语义。
    #[error("支付 webhook 已拒绝")]
    VerificationRejected,
    /// Provider 事件或订单状态发生幂等冲突。
    #[error("支付 webhook 状态冲突")]
    Conflict,
    /// 提交结果未知，只能重放同一 Provider 事件。
    #[error("支付 webhook 处理结果未知")]
    OutcomeUnknown,
    /// 持久化当前不可用。
    #[error("支付 webhook 处理暂不可用")]
    Unavailable,
    /// 验签器或持久化结果违反内部不变量。
    #[error("支付 webhook 内部状态损坏")]
    Invariant,
}

impl From<TopupPaymentEventPortError> for TopupWebhookProcessorError {
    fn from(error: TopupPaymentEventPortError) -> Self {
        match error {
            TopupPaymentEventPortError::Conflict => Self::Conflict,
            TopupPaymentEventPortError::OutcomeUnknown => Self::OutcomeUnknown,
            TopupPaymentEventPortError::Unavailable => Self::Unavailable,
            TopupPaymentEventPortError::Invariant => Self::Invariant,
        }
    }
}

fn map_repository_outcome(outcome: TopupPaymentEventOutcome) -> TopupWebhookOutcome {
    match outcome {
        TopupPaymentEventOutcome::Applied(order) => TopupWebhookOutcome::Applied(order.status()),
        TopupPaymentEventOutcome::Acknowledged(order) => {
            TopupWebhookOutcome::Acknowledged(order.status())
        }
        TopupPaymentEventOutcome::Existing(order) => TopupWebhookOutcome::Existing(order.status()),
        TopupPaymentEventOutcome::RecordedUnprocessed { order, reason } => {
            TopupWebhookOutcome::RecordedUnprocessed {
                status: order.status(),
                reason,
            }
        }
        TopupPaymentEventOutcome::NotFound => TopupWebhookOutcome::NotFound,
    }
}

fn map_repository_error(error: TopupRepositoryError) -> TopupPaymentEventPortError {
    match error {
        TopupRepositoryError::Conflict => TopupPaymentEventPortError::Conflict,
        TopupRepositoryError::OutcomeUnknown => TopupPaymentEventPortError::OutcomeUnknown,
        TopupRepositoryError::Query
        | TopupRepositoryError::Timeout
        | TopupRepositoryError::UnsupportedTarget => TopupPaymentEventPortError::Unavailable,
        TopupRepositoryError::Invariant => TopupPaymentEventPortError::Invariant,
    }
}

/// HTTP webhook 只关心支付事实是否已闭合，不暴露充值或订阅内部状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentWebhookHandlerOutcome {
    Applied,
    Acknowledged,
    Existing,
    RecordedUnprocessed,
    IgnoredNonTerminal,
    NotFound,
}

/// 支付 webhook 在验签、路由和持久化边界上的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentWebhookHandlerError {
    #[error("支付 webhook 已拒绝")]
    VerificationRejected,
    #[error("支付 webhook 状态冲突")]
    Conflict,
    #[error("支付 webhook 订阅绑定冲突")]
    BindingConflict,
    #[error("支付 webhook 处理结果未知")]
    OutcomeUnknown,
    #[error("支付 webhook 处理暂不可用")]
    Unavailable,
    #[error("支付 webhook 内部状态损坏")]
    Invariant,
}

/// 通过已验签事件选择充值或订阅持久化端口。
pub trait PaymentWebhookEventRouter: Send + Sync + 'static {
    fn route<'a>(
        &'a self,
        provider: String,
        event: VerifiedTopupPaymentEvent,
        payload_sha256: [u8; 32],
        received_at: u64,
    ) -> PaymentWebhookRouteFuture<'a>;
}

pub type PaymentWebhookRouteFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PaymentWebhookHandlerOutcome, PaymentWebhookHandlerError>>
            + Send
            + 'a,
    >,
>;

/// 数据库支付路由：双查订单类型后才允许进入对应的原子确认事务。
#[derive(Clone)]
pub struct DatabasePaymentWebhookRouter {
    topup_repository: TopupRepository,
    subscription_repository: SubscriptionRepository,
}

impl DatabasePaymentWebhookRouter {
    #[must_use]
    pub fn new(
        topup_repository: TopupRepository,
        subscription_repository: SubscriptionRepository,
    ) -> Self {
        Self {
            topup_repository,
            subscription_repository,
        }
    }
}

impl PaymentWebhookEventRouter for DatabasePaymentWebhookRouter {
    fn route<'a>(
        &'a self,
        provider: String,
        event: VerifiedTopupPaymentEvent,
        payload_sha256: [u8; 32],
        received_at: u64,
    ) -> PaymentWebhookRouteFuture<'a> {
        Box::pin(async move {
            let topup = self
                .topup_repository
                .get_order(event.order_id)
                .await
                .map_err(map_topup_route_error)?;
            let subscription_id =
                SubscriptionOrderId::from_persistence_key(&event.order_id.persistence_key())
                    .map_err(|_| PaymentWebhookHandlerError::Invariant)?;
            let subscription = self
                .subscription_repository
                .get_order(subscription_id)
                .await
                .map_err(map_subscription_route_error)?;

            // 同一个外部订单键同时命中两类订单时，拒绝猜测资金归属。
            if topup.is_some() && subscription.is_some() {
                return Err(PaymentWebhookHandlerError::Conflict);
            }
            if topup.is_none() && subscription.is_none() {
                return Ok(PaymentWebhookHandlerOutcome::NotFound);
            }

            let event_id = *Uuid::new_v4().as_bytes();
            if topup.is_some() {
                let write = TopupPaymentEventWrite::new(
                    TopupPaymentEventId::new(event_id)
                        .map_err(|_| PaymentWebhookHandlerError::Invariant)?,
                    event.order_id,
                    provider,
                    event.provider_event_id,
                    event.trade_no,
                    event.amount_minor,
                    event.currency,
                    event.payment_method,
                    event.event_type,
                    event.signature_key_fingerprint,
                    payload_sha256,
                    received_at,
                )
                .map_err(|_| PaymentWebhookHandlerError::Invariant)?;
                return self
                    .topup_repository
                    .accept_verified_event(write)
                    .await
                    .map(map_topup_outcome)
                    .map_err(map_topup_route_error);
            }

            let write = SubscriptionPaymentEventWrite::new(
                SubscriptionPaymentEventId::new(event_id)
                    .map_err(|_| PaymentWebhookHandlerError::Invariant)?,
                subscription_id,
                provider,
                event.provider_event_id,
                event.trade_no,
                Some(event.amount_minor),
                Some(event.currency),
                Some(event.payment_method),
                map_subscription_event_type(event.event_type),
                event.signature_key_fingerprint,
                payload_sha256,
                received_at,
            )
            .map_err(|_| PaymentWebhookHandlerError::Invariant)?;
            self.subscription_repository
                .accept_verified_event(&write)
                .await
                .map(map_subscription_outcome)
                .map_err(map_subscription_route_error)
        })
    }
}

/// 统一 Provider 验签与订单类型路由，Provider 适配器不直接写订阅状态。
#[derive(Clone)]
pub struct PaymentWebhookProcessor {
    verifier: Arc<dyn PaymentWebhookVerifier>,
    router: Arc<dyn PaymentWebhookEventRouter>,
}

impl PaymentWebhookProcessor {
    #[must_use]
    pub fn new(
        verifier: Arc<dyn PaymentWebhookVerifier>,
        router: Arc<dyn PaymentWebhookEventRouter>,
    ) -> Self {
        Self { verifier, router }
    }

    pub async fn process(
        &self,
        request: PaymentWebhookRequest<'_>,
    ) -> Result<PaymentWebhookHandlerOutcome, PaymentWebhookHandlerError> {
        let verified = match self.verifier.verify(&request) {
            Ok(verified) => verified,
            Err(PaymentWebhookVerificationError::NonTerminalEvent) => {
                return Ok(PaymentWebhookHandlerOutcome::IgnoredNonTerminal);
            }
            Err(_) => return Err(PaymentWebhookHandlerError::VerificationRejected),
        };
        let payload_sha256: [u8; 32] = Sha256::digest(request.payload()).into();
        self.router
            .route(
                self.verifier.provider().to_owned(),
                verified,
                payload_sha256,
                request.received_at(),
            )
            .await
    }
}

impl fmt::Debug for PaymentWebhookProcessor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PaymentWebhookProcessor(<redacted>)")
    }
}

/// HTTP 层使用的统一 webhook handler，兼容旧充值处理器测试和新订单路由。
pub trait PaymentWebhookHandler: Send + Sync + 'static {
    fn handle<'a>(&'a self, request: PaymentWebhookRequest<'a>) -> PaymentWebhookHandlerFuture<'a>;
}

pub type PaymentWebhookHandlerFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PaymentWebhookHandlerOutcome, PaymentWebhookHandlerError>>
            + Send
            + 'a,
    >,
>;

impl PaymentWebhookHandler for PaymentWebhookProcessor {
    fn handle<'a>(&'a self, request: PaymentWebhookRequest<'a>) -> PaymentWebhookHandlerFuture<'a> {
        Box::pin(self.process(request))
    }
}

impl PaymentWebhookHandler for TopupWebhookProcessor {
    fn handle<'a>(&'a self, request: PaymentWebhookRequest<'a>) -> PaymentWebhookHandlerFuture<'a> {
        Box::pin(async move {
            self.process(request)
                .await
                .map(map_topup_webhook_outcome)
                .map_err(map_topup_processor_error)
        })
    }
}

fn map_topup_webhook_outcome(outcome: TopupWebhookOutcome) -> PaymentWebhookHandlerOutcome {
    match outcome {
        TopupWebhookOutcome::Applied(_) => PaymentWebhookHandlerOutcome::Applied,
        TopupWebhookOutcome::Acknowledged(_) => PaymentWebhookHandlerOutcome::Acknowledged,
        TopupWebhookOutcome::Existing(_) => PaymentWebhookHandlerOutcome::Existing,
        TopupWebhookOutcome::RecordedUnprocessed { .. } => {
            PaymentWebhookHandlerOutcome::RecordedUnprocessed
        }
        TopupWebhookOutcome::IgnoredNonTerminal => PaymentWebhookHandlerOutcome::IgnoredNonTerminal,
        TopupWebhookOutcome::NotFound => PaymentWebhookHandlerOutcome::NotFound,
    }
}

fn map_topup_processor_error(error: TopupWebhookProcessorError) -> PaymentWebhookHandlerError {
    match error {
        TopupWebhookProcessorError::VerificationRejected => {
            PaymentWebhookHandlerError::VerificationRejected
        }
        TopupWebhookProcessorError::Conflict => PaymentWebhookHandlerError::Conflict,
        TopupWebhookProcessorError::OutcomeUnknown => PaymentWebhookHandlerError::OutcomeUnknown,
        TopupWebhookProcessorError::Unavailable => PaymentWebhookHandlerError::Unavailable,
        TopupWebhookProcessorError::Invariant => PaymentWebhookHandlerError::Invariant,
    }
}

fn map_topup_outcome(outcome: TopupPaymentEventOutcome) -> PaymentWebhookHandlerOutcome {
    match outcome {
        TopupPaymentEventOutcome::Applied(_) => PaymentWebhookHandlerOutcome::Applied,
        TopupPaymentEventOutcome::Acknowledged(_) => PaymentWebhookHandlerOutcome::Acknowledged,
        TopupPaymentEventOutcome::Existing(_) => PaymentWebhookHandlerOutcome::Existing,
        TopupPaymentEventOutcome::RecordedUnprocessed { .. } => {
            PaymentWebhookHandlerOutcome::RecordedUnprocessed
        }
        TopupPaymentEventOutcome::NotFound => PaymentWebhookHandlerOutcome::NotFound,
    }
}

fn map_subscription_outcome(
    outcome: SubscriptionPaymentEventOutcome,
) -> PaymentWebhookHandlerOutcome {
    match outcome {
        SubscriptionPaymentEventOutcome::Applied { .. } => PaymentWebhookHandlerOutcome::Applied,
        SubscriptionPaymentEventOutcome::Acknowledged { .. } => {
            PaymentWebhookHandlerOutcome::Acknowledged
        }
        SubscriptionPaymentEventOutcome::Existing { .. } => PaymentWebhookHandlerOutcome::Existing,
        SubscriptionPaymentEventOutcome::RecordedUnprocessed { .. } => {
            PaymentWebhookHandlerOutcome::RecordedUnprocessed
        }
        SubscriptionPaymentEventOutcome::NotFound => PaymentWebhookHandlerOutcome::NotFound,
    }
}

fn map_subscription_event_type(event_type: TopupPaymentEventType) -> SubscriptionPaymentEventType {
    match event_type {
        TopupPaymentEventType::Succeeded => SubscriptionPaymentEventType::Succeeded,
        TopupPaymentEventType::Failed => SubscriptionPaymentEventType::Failed,
        TopupPaymentEventType::Expired => SubscriptionPaymentEventType::Expired,
    }
}

fn map_topup_route_error(error: TopupRepositoryError) -> PaymentWebhookHandlerError {
    match error {
        TopupRepositoryError::Conflict => PaymentWebhookHandlerError::Conflict,
        TopupRepositoryError::OutcomeUnknown => PaymentWebhookHandlerError::OutcomeUnknown,
        TopupRepositoryError::Query
        | TopupRepositoryError::Timeout
        | TopupRepositoryError::UnsupportedTarget => PaymentWebhookHandlerError::Unavailable,
        TopupRepositoryError::Invariant => PaymentWebhookHandlerError::Invariant,
    }
}

fn map_subscription_route_error(error: SubscriptionRepositoryError) -> PaymentWebhookHandlerError {
    match error {
        SubscriptionRepositoryError::Conflict => PaymentWebhookHandlerError::Conflict,
        SubscriptionRepositoryError::BindingConflict => PaymentWebhookHandlerError::BindingConflict,
        SubscriptionRepositoryError::OutcomeUnknown => PaymentWebhookHandlerError::OutcomeUnknown,
        SubscriptionRepositoryError::Query | SubscriptionRepositoryError::Timeout => {
            PaymentWebhookHandlerError::Unavailable
        }
        SubscriptionRepositoryError::Invariant => PaymentWebhookHandlerError::Invariant,
    }
}
