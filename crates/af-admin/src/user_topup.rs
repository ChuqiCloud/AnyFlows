use std::{
    collections::HashMap,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

use af_billing::{
    EASYPAY_PAYMENT_PROVIDER, PaymentCheckoutAction, PaymentClientSecret, PaymentOrderProvider,
    PaymentOrderProviderError, PaymentOrderRecoveryRequest, PaymentOrderRequest,
    PaymentOrderSession, PaymentRedirectUrl, STRIPE_PAYMENT_PROVIDER,
    quota_math::{quota_from_cny_minor, quota_from_usd},
};
use af_db::{
    TopupOrderCreate, TopupOrderCreateOutcome, TopupOrderRecord, TopupOrderSubmission,
    TopupOrderSubmitOutcome, TopupRepository, TopupRepositoryError,
};
use af_domain::{OrganizationId, Quota, TopupOrderId, TopupOrderStatus, TopupRequestId, UserId};
use rust_decimal::Decimal;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::SessionPrincipal;

/// Stripe 首切片固定使用的结算币种。
pub const STRIPE_TOPUP_CURRENCY: &str = "USD";
/// Stripe 美元支付的官方最低非零金额，单位为美分。
pub const MIN_STRIPE_TOPUP_AMOUNT_MINOR: i64 = 50;
/// 为兼容 Stripe 非银行卡支付方式而采用的八位最小单位上限。
pub const MAX_STRIPE_TOPUP_AMOUNT_MINOR: i64 = 99_999_999;
/// 易支付首版允许的最小人民币金额，单位为分。
pub const MIN_EPAY_TOPUP_AMOUNT_MINOR: i64 = 1;
/// 易支付人民币金额上限，保持与订单整数边界一致。
pub const MAX_EPAY_TOPUP_AMOUNT_MINOR: i64 = 99_999_999;
/// Stripe 当前公开的支付方式。
pub const STRIPE_CARD_PAYMENT_METHOD: &str = "card";
/// 易支付首版支持的支付宝方式。
pub const EPAY_ALIPAY_PAYMENT_METHOD: &str = "alipay";
/// 易支付首版支持的微信支付方式。
pub const EPAY_WXPAY_PAYMENT_METHOD: &str = "wxpay";
/// 易支付固定使用人民币结算。
pub const EPAY_TOPUP_CURRENCY: &str = "CNY";

const USD_MINOR_UNITS: i64 = 100;
const TOPUP_IDENTIFIER_BYTES: usize = 16;
const MAX_STRIPE_PUBLISHABLE_KEY_BYTES: usize = 512;
const REQUEST_ID_DOMAIN: &[u8] = b"anyflows:topup-request:v1";
const ORDER_ID_DOMAIN: &[u8] = b"anyflows:topup-order:v1";
const STRIPE_TOPUP_ORDER_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// 当前用户创建充值订单的结构化命令。
#[derive(Clone, Eq, PartialEq)]
pub struct UserTopupOrderCreateCommand {
    client_request_id: TopupRequestId,
    organization_id: Option<OrganizationId>,
    provider: String,
    payment_method: String,
    amount_minor: u64,
}

impl UserTopupOrderCreateCommand {
    /// 校验固定长度幂等键、Provider、支付方式和整数最小单位边界。
    pub fn new(
        idempotency_key: &str,
        provider: String,
        payment_method: String,
        amount_minor: i64,
    ) -> Result<Self, UserTopupError> {
        if amount_minor <= 0 || !valid_route_part(&provider) || !valid_route_part(&payment_method) {
            return Err(UserTopupError::InvalidInput);
        }
        let client_request_id = TopupRequestId::from_persistence_key(idempotency_key)
            .map_err(|_| UserTopupError::InvalidInput)?;
        Ok(Self {
            client_request_id,
            organization_id: None,
            provider,
            payment_method,
            amount_minor: u64::try_from(amount_minor).map_err(|_| UserTopupError::InvalidInput)?,
        })
    }

    /// 构造企业收款主体的充值命令；付款人仍由服务端会话派生。
    pub fn new_for_organization(
        organization_id: OrganizationId,
        idempotency_key: &str,
        provider: String,
        payment_method: String,
        amount_minor: i64,
    ) -> Result<Self, UserTopupError> {
        let mut command = Self::new(idempotency_key, provider, payment_method, amount_minor)?;
        command.organization_id = Some(organization_id);
        Ok(command)
    }

    /// 返回客户端为失败重试复用的原始幂等标识。
    #[must_use]
    pub const fn client_request_id(&self) -> TopupRequestId {
        self.client_request_id
    }

    #[must_use]
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }

    /// 返回客户端选择的支付 Provider。
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// 返回客户端选择的支付方式。
    #[must_use]
    pub fn payment_method(&self) -> &str {
        &self.payment_method
    }

    /// 返回待支付的整数美分金额。
    #[must_use]
    pub const fn amount_minor(&self) -> u64 {
        self.amount_minor
    }
}

