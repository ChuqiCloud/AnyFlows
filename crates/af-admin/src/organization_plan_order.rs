use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_billing::{
    PaymentOrderId, PaymentOrderProvider, PaymentOrderProviderError, PaymentOrderRecoveryRequest,
    PaymentOrderRequest, PaymentOrderSession,
};
use af_db::{
    MAX_ORGANIZATION_PLAN_ORDER_PAYMENT_METHOD_BYTES, OrganizationPlanOrderRecord,
    OrganizationPlanOrderRepository, OrganizationPlanOrderRepositoryError,
    OrganizationPlanOrderSubmission, OrganizationPlanOrderSubmitOutcome,
};
use af_domain::{OrganizationId, OrganizationPlanOrderId, OrganizationPlanOrderStatus};
use thiserror::Error;

use crate::{SessionPrincipal, UserTopupPaymentSession};

/// 企业授权订单支付提交命令；企业和订单标识必须由服务端路由上下文共同提供。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationPlanOrderPaymentCommand {
    organization_id: OrganizationId,
    order_id: OrganizationPlanOrderId,
    payment_method: String,
}

impl OrganizationPlanOrderPaymentCommand {
    /// 校验支付方式的稳定小写标识并绑定企业订单范围。
    pub fn new(
        organization_id: OrganizationId,
        order_id: OrganizationPlanOrderId,
        payment_method: String,
    ) -> Result<Self, OrganizationPlanOrderPaymentError> {
        if !valid_payment_method(&payment_method) {
            return Err(OrganizationPlanOrderPaymentError::InvalidInput);
        }
        Ok(Self {
            organization_id,
            order_id,
            payment_method,
        })
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn order_id(&self) -> OrganizationPlanOrderId {
        self.order_id
    }

    #[must_use]
    pub fn payment_method(&self) -> &str {
        &self.payment_method
    }
}

impl fmt::Debug for OrganizationPlanOrderPaymentCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanOrderPaymentCommand(<redacted>)")
    }
}

/// 企业授权订单的支付会话结果；Provider 标识和短期支付材料只在内存中暴露。
pub struct OrganizationPlanOrderPayment {
    order: OrganizationPlanOrderRecord,
    payment: UserTopupPaymentSession,
    replayed: bool,
}

impl OrganizationPlanOrderPayment {
    fn new(
        order: OrganizationPlanOrderRecord,
        payment: PaymentOrderSession,
        replayed: bool,
    ) -> Self {
        Self {
            order,
            payment: UserTopupPaymentSession::new(payment),
            replayed,
        }
    }

    #[must_use]
    pub const fn order(&self) -> &OrganizationPlanOrderRecord {
        &self.order
    }

    #[must_use]
    pub const fn payment(&self) -> &UserTopupPaymentSession {
        &self.payment
    }

    /// 标记本次结果是否来自同一企业订单的恢复路径。
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }

    /// 将已校验的订单事实和短期支付材料交给上层安全投影。
    #[must_use]
    pub(crate) fn into_parts(self) -> (OrganizationPlanOrderRecord, UserTopupPaymentSession, bool) {
        (self.order, self.payment, self.replayed)
    }
}

impl fmt::Debug for OrganizationPlanOrderPayment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanOrderPayment(<redacted>)")
    }
}

/// 企业授权订单支付编排错误；不携带 Provider 标识、订单号或支付材料。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanOrderPaymentError {
    #[error("企业授权订单支付输入无效")]
    InvalidInput,
    #[error("企业授权订单不存在")]
    OrderNotFound,
    #[error("企业授权订单状态或事实冲突")]
    Conflict,
    #[error("支付 Provider 拒绝企业授权订单")]
    PaymentRejected,
    #[error("支付 Provider 暂不可用")]
    PaymentUnavailable,
    #[error("企业授权订单支付结果未知")]
    OutcomeUnknown,
    #[error("企业授权订单支付内部失败")]
    Internal,
}

/// 企业授权订单支付提交的对象安全 Future。
pub type OrganizationPlanOrderPaymentFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationPlanOrderPayment, OrganizationPlanOrderPaymentError>>
            + Send
            + 'a,
    >,
>;

