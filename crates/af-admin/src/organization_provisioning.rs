use std::{fmt, future::Future, pin::Pin, sync::Arc};

pub use af_db::OrganizationProvisioningMaterialWrite;
use af_db::{
    OrganizationEntitlementWrite, OrganizationProvisioningDecisionWrite,
    OrganizationProvisioningRepository, OrganizationProvisioningRepositoryError,
};
use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    OrganizationProvisioningCreateOutcome, OrganizationProvisioningDecisionKind,
    OrganizationProvisioningDecisionOutcome, OrganizationProvisioningIdempotencyKey,
    OrganizationProvisioningRequest, OrganizationProvisioningRequestKey, PlatformPermission,
    UserId,
};
pub use af_domain::{
    OrganizationProvisioningMaterial, OrganizationProvisioningRecord,
    OrganizationProvisioningStatus,
};
use thiserror::Error;

use crate::{
    PlatformAuditEntry, PlatformAuditOutcome, PlatformAuditService, SessionPrincipal, SessionRole,
};

/// 当前用户提交企业开通申请所需的资料。
pub struct OrganizationProvisioningCreateCommand {
    idempotency_key: String,
    organization_name: String,
    organization_slug: String,
    business_reason: String,
    materials: Vec<OrganizationProvisioningMaterialWrite>,
}

impl OrganizationProvisioningCreateCommand {
    /// 构造申请命令；具体名称、标识和说明由领域层执行完整校验。
    #[must_use]
    pub fn new(
        idempotency_key: String,
        organization_name: String,
        organization_slug: String,
        business_reason: String,
    ) -> Self {
        Self {
            idempotency_key,
            organization_name,
            organization_slug,
            business_reason,
            materials: Vec::new(),
        }
    }

    /// 附加本次申请要在同一事务保存的真实材料。
    #[must_use]
    pub fn with_materials(mut self, materials: Vec<OrganizationProvisioningMaterialWrite>) -> Self {
        self.materials = materials;
        self
    }
}

/// 平台管理员审批时写入的初始企业授权快照。
pub struct OrganizationProvisioningEntitlementCommand {
    seat_limit: i64,
    team_limit: i64,
    organization_key_limit: i64,
    team_management_enabled: bool,
    sso_enabled: bool,
    custom_roles_enabled: bool,
    audit_retention_days: i64,
    valid_from: i64,
    valid_until: i64,
    grace_until: Option<i64>,
}

impl OrganizationProvisioningEntitlementCommand {
    /// 构造授权快照命令；容量、能力和时间边界在领域层集中校验。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        seat_limit: i64,
        team_limit: i64,
        organization_key_limit: i64,
        team_management_enabled: bool,
        sso_enabled: bool,
        custom_roles_enabled: bool,
        audit_retention_days: i64,
        valid_from: i64,
        valid_until: i64,
        grace_until: Option<i64>,
    ) -> Self {
        Self {
            seat_limit,
            team_limit,
            organization_key_limit,
            team_management_enabled,
            sso_enabled,
            custom_roles_enabled,
            audit_retention_days,
            valid_from,
            valid_until,
            grace_until,
        }
    }
}

/// 平台管理员批准或拒绝申请的命令。
pub struct OrganizationProvisioningDecisionCommand {
    expected_version: u64,
    decision: OrganizationProvisioningDecisionKind,
    decision_reason: Option<String>,
    entitlement: Option<OrganizationProvisioningEntitlementCommand>,
}

impl OrganizationProvisioningDecisionCommand {
    /// 构造决定命令；批准必须携带授权，拒绝必须携带理由。
    #[must_use]
    pub fn new(
        expected_version: u64,
        decision: OrganizationProvisioningDecisionKind,
        decision_reason: Option<String>,
        entitlement: Option<OrganizationProvisioningEntitlementCommand>,
    ) -> Self {
        Self {
            expected_version,
            decision,
            decision_reason,
            entitlement,
        }
    }
}

/// 平台审批列表的倒序稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationProvisioningListQuery {
    before: Option<i64>,
    limit: u64,
    status: Option<OrganizationProvisioningStatus>,
}