impl fmt::Debug for UserTopupOrderCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTopupOrderCreateCommand(<redacted>)")
    }
}

/// 当前用户可继续确认的 Stripe 支付会话。
pub struct UserTopupPaymentSession {
    session: PaymentOrderSession,
}

impl UserTopupPaymentSession {
    /// 组合只在公开 HTTPS 响应中出现的支付会话。
    #[must_use]
    pub(crate) const fn new(session: PaymentOrderSession) -> Self {
        Self { session }
    }

    /// 供替代服务实现和传输层测试构造已校验支付会话。
    pub fn from_parts(
        payment_intent_id: String,
        client_secret: String,
    ) -> Result<Self, UserTopupError> {
        let secret =
            PaymentClientSecret::new(client_secret).map_err(|_| UserTopupError::Internal)?;
        let session = PaymentOrderSession::new(payment_intent_id, secret)
            .map_err(|_| UserTopupError::Internal)?;
        Ok(Self::new(session))
    }

    /// 供替代服务实现和传输层测试构造托管收银台跳转。
    pub fn from_redirect_parts(
        provider_order_id: String,
        redirect_url: String,
    ) -> Result<Self, UserTopupError> {
        let redirect =
            PaymentRedirectUrl::new(redirect_url).map_err(|_| UserTopupError::Internal)?;
        let session = PaymentOrderSession::with_redirect_url(provider_order_id, redirect)
            .map_err(|_| UserTopupError::Internal)?;
        Ok(Self::new(session))
    }

    /// 返回 Stripe PaymentIntent 标识；调用方不得写入日志。
    #[must_use]
    pub fn payment_intent_id(&self) -> &str {
        self.session.provider_order_id()
    }

    /// 返回仅供支付 SDK 使用的客户端密钥。
    #[must_use]
    pub fn client_secret(&self) -> Option<&str> {
        self.session
            .client_secret()
            .map(PaymentClientSecret::expose)
    }

    /// 返回托管收银台跳转地址。
    #[must_use]
    pub fn redirect_url(&self) -> Option<&str> {
        self.session.redirect_url().map(PaymentRedirectUrl::expose)
    }

    /// 返回闭合支付动作，传输层据此生成带判别字段的响应。
    #[must_use]
    pub const fn action(&self) -> &PaymentCheckoutAction {
        self.session.action()
    }
}

impl fmt::Debug for UserTopupPaymentSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTopupPaymentSession(<已脱敏>)")
    }
}

/// 当前用户可选择的单个充值支付方式，不包含任何服务端密钥。
#[derive(Clone, Eq, PartialEq)]
pub struct UserTopupMethod {
    provider: String,
    payment_method: String,
    currency: String,
    min_amount_minor: i64,
    max_amount_minor: i64,
    publishable_key: Option<String>,
    qr_enabled: bool,
}

impl UserTopupMethod {
    /// 构造 Stripe.js 银行卡方式。
    pub fn stripe(publishable_key: String) -> Result<Self, UserTopupError> {
        if publishable_key.is_empty()
            || publishable_key.len() > MAX_STRIPE_PUBLISHABLE_KEY_BYTES
            || !publishable_key
                .bytes()
                .all(|byte| (0x21..=0x7e).contains(&byte))
            || !(publishable_key.starts_with("pk_test_") || publishable_key.starts_with("pk_live_"))
        {
            return Err(UserTopupError::InvalidInput);
        }
        Ok(Self {
            provider: STRIPE_PAYMENT_PROVIDER.to_owned(),
            payment_method: STRIPE_CARD_PAYMENT_METHOD.to_owned(),
            currency: STRIPE_TOPUP_CURRENCY.to_owned(),
            min_amount_minor: MIN_STRIPE_TOPUP_AMOUNT_MINOR,
            max_amount_minor: MAX_STRIPE_TOPUP_AMOUNT_MINOR,
            publishable_key: Some(publishable_key),
            qr_enabled: false,
        })
    }

    /// 构造易支付人民币方式。
    pub fn epay(payment_method: &str) -> Result<Self, UserTopupError> {
        Self::epay_with_qr(payment_method, false)
    }

