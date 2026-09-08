use std::{future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationVerificationRepository, OrganizationVerificationRepositoryError,
    OrganizationVerificationWrite, OrganizationVerificationWriteOutcome,
};
use af_domain::{
    OrganizationId, OrganizationPermission, OrganizationVerificationCaseId,
    OrganizationVerificationMaterialId, OrganizationVerificationStatus, PlatformPermission,
};
use serde_json::json;
use thiserror::Error;

use crate::{
    OrganizationAuthorizationError, OrganizationAuthorizationService, PlatformAuditEntry,
    PlatformAuditOutcome, PlatformAuditService, PlatformPolicy, SessionPrincipal,
};

pub use af_db::{OrganizationVerificationBundle, OrganizationVerificationMaterialWrite};

/// 企业认证材料提交命令；正文由受控存储写入，服务层不把正文放进响应或审计。
pub struct OrganizationVerificationSubmitCommand {
    pub summary: String,
    pub materials: Vec<OrganizationVerificationMaterialWrite>,
}

/// 管理员下载材料时返回的受控正文与下载所需响应头信息。
pub struct OrganizationVerificationMaterialDownload {
    pub file_name: String,
    pub content_type: String,
    pub content: Vec<u8>,
}

/// 平台审核动作。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationVerificationDecisionKind {
    RequestSupplement,
    Approve,
    Reject,
}

/// 平台审核命令，使用案件版本做并发保护。
pub struct OrganizationVerificationDecisionCommand {
    pub expected_version: i64,
    pub decision: OrganizationVerificationDecisionKind,
    pub reason: Option<String>,
}

/// 平台认证队列查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationVerificationListQuery {
    pub status: Option<OrganizationVerificationStatus>,
    pub before: Option<i64>,
    pub limit: usize,
}

/// 平台认证队列分页结果。
pub struct OrganizationVerificationPage {
    bundles: Vec<OrganizationVerificationBundle>,
    next_cursor: Option<i64>,
}

impl OrganizationVerificationPage {
    #[must_use]
    pub fn bundles(&self) -> &[OrganizationVerificationBundle] {
        &self.bundles
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

pub type OrganizationVerificationBundleFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationVerificationBundle,
                    OrganizationVerificationServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationVerificationPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationVerificationPage, OrganizationVerificationServiceError>,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationVerificationMaterialDownloadFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationVerificationMaterialDownload,
                    OrganizationVerificationServiceError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 企业认证服务；企业成员权限和平台管理员权限保持独立。
pub trait OrganizationVerificationService: Send + Sync {
    fn submit<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationVerificationSubmitCommand,
    ) -> OrganizationVerificationBundleFuture<'a>;
    fn get_self<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
    ) -> OrganizationVerificationBundleFuture<'a>;
    fn list_admin<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: OrganizationVerificationListQuery,
        request_id: String,
    ) -> OrganizationVerificationPageFuture<'a>;
    fn get_admin<'a>(
        &'a self,
        principal: SessionPrincipal,
        case_id: OrganizationVerificationCaseId,
        request_id: String,
    ) -> OrganizationVerificationBundleFuture<'a>;
    fn decide<'a>(
        &'a self,
        principal: SessionPrincipal,
        case_id: OrganizationVerificationCaseId,
        command: OrganizationVerificationDecisionCommand,
        request_id: String,
    ) -> OrganizationVerificationBundleFuture<'a>;
    fn download_admin<'a>(
        &'a self,
        principal: SessionPrincipal,
        case_id: OrganizationVerificationCaseId,
        material_id: OrganizationVerificationMaterialId,
        request_id: String,
    ) -> OrganizationVerificationMaterialDownloadFuture<'a>;
}

/// 数据库企业认证服务实现。
pub struct DatabaseOrganizationVerificationService {
    repository: Arc<OrganizationVerificationRepository>,
    authorization: Arc<dyn OrganizationAuthorizationService>,
    audit: Arc<dyn PlatformAuditService>,
}

impl DatabaseOrganizationVerificationService {
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationVerificationRepository>,
        authorization: Arc<dyn OrganizationAuthorizationService>,
        audit: Arc<dyn PlatformAuditService>,
    ) -> Self {
        Self {
            repository,
            authorization,
            audit,
        }
    }
}