impl OrganizationProvisioningListQuery {
    /// 校验正数游标和不超过 100 的页大小。
    pub fn new(
        before: Option<i64>,
        limit: u64,
        status: Option<OrganizationProvisioningStatus>,
    ) -> Result<Self, OrganizationProvisioningServiceError> {
        if !(1..=100).contains(&limit) || before.is_some_and(|value| value <= 0) {
            return Err(OrganizationProvisioningServiceError::InvalidRequest);
        }
        Ok(Self {
            before,
            limit,
            status,
        })
    }

    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }
    #[must_use]
    pub const fn limit(self) -> u64 {
        self.limit
    }
    #[must_use]
    pub const fn status(self) -> Option<OrganizationProvisioningStatus> {
        self.status
    }
}

impl Default for OrganizationProvisioningListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: 50,
            status: None,
        }
    }
}

/// 申请写入或审批的结果；`replayed` 表示恢复已有事实。
#[derive(Debug)]
pub struct OrganizationProvisioningResult {
    record: OrganizationProvisioningRecord,
    replayed: bool,
}

impl OrganizationProvisioningResult {
    #[must_use]
    pub fn record(&self) -> &OrganizationProvisioningRecord {
        &self.record
    }
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

/// 平台审批申请稳定游标页。
pub struct OrganizationProvisioningPage {
    records: Vec<OrganizationProvisioningRecord>,
    next_cursor: Option<i64>,
}

impl OrganizationProvisioningPage {
    #[must_use]
    pub fn records(&self) -> &[OrganizationProvisioningRecord] {
        &self.records
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

pub type OrganizationProvisioningResultFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationProvisioningResult,
                    OrganizationProvisioningServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationProvisioningRecordFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationProvisioningRecord,
                    OrganizationProvisioningServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationProvisioningPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationProvisioningPage, OrganizationProvisioningServiceError>,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationProvisioningMaterialPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Vec<OrganizationProvisioningMaterial>,
                    OrganizationProvisioningServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationProvisioningMaterialDownloadFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<(String, String, Vec<u8>), OrganizationProvisioningServiceError>>
            + Send
            + 'a,
    >,
>;

/// 企业开通申请服务；普通用户与平台管理员入口共享仓储但不共享读取边界。
pub trait OrganizationProvisioningService: Send + Sync {
    fn list_self<'a>(&'a self, _user: UserId, _query: OrganizationProvisioningListQuery) -> OrganizationProvisioningPageFuture<'a> {
        Box::pin(async { Err(OrganizationProvisioningServiceError::Forbidden) })
    }
    fn list_materials_self<'a>(&'a self, _user: UserId, _key: OrganizationProvisioningRequestKey) -> OrganizationProvisioningMaterialPageFuture<'a> {
        Box::pin(async { Err(OrganizationProvisioningServiceError::Forbidden) })
    }
    fn download_material_self<'a>(&'a self, _user: UserId, _key: OrganizationProvisioningRequestKey, _id: i64) -> OrganizationProvisioningMaterialDownloadFuture<'a> {
        Box::pin(async { Err(OrganizationProvisioningServiceError::Forbidden) })
    }
    fn create<'a>(
        &'a self,
        user_id: UserId,
        command: OrganizationProvisioningCreateCommand,
    ) -> OrganizationProvisioningResultFuture<'a>;
    fn get_self<'a>(
        &'a self,
        user_id: UserId,
        request_key: OrganizationProvisioningRequestKey,
    ) -> OrganizationProvisioningRecordFuture<'a>;
    fn get_active_self<'a>(&'a self, user_id: UserId) -> OrganizationProvisioningRecordFuture<'a>;
    fn list_admin<'a>(
        &'a self,
        query: OrganizationProvisioningListQuery,
        principal: SessionPrincipal,
    ) -> OrganizationProvisioningPageFuture<'a>;
    fn get_admin<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        principal: SessionPrincipal,
    ) -> OrganizationProvisioningRecordFuture<'a>;
    fn decide<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        principal: SessionPrincipal,
        command: OrganizationProvisioningDecisionCommand,
    ) -> OrganizationProvisioningResultFuture<'a>;
    fn list_materials_admin<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        principal: SessionPrincipal,
        request_id: String,
    ) -> OrganizationProvisioningMaterialPageFuture<'a>;
    fn download_material_admin<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        material_id: i64,
        principal: SessionPrincipal,
        request_id: String,
    ) -> OrganizationProvisioningMaterialDownloadFuture<'a>;
}