/// 企业授权订单支付提交端口；不负责确认支付或发放授权。
pub trait OrganizationPlanOrderPaymentService: Send + Sync {
    /// 创建或恢复同一企业订单的 Provider 支付会话。
    fn submit<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: OrganizationPlanOrderPaymentCommand,
        provider: Arc<dyn PaymentOrderProvider>,
    ) -> OrganizationPlanOrderPaymentFuture<'a>;
}

/// 使用企业订单仓储和现有 Provider 端口完成支付提交/恢复。
pub struct DatabaseOrganizationPlanOrderPaymentService {
    repository: OrganizationPlanOrderRepository,
}

impl DatabaseOrganizationPlanOrderPaymentService {
    /// 使用共享企业订单仓储构造支付编排服务。
    #[must_use]
    pub const fn new(repository: OrganizationPlanOrderRepository) -> Self {
        Self { repository }
    }
}

impl OrganizationPlanOrderPaymentService for DatabaseOrganizationPlanOrderPaymentService {
    fn submit<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: OrganizationPlanOrderPaymentCommand,
        provider: Arc<dyn PaymentOrderProvider>,
    ) -> OrganizationPlanOrderPaymentFuture<'a> {
        Box::pin(async move {
            let record = self
                .repository
                .get_order(command.organization_id(), command.order_id())
                .await
                .map_err(map_repository_error)?
                .filter(|record| record.payer_user_id() == principal.user_id())
                .ok_or(OrganizationPlanOrderPaymentError::OrderNotFound)?;
            if record.provider() != provider.provider() {
                return Err(OrganizationPlanOrderPaymentError::Conflict);
            }

            match record.status() {
                OrganizationPlanOrderStatus::Created => {
                    submit_created(&self.repository, record, &command, provider).await
                }
                OrganizationPlanOrderStatus::Pending => {
                    recover_pending(record, &command, provider).await
                }
                OrganizationPlanOrderStatus::Paid
                | OrganizationPlanOrderStatus::Failed
                | OrganizationPlanOrderStatus::Canceled
                | OrganizationPlanOrderStatus::Expired => {
                    Err(OrganizationPlanOrderPaymentError::Conflict)
                }
            }
        })
    }
}

impl fmt::Debug for DatabaseOrganizationPlanOrderPaymentService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationPlanOrderPaymentService(<redacted>)")
    }
}

async fn submit_created(
    repository: &OrganizationPlanOrderRepository,
    record: OrganizationPlanOrderRecord,
    command: &OrganizationPlanOrderPaymentCommand,
    provider: Arc<dyn PaymentOrderProvider>,
) -> Result<OrganizationPlanOrderPayment, OrganizationPlanOrderPaymentError> {
    let expires_at = record
        .expires_at()
        .ok_or(OrganizationPlanOrderPaymentError::Conflict)?;
    let now = unix_now()?;
    if record.created_at() > now || expires_at <= now {
        return Err(OrganizationPlanOrderPaymentError::Conflict);
    }
    let request = payment_request(&record, command.payment_method())?;
    let session = provider.create(request).await.map_err(map_provider_error)?;
    let submitted_at = unix_now()?;
    if submitted_at >= expires_at {
        return Err(OrganizationPlanOrderPaymentError::Conflict);
    }
    let write = OrganizationPlanOrderSubmission::new(
        command.organization_id(),
        record.order_id(),
        record.version(),
        provider.provider().to_owned(),
        session.provider_order_id().to_owned(),
        command.payment_method().to_owned(),
        submitted_at,
        expires_at,
    )
    .map_err(|_| OrganizationPlanOrderPaymentError::Internal)?;
    let (submitted, replayed) = recover_submission(repository, write).await?;
    if submitted.provider_order_id() != Some(session.provider_order_id())
        || submitted.payment_method() != Some(command.payment_method())
        || submitted.expires_at() != Some(expires_at)
    {
        return Err(OrganizationPlanOrderPaymentError::Conflict);
    }
    Ok(OrganizationPlanOrderPayment::new(
        submitted, session, replayed,
    ))
}

