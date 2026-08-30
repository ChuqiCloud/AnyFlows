use std::{fmt, future::Future, pin::Pin, sync::Arc};

pub use af_db::{OrganizationApprovalDecisionRecord, OrganizationApprovalTemplateRecord};
use af_db::{
    OrganizationApprovalDecisionWrite, OrganizationApprovalRepository,
    OrganizationApprovalRepositoryError, OrganizationApprovalRequestCreateOutcome,
    OrganizationApprovalRequestRecord, OrganizationApprovalRequestWrite,
    OrganizationApprovalTemplateWrite,
};
use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKey,
    OrganizationApprovalDecisionKind, OrganizationApprovalRequest, OrganizationApprovalRequestId,
    OrganizationApprovalRequestKey, OrganizationApprovalResourceType,
    OrganizationApprovalTemplateDigest, OrganizationApprovalTemplateId,
    OrganizationApprovalTemplateKey, OrganizationPermission,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 管理端模板版本创建命令；模板正文仍由外部系统管理，本命令只接收摘要。
pub struct OrganizationApprovalTemplateCreateCommand {
    template_key: String,
    resource_type: OrganizationApprovalResourceType,
    definition_digest: String,
    expected_version: Option<i64>,
}

impl OrganizationApprovalTemplateCreateCommand {
    /// 构造模板创建命令并校验 CAS 的非负边界。
    pub fn new(
        template_key: String,
        resource_type: OrganizationApprovalResourceType,
        definition_digest: String,
        expected_version: Option<i64>,
    ) -> Result<Self, OrganizationApprovalServiceError> {
        if expected_version.is_some_and(|value| value < 0) {
            return Err(OrganizationApprovalServiceError::InvalidRequest);
        }
        Ok(Self {
            template_key,
            resource_type,
            definition_digest,
            expected_version,
        })
    }
}

/// 管理端审批申请命令；资源摘要不等同于目标业务资源。
pub struct OrganizationApprovalRequestCreateCommand {
    request_key: String,
    resource_type: OrganizationApprovalResourceType,
    resource_summary: String,
    template_id: i64,
    template_version: i64,
    expires_at: i64,
}

impl OrganizationApprovalRequestCreateCommand {
    /// 构造申请命令并校验时间及版本的基本边界。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_key: String,
        resource_type: OrganizationApprovalResourceType,
        resource_summary: String,
        template_id: i64,
        template_version: i64,
        expires_at: i64,
    ) -> Result<Self, OrganizationApprovalServiceError> {
        if template_id <= 0 || template_version <= 0 || expires_at < 0 {
            return Err(OrganizationApprovalServiceError::InvalidRequest);
        }
        Ok(Self {
            request_key,
            resource_type,
            resource_summary,
            template_id,
            template_version,
            expires_at,
        })
    }
}

/// 管理端人工决定命令；超时决定只能由后续调度器写入。
pub struct OrganizationApprovalDecisionCommand {
    decision_key: String,
    kind: OrganizationApprovalDecisionKind,
    reason: Option<String>,
    decided_at: Option<i64>,
}

impl OrganizationApprovalDecisionCommand {
    /// 构造批准或拒绝命令，拒绝客户端伪造 TimedOut 决定。
    pub fn new(
        decision_key: String,
        kind: OrganizationApprovalDecisionKind,
        reason: Option<String>,
        decided_at: Option<i64>,
    ) -> Result<Self, OrganizationApprovalServiceError> {
        if matches!(kind, OrganizationApprovalDecisionKind::TimedOut)
            || decided_at.is_some_and(|value| value < 0)
        {
            return Err(OrganizationApprovalServiceError::InvalidRequest);
        }
        Ok(Self {
            decision_key,
            kind,
            reason,
            decided_at,
        })
    }
}

/// 管理端审批申请状态；TimedOut 在没有决定事实时也可以由服务端读取投影得到。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalRequestStatus {
    Pending,
    Approved,
    Rejected,
    TimedOut,
}

