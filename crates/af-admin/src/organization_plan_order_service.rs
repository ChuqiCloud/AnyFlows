use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationPlanOrderCreate, OrganizationPlanOrderCreateOutcome, OrganizationPlanOrderRecord,
    OrganizationPlanOrderRepository, OrganizationPlanOrderRepositoryError, OrganizationRepository,
};
use af_domain::{
    OrganizationId, OrganizationPermission, OrganizationPlanId, OrganizationPlanOrderId,
    OrganizationPlanOrderKind, OrganizationPlanOrderRequestId, OrganizationPlanOrderStatus, UserId,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    DatabaseOrganizationEntitlementReader, DatabaseOrganizationPlanOrderPaymentService,
    OrganizationActivityService, OrganizationAuditRequestContext, OrganizationAuthorizationError,
    OrganizationAuthorizationService, OrganizationEntitlementServiceError,
    OrganizationEntitlementView, OrganizationPlanOrderPaymentCommand,
    OrganizationPlanOrderPaymentError, OrganizationPlanOrderPaymentService,
    OrganizationPlanService, OrganizationPlanServiceError, SessionPrincipal, UserTopupError,
    UserTopupPaymentSession, UserTopupService,
};

const ORDER_ID_DOMAIN: &[u8] = b"anyflows:organization-plan-order:v1";
const ORGANIZATION_PLAN_ORDER_TTL_SECONDS: u64 = 30 * 60;

/// 企业授权订单创建命令；价格、币种和 Provider 必须由服务端套餐快照补齐。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationPlanOrderCreateCommand {
    plan_id: OrganizationPlanId,
    request_id: OrganizationPlanOrderRequestId,
    kind: OrganizationPlanOrderKind,
    effective_at: Option<u64>,
}

impl OrganizationPlanOrderCreateCommand {
    #[must_use]
    pub const fn new(
        plan_id: OrganizationPlanId,
        request_id: OrganizationPlanOrderRequestId,
    ) -> Self {
        Self {
            plan_id,
            request_id,
            kind: OrganizationPlanOrderKind::Initial,
            effective_at: None,
        }
    }

    /// 创建续费、升级或降级订单；生效时间由服务端持久化并在确认时复验。
    #[must_use]
    pub const fn new_with_change(
        plan_id: OrganizationPlanId,
        request_id: OrganizationPlanOrderRequestId,
        kind: OrganizationPlanOrderKind,
        effective_at: Option<u64>,
    ) -> Self {
        Self {
            plan_id,
            request_id,
            kind,
            effective_at,
        }
    }

    #[must_use]
    pub const fn plan_id(self) -> OrganizationPlanId {
        self.plan_id
    }

    #[must_use]
    pub const fn request_id(self) -> OrganizationPlanOrderRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn kind(self) -> OrganizationPlanOrderKind {
        self.kind
    }
    #[must_use]
    pub const fn effective_at(self) -> Option<u64> {
        self.effective_at
    }
}

/// 可安全交给企业控制台的授权订单投影；不包含 Provider 订单号或交易流水号。
pub struct OrganizationPlanOrderView {
    order_id: OrganizationPlanOrderId,
    organization_id: OrganizationId,
    payer_user_id: UserId,
    plan_id: OrganizationPlanId,
    plan_version: u64,
    provider: String,
    currency: String,
    amount_minor: i64,
    kind: OrganizationPlanOrderKind,
    effective_at: Option<u64>,
    status: OrganizationPlanOrderStatus,
    version: u64,
    payment_method: Option<String>,
    expires_at: Option<u64>,
    paid_at: Option<u64>,
    closed_at: Option<u64>,
    created_at: u64,
    updated_at: u64,
    replayed: bool,
}

