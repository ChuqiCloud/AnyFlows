use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    OrganizationRepository, OrganizationRepositoryError, PlatformOrganizationDirectoryRecord,
    PlatformUserOrganizationRecord,
};
use af_domain::{
    OrganizationEntitlementStatus, OrganizationId, OrganizationMembershipId,
    OrganizationMembershipStatus, OrganizationRole, OrganizationStatus, PlatformPermission, UserId,
};
use serde_json::json;
use thiserror::Error;

use crate::{
    PlatformAuditEntry, PlatformAuditOutcome, PlatformAuditService, PlatformPolicy,
    SessionPrincipal,
};

/// 平台企业目录默认页大小。
pub const DEFAULT_PLATFORM_ORGANIZATION_DIRECTORY_PAGE_SIZE: usize = 50;
/// 跨企业访问理由最大 UTF-8 字节数。
pub const MAX_PLATFORM_ORGANIZATION_REASON_BYTES: usize = 255;

/// 平台企业目录使用的稳定正向游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformOrganizationDirectoryQuery {
    after: Option<OrganizationId>,
    limit: usize,
}

impl PlatformOrganizationDirectoryQuery {
    /// 校验企业 ID 游标与页大小。
    pub fn new(
        after: Option<OrganizationId>,
        limit: usize,
    ) -> Result<Self, PlatformOrganizationDirectoryError> {
        validate_limit(limit)?;
        Ok(Self { after, limit })
    }

    #[must_use]
    pub const fn after(self) -> Option<OrganizationId> {
        self.after
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for PlatformOrganizationDirectoryQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_PLATFORM_ORGANIZATION_DIRECTORY_PAGE_SIZE,
        }
    }
}

/// 平台按用户读取企业归属时使用的成员游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformUserOrganizationQuery {
    after: Option<OrganizationMembershipId>,
    limit: usize,
}

impl PlatformUserOrganizationQuery {
    /// 校验成员关系 ID 游标与页大小。
    pub fn new(
        after: Option<OrganizationMembershipId>,
        limit: usize,
    ) -> Result<Self, PlatformOrganizationDirectoryError> {
        validate_limit(limit)?;
        Ok(Self { after, limit })
    }

    #[must_use]
    pub const fn after(self) -> Option<OrganizationMembershipId> {
        self.after
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for PlatformUserOrganizationQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_PLATFORM_ORGANIZATION_DIRECTORY_PAGE_SIZE,
        }
    }
}

/// 平台企业目录公开的最小安全摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformOrganizationSummary {
    organization_id: OrganizationId,
    name: String,
    slug: String,
    organization_status: OrganizationStatus,
    entitlement_status: OrganizationEntitlementStatus,
    owner_membership_id: OrganizationMembershipId,
    owner_user_id: UserId,
    owner_username: String,
    active_member_count: i64,
    seat_limit: i64,
    valid_until: i64,
    grace_until: Option<i64>,
    version: i64,
}

