use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationAuditOutcome, OrganizationEntitlementReadRecord, OrganizationPlanOrderRepository,
    OrganizationPlanOrderRepositoryError, OrganizationRepository, OrganizationRepositoryError,
};
use af_domain::{
    OrganizationEntitlementSnapshot, OrganizationEntitlementStatus, OrganizationId,
    OrganizationPermission, OrganizationPlanId, OrganizationPlanOrderId,
    OrganizationPlanOrderStatus, OrganizationRole, PlatformPermission,
};
use thiserror::Error;

use crate::{
    OrganizationActivityError, OrganizationActivityService, OrganizationAuditRequestContext,
    OrganizationAuthorizationError, OrganizationAuthorizationService, SessionPrincipal,
    organization_activity::authorize_platform,
};

const PLATFORM_PROVISIONING_SOURCE: &str = "platform_provisioning";
const PLAN_ORDER_SOURCE: &str = "organization_plan_order";

/// 企业授权对外展示的闭合来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationEntitlementSource {
    PlatformProvisioning,
    OrganizationPlanOrder,
    Other,
}

impl OrganizationEntitlementSource {
    /// 返回 API 使用的稳定来源码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::PlatformProvisioning => "platform_provisioning",
            Self::OrganizationPlanOrder => "organization_plan_order",
            Self::Other => "other",
        }
    }
}

/// 可安全展示给 Owner 或平台管理员的企业授权事实投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationEntitlementView {
    organization_id: OrganizationId,
    status: OrganizationEntitlementStatus,
    snapshot: OrganizationEntitlementSnapshot,
    source: OrganizationEntitlementSource,
    version: i64,
    updated_at: i64,
    applied_order_id: Option<OrganizationPlanOrderId>,
    plan_id: Option<OrganizationPlanId>,
    plan_version: Option<u64>,
}

impl OrganizationEntitlementView {
    /// 返回投影所属企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    /// 返回结合企业生命周期和授权时间窗推导的当前状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationEntitlementStatus {
        self.status
    }
    /// 返回服务端授权能力与容量快照。
    #[must_use]
    pub const fn snapshot(&self) -> OrganizationEntitlementSnapshot {
        self.snapshot
    }
    /// 返回闭合后的授权来源。
    #[must_use]
    pub const fn source(&self) -> OrganizationEntitlementSource {
        self.source
    }
    /// 返回授权事实版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    /// 返回授权事实最近更新时间。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
    /// 返回已严格核对为同企业已支付订单的标识。
    #[must_use]
    pub const fn applied_order_id(&self) -> Option<OrganizationPlanOrderId> {
        self.applied_order_id
    }
    /// 返回已生效订单锁定的套餐标识。
    #[must_use]
    pub const fn plan_id(&self) -> Option<OrganizationPlanId> {
        self.plan_id
    }
    /// 返回已生效订单锁定的套餐版本。
    #[must_use]
    pub const fn plan_version(&self) -> Option<u64> {
        self.plan_version
    }
}

/// 企业授权投影读取错误；损坏的订单引用不会降级为“已生效”。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationEntitlementServiceError {
    #[error("企业授权投影请求无效")]
    InvalidRequest,
    #[error("企业授权投影访问权限不足")]
    Forbidden,
    #[error("企业授权投影不存在")]
    NotFound,
    #[error("企业授权投影服务暂不可用")]
    Unavailable,
    #[error("企业授权投影内部不变量失败")]
    Internal,
}

/// 组合企业 RBAC、平台审计与订单事实的授权投影读取器。
pub struct DatabaseOrganizationEntitlementReader {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    activity_service: Arc<dyn OrganizationActivityService>,
    organization_repository: Arc<OrganizationRepository>,
    order_repository: OrganizationPlanOrderRepository,
}

