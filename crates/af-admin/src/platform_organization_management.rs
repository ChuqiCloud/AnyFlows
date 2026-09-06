use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationMembershipChange, OrganizationRepository, OrganizationRepositoryError,
    PlatformOrganizationMemberRecord,
};
use af_domain::{
    OrganizationId, OrganizationMembershipId, OrganizationMembershipStatus, OrganizationRole,
    OrganizationTeamId, PlatformPermission, UserId,
};
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    IssuedOrganizationInvitation, MAX_PLATFORM_ORGANIZATION_REASON_BYTES,
    OrganizationInvitationCommand, OrganizationInvitationError, OrganizationInvitationService,
    PlatformAuditEntry, PlatformAuditOutcome, PlatformAuditService, PlatformPolicy,
    SessionPrincipal,
};

/// 平台企业成员目录默认页大小。
pub const DEFAULT_PLATFORM_ORGANIZATION_MEMBER_PAGE_SIZE: usize = 50;

/// 平台管理员读取单个企业成员时使用的稳定游标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformOrganizationMemberQuery {
    after: Option<OrganizationMembershipId>,
    limit: usize,
}

impl PlatformOrganizationMemberQuery {
    /// 校验成员 ID 游标和单页容量。
    pub fn new(
        after: Option<OrganizationMembershipId>,
        limit: usize,
    ) -> Result<Self, PlatformOrganizationManagementError> {
        if !(1..=af_db::MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE).contains(&limit) {
            return Err(PlatformOrganizationManagementError::InvalidInput);
        }
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

impl Default for PlatformOrganizationMemberQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_PLATFORM_ORGANIZATION_MEMBER_PAGE_SIZE,
        }
    }
}

/// 平台成员目录返回的身份与成员关系摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformOrganizationMember {
    membership_id: OrganizationMembershipId,
    organization_id: OrganizationId,
    user_id: UserId,
    username: String,
    email: Option<String>,
    role: OrganizationRole,
    team_id: Option<OrganizationTeamId>,
    status: OrganizationMembershipStatus,
    joined_at: i64,
    suspended_at: Option<i64>,
    removed_at: Option<i64>,
    version: i64,
}

impl PlatformOrganizationMember {
    fn from_record(record: PlatformOrganizationMemberRecord) -> Self {
        let membership = record.membership();
        Self {
            membership_id: membership.id(),
            organization_id: membership.organization_id(),
            user_id: membership.user_id(),
            username: record.username().to_owned(),
            email: record.email().map(str::to_owned),
            role: membership.role(),
            team_id: membership.team_id(),
            status: membership.status(),
            joined_at: membership.joined_at(),
            suspended_at: membership.suspended_at(),
            removed_at: membership.removed_at(),
            version: membership.version(),
        }
    }

    fn from_membership(membership: &af_db::OrganizationMembershipRecord, identity: &Self) -> Self {
        Self {
            membership_id: membership.id(),
            organization_id: membership.organization_id(),
            user_id: membership.user_id(),
            username: identity.username.clone(),
            email: identity.email.clone(),
            role: membership.role(),
            team_id: membership.team_id(),
            status: membership.status(),
            joined_at: membership.joined_at(),
            suspended_at: membership.suspended_at(),
            removed_at: membership.removed_at(),
            version: membership.version(),
        }
    }