impl OrganizationApprovalRequestStatus {
    /// 返回 OpenAPI 与前端使用的稳定编码。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::TimedOut => "timed_out",
        }
    }
}

/// 申请及其唯一决定的安全读取投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalRequestView {
    request: OrganizationApprovalRequestRecord,
    decision: Option<OrganizationApprovalDecisionRecord>,
    status: OrganizationApprovalRequestStatus,
}

impl OrganizationApprovalRequestView {
    fn new(
        request: OrganizationApprovalRequestRecord,
        decision: Option<OrganizationApprovalDecisionRecord>,
        now: i64,
    ) -> Self {
        let status = match decision
            .as_ref()
            .map(OrganizationApprovalDecisionRecord::kind)
        {
            Some(OrganizationApprovalDecisionKind::Approved) => {
                OrganizationApprovalRequestStatus::Approved
            }
            Some(OrganizationApprovalDecisionKind::Rejected) => {
                OrganizationApprovalRequestStatus::Rejected
            }
            Some(OrganizationApprovalDecisionKind::TimedOut) => {
                OrganizationApprovalRequestStatus::TimedOut
            }
            None if request.expires_at() <= now => OrganizationApprovalRequestStatus::TimedOut,
            None => OrganizationApprovalRequestStatus::Pending,
        };
        Self {
            request,
            decision,
            status,
        }
    }

    #[must_use]
    pub const fn request(&self) -> &OrganizationApprovalRequestRecord {
        &self.request
    }
    #[must_use]
    pub const fn decision(&self) -> Option<&OrganizationApprovalDecisionRecord> {
        self.decision.as_ref()
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationApprovalRequestStatus {
        self.status
    }
}

pub type OrganizationApprovalTemplateListFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Vec<OrganizationApprovalTemplateRecord>,
                    OrganizationApprovalServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationApprovalTemplateFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalTemplateRecord,
                    OrganizationApprovalServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationApprovalRequestListFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Vec<OrganizationApprovalRequestView>,
                    OrganizationApprovalServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationApprovalRequestFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationApprovalRequestView, OrganizationApprovalServiceError>,
            > + Send
            + 'a,
    >,
>;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalServiceError {
    #[error("企业审批请求无效")]
    InvalidRequest,
    #[error("企业审批权限不足")]
    Forbidden,
    #[error("企业审批事实不存在")]
    NotFound,
    #[error("企业审批版本或幂等键冲突")]
    Conflict,
    #[error("企业审批申请已超时")]
    Expired,
    #[error("企业审批内部错误")]
    Internal,
}

/// 企业审批管理端口；服务层再次执行企业权限和申请可见性校验。
pub trait OrganizationApprovalService: Send + Sync {
    fn list_templates<'a>(
        &'a self,
        context: OrganizationContext,
        after: Option<i64>,
        limit: u64,
    ) -> OrganizationApprovalTemplateListFuture<'a>;
    fn create_template<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationApprovalTemplateCreateCommand,
    ) -> OrganizationApprovalTemplateFuture<'a>;
    fn list_requests<'a>(
        &'a self,
        context: OrganizationContext,
        after: Option<i64>,
        limit: u64,
    ) -> OrganizationApprovalRequestListFuture<'a>;
    fn get_request<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
    ) -> OrganizationApprovalRequestFuture<'a>;
    fn create_request<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationApprovalRequestCreateCommand,
    ) -> OrganizationApprovalRequestFuture<'a>;
    fn decide<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalDecisionCommand,
    ) -> OrganizationApprovalRequestFuture<'a>;
}

/// 使用数据库事实仓储实现企业审批管理。
pub struct DatabaseOrganizationApprovalService {
    repository: Arc<OrganizationApprovalRepository>,
}

