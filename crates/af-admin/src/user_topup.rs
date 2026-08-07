use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

use af_billing::{
    PaymentClientSecret, PaymentOrderProvider, PaymentOrderProviderError,
    PaymentOrderRecoveryRequest, PaymentOrderRequest, PaymentOrderSession, STRIPE_PAYMENT_PROVIDER,
    quota_math::quota_from_usd,
};
use af_db::{
    TopupOrderCreate, TopupOrderCreateOutcome, TopupOrderRecord, TopupOrderSubmission,
    TopupOrderSubmitOutcome, TopupRepository, TopupRepositoryError,
};
use af_domain::{Quota, TopupOrderId, TopupOrderStatus, TopupRequestId, UserId};
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

const USD_MINOR_UNITS: i64 = 100;
const TOPUP_IDENTIFIER_BYTES: usize = 16;
const REQUEST_ID_DOMAIN: &[u8] = b"anyflows:topup-request:v1";
const ORDER_ID_DOMAIN: &[u8] = b"anyflows:topup-order:v1";
const STRIPE_TOPUP_ORDER_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// 当前用户创建充值订单的结构化命令。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct UserTopupOrderCreateCommand {
    client_request_id: TopupRequestId,
    amount_minor: u64,
}

impl UserTopupOrderCreateCommand {
    /// 校验固定长度幂等键和 Stripe/USD 整数最小单位边界。
    pub fn new(idempotency_key: &str, amount_minor: i64) -> Result<Self, UserTopupError> {
        if !(MIN_STRIPE_TOPUP_AMOUNT_MINOR..=MAX_STRIPE_TOPUP_AMOUNT_MINOR).contains(&amount_minor)
        {
            return Err(UserTopupError::InvalidInput);
        }
        let client_request_id = TopupRequestId::from_persistence_key(idempotency_key)
            .map_err(|_| UserTopupError::InvalidInput)?;
        Ok(Self {
            client_request_id,
            amount_minor: u64::try_from(amount_minor).map_err(|_| UserTopupError::InvalidInput)?,
        })
    }

    /// 返回客户端为失败重试复用的原始幂等标识。
    #[must_use]
    pub const fn client_request_id(self) -> TopupRequestId {
        self.client_request_id
    }

    /// 返回待支付的整数美分金额。
    #[must_use]
    pub const fn amount_minor(self) -> u64 {
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
    const fn new(session: PaymentOrderSession) -> Self {
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

    /// 返回 Stripe PaymentIntent 标识；调用方不得写入日志。
    #[must_use]
    pub fn payment_intent_id(&self) -> &str {
        self.session.provider_order_id()
    }

    /// 返回仅供支付 SDK 使用的客户端密钥。
    #[must_use]
    pub fn client_secret(&self) -> &str {
        self.session.client_secret().expose()
    }
}

impl fmt::Debug for UserTopupPaymentSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTopupPaymentSession(<已脱敏>)")
    }
}

/// 当前用户可见的充值订单快照，不包含支付流水号。
pub struct UserTopupOrder {
    order_id: TopupOrderId,
    provider: String,
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
            provider,
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

    /// 返回规范支付 Provider 标识。
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
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
            provider: record.provider().to_owned(),
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
    /// 为当前会话用户幂等创建本地 Stripe 充值订单。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTopupOrderCreateCommand,
    ) -> UserTopupOrderCreateFuture<'a>;
}

/// 使用原子充值仓储和支付 Provider 实现订单创建与提交。
pub struct DatabaseUserTopupService {
    repository: TopupRepository,
    provider: Arc<dyn PaymentOrderProvider>,
    order_ttl: Duration,
}

impl DatabaseUserTopupService {
    /// 绑定充值仓储、Stripe Provider 与固定本地订单有效期。
    #[must_use]
    pub fn new(repository: TopupRepository, provider: Arc<dyn PaymentOrderProvider>) -> Self {
        Self {
            repository,
            provider,
            order_ttl: STRIPE_TOPUP_ORDER_TTL,
        }
    }
}

impl UserTopupService for DatabaseUserTopupService {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserTopupOrderCreateCommand,
    ) -> UserTopupOrderCreateFuture<'a> {
        Box::pin(async move {
            let (request_id, order_id) =
                scoped_identifiers(principal.user_id(), command.client_request_id())?;
            let quota_amount = quota_for_amount_minor(command.amount_minor())?;
            let write = TopupOrderCreate::new(
                order_id,
                request_id,
                principal.user_id(),
                STRIPE_PAYMENT_PROVIDER.to_owned(),
                command.amount_minor(),
                STRIPE_TOPUP_CURRENCY.to_owned(),
                quota_amount,
                unix_now()?,
            )
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
            self.prepare_payment(record, replayed).await
        })
    }
}