    /// 构造易支付人民币方式，并声明是否由前端展示支付二维码。
    pub fn epay_with_qr(payment_method: &str, qr_enabled: bool) -> Result<Self, UserTopupError> {
        if !matches!(
            payment_method,
            EPAY_ALIPAY_PAYMENT_METHOD | EPAY_WXPAY_PAYMENT_METHOD
        ) {
            return Err(UserTopupError::InvalidInput);
        }
        Ok(Self {
            provider: EASYPAY_PAYMENT_PROVIDER.to_owned(),
            payment_method: payment_method.to_owned(),
            currency: EPAY_TOPUP_CURRENCY.to_owned(),
            min_amount_minor: MIN_EPAY_TOPUP_AMOUNT_MINOR,
            max_amount_minor: MAX_EPAY_TOPUP_AMOUNT_MINOR,
            publishable_key: None,
            qr_enabled,
        })
    }

    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    #[must_use]
    pub fn payment_method(&self) -> &str {
        &self.payment_method
    }

    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }

    #[must_use]
    pub const fn min_amount_minor(&self) -> i64 {
        self.min_amount_minor
    }

    #[must_use]
    pub const fn max_amount_minor(&self) -> i64 {
        self.max_amount_minor
    }

    #[must_use]
    pub fn publishable_key(&self) -> Option<&str> {
        self.publishable_key.as_deref()
    }

    #[must_use]
    pub const fn qr_enabled(&self) -> bool {
        self.qr_enabled
    }
}

/// 当前用户可读取的充值方式目录。
#[derive(Clone, Eq, PartialEq)]
pub struct UserTopupConfiguration {
    methods: Vec<UserTopupMethod>,
}

impl UserTopupConfiguration {
    pub fn new(methods: Vec<UserTopupMethod>) -> Result<Self, UserTopupError> {
        if methods.is_empty() {
            return Err(UserTopupError::InvalidInput);
        }
        Ok(Self { methods })
    }

    #[must_use]
    pub fn methods(&self) -> &[UserTopupMethod] {
        &self.methods
    }
}

impl fmt::Debug for UserTopupConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTopupConfiguration(<已脱敏>)")
    }
}

/// 当前用户可见的充值订单快照，不包含支付流水号。
pub struct UserTopupOrder {
    order_id: TopupOrderId,
    organization_id: Option<OrganizationId>,
    provider: String,
    payment_method: String,
    status: TopupOrderStatus,
    amount_minor: u64,
    currency: String,
    quota_amount: Quota,
    version: u64,
    created_at: u64,
    replayed: bool,
    payment: Option<UserTopupPaymentSession>,
}

impl UserTopupOrder {
    /// 组合经过应用边界校验的订单快照，供替代实现和传输层测试使用。
    #[allow(
        clippy::too_many_arguments,
        reason = "参数与对外充值订单快照字段一一对应"
    )]
    #[must_use]
    pub fn from_parts(
        order_id: TopupOrderId,
        provider: String,
        payment_method: String,
        status: TopupOrderStatus,
        amount_minor: u64,
        currency: String,
        quota_amount: Quota,
        version: u64,
        created_at: u64,
        replayed: bool,
        payment: Option<UserTopupPaymentSession>,
    ) -> Self {
        Self {
            order_id,
            organization_id: None,
            provider,
            payment_method,
            status,
            amount_minor,
            currency,
            quota_amount,
            version,
            created_at,
            replayed,
            payment,
        }
    }

    /// 返回稳定本地订单标识。
    #[must_use]
    pub const fn order_id(&self) -> TopupOrderId {
        self.order_id
    }

    #[must_use]
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }

    /// 返回规范支付 Provider 标识。
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// 返回订单锁定的支付方式。
    #[must_use]
    pub fn payment_method(&self) -> &str {
        &self.payment_method
    }

    /// 返回当前闭合订单状态。
    #[must_use]
    pub const fn status(&self) -> TopupOrderStatus {
        self.status
    }

    /// 返回待支付的整数最小单位金额。
    #[must_use]
    pub const fn amount_minor(&self) -> u64 {
        self.amount_minor
    }

    /// 返回三位大写结算币种。
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }

    /// 返回支付成功后应增加的钱包额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回订单状态机当前版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回首次创建时间 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回本次调用是否恢复了已有幂等事实。
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }

    /// 返回待确认的支付会话；终态订单不会携带该字段。
    #[must_use]
    pub const fn payment(&self) -> Option<&UserTopupPaymentSession> {
        self.payment.as_ref()
    }

    fn from_record(
        record: TopupOrderRecord,
        replayed: bool,
        payment: Option<UserTopupPaymentSession>,
    ) -> Self {
        Self {
            order_id: record.order_id(),
            organization_id: record.organization_id(),
            provider: record.provider().to_owned(),
            payment_method: record.payment_method().unwrap_or_default().to_owned(),
            status: record.status(),
            amount_minor: record.amount_minor(),
            currency: record.currency().to_owned(),
            quota_amount: record.quota_amount(),
            version: record.version(),
            created_at: record.created_at(),
            replayed,
            payment,
        }
    }
}