impl DatabaseOrganizationApprovalService {
    /// 绑定启动期创建的审批事实仓储。
    #[must_use]
    pub fn new(repository: Arc<OrganizationApprovalRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationApprovalService for DatabaseOrganizationApprovalService {
    fn list_templates<'a>(
        &'a self,
        context: OrganizationContext,
        after: Option<i64>,
        limit: u64,
    ) -> OrganizationApprovalTemplateListFuture<'a> {
        Box::pin(async move {
            require_any(
                &context,
                [
                    OrganizationPermission::ApprovalsRead,
                    OrganizationPermission::ApprovalsCreate,
                ],
            )?;
            self.repository
                .list_templates(context.organization_id(), after, limit)
                .await
                .map_err(map_repository_error)
        })
    }

    fn create_template<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationApprovalTemplateCreateCommand,
    ) -> OrganizationApprovalTemplateFuture<'a> {
        Box::pin(async move {
            require(&context, OrganizationPermission::ApprovalsManage)?;
            let write = OrganizationApprovalTemplateWrite::new(
                context.organization_id(),
                OrganizationApprovalTemplateKey::new(command.template_key)
                    .map_err(|_| OrganizationApprovalServiceError::InvalidRequest)?,
                command.resource_type,
                OrganizationApprovalTemplateDigest::from_persistence_key(
                    &command.definition_digest,
                )
                .map_err(|_| OrganizationApprovalServiceError::InvalidRequest)?,
                context.user_id(),
                command.expected_version,
            );
            self.repository
                .create_template(write)
                .await
                .map_err(map_repository_error)
        })
    }

    fn list_requests<'a>(
        &'a self,
        context: OrganizationContext,
        after: Option<i64>,
        limit: u64,
    ) -> OrganizationApprovalRequestListFuture<'a> {
        Box::pin(async move {
            let requested_by = if context.allows(OrganizationPermission::ApprovalsRead) {
                None
            } else if context.allows(OrganizationPermission::ApprovalsCreate) {
                Some(context.user_id())
            } else {
                return Err(OrganizationApprovalServiceError::Forbidden);
            };
            let requests = self
                .repository
                .list_requests(context.organization_id(), requested_by, after, limit)
                .await
                .map_err(map_repository_error)?;
            build_views(&self.repository, requests).await
        })
    }

    fn get_request<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
    ) -> OrganizationApprovalRequestFuture<'a> {
        Box::pin(async move {
            let request = self
                .repository
                .get_request(context.organization_id(), request_id)
                .await
                .map_err(map_repository_error)?;
            if !context.allows(OrganizationPermission::ApprovalsRead)
                && (!context.allows(OrganizationPermission::ApprovalsCreate)
                    || request.requested_by_user_id() != context.user_id())
            {
                return Err(OrganizationApprovalServiceError::NotFound);
            }
            build_view(&self.repository, request).await
        })
    }

    fn create_request<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationApprovalRequestCreateCommand,
    ) -> OrganizationApprovalRequestFuture<'a> {
        Box::pin(async move {
            require(&context, OrganizationPermission::ApprovalsCreate)?;
            let request = OrganizationApprovalRequest::new(
                OrganizationApprovalRequestKey::from_persistence_key(&command.request_key)
                    .map_err(|_| OrganizationApprovalServiceError::InvalidRequest)?,
                context.organization_id(),
                command.resource_type,
                command.resource_summary,
                OrganizationApprovalTemplateId::new(command.template_id)
                    .map_err(|_| OrganizationApprovalServiceError::InvalidRequest)?,
                command.template_version,
                context.user_id(),
                command.expires_at,
            )
            .map_err(|_| OrganizationApprovalServiceError::InvalidRequest)?;
            let outcome = self
                .repository
                .create_request(OrganizationApprovalRequestWrite::new(request))
                .await
                .map_err(map_repository_error)?;
            let request = match outcome {
                OrganizationApprovalRequestCreateOutcome::Created(record)
                | OrganizationApprovalRequestCreateOutcome::Existing(record) => record,
            };
            build_view(&self.repository, request).await
        })
    }

    fn decide<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalDecisionCommand,
    ) -> OrganizationApprovalRequestFuture<'a> {
        Box::pin(async move {
            require(&context, OrganizationPermission::ApprovalsDecide)?;
            let request = self
                .repository
                .get_request(context.organization_id(), request_id)
                .await
                .map_err(map_repository_error)?;
            let decision = OrganizationApprovalDecision::new(
                OrganizationApprovalDecisionKey::from_persistence_key(&command.decision_key)
                    .map_err(|_| OrganizationApprovalServiceError::InvalidRequest)?,
                context.organization_id(),
                request_id,
                command.kind,
                Some(context.user_id()),
                command.reason,
                command.decided_at.unwrap_or_else(now),
            )
            .map_err(|_| OrganizationApprovalServiceError::InvalidRequest)?;
            self.repository
                .record_decision(OrganizationApprovalDecisionWrite::new(decision))
                .await
                .map_err(map_repository_error)?;
            build_view(&self.repository, request).await
        })
    }
}