impl OrganizationPlanOrderView {
    fn from_record(record: OrganizationPlanOrderRecord, replayed: bool) -> Self {
        Self {
            order_id: record.order_id(),
            organization_id: record.organization_id(),
            payer_user_id: record.payer_user_id(),
            plan_id: record.plan_id(),
            plan_version: record.plan_version(),
            provider: record.provider().to_owned(),
            currency: record.currency().to_owned(),
            amount_minor: record.amount_minor(),
            kind: record.kind(),
            effective_at: record.effective_at(),
            status: record.status(),
            version: record.version(),
            payment_method: record.payment_method().map(str::to_owned),
            expires_at: record.expires_at(),
            paid_at: record.paid_at(),
            closed_at: record.closed_at(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            replayed,
        }
    }

    #[must_use]
    pub const fn order_id(&self) -> OrganizationPlanOrderId {
        self.order_id
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
    pub const fn kind(&self) -> OrganizationPlanOrderKind {
        self.kind
    }
    #[must_use]
    pub const fn effective_at(&self) -> Option<u64> {
        self.effective_at
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
    /// 创建响应是否恢复了同一幂等事实。
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl fmt::Debug for OrganizationPlanOrderView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanOrderView(<redacted>)")
    }
}

/// 企业控制台支付提交结果；订单投影不暴露 Provider 订单号或交易流水号。
pub struct OrganizationPlanOrderPaymentView {
    order: OrganizationPlanOrderView,
    payment: UserTopupPaymentSession,
}

impl OrganizationPlanOrderPaymentView {
    fn from_payment(payment: crate::OrganizationPlanOrderPayment) -> Self {
        let (order, payment, replayed) = payment.into_parts();
        Self {
            order: OrganizationPlanOrderView::from_record(order, replayed),
            payment,
        }
    }

    #[must_use]
    pub const fn order(&self) -> &OrganizationPlanOrderView {
        &self.order
    }

    #[must_use]
    pub const fn payment(&self) -> &UserTopupPaymentSession {
        &self.payment
    }
}

impl fmt::Debug for OrganizationPlanOrderPaymentView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationPlanOrderPaymentView(<redacted>)")
    }
}

/// 企业授权订单应用层错误；跨企业探测统一折叠为不存在。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationPlanOrderServiceError {
    #[error("企业授权订单请求无效")]
    InvalidRequest,
    #[error("当前企业角色无权管理授权订单")]
    Forbidden,
    #[error("企业授权订单或套餐不存在")]
    NotFound,
    #[error("企业授权订单幂等事实冲突")]
    Conflict,
    #[error("支付 Provider 拒绝企业授权订单")]
    PaymentRejected,
    #[error("企业授权订单写入结果未知")]
    OutcomeUnknown,
    #[error("企业授权订单服务暂不可用")]
    Unavailable,
    #[error("企业授权订单内部失败")]
    Internal,
}

pub type OrganizationPlanOrderFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationPlanOrderView, OrganizationPlanOrderServiceError>>
            + Send
            + 'a,
    >,
>;

pub type OrganizationPlanOrderPaymentViewFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationPlanOrderPaymentView,
                    OrganizationPlanOrderServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
/// 企业授权投影读取结果。
pub type OrganizationEntitlementFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationEntitlementView, OrganizationEntitlementServiceError>,
            > + Send
            + 'a,
    >,
>;

/// 企业授权订单与授权生效投影端口；支付确认和授权发放仍由验签事件负责。
pub trait OrganizationPlanOrderService: Send + Sync {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationPlanOrderCreateCommand,
    ) -> OrganizationPlanOrderFuture<'a>;

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        order_id: OrganizationPlanOrderId,
    ) -> OrganizationPlanOrderFuture<'a>;

    /// 复验付款人企业权限后创建或恢复同一订单的支付会话。
    fn submit_payment<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: OrganizationPlanOrderPaymentCommand,
    ) -> OrganizationPlanOrderPaymentViewFuture<'a>;

    /// 仅允许 Owner 读取路径企业的授权投影。
    fn get_entitlement<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
    ) -> OrganizationEntitlementFuture<'a>;

    /// 平台管理员说明理由后跨企业读取授权投影。
    fn get_entitlement_as_platform<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        platform_reason: String,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationEntitlementFuture<'a>;
}

/// 使用企业 RBAC、活动套餐目录和幂等仓储实现订单创建与读取。
pub struct DatabaseOrganizationPlanOrderService {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    plan_service: Arc<dyn OrganizationPlanService>,
    topup_service: Arc<dyn UserTopupService>,
    repository: OrganizationPlanOrderRepository,
    payment_service: DatabaseOrganizationPlanOrderPaymentService,
    entitlement_reader: DatabaseOrganizationEntitlementReader,
}