async fn recover_pending(
    record: OrganizationPlanOrderRecord,
    command: &OrganizationPlanOrderPaymentCommand,
    provider: Arc<dyn PaymentOrderProvider>,
) -> Result<OrganizationPlanOrderPayment, OrganizationPlanOrderPaymentError> {
    if record.payment_method() != Some(command.payment_method()) {
        return Err(OrganizationPlanOrderPaymentError::Conflict);
    }
    // Pending 订单必须保留首次提交时的有效期；缺失说明持久化事实损坏，不能继续发起支付。
    if record.expires_at().is_none() {
        return Err(OrganizationPlanOrderPaymentError::Internal);
    }
    let provider_order_id = record
        .provider_order_id()
        .ok_or(OrganizationPlanOrderPaymentError::Internal)?
        .to_owned();
    let request = payment_request(&record, command.payment_method())?;
    let session = provider
        .recover(
            PaymentOrderRecoveryRequest::new(request, provider_order_id.clone())
                .map_err(|_| OrganizationPlanOrderPaymentError::Internal)?,
        )
        .await
        .map_err(map_provider_error)?;
    if session.provider_order_id() != provider_order_id {
        return Err(OrganizationPlanOrderPaymentError::Conflict);
    }
    Ok(OrganizationPlanOrderPayment::new(record, session, true))
}

async fn recover_submission(
    repository: &OrganizationPlanOrderRepository,
    write: OrganizationPlanOrderSubmission,
) -> Result<(OrganizationPlanOrderRecord, bool), OrganizationPlanOrderPaymentError> {
    match repository.submit_order(write.clone()).await {
        Ok(OrganizationPlanOrderSubmitOutcome::Applied(record)) => Ok((record, false)),
        Ok(OrganizationPlanOrderSubmitOutcome::Existing(record)) => Ok((record, true)),
        Ok(OrganizationPlanOrderSubmitOutcome::NotFound) => {
            Err(OrganizationPlanOrderPaymentError::OrderNotFound)
        }
        Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown) => {
            match repository.submit_order(write).await {
                Ok(OrganizationPlanOrderSubmitOutcome::Applied(record)) => Ok((record, true)),
                Ok(OrganizationPlanOrderSubmitOutcome::Existing(record)) => Ok((record, true)),
                Ok(OrganizationPlanOrderSubmitOutcome::NotFound) => {
                    Err(OrganizationPlanOrderPaymentError::OrderNotFound)
                }
                Err(error) => Err(map_repository_error(error)),
            }
        }
        Err(error) => Err(map_repository_error(error)),
    }
}

fn payment_request(
    record: &OrganizationPlanOrderRecord,
    payment_method: &str,
) -> Result<PaymentOrderRequest, OrganizationPlanOrderPaymentError> {
    let amount_minor = u64::try_from(record.amount_minor())
        .map_err(|_| OrganizationPlanOrderPaymentError::Internal)?;
    PaymentOrderRequest::new(
        PaymentOrderId::OrganizationPlanOrder(record.order_id()),
        amount_minor,
        record.currency().to_owned(),
        payment_method.to_owned(),
    )
    .map_err(|_| OrganizationPlanOrderPaymentError::InvalidInput)
}

fn map_repository_error(
    error: OrganizationPlanOrderRepositoryError,
) -> OrganizationPlanOrderPaymentError {
    match error {
        OrganizationPlanOrderRepositoryError::InvalidInput => {
            OrganizationPlanOrderPaymentError::InvalidInput
        }
        OrganizationPlanOrderRepositoryError::Conflict => {
            OrganizationPlanOrderPaymentError::Conflict
        }
        OrganizationPlanOrderRepositoryError::OutcomeUnknown => {
            OrganizationPlanOrderPaymentError::OutcomeUnknown
        }
        OrganizationPlanOrderRepositoryError::Query
        | OrganizationPlanOrderRepositoryError::Timeout => {
            OrganizationPlanOrderPaymentError::PaymentUnavailable
        }
        OrganizationPlanOrderRepositoryError::Invariant => {
            OrganizationPlanOrderPaymentError::Internal
        }
    }
}

fn map_provider_error(error: PaymentOrderProviderError) -> OrganizationPlanOrderPaymentError {
    match error {
        PaymentOrderProviderError::Rejected => OrganizationPlanOrderPaymentError::PaymentRejected,
        PaymentOrderProviderError::OutcomeUnknown => {
            OrganizationPlanOrderPaymentError::OutcomeUnknown
        }
        PaymentOrderProviderError::Unavailable => {
            OrganizationPlanOrderPaymentError::PaymentUnavailable
        }
        PaymentOrderProviderError::InvalidResponse => OrganizationPlanOrderPaymentError::Internal,
    }
}