impl fmt::Debug for UserTopupOrder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTopupOrder(<redacted>)")
    }
}

/// 当前用户充值订单创建错误；不携带用户、金额或幂等标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserTopupError {
    /// 幂等键或金额不满足公开边界。
    #[error("充值订单请求无效")]
    InvalidInput,
    /// 当前会话用户已经不存在或失效。
    #[error("充值订单会话无效")]
    InvalidSession,
    /// 相同幂等键已经绑定不同充值事实。
    #[error("充值订单幂等事实冲突")]
    Conflict,
    /// 数据库提交结果未知，调用方必须复用同一幂等键确认。
    #[error("充值订单提交结果未知")]
    OutcomeUnknown,
    /// 数据库暂时无法完成充值订单写入。
    #[error("充值订单服务暂不可用")]
    Unavailable,
    /// Stripe 确定拒绝了服务端构造的支付请求。
    #[error("充值支付请求被 Provider 拒绝")]
    ProviderRejected,
    /// 时钟、额度换算或持久化不变量失败。
    #[error("充值订单内部失败")]
    Internal,
}

/// 当前用户充值订单创建的对象安全 Future。
pub type UserTopupOrderCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserTopupOrder, UserTopupError>> + Send + 'a>>;

/// 当前用户充值订单应用端口；订单所有权只能从会话主体推导。
pub trait UserTopupService: Send + Sync {
    /// 返回当前用户可读取的支付方式目录。
    fn configuration(&self) -> Result<UserTopupConfiguration, UserTopupError>;

    /// 为当前会话用户幂等创建本地 Stripe 充值订单。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTopupOrderCreateCommand,
    ) -> UserTopupOrderCreateFuture<'a>;

    /// 按已配置的 Provider 和支付方式返回支付订单适配器。
    fn provider_for(
        &self,
        provider: &str,
        payment_method: &str,
    ) -> Result<Arc<dyn PaymentOrderProvider>, UserTopupError> {
        let _ = (provider, payment_method);
        Err(UserTopupError::Unavailable)
    }
}

/// 使用原子充值仓储和支付 Provider 实现订单创建与提交。
pub struct DatabaseUserTopupService {
    repository: TopupRepository,
    routes: HashMap<String, UserTopupProviderRoute>,
    configuration: UserTopupConfiguration,
    order_ttl: Duration,
}

/// 单个公开充值方式绑定的 Provider 和额度换算策略。
pub struct UserTopupProviderRoute {
    method: UserTopupMethod,
    provider: Arc<dyn PaymentOrderProvider>,
    quota_per_cny: Option<i64>,
}

impl UserTopupProviderRoute {
    /// 绑定 Stripe 银行卡 Provider。
    pub fn stripe(
        publishable_key: String,
        provider: Arc<dyn PaymentOrderProvider>,
    ) -> Result<Self, UserTopupError> {
        Ok(Self {
            method: UserTopupMethod::stripe(publishable_key)?,
            provider,
            quota_per_cny: None,
        })
    }

    /// 绑定易支付方式和每人民币额度换算基数。
    pub fn epay(
        payment_method: &str,
        quota_per_cny: i64,
        provider: Arc<dyn PaymentOrderProvider>,
    ) -> Result<Self, UserTopupError> {
        Self::epay_with_qr(payment_method, false, quota_per_cny, provider)
    }

    /// 绑定易支付方式、二维码展示策略和每人民币额度换算基数。
    pub fn epay_with_qr(
        payment_method: &str,
        qr_enabled: bool,
        quota_per_cny: i64,
        provider: Arc<dyn PaymentOrderProvider>,
    ) -> Result<Self, UserTopupError> {
        if quota_per_cny <= 0 {
            return Err(UserTopupError::InvalidInput);
        }
        Ok(Self {
            method: UserTopupMethod::epay_with_qr(payment_method, qr_enabled)?,
            provider,
            quota_per_cny: Some(quota_per_cny),
        })
    }