impl DatabaseOrganizationPlanOrderService {
    #[must_use]
    pub fn new(
        authorization_service: Arc<dyn OrganizationAuthorizationService>,
        plan_service: Arc<dyn OrganizationPlanService>,
        topup_service: Arc<dyn UserTopupService>,
        repository: OrganizationPlanOrderRepository,
        organization_repository: Arc<OrganizationRepository>,
        activity_service: Arc<dyn OrganizationActivityService>,
    ) -> Self {
        let payment_service = DatabaseOrganizationPlanOrderPaymentService::new(repository.clone());
        let entitlement_reader = DatabaseOrganizationEntitlementReader::new(
            Arc::clone(&authorization_service),
            activity_service,
            organization_repository,
            repository.clone(),
        );
        Self {
            authorization_service,
            plan_service,
            topup_service,
            repository,
            payment_service,
            entitlement_reader,
        }
    }
}

impl OrganizationPlanOrderService for DatabaseOrganizationPlanOrderService {
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationPlanOrderCreateCommand,
    ) -> OrganizationPlanOrderFuture<'a> {
        Box::pin(async move {
            self.authorization_service
                .context(
                    principal,
                    organization_id,
                    OrganizationPermission::WalletManage,
                )
                .await
                .map_err(map_authorization_error)?;

            let order_id = scoped_order_id(principal, organization_id, command)?;
            if let Some(existing) = self
                .repository
                .get_order(organization_id, order_id)
                .await
                .map_err(map_repository_error)?
            {
                return replay_existing(existing, principal, command);
            }

            let catalog = self
                .plan_service
                .list_catalog(principal)
                .await
                .map_err(map_plan_error)?;
            let plan = catalog
                .plans()
                .iter()
                .find(|plan| plan.plan_id() == command.plan_id())
                .ok_or(OrganizationPlanOrderServiceError::NotFound)?;
            let payment_configuration = self
                .topup_service
                .configuration()
                .map_err(map_topup_error)?;
            if !payment_configuration.methods().iter().any(|method| {
                method.provider() == plan.provider()
                    && method.currency() == plan.currency()
                    && plan.amount_minor() >= method.min_amount_minor()
                    && plan.amount_minor() <= method.max_amount_minor()
            }) {
                return Err(OrganizationPlanOrderServiceError::NotFound);
            }
            let created_at = unix_now()?;
            let expires_at = created_at
                .checked_add(ORGANIZATION_PLAN_ORDER_TTL_SECONDS)
                .ok_or(OrganizationPlanOrderServiceError::Internal)?;
            let effective_at = match command.kind() {
                OrganizationPlanOrderKind::Initial | OrganizationPlanOrderKind::Renewal => {
                    if command.effective_at().is_some() {
                        return Err(OrganizationPlanOrderServiceError::InvalidRequest);
                    }
                    None
                }
                OrganizationPlanOrderKind::Upgrade | OrganizationPlanOrderKind::Downgrade => {
                    let effective_at = command
                        .effective_at()
                        .ok_or(OrganizationPlanOrderServiceError::InvalidRequest)?;
                    if effective_at < created_at {
                        return Err(OrganizationPlanOrderServiceError::InvalidRequest);
                    }
                    Some(effective_at)
                }
            };
            let plan_version = u64::try_from(plan.version())
                .map_err(|_| OrganizationPlanOrderServiceError::Internal)?;
            let write = OrganizationPlanOrderCreate::new_with_change(
                order_id,
                command.request_id(),
                organization_id,
                principal.user_id(),
                plan.plan_id(),
                plan_version,
                plan.provider().to_owned(),
                plan.currency().to_owned(),
                plan.amount_minor(),
                command.kind(),
                effective_at,
                created_at,
                Some(expires_at),
            )
            .map_err(|_| OrganizationPlanOrderServiceError::Internal)?;

            match self.repository.create_order(write).await {
                Ok(OrganizationPlanOrderCreateOutcome::Created(record)) => {
                    Ok(OrganizationPlanOrderView::from_record(record, false))
                }
                Ok(OrganizationPlanOrderCreateOutcome::Existing(record)) => {
                    replay_existing(record, principal, command)
                }
                Ok(OrganizationPlanOrderCreateOutcome::NotFound) => {
                    Err(OrganizationPlanOrderServiceError::NotFound)
                }
                Err(OrganizationPlanOrderRepositoryError::OutcomeUnknown) => {
                    let recovered = self
                        .repository
                        .get_order(organization_id, order_id)
                        .await
                        .map_err(map_repository_error)?
                        .ok_or(OrganizationPlanOrderServiceError::OutcomeUnknown)?;
                    replay_existing(recovered, principal, command)
                }
                Err(error) => Err(map_repository_error(error)),
            }
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        order_id: OrganizationPlanOrderId,
    ) -> OrganizationPlanOrderFuture<'a> {
        Box::pin(async move {
            self.authorization_service
                .context(
                    principal,
                    organization_id,
                    OrganizationPermission::WalletRead,
                )
                .await
                .map_err(map_authorization_error)?;
            self.repository
                .get_order(organization_id, order_id)
                .await
                .map_err(map_repository_error)?
                .map(|record| OrganizationPlanOrderView::from_record(record, false))
                .ok_or(OrganizationPlanOrderServiceError::NotFound)
        })
    }

    fn submit_payment<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: OrganizationPlanOrderPaymentCommand,
    ) -> OrganizationPlanOrderPaymentViewFuture<'a> {
        Box::pin(async move {
            self.authorization_service
                .context(
                    principal,
                    command.organization_id(),
                    OrganizationPermission::WalletManage,
                )
                .await
                .map_err(map_authorization_error)?;

            // Provider 只能由路径企业内、当前付款人的已锁定订单事实决定。
            let order = self
                .repository
                .get_order(command.organization_id(), command.order_id())
                .await
                .map_err(map_repository_error)?
                .filter(|order| order.payer_user_id() == principal.user_id())
                .ok_or(OrganizationPlanOrderServiceError::NotFound)?;
            let provider = self
                .topup_service
                .provider_for(order.provider(), command.payment_method())
                .map_err(map_topup_error)?;
            self.payment_service
                .submit(principal, command, provider)
                .await
                .map(OrganizationPlanOrderPaymentView::from_payment)
                .map_err(map_payment_error)
        })
    }

    fn get_entitlement<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
    ) -> OrganizationEntitlementFuture<'a> {
        Box::pin(async move {
            self.entitlement_reader
                .get_for_owner(principal, organization_id)
                .await
        })
    }

    fn get_entitlement_as_platform<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        platform_reason: String,
        request: OrganizationAuditRequestContext,
    ) -> OrganizationEntitlementFuture<'a> {
        Box::pin(async move {
            self.entitlement_reader
                .get_as_platform(principal, organization_id, platform_reason, request)
                .await
        })
    }
}