/// 数据库开通申请服务实现；管理员资格仍在仓储审批事务中复验。
pub struct DatabaseOrganizationProvisioningService {
    repository: Arc<OrganizationProvisioningRepository>,
    audit: Arc<dyn PlatformAuditService>,
}

impl DatabaseOrganizationProvisioningService {
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationProvisioningRepository>,
        audit: Arc<dyn PlatformAuditService>,
    ) -> Self {
        Self { repository, audit }
    }
}

impl OrganizationProvisioningService for DatabaseOrganizationProvisioningService {
    fn list_self<'a>(&'a self, user: UserId, query: OrganizationProvisioningListQuery) -> OrganizationProvisioningPageFuture<'a> {
        Box::pin(async move {
            let records = self.repository.list_requests_self(user, query.status(), query.before(), query.limit()).await.map_err(map_error)?;
            let next_cursor = (records.len() == query.limit() as usize).then(|| records.last().map(|r| r.id().get())).flatten();
            Ok(OrganizationProvisioningPage {records, next_cursor})
        })
    }
    fn list_materials_self<'a>(&'a self, user: UserId, key: OrganizationProvisioningRequestKey) -> OrganizationProvisioningMaterialPageFuture<'a> {
        Box::pin(async move {
            self.repository.get_request(user, key).await.map_err(map_error)?.ok_or(OrganizationProvisioningServiceError::NotFound)?;
            self.repository.list_materials(key).await.map_err(map_error)
        })
    }
    fn download_material_self<'a>(&'a self, user: UserId, key: OrganizationProvisioningRequestKey, id: i64) -> OrganizationProvisioningMaterialDownloadFuture<'a> {
        Box::pin(async move {
            let material = self.list_materials_self(user, key).await?.into_iter().find(|m| m.id() == id).ok_or(OrganizationProvisioningServiceError::NotFound)?;
            let bytes = self.repository.read_material_content(key,id).await.map_err(map_error)?;
            Ok((material.file_name().to_owned(),material.content_type().to_owned(),bytes))
        })
    }
    fn create<'a>(
        &'a self,
        user_id: UserId,
        command: OrganizationProvisioningCreateCommand,
    ) -> OrganizationProvisioningResultFuture<'a> {
        Box::pin(async move {
            let idempotency_key = OrganizationProvisioningIdempotencyKey::from_persistence_key(
                &command.idempotency_key,
            )
            .map_err(|_| OrganizationProvisioningServiceError::InvalidRequest)?;
            if let Some(record) = self
                .repository
                .get_by_idempotency_key(user_id, idempotency_key)
                .await
                .map_err(map_error)?
            {
                return Ok(OrganizationProvisioningResult {
                    record,
                    replayed: true,
                });
            }
            let mut bytes = [0_u8; 16];
            getrandom::fill(&mut bytes)
                .map_err(|_| OrganizationProvisioningServiceError::Internal)?;
            let request = OrganizationProvisioningRequest::new(
                OrganizationProvisioningRequestKey::new(bytes)
                    .map_err(|_| OrganizationProvisioningServiceError::Internal)?,
                idempotency_key,
                user_id,
                command.organization_name,
                command.organization_slug,
                command.business_reason,
                now(),
            )
            .map_err(|_| OrganizationProvisioningServiceError::InvalidRequest)?;
            let outcome = self
                .repository
                .create_request_with_materials(request, command.materials)
                .await
                .map_err(map_error)?;
            let (record, replayed) = match outcome {
                OrganizationProvisioningCreateOutcome::Created(record) => (record, false),
                OrganizationProvisioningCreateOutcome::Existing(record) => (record, true),
            };
            Ok(OrganizationProvisioningResult { record, replayed })
        })
    }

    fn get_self<'a>(
        &'a self,
        user_id: UserId,
        request_key: OrganizationProvisioningRequestKey,
    ) -> OrganizationProvisioningRecordFuture<'a> {
        Box::pin(async move {
            self.repository
                .get_request(user_id, request_key)
                .await
                .map_err(map_error)?
                .ok_or(OrganizationProvisioningServiceError::NotFound)
        })
    }

    fn get_active_self<'a>(&'a self, user_id: UserId) -> OrganizationProvisioningRecordFuture<'a> {
        Box::pin(async move {
            self.repository
                .get_active_request(user_id)
                .await
                .map_err(map_error)?
                .ok_or(OrganizationProvisioningServiceError::NotFound)
        })
    }

    fn list_admin<'a>(
        &'a self,
        query: OrganizationProvisioningListQuery,
        principal: SessionPrincipal,
    ) -> OrganizationProvisioningPageFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let records = self
                .repository
                .list_requests(query.status(), query.before(), query.limit())
                .await
                .map_err(map_error)?;
            let next_cursor = (records.len() == query.limit() as usize)
                .then(|| records.last().map(|record| record.id().get()))
                .flatten();
            Ok(OrganizationProvisioningPage {
                next_cursor,
                records,
            })
        })
    }

    fn get_admin<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        principal: SessionPrincipal,
    ) -> OrganizationProvisioningRecordFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .get_request_for_admin(request_key)
                .await
                .map_err(map_error)?
                .ok_or(OrganizationProvisioningServiceError::NotFound)
        })
    }

    fn decide<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        principal: SessionPrincipal,
        command: OrganizationProvisioningDecisionCommand,
    ) -> OrganizationProvisioningResultFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let entitlement = command.entitlement.map(to_entitlement).transpose()?;
            let write = OrganizationProvisioningDecisionWrite::new(
                request_key,
                command.expected_version,
                principal.user_id(),
                command.decision,
                command.decision_reason,
                entitlement,
            )
            .map_err(map_error)?;
            let outcome = self
                .repository
                .decide_request(write)
                .await
                .map_err(map_error)?;
            let (record, replayed) = match outcome {
                OrganizationProvisioningDecisionOutcome::Applied(record) => (record, false),
                OrganizationProvisioningDecisionOutcome::Existing(record) => (record, true),
            };
            Ok(OrganizationProvisioningResult { record, replayed })
        })
    }

    fn list_materials_admin<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        principal: SessionPrincipal,
        request_id: String,
    ) -> OrganizationProvisioningMaterialPageFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let materials = self
                .repository
                .list_materials(request_key)
                .await
                .map_err(map_error)?;
            self.audit_material_access(
                principal,
                request_key,
                None,
                "list",
                &request_id,
                materials.len(),
            )
            .await?;
            Ok(materials)
        })
    }

    fn download_material_admin<'a>(
        &'a self,
        request_key: OrganizationProvisioningRequestKey,
        material_id: i64,
        principal: SessionPrincipal,
        request_id: String,
    ) -> OrganizationProvisioningMaterialDownloadFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let material = self
                .repository
                .list_materials(request_key)
                .await
                .map_err(map_error)?
                .into_iter()
                .find(|material| material.id() == material_id)
                .ok_or(OrganizationProvisioningServiceError::NotFound)?;
            let content = self
                .repository
                .read_material_content(request_key, material_id)
                .await
                .map_err(map_error)?;
            self.audit_material_access(
                principal,
                request_key,
                Some(material_id),
                "download",
                &request_id,
                1,
            )
            .await?;
            Ok((
                material.file_name().to_owned(),
                material.content_type().to_owned(),
                content,
            ))
        })
    }
}