fn valid_payment_method(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ORGANIZATION_PLAN_ORDER_PAYMENT_METHOD_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn unix_now() -> Result<u64, OrganizationPlanOrderPaymentError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| OrganizationPlanOrderPaymentError::Internal)
}

#[cfg(test)]
mod tests {
    use std::{
        error::Error,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
    };

    use super::*;
    use af_billing::{PaymentClientSecret, PaymentOrderFuture, PaymentOrderProvider};
    use af_db::{
        AdminGroupRepository, AdminGroupWriteRecord, AdminUserCreateRecord, AdminUserRepository,
        DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
        OrganizationPlanOrderCreate, OrganizationPlanOrderCreateOutcome,
        OrganizationPlanOrderRepository, OrganizationPlanRepository, OrganizationPlanWrite,
        OrganizationRepository,
    };
    use af_domain::{
        OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
        OrganizationPlanFeatures, OrganizationPlanId, OrganizationPlanOrderId,
        OrganizationPlanOrderRequestId, OrganizationPlanPrice, OrganizationPlanSnapshot, UserId,
    };
    #[tokio::test]
    async fn submit_retries_unknown_provider_result_and_recovers_pending_order()
    -> Result<(), Box<dyn Error>> {
        let now = unix_now().expect("测试时钟有效");
        let (pool, repository, organization_id, payer_user_id, order_id) = fixture(now).await?;
        let provider = Arc::new(RecordingProvider::default());
        provider.fail_create_once.store(true, Ordering::Release);
        let service = DatabaseOrganizationPlanOrderPaymentService::new(repository.clone());
        let principal = SessionPrincipal::new(payer_user_id, crate::SessionRole::Admin);
        let command =
            OrganizationPlanOrderPaymentCommand::new(organization_id, order_id, "card".to_owned())?;

        assert!(matches!(
            service
                .submit(principal, command.clone(), provider.clone())
                .await,
            Err(OrganizationPlanOrderPaymentError::OutcomeUnknown)
        ));
        let submitted = service
            .submit(principal, command.clone(), provider.clone())
            .await?;
        assert_eq!(
            submitted.order().status(),
            OrganizationPlanOrderStatus::Pending
        );
        assert_eq!(submitted.order().provider_order_id(), Some("org_pi_1"));
        assert_eq!(submitted.order().payment_method(), Some("card"));
        assert!(!submitted.replayed());

        let recovered = service.submit(principal, command, provider.clone()).await?;
        assert!(recovered.replayed());
        assert_eq!(provider.create_calls.load(Ordering::Acquire), 2);
        assert_eq!(provider.recover_calls.load(Ordering::Acquire), 1);
        assert_eq!(
            provider.order_keys.lock().expect("测试锁未中毒").as_slice(),
            &[order_id.persistence_key(), order_id.persistence_key()]
        );

        let other_principal = SessionPrincipal::new(
            UserId::new(99).expect("用户标识有效"),
            crate::SessionRole::Admin,
        );
        assert!(matches!(
            service
                .submit(
                    other_principal,
                    OrganizationPlanOrderPaymentCommand::new(
                        organization_id,
                        order_id,
                        "card".to_owned(),
                    )?,
                    provider,
                )
                .await,
            Err(OrganizationPlanOrderPaymentError::OrderNotFound)
        ));
        pool.close().await?;
        Ok(())
    }

    #[derive(Default)]
    struct RecordingProvider {
        fail_create_once: AtomicBool,
        create_calls: AtomicUsize,
        recover_calls: AtomicUsize,
        order_keys: Mutex<Vec<String>>,
    }

    impl PaymentOrderProvider for RecordingProvider {
        fn provider(&self) -> &str {
            "stripe"
        }