    /// 使用已校验公开方式和 Provider 构造运行时路由。
    pub fn from_method(
        method: UserTopupMethod,
        provider: Arc<dyn PaymentOrderProvider>,
        quota_per_cny: Option<i64>,
    ) -> Result<Self, UserTopupError> {
        if provider.provider() != method.provider()
            || (method.provider() == EASYPAY_PAYMENT_PROVIDER
                && quota_per_cny.is_none_or(|value| value <= 0))
            || (method.provider() == STRIPE_PAYMENT_PROVIDER && quota_per_cny.is_some())
        {
            return Err(UserTopupError::InvalidInput);
        }
        Ok(Self {
            method,
            provider,
            quota_per_cny,
        })
    }

    fn key(&self) -> String {
        route_key(self.method.provider(), self.method.payment_method())
    }
}

impl fmt::Debug for UserTopupProviderRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTopupProviderRoute(<已脱敏>)")
    }
}

impl DatabaseUserTopupService {
    /// 绑定充值仓储、支付方式目录与固定本地订单有效期。
    pub fn new(
        repository: TopupRepository,
        routes: Vec<UserTopupProviderRoute>,
    ) -> Result<Self, UserTopupError> {
        let mut route_map = HashMap::with_capacity(routes.len());
        let mut methods = Vec::with_capacity(routes.len());
        for route in routes {
            if route.provider.provider() != route.method.provider()
                || route_map.insert(route.key(), route).is_some()
            {
                return Err(UserTopupError::InvalidInput);
            }
        }
        for route in route_map.values() {
            methods.push(route.method.clone());
        }
        methods.sort_by(|left, right| {
            (left.provider(), left.payment_method())
                .cmp(&(right.provider(), right.payment_method()))
        });
        Ok(Self {
            repository,
            routes: route_map,
            configuration: UserTopupConfiguration::new(methods)?,
            order_ttl: STRIPE_TOPUP_ORDER_TTL,
        })
    }
}

impl UserTopupService for DatabaseUserTopupService {
    fn configuration(&self) -> Result<UserTopupConfiguration, UserTopupError> {
        Ok(self.configuration.clone())
    }

    fn provider_for(
        &self,
        provider: &str,
        payment_method: &str,
    ) -> Result<Arc<dyn PaymentOrderProvider>, UserTopupError> {
        self.routes
            .get(&route_key(provider, payment_method))
            .map(|route| Arc::clone(&route.provider))
            .ok_or(UserTopupError::Unavailable)
    }

    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTopupOrderCreateCommand,
    ) -> UserTopupOrderCreateFuture<'a> {
        Box::pin(async move {
            let route = self
                .routes
                .get(&route_key(command.provider(), command.payment_method()))
                .ok_or(UserTopupError::InvalidInput)?;
            if command.amount_minor() < route.method.min_amount_minor() as u64
                || command.amount_minor() > route.method.max_amount_minor() as u64
            {
                return Err(UserTopupError::InvalidInput);
            }
            let (request_id, order_id) = scoped_identifiers(
                principal.user_id(),
                command.organization_id(),
                command.client_request_id(),
                command.provider(),
                command.payment_method(),
            )?;
            let quota_amount = quota_for_route(route, command.amount_minor())?;
            let created_at = unix_now()?;
            let write = if let Some(organization_id) = command.organization_id() {
                TopupOrderCreate::new_for_organization(
                    order_id,
                    request_id,
                    principal.user_id(),
                    organization_id,
                    command.provider().to_owned(),
                    command.payment_method().to_owned(),
                    command.amount_minor(),
                    route.method.currency().to_owned(),
                    quota_amount,
                    created_at,
                )
            } else {
                TopupOrderCreate::new(
                    order_id,
                    request_id,
                    principal.user_id(),
                    command.provider().to_owned(),
                    command.payment_method().to_owned(),
                    command.amount_minor(),
                    route.method.currency().to_owned(),
                    quota_amount,
                    created_at,
                )
            }
            .map_err(|_| UserTopupError::Internal)?;
            let (record, replayed) = match self
                .repository
                .create_order(write)
                .await
                .map_err(map_repository_error)?
            {
                TopupOrderCreateOutcome::Created(record) => (record, false),
                TopupOrderCreateOutcome::Existing(record) => (record, true),
                TopupOrderCreateOutcome::NotFound => return Err(UserTopupError::InvalidSession),
            };
            self.prepare_payment(route, record, replayed).await
        })
    }
}