impl fmt::Debug for DatabaseOrganizationPlanOrderService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationPlanOrderService(<redacted>)")
    }
}

fn replay_existing(
    record: OrganizationPlanOrderRecord,
    principal: SessionPrincipal,
    command: OrganizationPlanOrderCreateCommand,
) -> Result<OrganizationPlanOrderView, OrganizationPlanOrderServiceError> {
    if record.payer_user_id() != principal.user_id()
        || record.plan_id() != command.plan_id()
        || record.request_id() != command.request_id()
        || record.kind() != command.kind()
        || record.effective_at() != command.effective_at()
    {
        return Err(OrganizationPlanOrderServiceError::Conflict);
    }
    Ok(OrganizationPlanOrderView::from_record(record, true))
}

fn scoped_order_id(
    principal: SessionPrincipal,
    organization_id: OrganizationId,
    command: OrganizationPlanOrderCreateCommand,
) -> Result<OrganizationPlanOrderId, OrganizationPlanOrderServiceError> {
    let mut digest = Sha256::new();
    digest.update(ORDER_ID_DOMAIN);
    digest.update(organization_id.get().to_be_bytes());
    digest.update(principal.user_id().get().to_be_bytes());
    digest.update(command.plan_id().persistence_key().as_bytes());
    digest.update(command.request_id().bytes());
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    OrganizationPlanOrderId::new(bytes).map_err(|_| OrganizationPlanOrderServiceError::Internal)
}

fn unix_now() -> Result<u64, OrganizationPlanOrderServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| OrganizationPlanOrderServiceError::Internal)
}

fn map_authorization_error(
    error: OrganizationAuthorizationError,
) -> OrganizationPlanOrderServiceError {
    match error {
        OrganizationAuthorizationError::NotFound => OrganizationPlanOrderServiceError::NotFound,
        OrganizationAuthorizationError::Forbidden => OrganizationPlanOrderServiceError::Forbidden,
        OrganizationAuthorizationError::Internal => OrganizationPlanOrderServiceError::Unavailable,
    }
}