    #[must_use]
    pub const fn membership_id(&self) -> OrganizationMembershipId {
        self.membership_id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }
    #[must_use]
    pub const fn team_id(&self) -> Option<OrganizationTeamId> {
        self.team_id
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationMembershipStatus {
        self.status
    }
    #[must_use]
    pub const fn joined_at(&self) -> i64 {
        self.joined_at
    }
    #[must_use]
    pub const fn suspended_at(&self) -> Option<i64> {
        self.suspended_at
    }
    #[must_use]
    pub const fn removed_at(&self) -> Option<i64> {
        self.removed_at
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

/// 平台企业成员稳定游标页。
#[derive(Debug, Eq, PartialEq)]
pub struct PlatformOrganizationMemberPage {
    records: Vec<PlatformOrganizationMember>,
    next_cursor: Option<OrganizationMembershipId>,
}

impl PlatformOrganizationMemberPage {
    #[must_use]
    pub fn records(&self) -> &[PlatformOrganizationMember] {
        &self.records
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationMembershipId> {
        self.next_cursor
    }
}

/// 平台管理员提交的成员角色与状态完整目标快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformOrganizationMemberChangeCommand {
    role: OrganizationRole,
    status: OrganizationMembershipStatus,
    expected_version: i64,
}

impl PlatformOrganizationMemberChangeCommand {
    /// 构造带 CAS 版本的固定角色和状态变更。
    pub fn new(
        role: OrganizationRole,
        status: OrganizationMembershipStatus,
        expected_version: i64,
    ) -> Result<Self, PlatformOrganizationManagementError> {
        if expected_version <= 0 {
            return Err(PlatformOrganizationManagementError::InvalidInput);
        }
        Ok(Self {
            role,
            status,
            expected_version,
        })
    }
}

/// 平台跨企业成员操作的闭合错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlatformOrganizationManagementError {
    #[error("平台企业成员操作输入无效")]
    InvalidInput,
    #[error("当前会话无权管理平台企业成员")]
    Forbidden,
    #[error("目标企业或成员不存在")]
    NotFound,
    #[error("企业成员版本冲突")]
    Conflict,
    #[error("企业成员状态转换无效")]
    InvalidTransition,
    #[error("企业必须保留至少一个活动所有者")]
    LastOwner,
    #[error("企业席位容量已用尽")]
    SeatLimitReached,
    #[error("企业商业授权限制当前操作")]
    EntitlementRestricted,
    #[error("平台企业成员操作内部失败")]
    Internal,
}

pub type PlatformOrganizationMemberPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    PlatformOrganizationMemberPage,
                    PlatformOrganizationManagementError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type PlatformOrganizationMemberFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PlatformOrganizationMember, PlatformOrganizationManagementError>>
            + Send
            + 'a,
    >,
>;
pub type PlatformOrganizationInvitationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<IssuedOrganizationInvitation, PlatformOrganizationManagementError>,
            > + Send
            + 'a,
    >,
>;

/// 平台成员管理端口；所有方法都要求一次性理由与请求标识。
pub trait PlatformOrganizationManagementService: Send + Sync {
    fn list_members<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: PlatformOrganizationMemberQuery,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationMemberPageFuture<'a>;

    fn change_member<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        command: PlatformOrganizationMemberChangeCommand,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationMemberFuture<'a>;

    fn invite_member<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationInvitationCommand,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationInvitationFuture<'a>;
}

/// 复用企业仓储不变量和现有邀请接受语义的平台成员管理实现。
pub struct DatabasePlatformOrganizationManagementService {
    repository: Arc<OrganizationRepository>,
    invitation_service: Arc<dyn OrganizationInvitationService>,
    audit_service: Arc<dyn PlatformAuditService>,
}

impl DatabasePlatformOrganizationManagementService {
    #[must_use]
    pub fn new(
        repository: Arc<OrganizationRepository>,
        invitation_service: Arc<dyn OrganizationInvitationService>,
        audit_service: Arc<dyn PlatformAuditService>,
    ) -> Self {
        Self {
            repository,
            invitation_service,
            audit_service,
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn audit(
        &self,
        principal: SessionPrincipal,
        route: &'static str,
        operation: &'static str,
        resource: &'static str,
        resource_id: Option<String>,
        outcome: PlatformAuditOutcome,
        before_value: Option<Value>,
        after_value: Option<Value>,
        audit_info: Value,
        request_id: &str,
    ) -> Result<(), PlatformOrganizationManagementError> {
        let entry = PlatformAuditEntry::new(
            principal,
            PlatformPermission::OrganizationsManage,
            route,
            operation,
            resource,
            resource_id,
            outcome,
            before_value,
            after_value,
            Some(audit_info),
            request_id,
        )
        .map_err(|_| PlatformOrganizationManagementError::Internal)?;
        self.audit_service
            .record(entry)
            .await
            .map_err(|_| PlatformOrganizationManagementError::Internal)
    }

    async fn audit_access_error(
        &self,
        principal: SessionPrincipal,
        route: &'static str,
        operation: &'static str,
        resource: &'static str,
        resource_id: String,
        error: PlatformOrganizationManagementError,
        request_id: &str,
    ) -> Result<(), PlatformOrganizationManagementError> {
        self.audit(
            principal,
            route,
            operation,
            resource,
            Some(resource_id),
            if error == PlatformOrganizationManagementError::Forbidden {
                PlatformAuditOutcome::Denied
            } else {
                PlatformAuditOutcome::Failed
            },
            None,
            None,
            json!({"failure": management_error_code(error)}),
            request_id,
        )
        .await
    }
}

impl PlatformOrganizationManagementService for DatabasePlatformOrganizationManagementService {
    fn list_members<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        query: PlatformOrganizationMemberQuery,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationMemberPageFuture<'a> {
        Box::pin(async move {
            const ROUTE: &str = "/api/platform/organizations/{organization_id}/members";
            const OPERATION: &str = "platform.organization_members.list";
            if let Err(error) = validate_access(principal, &reason) {
                self.audit_access_error(
                    principal,
                    ROUTE,
                    OPERATION,
                    "organization",
                    organization_id.get().to_string(),
                    error,
                    &request_id,
                )
                .await?;
                return Err(error);
            }
            let page = match self
                .repository
                .list_platform_members(organization_id, query.after(), query.limit())
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    self.audit(
                        principal,
                        ROUTE,
                        OPERATION,
                        "organization",
                        Some(organization_id.get().to_string()),
                        PlatformAuditOutcome::Failed,
                        None,
                        None,
                        json!({"platform_reason": reason, "failure": repository_error_code(error)}),
                        &request_id,
                    )
                    .await?;
                    return Err(map_repository_error(error));
                }
            };
            let (records, next_cursor) = page.into_parts();
            let records = records
                .into_iter()
                .map(PlatformOrganizationMember::from_record)
                .collect::<Vec<_>>();
            self.audit(
                principal,
                ROUTE,
                OPERATION,
                "organization",
                Some(organization_id.get().to_string()),
                PlatformAuditOutcome::Succeeded,
                None,
                None,
                json!({"platform_reason": reason, "after": query.after().map(OrganizationMembershipId::get), "limit": query.limit(), "returned_count": records.len()}),
                &request_id,
            )
            .await?;
            Ok(PlatformOrganizationMemberPage {
                records,
                next_cursor,
            })
        })
    }

    fn change_member<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        command: PlatformOrganizationMemberChangeCommand,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationMemberFuture<'a> {
        Box::pin(async move {
            const ROUTE: &str =
                "/api/platform/organizations/{organization_id}/members/{membership_id}";
            const OPERATION: &str = "platform.organization_member.change";
            if let Err(error) = validate_access(principal, &reason) {
                self.audit_access_error(
                    principal,
                    ROUTE,
                    OPERATION,
                    "membership",
                    membership_id.get().to_string(),
                    error,
                    &request_id,
                )
                .await?;
                return Err(error);
            }
            let identity = match self
                .repository
                .platform_member(organization_id, membership_id)
                .await
            {
                Ok(record) => PlatformOrganizationMember::from_record(record),
                Err(error) => {
                    self.audit(
                        principal,
                        ROUTE,
                        OPERATION,
                        "membership",
                        Some(membership_id.get().to_string()),
                        PlatformAuditOutcome::Failed,
                        None,
                        None,
                        json!({"platform_reason": reason, "organization_id": organization_id.get(), "failure": repository_error_code(error)}),
                        &request_id,
                    )
                    .await?;
                    return Err(map_repository_error(error));
                }
            };
            let before = member_audit_value(&identity);
            // Owner 身份仍只能走企业所有权转移，平台代操作不能绕过双成员原子 CAS。
            if identity.role().is_owner() != command.role.is_owner() {
                self.audit(
                    principal,
                    ROUTE,
                    OPERATION,
                    "membership",
                    Some(membership_id.get().to_string()),
                    PlatformAuditOutcome::Failed,
                    Some(before),
                    None,
                    json!({"platform_reason": reason, "organization_id": organization_id.get(), "failure": "owner_transfer_required"}),
                    &request_id,
                )
                .await?;
                return Err(PlatformOrganizationManagementError::InvalidTransition);
            }
            let before = member_audit_value(&identity);
            let updated = match self
                .repository
                .change_membership(
                    organization_id,
                    membership_id,
                    OrganizationMembershipChange::new(
                        command.role,
                        command.status,
                        identity.team_id(),
                        command.expected_version,
                    ),
                )
                .await
            {
                Ok(record) => record,
                Err(error) => {
                    self.audit(
                        principal,
                        ROUTE,
                        OPERATION,
                        "membership",
                        Some(membership_id.get().to_string()),
                        PlatformAuditOutcome::Failed,
                        Some(before),
                        None,
                        json!({"platform_reason": reason, "organization_id": organization_id.get(), "failure": repository_error_code(error)}),
                        &request_id,
                    )
                    .await?;
                    return Err(map_repository_error(error));
                }
            };
            let updated = PlatformOrganizationMember::from_membership(&updated, &identity);
            self.audit(
                principal,
                ROUTE,
                OPERATION,
                "membership",
                Some(membership_id.get().to_string()),
                PlatformAuditOutcome::Succeeded,
                Some(before),
                Some(member_audit_value(&updated)),
                json!({"platform_reason": reason, "organization_id": organization_id.get()}),
                &request_id,
            )
            .await?;
            Ok(updated)
        })
    }

