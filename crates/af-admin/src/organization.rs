use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{OrganizationAccessRecord, OrganizationRepository, OrganizationRepositoryError};
use af_domain::{
    OrganizationEntitlementStatus, OrganizationId, OrganizationMembershipId,
    OrganizationPermission, OrganizationRole, UserId,
};
use thiserror::Error;

use crate::SessionPrincipal;

/// 已按会话用户和企业路径完成校验的企业上下文。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationContext {
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    user_id: UserId,
    role: OrganizationRole,
    permissions: Vec<OrganizationPermission>,
    name: String,
    slug: String,
    entitlement: OrganizationEntitlementSummary,
}

/// 企业概览中可安全展示的授权与容量投影。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationEntitlementSummary {
    status: OrganizationEntitlementStatus,
    team_management_enabled: bool,
    custom_roles_enabled: bool,
    seat_limit: i64,
    active_seats: i64,
    team_limit: i64,
    active_teams: i64,
    organization_key_limit: i64,
    audit_retention_days: i64,
    valid_until: i64,
    grace_until: Option<i64>,
}

impl OrganizationContext {
    fn from_access(access: &OrganizationAccessRecord) -> Self {
        let membership = access.membership();
        let role = membership.role();
        Self {
            organization_id: access.organization().id(),
            membership_id: membership.id(),
            user_id: membership.user_id(),
            role,
            permissions: effective_permissions(role, access.custom_role_permissions()),
            name: access.organization().name().to_owned(),
            slug: access.organization().slug().to_owned(),
            entitlement: OrganizationEntitlementSummary::from_access(access),
        }
    }

    /// 返回已校验的企业标识。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回当前用户在企业中的成员关系标识。
    #[must_use]
    pub const fn membership_id(&self) -> OrganizationMembershipId {
        self.membership_id
    }

    /// 返回被校验的会话用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回固定企业角色。
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }

    /// 返回固定角色基线与有效自定义角色合并后的权限集合。
    #[must_use]
    pub fn permissions(&self) -> &[OrganizationPermission] {
        &self.permissions
    }

    /// 返回已校验的企业名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回已校验的企业短标识。
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// 判断当前企业角色是否拥有指定权限。
    #[must_use]
    pub fn allows(&self, permission: OrganizationPermission) -> bool {
        self.permissions.contains(&permission)
    }

    /// 返回当前企业的授权和容量投影。
    #[must_use]
    pub const fn entitlement(&self) -> OrganizationEntitlementSummary {
        self.entitlement
    }
}

fn effective_permissions(
    role: OrganizationRole,
    custom_role_permissions: Option<&[OrganizationPermission]>,
) -> Vec<OrganizationPermission> {
    let mut permissions = role.permissions().to_vec();
    if role != OrganizationRole::Member {
        return permissions;
    }
    for permission in custom_role_permissions.into_iter().flatten() {
        if !permissions.contains(permission) {
            permissions.push(*permission);
        }
    }
    permissions
}

impl OrganizationEntitlementSummary {
    fn from_access(access: &OrganizationAccessRecord) -> Self {
        let snapshot = access.entitlement();
        let capacity = snapshot.capacity();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs().min(i64::MAX as u64) as i64)
            .unwrap_or_default();
        Self {
            status: snapshot.status(access.organization().status(), now),
            team_management_enabled: snapshot.team_management_enabled(),
            custom_roles_enabled: snapshot.custom_roles_enabled(),
            seat_limit: capacity.seat_limit(),
            active_seats: access.active_seats(),
            team_limit: capacity.team_limit(),
            active_teams: access.active_teams(),
            organization_key_limit: capacity.organization_key_limit(),
            audit_retention_days: snapshot.audit_retention_days(),
            valid_until: snapshot.valid_until(),
            grace_until: snapshot.grace_until(),
        }
    }

    /// 返回授权状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationEntitlementStatus {
        self.status
    }
    /// 是否开启团队管理。
    #[must_use]
    pub const fn team_management_enabled(&self) -> bool {
        self.team_management_enabled
    }

    /// 判断商业授权是否正式开放自定义角色能力。
    #[must_use]
    pub const fn custom_roles_enabled(&self) -> bool {
        self.custom_roles_enabled
    }
    /// 返回席位上限。
    #[must_use]
    pub const fn seat_limit(&self) -> i64 {
        self.seat_limit
    }
    /// 返回活动席位数。
    #[must_use]
    pub const fn active_seats(&self) -> i64 {
        self.active_seats
    }
    /// 返回团队上限。
    #[must_use]
    pub const fn team_limit(&self) -> i64 {
        self.team_limit
    }
    /// 返回活动团队数。
    #[must_use]
    pub const fn active_teams(&self) -> i64 {
        self.active_teams
    }
    /// 返回企业 Key 上限。
    #[must_use]
    pub const fn organization_key_limit(&self) -> i64 {
        self.organization_key_limit
    }
    /// 返回审计保留天数。
    #[must_use]
    pub const fn audit_retention_days(&self) -> i64 {
        self.audit_retention_days
    }
    /// 返回正常授权结束时间。
    #[must_use]
    pub const fn valid_until(&self) -> i64 {
        self.valid_until
    }
    /// 返回宽限期结束时间。
    #[must_use]
    pub const fn grace_until(&self) -> Option<i64> {
        self.grace_until
    }
}