fn map_plan_error(error: OrganizationPlanServiceError) -> OrganizationPlanOrderServiceError {
    match error {
        OrganizationPlanServiceError::InvalidRequest => {
            OrganizationPlanOrderServiceError::InvalidRequest
        }
        OrganizationPlanServiceError::Forbidden => OrganizationPlanOrderServiceError::Forbidden,
        OrganizationPlanServiceError::NotFound => OrganizationPlanOrderServiceError::NotFound,
        OrganizationPlanServiceError::Conflict => OrganizationPlanOrderServiceError::Conflict,
        OrganizationPlanServiceError::Internal => OrganizationPlanOrderServiceError::Unavailable,
    }
}

fn map_topup_error(_error: UserTopupError) -> OrganizationPlanOrderServiceError {
    OrganizationPlanOrderServiceError::Unavailable
}

fn map_payment_error(
    error: OrganizationPlanOrderPaymentError,
) -> OrganizationPlanOrderServiceError {
    match error {
        OrganizationPlanOrderPaymentError::InvalidInput => {
            OrganizationPlanOrderServiceError::InvalidRequest
        }
        OrganizationPlanOrderPaymentError::OrderNotFound => {
            OrganizationPlanOrderServiceError::NotFound
        }
        OrganizationPlanOrderPaymentError::Conflict => OrganizationPlanOrderServiceError::Conflict,
        OrganizationPlanOrderPaymentError::PaymentRejected => {
            OrganizationPlanOrderServiceError::PaymentRejected
        }
        OrganizationPlanOrderPaymentError::PaymentUnavailable => {
            OrganizationPlanOrderServiceError::Unavailable
        }
        OrganizationPlanOrderPaymentError::OutcomeUnknown => {
            OrganizationPlanOrderServiceError::OutcomeUnknown
        }
        OrganizationPlanOrderPaymentError::Internal => OrganizationPlanOrderServiceError::Internal,
    }
}