impl DatabaseUserTopupService {
    async fn prepare_payment(
        &self,
        record: TopupOrderRecord,
        replayed: bool,
    ) -> Result<UserTopupOrder, UserTopupError> {
        if record.provider() != self.provider.provider() {
            return Err(UserTopupError::Internal);
        }
        match record.status() {
            TopupOrderStatus::Created => self.submit_created_order(record, replayed).await,
            TopupOrderStatus::Pending => self.recover_pending_order(record, replayed).await,
            TopupOrderStatus::Paid
            | TopupOrderStatus::Failed
            | TopupOrderStatus::Canceled
            | TopupOrderStatus::Expired => Ok(UserTopupOrder::from_record(record, replayed, None)),
        }
    }

    async fn submit_created_order(
        &self,
        record: TopupOrderRecord,
        replayed: bool,
    ) -> Result<UserTopupOrder, UserTopupError> {
        let submitted_at = unix_now()?;
        let expires_at = record
            .created_at()
            .checked_add(self.order_ttl.as_secs())
            .ok_or(UserTopupError::Internal)?;
        if expires_at <= submitted_at {
            return Err(UserTopupError::Conflict);
        }
        let request = payment_request(&record)?;
        let session = self
            .provider
            .create(request)
            .await
            .map_err(map_provider_error)?;
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
        let session = self
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
    client_request_id: TopupRequestId,
) -> Result<(TopupRequestId, TopupOrderId), UserTopupError> {
    let request_bytes = derive_identifier(REQUEST_ID_DOMAIN, user_id, client_request_id);
    let order_bytes = derive_identifier(ORDER_ID_DOMAIN, user_id, client_request_id);
    Ok((
        TopupRequestId::new(request_bytes).map_err(|_| UserTopupError::Internal)?,
        TopupOrderId::new(order_bytes).map_err(|_| UserTopupError::Internal)?,
    ))
}

fn derive_identifier(
    domain: &[u8],
    user_id: UserId,
    client_request_id: TopupRequestId,
) -> [u8; TOPUP_IDENTIFIER_BYTES] {
    let mut digest = Sha256::new();
    digest.update(domain);
    digest.update(user_id.get().to_be_bytes());
    digest.update(client_request_id.bytes());
    let digest = digest.finalize();
    let mut identifier = [0_u8; TOPUP_IDENTIFIER_BYTES];
    identifier.copy_from_slice(&digest[..TOPUP_IDENTIFIER_BYTES]);
    identifier
}

fn quota_for_amount_minor(amount_minor: u64) -> Result<Quota, UserTopupError> {
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
    )
    .map_err(|_| UserTopupError::Internal)
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
        TopupRepositoryError::Query | TopupRepositoryError::Timeout => UserTopupError::Unavailable,
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
    fn command_rejects_invalid_key_and_stripe_amount_boundaries() {
        assert_eq!(
            UserTopupOrderCreateCommand::new("invalid", 500),
            Err(UserTopupError::InvalidInput)
        );
        assert_eq!(
            UserTopupOrderCreateCommand::new(IDEMPOTENCY_KEY, 49),
            Err(UserTopupError::InvalidInput)
        );
        assert_eq!(
            UserTopupOrderCreateCommand::new(IDEMPOTENCY_KEY, MAX_STRIPE_TOPUP_AMOUNT_MINOR + 1,),
            Err(UserTopupError::InvalidInput)
        );
        assert_eq!(
            UserTopupOrderCreateCommand::new(IDEMPOTENCY_KEY, 500)
                .unwrap()
                .amount_minor(),
            500
        );
    }

    #[test]
    fn usd_minor_units_use_the_shared_quota_conversion() {
        assert_eq!(quota_for_amount_minor(50).unwrap().units(), 250_000);
        assert_eq!(quota_for_amount_minor(1_000).unwrap().units(), 5_000_000);
    }

    #[test]
    fn idempotency_identifiers_are_stable_and_scoped_by_user() {
        let client = TopupRequestId::from_persistence_key(IDEMPOTENCY_KEY).unwrap();
        let first = scoped_identifiers(UserId::new(1).unwrap(), client).unwrap();
        let replay = scoped_identifiers(UserId::new(1).unwrap(), client).unwrap();
        let other_user = scoped_identifiers(UserId::new(2).unwrap(), client).unwrap();

        assert_eq!(first, replay);
        assert_ne!(first, other_user);
        assert_ne!(first.0.bytes(), first.1.bytes());
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
        let service = DatabaseUserTopupService::new(repository, provider.clone());
        let principal = SessionPrincipal::new(user_id, crate::SessionRole::Admin);
        let command = UserTopupOrderCreateCommand::new(IDEMPOTENCY_KEY, 500)?;

        let created = service.create(principal, command).await?;
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
            "pi_test_123_secret_client"
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
