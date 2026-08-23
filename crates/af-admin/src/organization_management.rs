use std::{future::Future, pin::Pin, sync::Arc};

use af_db::{
    MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE, OrganizationMembershipChange,
    OrganizationMembershipRecord, OrganizationOwnershipTransfer, OrganizationRepository,
    OrganizationRepositoryError, OrganizationTeamChange, OrganizationTeamCreate,
    OrganizationTeamRecord,
};
use af_domain::{
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationPermission,
    OrganizationRole, OrganizationTeamId,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 企业成员和团队目录默认页大小。
pub const DEFAULT_ORGANIZATION_DIRECTORY_PAGE_SIZE: usize = 50;

/// 已校验的成员目录查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationMembershipListQuery {
    after: Option<OrganizationMembershipId>,
    limit: usize,
}

impl OrganizationMembershipListQuery {
    /// 校验单调成员 ID 游标和页大小。
    pub fn new(
        after: Option<OrganizationMembershipId>,
        limit: usize,
    ) -> Result<Self, OrganizationManagementError> {
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

impl Default for OrganizationMembershipListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ORGANIZATION_DIRECTORY_PAGE_SIZE,
        }
    }
}

/// 已校验的团队目录查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationTeamListQuery {
    after: Option<OrganizationTeamId>,
    limit: usize,
}

impl OrganizationTeamListQuery {
    /// 校验单调团队 ID 游标和页大小。
    pub fn new(
        after: Option<OrganizationTeamId>,
        limit: usize,
    ) -> Result<Self, OrganizationManagementError> {
        validate_limit(limit)?;
        Ok(Self { after, limit })
    }

    #[must_use]
    pub const fn after(self) -> Option<OrganizationTeamId> {
        self.after
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for OrganizationTeamListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ORGANIZATION_DIRECTORY_PAGE_SIZE,
        }
    }
}

/// 成员角色、状态和主团队的完整目标快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationMembershipChangeCommand {
    membership_id: OrganizationMembershipId,
    role: OrganizationRole,
    status: OrganizationMembershipStatus,
    team_id: Option<OrganizationTeamId>,
    expected_version: i64,
}

impl OrganizationMembershipChangeCommand {
    /// 构造带 CAS 版本的成员变更。
    pub fn new(
        membership_id: OrganizationMembershipId,
        role: OrganizationRole,
        status: OrganizationMembershipStatus,
        team_id: Option<OrganizationTeamId>,
        expected_version: i64,
    ) -> Result<Self, OrganizationManagementError> {
        validate_version(expected_version)?;
        Ok(Self {
            membership_id,
            role,
            status,
            team_id,
            expected_version,
        })
    }
}

/// 由当前 Owner 发起的原子所有权转移。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationOwnershipTransferCommand {
    target_membership_id: OrganizationMembershipId,
    current_owner_version: i64,
    target_expected_version: i64,
}

impl OrganizationOwnershipTransferCommand {
    /// 构造双方均携带 CAS 版本的转移命令。
    pub fn new(
        target_membership_id: OrganizationMembershipId,
        current_owner_version: i64,
        target_expected_version: i64,
    ) -> Result<Self, OrganizationManagementError> {
        validate_version(current_owner_version)?;
        validate_version(target_expected_version)?;
        Ok(Self {
            target_membership_id,
            current_owner_version,
            target_expected_version,
        })
    }
}

/// 创建企业团队所需资料。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationTeamCreateCommand {
    name: String,
    description: Option<String>,
}

impl OrganizationTeamCreateCommand {
    #[must_use]
    pub fn new(name: String, description: Option<String>) -> Self {
        Self { name, description }
    }
}

/// 更新企业团队资料所需的完整目标快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationTeamChangeCommand {
    team_id: OrganizationTeamId,
    name: String,
    description: Option<String>,
    expected_version: i64,
}

impl OrganizationTeamChangeCommand {
    pub fn new(
        team_id: OrganizationTeamId,
        name: String,
        description: Option<String>,
        expected_version: i64,
    ) -> Result<Self, OrganizationManagementError> {
        validate_version(expected_version)?;
        Ok(Self {
            team_id,
            name,
            description,
            expected_version,
        })
    }
}

/// 禁用企业团队所需的 CAS 命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationTeamDisableCommand {
    team_id: OrganizationTeamId,
    expected_version: i64,
}

impl OrganizationTeamDisableCommand {
    pub fn new(
        team_id: OrganizationTeamId,
        expected_version: i64,
    ) -> Result<Self, OrganizationManagementError> {
        validate_version(expected_version)?;
        Ok(Self {
            team_id,
            expected_version,
        })
    }
}

/// 企业成员稳定游标页。
#[derive(Debug, PartialEq)]
pub struct OrganizationMembershipPage {
    records: Vec<OrganizationMembershipRecord>,
    next_cursor: Option<OrganizationMembershipId>,
}