impl OrganizationVerificationService for DatabaseOrganizationVerificationService {
    fn submit<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationVerificationSubmitCommand,
    ) -> OrganizationVerificationBundleFuture<'a> {
        Box::pin(async move {
            let context = self
                .authorization
                .context(
                    principal,
                    organization_id,
                    OrganizationPermission::ProfileWrite,
                )
                .await
                .map_err(map_authorization)?;
            let write = OrganizationVerificationWrite {
                organization_id: context.organization_id(),
                applicant_user_id: principal.user_id(),
                summary: command.summary,
                materials: command.materials,
                created_at: now(),
            };
            let outcome = self
                .repository
                .create_or_resubmit(write)
                .await
                .map_err(map_repository)?;
            match outcome {
                OrganizationVerificationWriteOutcome::Created(bundle)
                | OrganizationVerificationWriteOutcome::Resubmitted(bundle)
                | OrganizationVerificationWriteOutcome::Existing(bundle) => Ok(bundle),
            }
        })
    }

    fn get_self<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
    ) -> OrganizationVerificationBundleFuture<'a> {
        Box::pin(async move {
            self.authorization
                .context(
                    principal,
                    organization_id,
                    OrganizationPermission::OverviewRead,
                )
                .await
                .map_err(map_authorization)?;
            self.repository
                .get_by_organization(organization_id)
                .await
                .map_err(map_repository)?
                .ok_or(OrganizationVerificationServiceError::NotFound)
        })
    }

    fn list_admin<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: OrganizationVerificationListQuery,
        request_id: String,
    ) -> OrganizationVerificationPageFuture<'a> {
        Box::pin(async move {
            PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsReadAll)
                .map_err(|_| OrganizationVerificationServiceError::Forbidden)?;
            if !(1..=100).contains(&query.limit) || query.before.is_some_and(|value| value <= 0) {
                return Err(OrganizationVerificationServiceError::InvalidRequest);
            }
            let bundles = self
                .repository
                .list(query.status, query.before, query.limit)
                .await
                .map_err(map_repository)?;
            for bundle in &bundles {
                self.audit_material_access(principal, bundle.case().id(), "list", &request_id)
                    .await?;
            }
            let next_cursor = (bundles.len() == query.limit)
                .then(|| bundles.last().map(|item| item.case().id().get()))
                .flatten();
            Ok(OrganizationVerificationPage {
                bundles,
                next_cursor,
            })
        })
    }

    fn get_admin<'a>(
        &'a self,
        principal: SessionPrincipal,
        case_id: OrganizationVerificationCaseId,
        request_id: String,
    ) -> OrganizationVerificationBundleFuture<'a> {
        Box::pin(async move {
            PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsReadAll)
                .map_err(|_| OrganizationVerificationServiceError::Forbidden)?;
            let bundle = self
                .repository
                .get_by_id(case_id)
                .await
                .map_err(map_repository)?
                .ok_or(OrganizationVerificationServiceError::NotFound)?;
            self.audit_material_access(principal, case_id, "get", &request_id)
                .await?;
            Ok(bundle)
        })
    }

    fn decide<'a>(
        &'a self,
        principal: SessionPrincipal,
        case_id: OrganizationVerificationCaseId,
        command: OrganizationVerificationDecisionCommand,
        request_id: String,
    ) -> OrganizationVerificationBundleFuture<'a> {
        Box::pin(async move {
            PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsManage)
                .map_err(|_| OrganizationVerificationServiceError::Forbidden)?;
            let status = match command.decision {
                OrganizationVerificationDecisionKind::RequestSupplement => {
                    OrganizationVerificationStatus::NeedsSupplement
                }
                OrganizationVerificationDecisionKind::Approve => {
                    OrganizationVerificationStatus::Approved
                }
                OrganizationVerificationDecisionKind::Reject => {
                    OrganizationVerificationStatus::Rejected
                }
            };
            let reason = match (status, command.reason) {
                (
                    OrganizationVerificationStatus::NeedsSupplement
                    | OrganizationVerificationStatus::Rejected,
                    Some(value),
                ) if !value.trim().is_empty() => Some(value),
                (OrganizationVerificationStatus::Approved, value) => value,
                _ => return Err(OrganizationVerificationServiceError::InvalidRequest),
            };
            let bundle = self
                .repository
                .decide(
                    case_id,
                    command.expected_version,
                    principal.user_id(),
                    status,
                    reason,
                    now(),
                )
                .await
                .map_err(map_repository)?;
            let entry = PlatformAuditEntry::new(
                principal,
                PlatformPermission::OrganizationsManage,
                "/api/admin/organization-verifications/{case_id}/decision",
                "decide",
                "organization_verification_case",
                Some(case_id.get().to_string()),
                PlatformAuditOutcome::Succeeded,
                None,
                Some(json!({"status": status_code(status), "version": bundle.case().version()})),
                Some(json!({"material_count": bundle.materials().len()})),
                &request_id,
            )
            .map_err(|_| OrganizationVerificationServiceError::Internal)?;
            self.audit
                .record(entry)
                .await
                .map_err(|_| OrganizationVerificationServiceError::Internal)?;
            Ok(bundle)
        })
    }

    fn download_admin<'a>(
        &'a self,
        principal: SessionPrincipal,
        case_id: OrganizationVerificationCaseId,
        material_id: OrganizationVerificationMaterialId,
        request_id: String,
    ) -> OrganizationVerificationMaterialDownloadFuture<'a> {
        Box::pin(async move {
            PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsReadAll)
                .map_err(|_| OrganizationVerificationServiceError::Forbidden)?;
            let bundle = self
                .repository
                .get_by_id(case_id)
                .await
                .map_err(map_repository)?
                .ok_or(OrganizationVerificationServiceError::NotFound)?;
            let material = bundle
                .materials()
                .iter()
                .find(|material| material.id() == material_id)
                .ok_or(OrganizationVerificationServiceError::NotFound)?;
            let content = self
                .repository
                .read_material_content(case_id, material_id)
                .await
                .map_err(map_repository)?;
            self.audit_material_access(principal, case_id, "download", &request_id)
                .await?;
            Ok(OrganizationVerificationMaterialDownload {
                file_name: material.file_name().to_owned(),
                content_type: material.content_type().to_owned(),
                content,
            })
        })
    }
}