    fn invite_member<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        command: OrganizationInvitationCommand,
        reason: String,
        request_id: String,
    ) -> PlatformOrganizationInvitationFuture<'a> {
        Box::pin(async move {
            const ROUTE: &str = "/api/platform/organizations/{organization_id}/invitations";
            const OPERATION: &str = "platform.organization_invitation.create";
            if let Err(error) = validate_access(principal, &reason) {
                self.audit_access_error(
                    principal,
                    ROUTE,
                    OPERATION,
                    "invitation",
                    "new".to_owned(),
                    error,
                    &request_id,
                )
                .await?;
                return Err(error);
            }
            let issued = match self
                .invitation_service
                .create(principal, organization_id, command)
                .await
            {
                Ok(issued) => issued,
                Err(error) => {
                    self.audit(
                        principal,
                        ROUTE,
                        OPERATION,
                        "invitation",
                        Some("new".to_owned()),
                        PlatformAuditOutcome::Failed,
                        None,
                        None,
                        json!({"platform_reason": reason, "organization_id": organization_id.get(), "failure": invitation_error_code(error)}),
                        &request_id,
                    )
                    .await?;
                    return Err(map_invitation_error(error));
                }
            };
            let record = issued.record();
            self.audit(
                principal,
                ROUTE,
                OPERATION,
                "invitation",
                Some(record.invitation_key().persistence_key()),
                PlatformAuditOutcome::Succeeded,
                None,
                Some(json!({
                    "organization_id": organization_id.get(),
                    "role": organization_role_code(record.role()),
                    "status": record.status().code(),
                    "expires_at": record.expires_at(),
                    "version": record.version(),
                })),
                json!({"platform_reason": reason}),
                &request_id,
            )
            .await?;
            Ok(issued)
        })
    }
}