impl DatabaseOrganizationProvisioningService {
    async fn audit_material_access(
        &self,
        principal: SessionPrincipal,
        request_key: OrganizationProvisioningRequestKey,
        material_id: Option<i64>,
        operation: &'static str,
        request_id: &str,
        material_count: usize,
    ) -> Result<(), OrganizationProvisioningServiceError> {
        let resource_id = material_id.map_or_else(
            || request_key.persistence_key(),
            |material_id| format!("{}:{material_id}", request_key.persistence_key()),
        );
        let entry = PlatformAuditEntry::new(
            principal,
            PlatformPermission::OrganizationsReadAll,
            "/api/admin/organization-provisioning/{request_key}/materials",
            operation,
            "organization_provisioning_material",
            Some(resource_id),
            PlatformAuditOutcome::Succeeded,
            None,
            None,
            Some(serde_json::json!({ "material_count": material_count })),
            request_id,
        )
        .map_err(|_| OrganizationProvisioningServiceError::Internal)?;
        self.audit
            .record(entry)
            .await
            .map_err(|_| OrganizationProvisioningServiceError::Internal)
    }
}

impl fmt::Debug for DatabaseOrganizationProvisioningService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationProvisioningService(<repository>)")
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationProvisioningServiceError {
    #[error("企业开通申请请求无效")]
    InvalidRequest,
    #[error("企业开通申请不存在")]
    NotFound,
    #[error("企业开通审批权限不足")]
    Forbidden,
    #[error("企业开通申请存在冲突")]
    Conflict,
    #[error("企业开通申请写入结果未知或服务暂不可用")]
    Unavailable,
    #[error("企业开通申请服务暂不可用")]
    Internal,
}