impl PlatformOrganizationSummary {
    fn from_record(record: PlatformOrganizationDirectoryRecord, now: i64) -> Self {
        let organization = record.organization();
        let entitlement = record.entitlement();
        let owner = record.owner_membership();
        Self {
            organization_id: organization.id(),
            name: organization.name().to_owned(),
            slug: organization.slug().to_owned(),
            organization_status: organization.status(),
            entitlement_status: entitlement.status(organization.status(), now),
            owner_membership_id: owner.id(),
            owner_user_id: owner.user_id(),
            owner_username: record.owner_username().to_owned(),
            active_member_count: record.active_member_count(),
            seat_limit: entitlement.capacity().seat_limit(),
            valid_until: entitlement.valid_until(),
            grace_until: entitlement.grace_until(),
            version: organization.version(),
        }
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.slug
    }
    #[must_use]
    pub const fn organization_status(&self) -> OrganizationStatus {
        self.organization_status
    }
    #[must_use]
    pub const fn entitlement_status(&self) -> OrganizationEntitlementStatus {
        self.entitlement_status
    }
    #[must_use]
    pub const fn owner_membership_id(&self) -> OrganizationMembershipId {
        self.owner_membership_id
    }
    #[must_use]
    pub const fn owner_user_id(&self) -> UserId {
        self.owner_user_id
    }
    #[must_use]
    pub fn owner_username(&self) -> &str {
        &self.owner_username
    }
    #[must_use]
    pub const fn active_member_count(&self) -> i64 {
        self.active_member_count
    }
    #[must_use]
    pub const fn seat_limit(&self) -> i64 {
        self.seat_limit
    }
    #[must_use]
    pub const fn valid_until(&self) -> i64 {
        self.valid_until
    }
    #[must_use]
    pub const fn grace_until(&self) -> Option<i64> {
        self.grace_until
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 一页平台企业目录。
#[derive(Debug, Eq, PartialEq)]
pub struct PlatformOrganizationDirectoryPage {
    organizations: Vec<PlatformOrganizationSummary>,
    next_cursor: Option<OrganizationId>,
}

impl PlatformOrganizationDirectoryPage {
    #[must_use]
    pub fn organizations(&self) -> &[PlatformOrganizationSummary] {
        &self.organizations
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationId> {
        self.next_cursor
    }
}

/// 用户在单个企业中的成员关系安全摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformUserOrganizationSummary {
    organization_id: OrganizationId,
    organization_name: String,
    organization_slug: String,
    organization_status: OrganizationStatus,
    entitlement_status: OrganizationEntitlementStatus,
    membership_id: OrganizationMembershipId,
    role: OrganizationRole,
    membership_status: OrganizationMembershipStatus,
    joined_at: i64,
    version: i64,
}

impl PlatformUserOrganizationSummary {
    fn from_record(record: PlatformUserOrganizationRecord, now: i64) -> Self {
        let organization = record.organization();
        let membership = record.membership();
        Self {
            organization_id: organization.id(),
            organization_name: organization.name().to_owned(),
            organization_slug: organization.slug().to_owned(),
            organization_status: organization.status(),
            entitlement_status: record.entitlement().status(organization.status(), now),
            membership_id: membership.id(),
            role: membership.role(),
            membership_status: membership.status(),
            joined_at: membership.joined_at(),
            version: membership.version(),
        }
    }

    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn organization_name(&self) -> &str {
        &self.organization_name
    }
    #[must_use]
    pub fn organization_slug(&self) -> &str {
        &self.organization_slug
    }
    #[must_use]
    pub const fn organization_status(&self) -> OrganizationStatus {
        self.organization_status
    }
    #[must_use]
    pub const fn entitlement_status(&self) -> OrganizationEntitlementStatus {
        self.entitlement_status
    }
    #[must_use]
    pub const fn membership_id(&self) -> OrganizationMembershipId {
        self.membership_id
    }
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }
    #[must_use]
    pub const fn membership_status(&self) -> OrganizationMembershipStatus {
        self.membership_status
    }
    #[must_use]
    pub const fn joined_at(&self) -> i64 {
        self.joined_at
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 一页指定用户的企业归属。
#[derive(Debug, Eq, PartialEq)]
pub struct PlatformUserOrganizationPage {
    organizations: Vec<PlatformUserOrganizationSummary>,
    next_cursor: Option<OrganizationMembershipId>,
}

impl PlatformUserOrganizationPage {
    #[must_use]
    pub fn organizations(&self) -> &[PlatformUserOrganizationSummary] {
        &self.organizations
    }
    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationMembershipId> {
        self.next_cursor
    }
}

/// 平台企业目录错误不携带企业或用户内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlatformOrganizationDirectoryError {
    #[error("平台企业目录输入无效")]
    InvalidInput,
    #[error("当前会话无权读取平台企业目录")]
    Forbidden,
    #[error("目标用户不存在")]
    NotFound,
    #[error("平台企业目录内部失败")]
    Internal,
}

pub type PlatformOrganizationDirectoryFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    PlatformOrganizationDirectoryPage,
                    PlatformOrganizationDirectoryError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type PlatformUserOrganizationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<PlatformUserOrganizationPage, PlatformOrganizationDirectoryError>,
            > + Send
            + 'a,
    >,
>;

/// 平台企业目录只读端口；所有跨企业读取都必须携带理由和请求标识。
pub trait PlatformOrganizationDirectoryService: Send + Sync {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: PlatformOrganizationDirectoryQuery,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationDirectoryFuture<'a>;
    fn list_for_user<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        query: PlatformUserOrganizationQuery,
        reason: String,
        request_id: String,
    ) -> PlatformUserOrganizationFuture<'a>;
}

/// 基于企业仓储和平台只追加审计的目录服务。
pub struct DatabasePlatformOrganizationDirectoryService {
    repository: Arc<OrganizationRepository>,
    audit_service: Arc<dyn PlatformAuditService>,
}

impl DatabasePlatformOrganizationDirectoryService {
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationRepository>,
        audit_service: Arc<dyn PlatformAuditService>,
    ) -> Self {
        Self {
            repository,
            audit_service,
        }
    }