impl OrganizationMembershipPage {
    #[must_use]
    pub fn records(&self) -> &[OrganizationMembershipRecord] {
        &self.records
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationMembershipId> {
        self.next_cursor
    }
}

/// 企业团队稳定游标页。
#[derive(Debug, PartialEq)]
pub struct OrganizationTeamPage {
    records: Vec<OrganizationTeamRecord>,
    next_cursor: Option<OrganizationTeamId>,
}

impl OrganizationTeamPage {
    #[must_use]
    pub fn records(&self) -> &[OrganizationTeamRecord] {
        &self.records
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationTeamId> {
        self.next_cursor
    }
}

/// 原子所有权转移后的双方成员快照。
#[derive(Debug, PartialEq)]
pub struct OrganizationOwnershipTransferResult {
    previous_owner: OrganizationMembershipRecord,
    current_owner: OrganizationMembershipRecord,
}

impl OrganizationOwnershipTransferResult {
    #[must_use]
    pub fn previous_owner(&self) -> &OrganizationMembershipRecord {
        &self.previous_owner
    }

    #[must_use]
    pub fn current_owner(&self) -> &OrganizationMembershipRecord {
        &self.current_owner
    }
}

/// 企业成员与团队生命周期错误；错误不携带企业资料或成员身份。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationManagementError {
    #[error("企业生命周期请求无效")]
    InvalidInput,
    #[error("当前企业角色无权执行该操作")]
    Forbidden,
    #[error("企业成员或团队不存在")]
    NotFound,
    #[error("企业成员或团队版本冲突")]
    Conflict,
    #[error("企业生命周期转换无效")]
    InvalidTransition,
    #[error("企业必须保留至少一个活动 Owner")]
    LastOwner,
    #[error("团队仍有活动成员")]
    TeamHasActiveMembers,
    #[error("企业授权不允许当前操作")]
    EntitlementRestricted,
    #[error("企业席位容量已用尽")]
    SeatLimitReached,
    #[error("企业团队容量已用尽")]
    TeamLimitReached,
    #[error("企业生命周期服务内部错误")]
    Internal,
}

pub type OrganizationMembershipPageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationMembershipPage, OrganizationManagementError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationMembershipFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationMembershipRecord, OrganizationManagementError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationOwnershipTransferFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<OrganizationOwnershipTransferResult, OrganizationManagementError>,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationTeamPageFuture<'a> = Pin<
    Box<dyn Future<Output = Result<OrganizationTeamPage, OrganizationManagementError>> + Send + 'a>,
>;
pub type OrganizationTeamFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationTeamRecord, OrganizationManagementError>>
            + Send
            + 'a,
    >,
>;

/// 企业成员与团队生命周期端口；调用方必须传入已校验的企业上下文。
pub trait OrganizationManagementService: Send + Sync {
    fn list_members<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationMembershipListQuery,
    ) -> OrganizationMembershipPageFuture<'a>;

    fn change_member<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationMembershipChangeCommand,
    ) -> OrganizationMembershipFuture<'a>;

    fn transfer_ownership<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationOwnershipTransferCommand,
    ) -> OrganizationOwnershipTransferFuture<'a>;

    fn list_teams<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationTeamListQuery,
    ) -> OrganizationTeamPageFuture<'a>;

    fn create_team<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationTeamCreateCommand,
    ) -> OrganizationTeamFuture<'a>;

    fn change_team<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationTeamChangeCommand,
    ) -> OrganizationTeamFuture<'a>;

    fn disable_team<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationTeamDisableCommand,
    ) -> OrganizationTeamFuture<'a>;
}

/// 使用组织仓储实现企业生命周期和固定角色约束。
pub struct DatabaseOrganizationManagementService {
    repository: Arc<OrganizationRepository>,
}