fn to_entitlement(
    command: OrganizationProvisioningEntitlementCommand,
) -> Result<OrganizationEntitlementWrite, OrganizationProvisioningServiceError> {
    let snapshot = OrganizationEntitlementSnapshot::new(
        OrganizationEntitlementCapacity::new(
            command.seat_limit,
            command.team_limit,
            command.organization_key_limit,
        )
        .map_err(|_| OrganizationProvisioningServiceError::InvalidRequest)?,
        command.team_management_enabled,
        command.sso_enabled,
        command.custom_roles_enabled,
        command.audit_retention_days,
        command.valid_from,
        command.valid_until,
        command.grace_until,
    )
    .map_err(|_| OrganizationProvisioningServiceError::InvalidRequest)?;
    OrganizationEntitlementWrite::new(snapshot, "platform_provisioning".to_owned(), None, None)
        .map_err(|_| OrganizationProvisioningServiceError::InvalidRequest)
}

fn now() -> u64 {
    af_db::DatabaseTimestamp::now_utc()
        .unix_timestamp()
        .try_into()
        .unwrap_or_default()
}

fn map_error(
    error: OrganizationProvisioningRepositoryError,
) -> OrganizationProvisioningServiceError {
    match error {
        OrganizationProvisioningRepositoryError::InvalidInput => {
            OrganizationProvisioningServiceError::InvalidRequest
        }
        OrganizationProvisioningRepositoryError::NotFound => {
            OrganizationProvisioningServiceError::NotFound
        }
        OrganizationProvisioningRepositoryError::Forbidden
        | OrganizationProvisioningRepositoryError::ApplicantUnavailable => {
            OrganizationProvisioningServiceError::Forbidden
        }
        OrganizationProvisioningRepositoryError::VersionConflict
        | OrganizationProvisioningRepositoryError::Conflict
        | OrganizationProvisioningRepositoryError::ActiveApplicantConflict
        | OrganizationProvisioningRepositoryError::ActiveSlugConflict
        | OrganizationProvisioningRepositoryError::OrganizationSlugConflict => {
            OrganizationProvisioningServiceError::Conflict
        }
        OrganizationProvisioningRepositoryError::Query
        | OrganizationProvisioningRepositoryError::Timeout
        | OrganizationProvisioningRepositoryError::OutcomeUnknown => {
            OrganizationProvisioningServiceError::Unavailable
        }
        OrganizationProvisioningRepositoryError::Invariant
        | OrganizationProvisioningRepositoryError::EntropyUnavailable => {
            OrganizationProvisioningServiceError::Internal
        }
        OrganizationProvisioningRepositoryError::MaterialNotFound => {
            OrganizationProvisioningServiceError::NotFound
        }
        OrganizationProvisioningRepositoryError::MaterialContentUnavailable => {
            OrganizationProvisioningServiceError::Unavailable
        }
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), OrganizationProvisioningServiceError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(OrganizationProvisioningServiceError::Forbidden)
}