impl DatabaseUserTopupService {
    async fn prepare_payment(
        &self,
        route: &UserTopupProviderRoute,
        record: TopupOrderRecord,
        replayed: bool,
    ) -> Result<UserTopupOrder, UserTopupError> {
        if record.provider() != route.provider.provider()
            || record.payment_method() != Some(route.method.payment_method())
        {
            return Err(UserTopupError::Internal);
        }
        match record.status() {
            TopupOrderStatus::Created => self.submit_created_order(route, record, replayed).await,
            TopupOrderStatus::Pending => self.recover_pending_order(route, record, replayed).await,
            TopupOrderStatus::Paid
            | TopupOrderStatus::Failed
            | TopupOrderStatus::Canceled
            | TopupOrderStatus::Expired => Ok(UserTopupOrder::from_record(record, replayed, None)),
        }
    }

    async fn submit_created_order(
        &self,
        route: &UserTopupProviderRoute,
        record: TopupOrderRecord,
        replayed: bool,
    ) -> Result<UserTopupOrder, UserTopupError> {
        let started_at = unix_now()?;
        let expires_at = record
            .created_at()
            .checked_add(self.order_ttl.as_secs())
            .ok_or(UserTopupError::Internal)?;
        if expires_at <= started_at {
            return Err(UserTopupError::Conflict);
        }
        let request = payment_request(&record)?;
        let session = route
            .provider
            .create(request)
            .await
            .map_err(map_provider_error)?;
        let submitted_at = unix_now()?;
        if expires_at <= submitted_at {
            return Err(UserTopupError::Conflict);
        }
        let submission = TopupOrderSubmission::new(
            record.order_id(),
            record.version(),
            session.provider_order_id().to_owned(),
            submitted_at,
            expires_at,
        )
        .map_err(|_| UserTopupError::Internal)?;
        let submitted = match self
            .repository
            .submit_order(submission)
            .await
            .map_err(map_repository_error)?
        {
            TopupOrderSubmitOutcome::Applied(record)
            | TopupOrderSubmitOutcome::Existing(record) => record,
            TopupOrderSubmitOutcome::NotFound => return Err(UserTopupError::Internal),
        };
        if submitted.provider_order_id() != Some(session.provider_order_id())
            || submitted.expires_at() != Some(expires_at)
        {
            return Err(UserTopupError::Internal);
        }
        Ok(UserTopupOrder::from_record(
            submitted,
            replayed,
            Some(UserTopupPaymentSession::new(session)),
        ))
    }

    async fn recover_pending_order(
        &self,
        route: &UserTopupProviderRoute,
        record: TopupOrderRecord,
        replayed: bool,
    ) -> Result<UserTopupOrder, UserTopupError> {
        let provider_order_id = record
            .provider_order_id()
            .ok_or(UserTopupError::Internal)?
            .to_owned();
        record.expires_at().ok_or(UserTopupError::Internal)?;
        let request =
            PaymentOrderRecoveryRequest::new(payment_request(&record)?, provider_order_id)
                .map_err(|_| UserTopupError::Internal)?;
        let session = route
            .provider
            .recover(request)
            .await
            .map_err(map_provider_error)?;
        Ok(UserTopupOrder::from_record(
            record,
            replayed,
            Some(UserTopupPaymentSession::new(session)),
        ))
    }
}

impl fmt::Debug for DatabaseUserTopupService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUserTopupService(<redacted>)")
    }
}

fn scoped_identifiers(
    user_id: UserId,
    organization_id: Option<OrganizationId>,
    client_request_id: TopupRequestId,
    provider: &str,
    payment_method: &str,
) -> Result<(TopupRequestId, TopupOrderId), UserTopupError> {
    let request_bytes = derive_identifier(
        REQUEST_ID_DOMAIN,
        user_id,
        organization_id,
        client_request_id,
        provider,
        payment_method,
    );
    let order_bytes = derive_identifier(
        ORDER_ID_DOMAIN,
        user_id,
        organization_id,
        client_request_id,
        provider,
        payment_method,
    );
    Ok((
        TopupRequestId::new(request_bytes).map_err(|_| UserTopupError::Internal)?,
        TopupOrderId::new(order_bytes).map_err(|_| UserTopupError::Internal)?,
    ))
}

fn derive_identifier(
    domain: &[u8],
    user_id: UserId,
    organization_id: Option<OrganizationId>,
    client_request_id: TopupRequestId,
    provider: &str,
    payment_method: &str,
) -> [u8; TOPUP_IDENTIFIER_BYTES] {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(user_id.get().to_be_bytes());
    if let Some(organization_id) = organization_id {
        digest.update(b"organization");
        digest.update(organization_id.get().to_be_bytes());
    }
    digest.update(client_request_id.bytes());
    digest.update([0]);
    digest.update(provider.as_bytes());
    digest.update([0]);
    digest.update(payment_method.as_bytes());
    let digest = digest.finalize();
    let mut identifier = [0_u8; TOPUP_IDENTIFIER_BYTES];
    identifier.copy_from_slice(&digest[..TOPUP_IDENTIFIER_BYTES]);
    identifier
}