impl DatabaseOrganizationVerificationService {
    async fn audit_material_access(
        &self,
        principal: SessionPrincipal,
        case_id: OrganizationVerificationCaseId,
        operation: &'static str,
        request_id: &str,
    ) -> Result<(), OrganizationVerificationServiceError> {
        let entry = PlatformAuditEntry::new(
            principal,
            PlatformPermission::OrganizationsReadAll,
            "/api/admin/organization-verifications",
            operation,
            "organization_verification_material",
            Some(case_id.get().to_string()),
            PlatformAuditOutcome::Succeeded,
            None,
            None,
            Some(json!({"materials": "metadata_only"})),
            request_id,
        )
        .map_err(|_| OrganizationVerificationServiceError::Internal)?;
        self.audit
            .record(entry)
            .await
            .map_err(|_| OrganizationVerificationServiceError::Internal)
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationVerificationServiceError {
    #[error("企业认证请求无效")]
    InvalidRequest,
    #[error("企业认证案件不存在")]
    NotFound,
    #[error("企业认证权限不足")]
    Forbidden,
    #[error("企业认证发生冲突")]
    Conflict,
    #[error("企业认证服务暂不可用")]
    Unavailable,
    #[error("企业认证内部错误")]
    Internal,
}

fn map_authorization(
    error: OrganizationAuthorizationError,
) -> OrganizationVerificationServiceError {
    match error {
        OrganizationAuthorizationError::NotFound => OrganizationVerificationServiceError::NotFound,
        OrganizationAuthorizationError::Forbidden => {
            OrganizationVerificationServiceError::Forbidden
        }
        OrganizationAuthorizationError::Internal => {
            OrganizationVerificationServiceError::Unavailable
        }
    }
}

fn map_repository(
    error: OrganizationVerificationRepositoryError,
) -> OrganizationVerificationServiceError {
    match error {
        OrganizationVerificationRepositoryError::InvalidInput => {
            OrganizationVerificationServiceError::InvalidRequest
        }
        OrganizationVerificationRepositoryError::NotFound => {
            OrganizationVerificationServiceError::NotFound
        }
        OrganizationVerificationRepositoryError::Conflict
        | OrganizationVerificationRepositoryError::AlreadyApproved => {
            OrganizationVerificationServiceError::Conflict
        }
        OrganizationVerificationRepositoryError::ApplicantUnavailable => {
            OrganizationVerificationServiceError::Forbidden
        }
        OrganizationVerificationRepositoryError::OutcomeUnknown
        | OrganizationVerificationRepositoryError::Query => {
            OrganizationVerificationServiceError::Unavailable
        }
        OrganizationVerificationRepositoryError::MaterialNotFound => {
            OrganizationVerificationServiceError::NotFound
        }
        OrganizationVerificationRepositoryError::MaterialContentUnavailable => {
            OrganizationVerificationServiceError::Unavailable
        }
        OrganizationVerificationRepositoryError::Invariant => {
            OrganizationVerificationServiceError::Internal
        }
    }
}

fn status_code(status: OrganizationVerificationStatus) -> &'static str {
    match status {
        OrganizationVerificationStatus::Submitted => "submitted",
        OrganizationVerificationStatus::InReview => "in_review",
        OrganizationVerificationStatus::NeedsSupplement => "needs_supplement",
        OrganizationVerificationStatus::Approved => "approved",
        OrganizationVerificationStatus::Rejected => "rejected",
    }
}

fn now() -> u64 {
    af_db::DatabaseTimestamp::now_utc()
        .unix_timestamp()
        .try_into()
        .unwrap_or_default()
}