impl DatabaseOrganizationEntitlementReader {
    #[must_use]
    pub fn new(
        authorization_service: Arc<dyn OrganizationAuthorizationService>,
        activity_service: Arc<dyn OrganizationActivityService>,
        organization_repository: Arc<OrganizationRepository>,
        order_repository: OrganizationPlanOrderRepository,
    ) -> Self {
        Self {
            authorization_service,
            activity_service,
            organization_repository,
            order_repository,
        }
    }

    /// 仅允许企业 Owner 读取当前授权事实。
    pub async fn get_for_owner(
        &self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
    ) -> Result<OrganizationEntitlementView, OrganizationEntitlementServiceError> {
        let context = self
            .authorization_service
            .context(
                principal,
                organization_id,
                OrganizationPermission::OverviewRead,
            )
            .await
            .map_err(map_authorization_error)?;
        if context.role() != OrganizationRole::Owner {
            return Err(OrganizationEntitlementServiceError::Forbidden);
        }
        self.read(organization_id).await
    }

    /// 平台管理员说明跨企业访问理由后读取授权，并在返回前完成企业审计。
    pub async fn get_as_platform(
        &self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        platform_reason: String,
        request: OrganizationAuditRequestContext,
    ) -> Result<OrganizationEntitlementView, OrganizationEntitlementServiceError> {
        authorize_platform(
            principal,
            PlatformPermission::OrganizationsReadAll,
            &platform_reason,
        )
        .map_err(map_activity_error)?;
        let view = self.read(organization_id).await?;
        self.activity_service
            .record_platform_action(
                principal,
                organization_id,
                request,
                PlatformPermission::OrganizationsReadAll,
                "platform.organization_entitlement.read",
                "organization_entitlement",
                organization_id.get().to_string(),
                OrganizationAuditOutcome::Succeeded,
                platform_reason,
                None,
            )
            .await
            .map_err(map_activity_error)?;
        Ok(view)
    }

    async fn read(
        &self,
        organization_id: OrganizationId,
    ) -> Result<OrganizationEntitlementView, OrganizationEntitlementServiceError> {
        let now = unix_now()?;
        self.order_repository
            .apply_due_entitlement_change(organization_id, now)
            .await
            .map_err(map_order_repository_error)?;
        let record = self
            .organization_repository
            .entitlement(organization_id)
            .await
            .map_err(map_organization_repository_error)?;
        self.project(record).await
    }

    async fn project(
        &self,
        record: OrganizationEntitlementReadRecord,
    ) -> Result<OrganizationEntitlementView, OrganizationEntitlementServiceError> {
        let entitlement = record.entitlement();
        let snapshot = entitlement.snapshot();
        let (source, order_id) =
            classify_source(entitlement.source(), entitlement.external_reference())?;
        let applied_order = match order_id {
            Some(order_id) => {
                let order = self
                    .order_repository
                    .get_order(entitlement.organization_id(), order_id)
                    .await
                    .map_err(map_order_repository_error)?
                    .ok_or(OrganizationEntitlementServiceError::Internal)?;
                if order.organization_id() != entitlement.organization_id()
                    || order.order_id() != order_id
                    || order.status() != OrganizationPlanOrderStatus::Paid
                {
                    return Err(OrganizationEntitlementServiceError::Internal);
                }
                Some((order.order_id(), order.plan_id(), order.plan_version()))
            }
            None => None,
        };
        let (applied_order_id, plan_id, plan_version) = applied_order
            .map(|(order_id, plan_id, plan_version)| {
                (Some(order_id), Some(plan_id), Some(plan_version))
            })
            .unwrap_or((None, None, None));
        Ok(OrganizationEntitlementView {
            organization_id: entitlement.organization_id(),
            status: snapshot.status(record.organization_status(), unix_now()?),
            snapshot,
            source,
            version: entitlement.version(),
            updated_at: entitlement.updated_at(),
            applied_order_id,
            plan_id,
            plan_version,
        })
    }
}

fn classify_source(
    source: &str,
    external_reference: Option<&str>,
) -> Result<
    (
        OrganizationEntitlementSource,
        Option<OrganizationPlanOrderId>,
    ),
    OrganizationEntitlementServiceError,