/// 企业目录中返回的最小安全摘要。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationAccessSummary {
    organization_id: OrganizationId,
    name: String,
    slug: String,
    role: OrganizationRole,
}

impl OrganizationAccessSummary {
    fn from_access(access: OrganizationAccessRecord) -> Self {
        Self {
            organization_id: access.organization().id(),
            name: access.organization().name().to_owned(),
            slug: access.organization().slug().to_owned(),
            role: access.membership().role(),
        }
    }

    /// 返回企业标识。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回企业名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回企业短标识。
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// 返回当前用户角色。
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationAuthorizationError {
    /// 企业路径、成员关系或企业状态不满足进入条件。
    #[error("企业上下文不存在或当前用户无权访问")]
    NotFound,
    /// 当前企业角色未授予所需权限。
    #[error("当前企业角色无权执行该操作")]
    Forbidden,
    /// 企业授权查询发生内部错误。
    #[error("企业授权查询失败")]
    Internal,
}

pub type OrganizationAccessFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationContext, OrganizationAuthorizationError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationListFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<OrganizationAccessSummary>, OrganizationAuthorizationError>>
            + Send
            + 'a,
    >,
>;

/// 企业上下文授权服务；平台管理员角色不会绕过企业成员校验。
pub trait OrganizationAuthorizationService: Send + Sync {
    fn list<'a>(&'a self, principal: SessionPrincipal) -> OrganizationListFuture<'a>;
    fn context<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        permission: OrganizationPermission,
    ) -> OrganizationAccessFuture<'a>;
}

/// 使用组织仓储实现固定企业 RBAC。
pub struct DatabaseOrganizationAuthorizationService {
    repository: Arc<OrganizationRepository>,
}

impl DatabaseOrganizationAuthorizationService {
    /// 绑定启动期创建的企业仓储。
    #[must_use]
    pub fn new(repository: Arc<OrganizationRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationAuthorizationService for DatabaseOrganizationAuthorizationService {
    fn list<'a>(&'a self, principal: SessionPrincipal) -> OrganizationListFuture<'a> {
        Box::pin(async move {
            self.repository
                .list_authorized_access(principal.user_id())
                .await
                .map_err(map_repository_error)
                .map(|records| {
                    records
                        .into_iter()
                        .map(OrganizationAccessSummary::from_access)
                        .collect()
                })
        })
    }

    fn context<'a>(
        &'a self,
        principal: SessionPrincipal,
        organization_id: OrganizationId,
        permission: OrganizationPermission,
    ) -> OrganizationAccessFuture<'a> {
        Box::pin(async move {
            let access = self
                .repository
                .authorized_access(organization_id, principal.user_id())
                .await
                .map_err(map_repository_error)?;
            let context = OrganizationContext::from_access(&access);
            if !context.allows(permission) {
                return Err(OrganizationAuthorizationError::Forbidden);
            }
            Ok(context)
        })
    }
}

impl std::fmt::Debug for DatabaseOrganizationAuthorizationService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationAuthorizationService(<repository>)")
    }
}

fn map_repository_error(error: OrganizationRepositoryError) -> OrganizationAuthorizationError {
    match error {
        OrganizationRepositoryError::NotFound => OrganizationAuthorizationError::NotFound,
        OrganizationRepositoryError::Query
        | OrganizationRepositoryError::Timeout
        | OrganizationRepositoryError::Invariant
        | OrganizationRepositoryError::InvalidInput
        | OrganizationRepositoryError::UserUnavailable
        | OrganizationRepositoryError::Conflict
        | OrganizationRepositoryError::InvalidTransition
        | OrganizationRepositoryError::LastOwner
        | OrganizationRepositoryError::TeamHasActiveMembers
        | OrganizationRepositoryError::EntitlementRestricted
        | OrganizationRepositoryError::SeatLimitReached
        | OrganizationRepositoryError::TeamLimitReached
        | OrganizationRepositoryError::InvitationExpired
        | OrganizationRepositoryError::InvitationNotPending
        | OrganizationRepositoryError::InvitationEmailMismatch
        | OrganizationRepositoryError::InvitationBatchConflict
        | OrganizationRepositoryError::EntropyUnavailable => {
            OrganizationAuthorizationError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_projection_keeps_baseline_and_adds_closed_permissions() {
        let custom = [
            OrganizationPermission::ProfileWrite,
            OrganizationPermission::OverviewRead,
        ];
        let permissions = effective_permissions(OrganizationRole::Member, Some(&custom));
        assert!(permissions.contains(&OrganizationPermission::ProfileWrite));
        assert!(permissions.contains(&OrganizationPermission::KeysManageSelf));
        assert_eq!(
            permissions
                .iter()
                .filter(|permission| **permission == OrganizationPermission::OverviewRead)
                .count(),
            1
        );
    }

    #[test]
    fn fixed_non_member_roles_ignore_custom_projection() {
        let custom = [OrganizationPermission::WalletManage];
        let viewer = effective_permissions(OrganizationRole::Viewer, Some(&custom));
        let owner = effective_permissions(OrganizationRole::Owner, None);
        assert!(!viewer.contains(&OrganizationPermission::WalletManage));
        assert!(owner.contains(&OrganizationPermission::OwnershipTransfer));
    }
}