impl DatabaseOrganizationManagementService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationManagementService for DatabaseOrganizationManagementService {
    fn list_members<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationMembershipListQuery,
    ) -> OrganizationMembershipPageFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::OverviewRead)?;
            let page = self
                .repository
                .list_memberships(context.organization_id(), query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            Ok(OrganizationMembershipPage {
                records,
                next_cursor,
            })
        })
    }

    fn change_member<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationMembershipChangeCommand,
    ) -> OrganizationMembershipFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::MembersManage)?;
            let current = self
                .repository
                .membership(context.organization_id(), command.membership_id)
                .await
                .map_err(map_repository_error)?;
            if current.role().is_owner() && !context.role().is_owner() {
                return Err(OrganizationManagementError::Forbidden);
            }
            // Owner 身份只能通过原子转移入口改变，避免普通更新留下短暂无 Owner 状态。
            if current.role().is_owner() != command.role.is_owner() {
                return Err(OrganizationManagementError::InvalidTransition);
            }
            self.repository
                .change_membership(
                    context.organization_id(),
                    command.membership_id,
                    OrganizationMembershipChange::new(
                        command.role,
                        command.status,
                        command.team_id,
                        command.expected_version,
                    ),
                )
                .await
                .map_err(map_repository_error)
        })
    }

    fn transfer_ownership<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationOwnershipTransferCommand,
    ) -> OrganizationOwnershipTransferFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::OwnershipTransfer)?;
            if context.membership_id() == command.target_membership_id {
                return Err(OrganizationManagementError::InvalidInput);
            }
            let record = self
                .repository
                .transfer_ownership(
                    context.organization_id(),
                    OrganizationOwnershipTransfer::new(
                        context.membership_id(),
                        command.current_owner_version,
                        command.target_membership_id,
                        command.target_expected_version,
                    ),
                )
                .await
                .map_err(map_repository_error)?;
            let (previous_owner, current_owner) = record.into_parts();
            Ok(OrganizationOwnershipTransferResult {
                previous_owner,
                current_owner,
            })
        })
    }

    fn list_teams<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationTeamListQuery,
    ) -> OrganizationTeamPageFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::OverviewRead)?;
            let page = self
                .repository
                .list_teams(context.organization_id(), query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            Ok(OrganizationTeamPage {
                records,
                next_cursor,
            })
        })
    }

    fn create_team<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationTeamCreateCommand,
    ) -> OrganizationTeamFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::TeamsManage)?;
            self.repository
                .create_team(
                    context.organization_id(),
                    OrganizationTeamCreate::new(command.name, command.description),
                )
                .await
                .map_err(map_repository_error)
        })
    }

    fn change_team<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationTeamChangeCommand,
    ) -> OrganizationTeamFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::TeamsManage)?;
            self.repository
                .update_team(
                    context.organization_id(),
                    command.team_id,
                    OrganizationTeamChange::new(
                        command.name,
                        command.description,
                        command.expected_version,
                    ),
                )
                .await
                .map_err(map_repository_error)
        })
    }

    fn disable_team<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationTeamDisableCommand,
    ) -> OrganizationTeamFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::TeamsManage)?;
            self.repository
                .disable_team(
                    context.organization_id(),
                    command.team_id,
                    command.expected_version,
                )
                .await
                .map_err(map_repository_error)
        })
    }
}

impl std::fmt::Debug for DatabaseOrganizationManagementService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationManagementService(<repository>)")
    }
}

fn require_permission(
    context: &OrganizationContext,
    permission: OrganizationPermission,
) -> Result<(), OrganizationManagementError> {
    if !context.allows(permission) {
        return Err(OrganizationManagementError::Forbidden);
    }
    Ok(())
}

fn validate_limit(limit: usize) -> Result<(), OrganizationManagementError> {
    if !(1..=MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE).contains(&limit) {
        return Err(OrganizationManagementError::InvalidInput);
    }
    Ok(())
}

fn validate_version(version: i64) -> Result<(), OrganizationManagementError> {
    if version <= 0 {
        return Err(OrganizationManagementError::InvalidInput);
    }
    Ok(())
}

fn map_repository_error(error: OrganizationRepositoryError) -> OrganizationManagementError {
    match error {
        OrganizationRepositoryError::InvalidInput => OrganizationManagementError::InvalidInput,
        OrganizationRepositoryError::NotFound => OrganizationManagementError::NotFound,
        OrganizationRepositoryError::Conflict => OrganizationManagementError::Conflict,
        OrganizationRepositoryError::InvalidTransition => {
            OrganizationManagementError::InvalidTransition
        }
        OrganizationRepositoryError::LastOwner => OrganizationManagementError::LastOwner,
        OrganizationRepositoryError::TeamHasActiveMembers => {
            OrganizationManagementError::TeamHasActiveMembers
        }
        OrganizationRepositoryError::EntitlementRestricted => {
            OrganizationManagementError::EntitlementRestricted
        }
        OrganizationRepositoryError::SeatLimitReached => {
            OrganizationManagementError::SeatLimitReached
        }
        OrganizationRepositoryError::TeamLimitReached => {
            OrganizationManagementError::TeamLimitReached
        }
        OrganizationRepositoryError::UserUnavailable
        | OrganizationRepositoryError::InvitationExpired
        | OrganizationRepositoryError::InvitationNotPending
        | OrganizationRepositoryError::InvitationEmailMismatch
        | OrganizationRepositoryError::InvitationBatchConflict
        | OrganizationRepositoryError::EntropyUnavailable
        | OrganizationRepositoryError::Query
        | OrganizationRepositoryError::Timeout
        | OrganizationRepositoryError::Invariant => OrganizationManagementError::Internal,
    }
}