impl fmt::Debug for DatabasePlatformOrganizationManagementService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabasePlatformOrganizationManagementService(<redacted>)")
    }
}

fn validate_access(
    principal: SessionPrincipal,
    reason: &str,
) -> Result<(), PlatformOrganizationManagementError> {
    PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsManage)
        .map_err(|_| PlatformOrganizationManagementError::Forbidden)?;
    if reason.is_empty()
        || reason.len() > MAX_PLATFORM_ORGANIZATION_REASON_BYTES
        || reason.trim() != reason
        || reason.chars().any(char::is_control)
    {
        return Err(PlatformOrganizationManagementError::InvalidInput);
    }
    Ok(())
}

fn member_audit_value(member: &PlatformOrganizationMember) -> Value {
    json!({
        "organization_id": member.organization_id().get(),
        "user_id": member.user_id().get(),
        "role": organization_role_code(member.role()),
        "status": membership_status_code(member.status()),
        "team_id": member.team_id().map(OrganizationTeamId::get),
        "version": member.version(),
    })
}

const fn map_repository_error(
    error: OrganizationRepositoryError,
) -> PlatformOrganizationManagementError {
    match error {
        OrganizationRepositoryError::InvalidInput => {
            PlatformOrganizationManagementError::InvalidInput
        }
        OrganizationRepositoryError::NotFound => PlatformOrganizationManagementError::NotFound,
        OrganizationRepositoryError::Conflict => PlatformOrganizationManagementError::Conflict,
        OrganizationRepositoryError::InvalidTransition => {
            PlatformOrganizationManagementError::InvalidTransition
        }
        OrganizationRepositoryError::LastOwner => PlatformOrganizationManagementError::LastOwner,
        OrganizationRepositoryError::SeatLimitReached => {
            PlatformOrganizationManagementError::SeatLimitReached
        }
        OrganizationRepositoryError::EntitlementRestricted => {
            PlatformOrganizationManagementError::EntitlementRestricted
        }
        _ => PlatformOrganizationManagementError::Internal,
    }
}