fn quota_for_route(
    route: &UserTopupProviderRoute,
    amount_minor: u64,
) -> Result<Quota, UserTopupError> {
    if let Some(quota_per_cny) = route.quota_per_cny {
        let quota = quota_from_cny_minor(amount_minor, quota_per_cny)
            .map_err(|_| UserTopupError::Internal)?;
        return (!quota.is_zero())
            .then_some(quota)
            .ok_or(UserTopupError::Internal);
    }
    let usd = Decimal::from(amount_minor)
        .checked_div(Decimal::from(USD_MINOR_UNITS))
        .ok_or(UserTopupError::Internal)?;
    let quota = quota_from_usd(usd).map_err(|_| UserTopupError::Internal)?;
    if quota.is_zero() {
        return Err(UserTopupError::Internal);
    }
    Ok(quota)
}

fn payment_request(record: &TopupOrderRecord) -> Result<PaymentOrderRequest, UserTopupError> {
    PaymentOrderRequest::new(
        record.order_id(),
        record.amount_minor(),
        record.currency().to_owned(),
        record
            .payment_method()
            .ok_or(UserTopupError::Internal)?
            .to_owned(),
    )
    .map_err(|_| UserTopupError::Internal)
}

fn route_key(provider: &str, payment_method: &str) -> String {
    format!("{provider}\0{payment_method}")
}

fn valid_route_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn unix_now() -> Result<u64, UserTopupError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| UserTopupError::Internal)
}

fn map_repository_error(error: TopupRepositoryError) -> UserTopupError {
    match error {
        TopupRepositoryError::Conflict => UserTopupError::Conflict,
        TopupRepositoryError::OutcomeUnknown => UserTopupError::OutcomeUnknown,
        TopupRepositoryError::Query
        | TopupRepositoryError::Timeout
        | TopupRepositoryError::UnsupportedTarget => UserTopupError::Unavailable,
        TopupRepositoryError::Invariant => UserTopupError::Internal,
    }
}