impl fmt::Debug for DatabaseOrganizationApprovalService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationApprovalService(<repository>)")
    }
}

async fn build_views(
    repository: &OrganizationApprovalRepository,
    requests: Vec<OrganizationApprovalRequestRecord>,
) -> Result<Vec<OrganizationApprovalRequestView>, OrganizationApprovalServiceError> {
    let mut views = Vec::with_capacity(requests.len());
    for request in requests {
        views.push(build_view(repository, request).await?);
    }
    Ok(views)
}

async fn build_view(
    repository: &OrganizationApprovalRepository,
    request: OrganizationApprovalRequestRecord,
) -> Result<OrganizationApprovalRequestView, OrganizationApprovalServiceError> {
    let decision = repository
        .decision_for_request(request.organization_id(), request.id())
        .await
        .map_err(map_repository_error)?;
    Ok(OrganizationApprovalRequestView::new(
        request,
        decision,
        now(),
    ))
}

fn require(
    context: &OrganizationContext,
    permission: OrganizationPermission,
) -> Result<(), OrganizationApprovalServiceError> {
    context
        .allows(permission)
        .then_some(())
        .ok_or(OrganizationApprovalServiceError::Forbidden)
}

fn require_any(
    context: &OrganizationContext,
    permissions: [OrganizationPermission; 2],
) -> Result<(), OrganizationApprovalServiceError> {
    permissions
        .into_iter()
        .any(|permission| context.allows(permission))
        .then_some(())
        .ok_or(OrganizationApprovalServiceError::Forbidden)
}

fn now() -> i64 {
    af_db::DatabaseTimestamp::now_utc().unix_timestamp()
}

fn map_repository_error(
    error: OrganizationApprovalRepositoryError,
) -> OrganizationApprovalServiceError {
    match error {
        OrganizationApprovalRepositoryError::InvalidInput => {
            OrganizationApprovalServiceError::InvalidRequest
        }
        OrganizationApprovalRepositoryError::NotFound => OrganizationApprovalServiceError::NotFound,
        OrganizationApprovalRepositoryError::TemplateResourceMismatch
        | OrganizationApprovalRepositoryError::Conflict
        | OrganizationApprovalRepositoryError::AlreadyDecided => {
            OrganizationApprovalServiceError::Conflict
        }
        OrganizationApprovalRepositoryError::Expired => OrganizationApprovalServiceError::Expired,
        OrganizationApprovalRepositoryError::OrganizationUnavailable
        | OrganizationApprovalRepositoryError::MemberUnavailable => {
            OrganizationApprovalServiceError::Forbidden
        }
        OrganizationApprovalRepositoryError::Query
        | OrganizationApprovalRepositoryError::Timeout
        | OrganizationApprovalRepositoryError::Invariant => {
            OrganizationApprovalServiceError::Internal
        }
    }
}