> {
    match source {
        PLATFORM_PROVISIONING_SOURCE if external_reference.is_none() => {
            Ok((OrganizationEntitlementSource::PlatformProvisioning, None))
        }
        PLATFORM_PROVISIONING_SOURCE => Err(OrganizationEntitlementServiceError::Internal),
        PLAN_ORDER_SOURCE => {
            let order_id = external_reference
                .ok_or(OrganizationEntitlementServiceError::Internal)
                .and_then(|value| {
                    OrganizationPlanOrderId::from_persistence_key(value)
                        .map_err(|_| OrganizationEntitlementServiceError::Internal)
                })?;
            Ok((
                OrganizationEntitlementSource::OrganizationPlanOrder,
                Some(order_id),
            ))
        }
        _ => Ok((OrganizationEntitlementSource::Other, None)),
    }
}

fn unix_now() -> Result<i64, OrganizationEntitlementServiceError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or(OrganizationEntitlementServiceError::Internal)
}

fn map_authorization_error(
    error: OrganizationAuthorizationError,
) -> OrganizationEntitlementServiceError {
    match error {
        OrganizationAuthorizationError::Forbidden => OrganizationEntitlementServiceError::Forbidden,
        OrganizationAuthorizationError::NotFound => OrganizationEntitlementServiceError::NotFound,
        OrganizationAuthorizationError::Internal => {
            OrganizationEntitlementServiceError::Unavailable
        }
    }
}

fn map_activity_error(error: OrganizationActivityError) -> OrganizationEntitlementServiceError {
    match error {
        OrganizationActivityError::InvalidInput => {
            OrganizationEntitlementServiceError::InvalidRequest
        }
        OrganizationActivityError::Forbidden => OrganizationEntitlementServiceError::Forbidden,
        OrganizationActivityError::NotFound => OrganizationEntitlementServiceError::NotFound,
        OrganizationActivityError::Internal => OrganizationEntitlementServiceError::Unavailable,
    }
}

fn map_organization_repository_error(
    error: OrganizationRepositoryError,
) -> OrganizationEntitlementServiceError {
    match error {
        OrganizationRepositoryError::NotFound => OrganizationEntitlementServiceError::NotFound,
        OrganizationRepositoryError::Query | OrganizationRepositoryError::Timeout => {
            OrganizationEntitlementServiceError::Unavailable
        }
        _ => OrganizationEntitlementServiceError::Internal,
    }
}

fn map_order_repository_error(
    error: OrganizationPlanOrderRepositoryError,
) -> OrganizationEntitlementServiceError {
    match error {
        OrganizationPlanOrderRepositoryError::Query
        | OrganizationPlanOrderRepositoryError::Timeout => {
            OrganizationEntitlementServiceError::Unavailable
        }
        _ => OrganizationEntitlementServiceError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 授权来源只接受完整的订单引用() {
        let order_id = OrganizationPlanOrderId::new([0x5a; 16]).expect("订单标识有效");
        assert_eq!(
            classify_source(PLAN_ORDER_SOURCE, Some(&order_id.persistence_key())),
            Ok((
                OrganizationEntitlementSource::OrganizationPlanOrder,
                Some(order_id)
            ))
        );
        assert_eq!(
            classify_source(PLAN_ORDER_SOURCE, None),
            Err(OrganizationEntitlementServiceError::Internal)
        );
        assert_eq!(
            classify_source(PLAN_ORDER_SOURCE, Some("broken")),
            Err(OrganizationEntitlementServiceError::Internal)
        );
    }

    #[test]
    fn 未知来源不会借用外部引用声明订单已生效() {
        assert_eq!(
            classify_source("contract", Some("not-an-order")),
            Ok((OrganizationEntitlementSource::Other, None))
        );
        assert_eq!(
            classify_source(PLATFORM_PROVISIONING_SOURCE, Some("unexpected")),
            Err(OrganizationEntitlementServiceError::Internal)
        );
    }
}