fn map_provider_error(error: PaymentOrderProviderError) -> UserTopupError {
    match error {
        PaymentOrderProviderError::Rejected => UserTopupError::ProviderRejected,
        PaymentOrderProviderError::OutcomeUnknown => UserTopupError::OutcomeUnknown,
        PaymentOrderProviderError::Unavailable => UserTopupError::Unavailable,
        PaymentOrderProviderError::InvalidResponse => UserTopupError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use af_billing::{PaymentOrderFuture, PaymentOrderInputError};
    use af_db::{
        DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
        MigrationOptions,
    };

    use super::*;

    const IDEMPOTENCY_KEY: &str = "1234567890abcdef1234567890abcdef";

    #[test]
    fn command_rejects_invalid_key_route_and_non_positive_amount() {
        assert_eq!(
            UserTopupOrderCreateCommand::new(
                "invalid",
                "stripe".to_owned(),
                "card".to_owned(),
                500,
            ),
            Err(UserTopupError::InvalidInput)
        );
        assert_eq!(
            UserTopupOrderCreateCommand::new(
                IDEMPOTENCY_KEY,
                "Stripe".to_owned(),
                "card".to_owned(),
                500,
            ),
            Err(UserTopupError::InvalidInput)
        );
        assert_eq!(
            UserTopupOrderCreateCommand::new(
                IDEMPOTENCY_KEY,
                "stripe".to_owned(),
                "card".to_owned(),
                0,
            ),
            Err(UserTopupError::InvalidInput)
        );
        assert_eq!(
            UserTopupOrderCreateCommand::new(
                IDEMPOTENCY_KEY,
                "stripe".to_owned(),
                "card".to_owned(),
                500,
            )
            .unwrap()
            .amount_minor(),
            500
        );
    }

    #[test]
    fn public_configuration_exposes_only_client_material() {
        let stripe = UserTopupMethod::stripe("pk_test_public".to_owned()).unwrap();
        let epay = UserTopupMethod::epay(EPAY_ALIPAY_PAYMENT_METHOD).unwrap();
        let configuration = UserTopupConfiguration::new(vec![stripe, epay]).unwrap();
        assert_eq!(configuration.methods().len(), 2);
        assert_eq!(
            configuration.methods()[0].publishable_key(),
            Some("pk_test_public")
        );
        assert_eq!(configuration.methods()[1].publishable_key(), None);
        assert!(matches!(
            UserTopupMethod::stripe("sk_test_secret".to_owned()),
            Err(UserTopupError::InvalidInput)
        ));
        assert!(!format!("{configuration:?}").contains("pk_test_public"));
    }

    #[test]
    fn idempotency_identifiers_include_provider_and_payment_method() {
        let client = TopupRequestId::from_persistence_key(IDEMPOTENCY_KEY).unwrap();
        let stripe =
            scoped_identifiers(UserId::new(1).unwrap(), None, client, "stripe", "card").unwrap();
        let replay =
            scoped_identifiers(UserId::new(1).unwrap(), None, client, "stripe", "card").unwrap();
        let alipay =
            scoped_identifiers(UserId::new(1).unwrap(), None, client, "epay", "alipay").unwrap();
        let other_user =
            scoped_identifiers(UserId::new(2).unwrap(), None, client, "stripe", "card").unwrap();

        assert_eq!(stripe, replay);
        assert_ne!(stripe, alipay);
        assert_ne!(stripe, other_user);
        assert_ne!(stripe.0.bytes(), stripe.1.bytes());
    }

    #[tokio::test]
    async fn created_order_is_submitted_once_and_pending_replay_recovers_it()
    -> Result<(), Box<dyn Error>> {
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let InitialSetupOutcome::Initialized { user_id } =
            InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
                .initialize(InitialSetupRecord::new(
                    "topup-owner".to_owned(),
                    "topup secure password".to_owned(),
                ))
                .await?
        else {
            panic!("空数据库必须完成首次安装");
        };
        let provider = Arc::new(RecordingPaymentProvider::default());
        let repository = TopupRepository::new(pool, Duration::from_secs(5))?;
        let service = DatabaseUserTopupService::new(
            repository,
            vec![UserTopupProviderRoute::stripe(
                "pk_test_public".to_owned(),
                provider.clone(),
            )?],
        )?;
        let principal = SessionPrincipal::new(user_id, crate::SessionRole::Admin);
        let command = UserTopupOrderCreateCommand::new(
            IDEMPOTENCY_KEY,
            "stripe".to_owned(),
            "card".to_owned(),
            500,
        )?;

        let created = service.create(principal, command.clone()).await?;
        assert_eq!(created.status(), TopupOrderStatus::Pending);
        assert!(!created.replayed());
        assert_eq!(
            created.payment().unwrap().payment_intent_id(),
            "pi_test_123"
        );

        let replay = service.create(principal, command).await?;
        assert_eq!(replay.status(), TopupOrderStatus::Pending);
        assert!(replay.replayed());
        assert_eq!(
            replay.payment().unwrap().client_secret(),
            Some("pi_test_123_secret_client")
        );
        assert_eq!(provider.create_calls.load(Ordering::Acquire), 1);
        assert_eq!(provider.recover_calls.load(Ordering::Acquire), 1);
        Ok(())
    }

    #[derive(Default)]
    struct RecordingPaymentProvider {
        create_calls: AtomicUsize,
        recover_calls: AtomicUsize,
    }

    impl PaymentOrderProvider for RecordingPaymentProvider {
        fn provider(&self) -> &str {
            STRIPE_PAYMENT_PROVIDER
        }

        fn create<'a>(&'a self, request: PaymentOrderRequest) -> PaymentOrderFuture<'a> {
            self.create_calls.fetch_add(1, Ordering::AcqRel);
            assert_eq!(request.amount_minor(), 500);
            assert_eq!(request.currency(), STRIPE_TOPUP_CURRENCY);
            assert_eq!(request.payment_method(), STRIPE_CARD_PAYMENT_METHOD);
            Box::pin(async {
                payment_session().map_err(|_| PaymentOrderProviderError::InvalidResponse)
            })
        }

        fn recover<'a>(&'a self, request: PaymentOrderRecoveryRequest) -> PaymentOrderFuture<'a> {
            self.recover_calls.fetch_add(1, Ordering::AcqRel);
            assert_eq!(request.provider_order_id(), "pi_test_123");
            assert_eq!(request.order().amount_minor(), 500);
            Box::pin(async {
                payment_session().map_err(|_| PaymentOrderProviderError::InvalidResponse)
            })
        }
    }

    fn payment_session() -> Result<PaymentOrderSession, PaymentOrderInputError> {
        PaymentOrderSession::new(
            "pi_test_123".to_owned(),
            PaymentClientSecret::new("pi_test_123_secret_client".to_owned())?,
        )
    }
}