const fn map_invitation_error(
    error: OrganizationInvitationError,
) -> PlatformOrganizationManagementError {
    match error {
        OrganizationInvitationError::InvalidInput | OrganizationInvitationError::BatchTooLarge => {
            PlatformOrganizationManagementError::InvalidInput
        }
        OrganizationInvitationError::Forbidden => PlatformOrganizationManagementError::Forbidden,
        OrganizationInvitationError::NotFound => PlatformOrganizationManagementError::NotFound,
        OrganizationInvitationError::Conflict => PlatformOrganizationManagementError::Conflict,
        OrganizationInvitationError::SeatLimitReached => {
            PlatformOrganizationManagementError::SeatLimitReached
        }
        OrganizationInvitationError::Expired
        | OrganizationInvitationError::EmailMismatch
        | OrganizationInvitationError::Internal => PlatformOrganizationManagementError::Internal,
    }
}

const fn repository_error_code(error: OrganizationRepositoryError) -> &'static str {
    match error {
        OrganizationRepositoryError::InvalidInput => "invalid_input",
        OrganizationRepositoryError::NotFound => "not_found",
        OrganizationRepositoryError::Conflict => "conflict",
        OrganizationRepositoryError::InvalidTransition => "invalid_transition",
        OrganizationRepositoryError::LastOwner => "last_owner",
        OrganizationRepositoryError::SeatLimitReached => "seat_limit_reached",
        OrganizationRepositoryError::EntitlementRestricted => "entitlement_restricted",
        _ => "internal",
    }
}

const fn invitation_error_code(error: OrganizationInvitationError) -> &'static str {
    match error {
        OrganizationInvitationError::InvalidInput => "invalid_input",
        OrganizationInvitationError::BatchTooLarge => "batch_too_large",
        OrganizationInvitationError::Forbidden => "forbidden",
        OrganizationInvitationError::NotFound => "not_found",
        OrganizationInvitationError::Conflict => "conflict",
        OrganizationInvitationError::SeatLimitReached => "seat_limit_reached",
        OrganizationInvitationError::Expired => "expired",
        OrganizationInvitationError::EmailMismatch => "email_mismatch",
        OrganizationInvitationError::Internal => "internal",
    }
}

const fn management_error_code(error: PlatformOrganizationManagementError) -> &'static str {
    match error {
        PlatformOrganizationManagementError::InvalidInput => "invalid_reason",
        PlatformOrganizationManagementError::Forbidden => "forbidden",
        PlatformOrganizationManagementError::NotFound => "not_found",
        PlatformOrganizationManagementError::Conflict => "conflict",
        PlatformOrganizationManagementError::InvalidTransition => "invalid_transition",
        PlatformOrganizationManagementError::LastOwner => "last_owner",
        PlatformOrganizationManagementError::SeatLimitReached => "seat_limit_reached",
        PlatformOrganizationManagementError::EntitlementRestricted => "entitlement_restricted",
        PlatformOrganizationManagementError::Internal => "internal",
    }
}

const fn organization_role_code(role: OrganizationRole) -> &'static str {
    match role {
        OrganizationRole::Owner => "owner",
        OrganizationRole::Admin => "admin",
        OrganizationRole::Billing => "billing",
        OrganizationRole::Member => "member",
        OrganizationRole::Viewer => "viewer",
    }
}

const fn membership_status_code(status: OrganizationMembershipStatus) -> &'static str {
    match status {
        OrganizationMembershipStatus::Active => "active",
        OrganizationMembershipStatus::Suspended => "suspended",
        OrganizationMembershipStatus::Removed => "removed",
    }
}

#[cfg(test)]
mod tests {
    use crate::SessionRole;

    use super::*;

    #[test]
    fn platform_member_management_requires_admin_reason_and_positive_version() {
        let admin = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin);
        let user = SessionPrincipal::new(UserId::new(2).unwrap(), SessionRole::User);
        assert_eq!(
            validate_access(user, "处理企业成员工单"),
            Err(PlatformOrganizationManagementError::Forbidden)
        );
        assert_eq!(
            validate_access(admin, " leading"),
            Err(PlatformOrganizationManagementError::InvalidInput)
        );
        assert!(validate_access(admin, "处理企业成员工单").is_ok());
        assert_eq!(
            PlatformOrganizationMemberChangeCommand::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Active,
                0,
            ),
            Err(PlatformOrganizationManagementError::InvalidInput)
        );
    }
}