    async fn audit(
        &self,
        principal: SessionPrincipal,
        route: &'static str,
        operation: &'static str,
        resource_id: Option<String>,
        outcome: PlatformAuditOutcome,
        audit_info: serde_json::Value,
        request_id: &str,
    ) -> Result<(), PlatformOrganizationDirectoryError> {
        let entry = PlatformAuditEntry::new(
            principal,
            PlatformPermission::OrganizationsReadAll,
            route,
            operation,
            "organization_directory",
            resource_id,
            outcome,
            None,
            None,
            Some(audit_info),
            request_id,
        )
        .map_err(|_| PlatformOrganizationDirectoryError::Internal)?;
        self.audit_service
            .record(entry)
            .await
            .map_err(|_| PlatformOrganizationDirectoryError::Internal)
    }
}

impl PlatformOrganizationDirectoryService for DatabasePlatformOrganizationDirectoryService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: PlatformOrganizationDirectoryQuery,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationDirectoryFuture<'a> {
        Box::pin(async move {
            if let Err(error) = validate_access(principal, &reason) {
                self.audit(
                    principal,
                    "/api/platform/organizations",
                    "platform.organization_directory.list",
                    None,
                    if error == PlatformOrganizationDirectoryError::Forbidden {
                        PlatformAuditOutcome::Denied
                    } else {
                        PlatformAuditOutcome::Failed
                    },
                    json!({"failure": access_error_code(error)}),
                    &request_id,
                )
                .await?;
                return Err(error);
            }
            let page = match self
                .repository
                .list_platform_directory(query.after(), query.limit())
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    self.audit(
                        principal,
                        "/api/platform/organizations",
                        "platform.organization_directory.list",
                        None,
                        PlatformAuditOutcome::Failed,
                        json!({"platform_reason": reason, "failure": repository_error_code(error)}),
                        &request_id,
                    )
                    .await?;
                    return Err(map_repository_error(error));
                }
            };
            let (records, next_cursor) = page.into_parts();
            let now = current_unix_timestamp()?;
            let organizations = records
                .into_iter()
                .map(|record| PlatformOrganizationSummary::from_record(record, now))
                .collect::<Vec<_>>();
            self.audit(principal, "/api/platform/organizations", "platform.organization_directory.list", None, PlatformAuditOutcome::Succeeded, json!({"platform_reason": reason, "after": query.after().map(OrganizationId::get), "limit": query.limit(), "returned_count": organizations.len()}), &request_id).await?;
            Ok(PlatformOrganizationDirectoryPage {
                organizations,
                next_cursor,
            })
        })
    }

    fn list_for_user<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        query: PlatformUserOrganizationQuery,
        reason: String,
        request_id: String,
    ) -> PlatformUserOrganizationFuture<'a> {
        Box::pin(async move {
            if let Err(error) = validate_access(principal, &reason) {
                self.audit(
                    principal,
                    "/api/platform/users/{user_id}/organizations",
                    "platform.user_organizations.list",
                    Some(user_id.get().to_string()),
                    if error == PlatformOrganizationDirectoryError::Forbidden {
                        PlatformAuditOutcome::Denied
                    } else {
                        PlatformAuditOutcome::Failed
                    },
                    json!({"failure": access_error_code(error)}),
                    &request_id,
                )
                .await?;
                return Err(error);
            }
            let page = match self
                .repository
                .list_user_organizations(user_id, query.after(), query.limit())
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    self.audit(
                        principal,
                        "/api/platform/users/{user_id}/organizations",
                        "platform.user_organizations.list",
                        Some(user_id.get().to_string()),
                        PlatformAuditOutcome::Failed,
                        json!({"platform_reason": reason, "failure": repository_error_code(error)}),
                        &request_id,
                    )
                    .await?;
                    return Err(map_repository_error(error));
                }
            };
            let (records, next_cursor) = page.into_parts();
            let now = current_unix_timestamp()?;
            let organizations = records
                .into_iter()
                .map(|record| PlatformUserOrganizationSummary::from_record(record, now))
                .collect::<Vec<_>>();
            self.audit(principal, "/api/platform/users/{user_id}/organizations", "platform.user_organizations.list", Some(user_id.get().to_string()), PlatformAuditOutcome::Succeeded, json!({"platform_reason": reason, "after": query.after().map(OrganizationMembershipId::get), "limit": query.limit(), "returned_count": organizations.len()}), &request_id).await?;
            Ok(PlatformUserOrganizationPage {
                organizations,
                next_cursor,
            })
        })
    }
}