fn map_repository_error(
    error: OrganizationPlanOrderRepositoryError,
) -> OrganizationPlanOrderServiceError {
    match error {
        OrganizationPlanOrderRepositoryError::InvalidInput => {
            OrganizationPlanOrderServiceError::InvalidRequest
        }
        OrganizationPlanOrderRepositoryError::Conflict => {
            OrganizationPlanOrderServiceError::Conflict
        }
        OrganizationPlanOrderRepositoryError::OutcomeUnknown => {
            OrganizationPlanOrderServiceError::OutcomeUnknown
        }
        OrganizationPlanOrderRepositoryError::Query
        | OrganizationPlanOrderRepositoryError::Timeout => {
            OrganizationPlanOrderServiceError::Unavailable
        }
        OrganizationPlanOrderRepositoryError::Invariant => {
            OrganizationPlanOrderServiceError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use af_db::{
        AdminUsageLogRepository, DatabaseOptions, MigrationOptions, OrganizationAuditRepository,
    };

    use super::*;
    use crate::{
        OrganizationAccessFuture, OrganizationListFuture, OrganizationPlanCatalogFuture,
        OrganizationPlanFuture, OrganizationPlanPageFuture, SessionRole, UserTopupConfiguration,
        UserTopupOrderCreateFuture,
    };

    #[test]
    fn 同一企业用户套餐和请求生成稳定订单标识() {
        let principal = SessionPrincipal::new(UserId::new(17).unwrap(), SessionRole::User);
        let organization_id = OrganizationId::new(23).unwrap();
        let command = OrganizationPlanOrderCreateCommand::new(
            OrganizationPlanId::new([0x31; 16]).unwrap(),
            OrganizationPlanOrderRequestId::new([0x52; 16]).unwrap(),
        );

        let first = scoped_order_id(principal, organization_id, command).unwrap();
        let second = scoped_order_id(principal, organization_id, command).unwrap();
        assert_eq!(first, second);

        let another_plan = OrganizationPlanOrderCreateCommand::new(
            OrganizationPlanId::new([0x32; 16]).unwrap(),
            command.request_id(),
        );
        assert_ne!(
            first,
            scoped_order_id(principal, organization_id, another_plan).unwrap()
        );
    }

    #[tokio::test]
    async fn 支付提交必须重新校验企业钱包管理权限() {
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").expect("测试数据库配置有效"),
            MigrationOptions::default(),
        )
        .await
        .expect("测试数据库迁移成功");
        let authorization = Arc::new(DenyingAuthorizationService::default());
        let organization_repository = Arc::new(
            OrganizationRepository::new(pool.clone(), std::time::Duration::from_secs(5))
                .expect("企业仓储配置有效"),
        );
        let activity_service = Arc::new(
            crate::DatabaseOrganizationActivityService::new(
                authorization.clone(),
                AdminUsageLogRepository::new(pool.clone(), std::time::Duration::from_secs(5))
                    .expect("调用日志仓储配置有效"),
                OrganizationAuditRepository::new(pool.clone()),
                Some("ERERERERERERERERERERERERERERERERERERERERERE"),
            )
            .expect("企业审计服务配置有效"),
        );
        let service = DatabaseOrganizationPlanOrderService::new(
            authorization.clone(),
            Arc::new(UnavailablePlanService),
            Arc::new(UnavailableTopupService),
            OrganizationPlanOrderRepository::new(pool.clone(), std::time::Duration::from_secs(5))
                .expect("企业订单仓储配置有效"),
            organization_repository,
            activity_service,
        );
        let organization_id = OrganizationId::new(23).expect("企业标识有效");
        let command = OrganizationPlanOrderPaymentCommand::new(
            organization_id,
            OrganizationPlanOrderId::new([0x62; 16]).expect("订单标识有效"),
            "card".to_owned(),
        )
        .expect("支付命令有效");

        let result = service
            .submit_payment(
                SessionPrincipal::new(UserId::new(17).expect("用户标识有效"), SessionRole::User),
                command,
            )
            .await;

        assert!(matches!(
            result,
            Err(OrganizationPlanOrderServiceError::Forbidden)
        ));
        assert_eq!(
            authorization
                .permissions
                .lock()
                .expect("测试权限锁未中毒")
                .as_slice(),
            &[OrganizationPermission::WalletManage]
        );
        pool.close().await.expect("测试数据库关闭成功");
    }

    #[derive(Default)]
    struct DenyingAuthorizationService {
        permissions: Mutex<Vec<OrganizationPermission>>,
    }

    impl OrganizationAuthorizationService for DenyingAuthorizationService {
        fn list<'a>(&'a self, _principal: SessionPrincipal) -> OrganizationListFuture<'a> {
            Box::pin(async { Err(OrganizationAuthorizationError::Internal) })
        }

        fn context<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _organization_id: OrganizationId,
            permission: OrganizationPermission,
        ) -> OrganizationAccessFuture<'a> {
            self.permissions
                .lock()
                .expect("测试权限锁未中毒")
                .push(permission);
            Box::pin(async { Err(OrganizationAuthorizationError::Forbidden) })
        }
    }

    struct UnavailablePlanService;

    impl OrganizationPlanService for UnavailablePlanService {
        fn list_catalog<'a>(
            &'a self,
            _principal: SessionPrincipal,
        ) -> OrganizationPlanCatalogFuture<'a> {
            Box::pin(async { Err(OrganizationPlanServiceError::Internal) })
        }

        fn list<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _query: crate::OrganizationPlanListQuery,
        ) -> OrganizationPlanPageFuture<'a> {
            Box::pin(async { Err(OrganizationPlanServiceError::Internal) })
        }

        fn create<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _command: crate::OrganizationPlanCreateCommand,
        ) -> OrganizationPlanFuture<'a> {
            Box::pin(async { Err(OrganizationPlanServiceError::Internal) })
        }

        fn disable<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _plan_id: OrganizationPlanId,
            _command: crate::OrganizationPlanDisableCommand,
        ) -> OrganizationPlanFuture<'a> {
            Box::pin(async { Err(OrganizationPlanServiceError::Internal) })
        }
    }

    struct UnavailableTopupService;

    impl UserTopupService for UnavailableTopupService {
        fn configuration(&self) -> Result<UserTopupConfiguration, UserTopupError> {
            Err(UserTopupError::Unavailable)
        }

        fn create<'a>(
            &'a self,
            _principal: SessionPrincipal,
            _command: crate::UserTopupOrderCreateCommand,
        ) -> UserTopupOrderCreateFuture<'a> {
            Box::pin(async { Err(UserTopupError::Unavailable) })
        }
    }
}