        fn create<'a>(&'a self, request: PaymentOrderRequest) -> PaymentOrderFuture<'a> {
            self.create_calls.fetch_add(1, Ordering::AcqRel);
            self.order_keys
                .lock()
                .expect("测试锁未中毒")
                .push(request.order_id().persistence_key());
            if self.fail_create_once.swap(false, Ordering::AcqRel) {
                return Box::pin(async { Err(PaymentOrderProviderError::OutcomeUnknown) });
            }
            Box::pin(async { payment_session() })
        }

        fn recover<'a>(&'a self, request: PaymentOrderRecoveryRequest) -> PaymentOrderFuture<'a> {
            self.recover_calls.fetch_add(1, Ordering::AcqRel);
            assert_eq!(request.provider_order_id(), "org_pi_1");
            Box::pin(async { payment_session() })
        }
    }

    fn payment_session() -> Result<PaymentOrderSession, PaymentOrderProviderError> {
        PaymentOrderSession::new(
            "org_pi_1".to_owned(),
            PaymentClientSecret::new("org_pi_1_secret".to_owned())
                .map_err(|_| PaymentOrderProviderError::InvalidResponse)?,
        )
        .map_err(|_| PaymentOrderProviderError::InvalidResponse)
    }

    async fn fixture(
        now: u64,
    ) -> Result<
        (
            af_db::DatabasePool,
            OrganizationPlanOrderRepository,
            OrganizationId,
            UserId,
            OrganizationPlanOrderId,
        ),
        Box<dyn Error>,
    > {
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let group = AdminGroupRepository::new(pool.clone(), std::time::Duration::from_secs(5))?
            .create(AdminGroupWriteRecord::new(
                "organization-plan-payment-test".to_owned(),
                "企业支付测试".to_owned(),
                1_000_000,
                None,
                false,
                None,
                None,
                None,
                None,
                None,
                serde_json::json!({}),
            ))
            .await?;
        let payer_user_id =
            AdminUserRepository::new(pool.clone(), std::time::Duration::from_secs(5))?
                .create(AdminUserCreateRecord::new(
                    "organization-plan-payment-user".to_owned(),
                    Some("organization-plan-payment@example.com".to_owned()),
                    None,
                    0,
                    1,
                    group.group_id(),
                    0,
                    None,
                    None,
                ))
                .await?
                .user_id();
        let now = i64::try_from(now)?;
        let entitlement = OrganizationEntitlementWrite::new(
            OrganizationEntitlementSnapshot::new(
                OrganizationEntitlementCapacity::new(10, 3, 2)?,
                true,
                false,
                false,
                90,
                now - 60,
                now + 86_400,
                Some(now + 172_800),
            )?,
            "contract".to_owned(),
            None,
            None,
        )?;
        let organization =
            OrganizationRepository::new(pool.clone(), std::time::Duration::from_secs(5))?
                .create(OrganizationCreate::new(
                    "企业支付测试".to_owned(),
                    "organization-plan-payment".to_owned(),
                    payer_user_id,
                    entitlement,
                ))
                .await?
                .into_parts()
                .0;
        let plan_id = OrganizationPlanId::new([0x41; 16])?;
        OrganizationPlanRepository::new(pool.clone(), std::time::Duration::from_secs(5))?
            .create(OrganizationPlanWrite::new(
                plan_id,
                "支付套餐".to_owned(),
                None,
                OrganizationPlanSnapshot::new(
                    OrganizationPlanFeatures::new(true, false, false),
                    10,
                    3,
                    2,
                    90,
                    30,
                )?,
                OrganizationPlanPrice::new("stripe".to_owned(), "USD".to_owned(), 500)?,
                payer_user_id,
                u64::try_from(now)?,
            )?)
            .await?;
        let order_id = OrganizationPlanOrderId::new([0x42; 16])?;
        let create = OrganizationPlanOrderCreate::new(
            order_id,
            OrganizationPlanOrderRequestId::new([0x43; 16])?,
            organization.id(),
            payer_user_id,
            plan_id,
            1,
            "stripe".to_owned(),
            "USD".to_owned(),
            500,
            u64::try_from(now)?,
            Some(u64::try_from(now + 3_600)?),
        )?;
        let repository =
            OrganizationPlanOrderRepository::new(pool.clone(), std::time::Duration::from_secs(5))?;
        assert!(matches!(
            repository.create_order(create).await?,
            OrganizationPlanOrderCreateOutcome::Created(_)
        ));
        Ok((pool, repository, organization.id(), payer_user_id, order_id))
    }
}