impl fmt::Debug for DatabasePlatformOrganizationDirectoryService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabasePlatformOrganizationDirectoryService(<redacted>)")
    }
}

fn validate_access(
    principal: SessionPrincipal,
    reason: &str,
) -> Result<(), PlatformOrganizationDirectoryError> {
    PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsReadAll)
        .map_err(|_| PlatformOrganizationDirectoryError::Forbidden)?;
    if reason.is_empty()
        || reason.len() > MAX_PLATFORM_ORGANIZATION_REASON_BYTES
        || reason.trim() != reason
        || reason.chars().any(char::is_control)
    {
        return Err(PlatformOrganizationDirectoryError::InvalidInput);
    }
    Ok(())
}

fn validate_limit(limit: usize) -> Result<(), PlatformOrganizationDirectoryError> {
    if !(1..=af_db::MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE).contains(&limit) {
        return Err(PlatformOrganizationDirectoryError::InvalidInput);
    }
    Ok(())
}

fn current_unix_timestamp() -> Result<i64, PlatformOrganizationDirectoryError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_secs()).ok())
        .ok_or(PlatformOrganizationDirectoryError::Internal)
}

const fn map_repository_error(
    error: OrganizationRepositoryError,
) -> PlatformOrganizationDirectoryError {
    match error {
        OrganizationRepositoryError::InvalidInput => {
            PlatformOrganizationDirectoryError::InvalidInput
        }
        OrganizationRepositoryError::NotFound => PlatformOrganizationDirectoryError::NotFound,
        _ => PlatformOrganizationDirectoryError::Internal,
    }
}

const fn repository_error_code(error: OrganizationRepositoryError) -> &'static str {
    match error {
        OrganizationRepositoryError::InvalidInput => "invalid_input",
        OrganizationRepositoryError::NotFound => "not_found",
        _ => "internal",
    }
}

const fn access_error_code(error: PlatformOrganizationDirectoryError) -> &'static str {
    match error {
        PlatformOrganizationDirectoryError::InvalidInput => "invalid_reason",
        PlatformOrganizationDirectoryError::Forbidden => "forbidden",
        PlatformOrganizationDirectoryError::NotFound => "not_found",
        PlatformOrganizationDirectoryError::Internal => "internal",
    }
}

#[cfg(test)]
mod tests {
    use crate::SessionRole;

    use super::*;

    #[test]
    fn platform_directory_requires_admin_and_a_bounded_reason() {
        let admin = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin);
        let user = SessionPrincipal::new(UserId::new(2).unwrap(), SessionRole::User);

        assert_eq!(
            validate_access(user, "核查企业归属"),
            Err(PlatformOrganizationDirectoryError::Forbidden)
        );
        assert_eq!(
            validate_access(admin, " leading"),
            Err(PlatformOrganizationDirectoryError::InvalidInput)
        );
        assert_eq!(
            validate_access(admin, "line\nbreak"),
            Err(PlatformOrganizationDirectoryError::InvalidInput)
        );
        assert_eq!(
            validate_access(admin, &"企".repeat(86)),
            Err(PlatformOrganizationDirectoryError::InvalidInput)
        );
        assert!(validate_access(admin, "核查企业归属").is_ok());
    }

    #[test]
    fn platform_directory_queries_enforce_page_bounds() {
        assert!(PlatformOrganizationDirectoryQuery::new(None, 1).is_ok());
        assert!(
            PlatformUserOrganizationQuery::new(None, af_db::MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE)
                .is_ok()
        );
        assert_eq!(
            PlatformOrganizationDirectoryQuery::new(None, 0),
            Err(PlatformOrganizationDirectoryError::InvalidInput)
        );
        assert_eq!(
            PlatformUserOrganizationQuery::new(
                None,
                af_db::MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE + 1,
            ),
            Err(PlatformOrganizationDirectoryError::InvalidInput)
        );
    }
}
