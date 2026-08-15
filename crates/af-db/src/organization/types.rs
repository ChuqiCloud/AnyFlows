use std::fmt;

use af_domain::{
    OrganizationId, OrganizationKey, OrganizationMembershipId, OrganizationMembershipStatus,
    OrganizationRole, OrganizationStatus, OrganizationTeamId, OrganizationTeamStatus, UserId,
};
use thiserror::Error;

/// 企业显示名最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_NAME_BYTES: usize = 128;
/// 企业短标识最大 ASCII 字节数。
pub const MAX_ORGANIZATION_SLUG_BYTES: usize = 64;
/// 企业团队名称最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_TEAM_NAME_BYTES: usize = 128;
/// 企业团队说明最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_TEAM_DESCRIPTION_BYTES: usize = 2_000;

/// 创建企业时由服务端验证的基础资料和初始 Owner。
pub struct OrganizationCreate {
    pub(super) name: String,
    pub(super) slug: String,
    pub(super) owner_user_id: UserId,
}

impl OrganizationCreate {
    /// 组装企业基础资料；公开标识和 Owner 关系由仓储原子生成。
    #[must_use]
    pub fn new(name: String, slug: String, owner_user_id: UserId) -> Self {
        Self {
            name,
            slug,
            owner_user_id,
        }
    }
}

impl fmt::Debug for OrganizationCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationCreate(<redacted>)")
    }
}

/// 创建成员关系所需的真实用户、固定角色和可选主团队。
pub struct OrganizationMembershipCreate {
    pub(super) user_id: UserId,
    pub(super) role: OrganizationRole,
    pub(super) team_id: Option<OrganizationTeamId>,
    pub(super) invited_by_user_id: Option<UserId>,
}

impl OrganizationMembershipCreate {
    /// 组装已完成身份确认的活动成员关系。
    #[must_use]
    pub fn new(
        user_id: UserId,
        role: OrganizationRole,
        team_id: Option<OrganizationTeamId>,
        invited_by_user_id: Option<UserId>,
    ) -> Self {
        Self {
            user_id,
            role,
            team_id,
            invited_by_user_id,
        }
    }
}

impl fmt::Debug for OrganizationMembershipCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationMembershipCreate")
            .field("role", &self.role)
            .field("has_team", &self.team_id.is_some())
            .field("has_inviter", &self.invited_by_user_id.is_some())
            .finish_non_exhaustive()
    }
}

/// 带 CAS 版本的成员角色、状态和主团队目标快照。
pub struct OrganizationMembershipChange {
    pub(super) role: OrganizationRole,
    pub(super) status: OrganizationMembershipStatus,
    pub(super) team_id: Option<OrganizationTeamId>,
    pub(super) expected_version: i64,
}

impl OrganizationMembershipChange {
    /// 组装完整目标状态，避免部分字段更新绕过生命周期校验。
    #[must_use]
    pub fn new(
        role: OrganizationRole,
        status: OrganizationMembershipStatus,
        team_id: Option<OrganizationTeamId>,
        expected_version: i64,
    ) -> Self {
        Self {
            role,
            status,
            team_id,
            expected_version,
        }
    }
}

/// 企业所有权转移的双方成员和 CAS 版本。
pub struct OrganizationOwnershipTransfer {
    pub(super) from_membership_id: OrganizationMembershipId,
    pub(super) from_expected_version: i64,
    pub(super) to_membership_id: OrganizationMembershipId,
    pub(super) to_expected_version: i64,
}

impl OrganizationOwnershipTransfer {
    /// 组装需要在同一事务内完成的所有权转移。
    #[must_use]
    pub fn new(
        from_membership_id: OrganizationMembershipId,
        from_expected_version: i64,
        to_membership_id: OrganizationMembershipId,
        to_expected_version: i64,
    ) -> Self {
        Self {
            from_membership_id,
            from_expected_version,
            to_membership_id,
            to_expected_version,
        }
    }
}

/// 创建企业团队所需的名称和可选说明。
pub struct OrganizationTeamCreate {
    pub(super) name: String,
    pub(super) description: Option<String>,
}

impl OrganizationTeamCreate {
    /// 组装企业内的扁平团队资料。
    #[must_use]
    pub fn new(name: String, description: Option<String>) -> Self {
        Self { name, description }
    }
}

/// 已持久化的企业主体快照。
#[derive(Clone, PartialEq)]
pub struct OrganizationRecord {
    pub(super) id: OrganizationId,
    pub(super) organization_key: OrganizationKey,
    pub(super) name: String,
    pub(super) slug: String,
    pub(super) status: OrganizationStatus,
    pub(super) created_by_user_id: UserId,
    pub(super) version: i64,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    pub(super) closed_at: Option<i64>,
}

impl OrganizationRecord {
    /// 返回企业显示名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回企业内部标识。
    #[must_use]
    pub const fn id(&self) -> OrganizationId {
        self.id
    }

    /// 返回不暴露数据库顺序的企业公开标识。
    #[must_use]
    pub const fn organization_key(&self) -> OrganizationKey {
        self.organization_key
    }

    /// 返回企业短标识。
    #[must_use]
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// 返回企业状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationStatus {
        self.status
    }

    /// 返回创建者用户标识。
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }

    /// 返回企业 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

impl fmt::Debug for OrganizationRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationRecord")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("closed_at", &self.closed_at)
            .finish_non_exhaustive()
    }
}

/// 已持久化的企业成员关系快照。
#[derive(Clone, PartialEq)]
pub struct OrganizationMembershipRecord {
    pub(super) id: OrganizationMembershipId,
    pub(super) organization_id: OrganizationId,
    pub(super) user_id: UserId,
    pub(super) role: OrganizationRole,
    pub(super) team_id: Option<OrganizationTeamId>,
    pub(super) status: OrganizationMembershipStatus,
    pub(super) joined_at: i64,
    pub(super) suspended_at: Option<i64>,
    pub(super) removed_at: Option<i64>,
    pub(super) invited_by_user_id: Option<UserId>,
    pub(super) version: i64,
}

impl OrganizationMembershipRecord {
    /// 返回成员所属企业标识，供授权上下文做二次边界校验。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回成员关系内部标识。
    #[must_use]
    pub const fn id(&self) -> OrganizationMembershipId {
        self.id
    }

    /// 返回成员对应的全局用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回企业内固定角色。
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }

    /// 返回成员生命周期状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationMembershipStatus {
        self.status
    }

    /// 返回成员主团队。
    #[must_use]
    pub const fn team_id(&self) -> Option<OrganizationTeamId> {
        self.team_id
    }

    /// 返回成员 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

impl fmt::Debug for OrganizationMembershipRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationMembershipRecord")
            .field("role", &self.role)
            .field("status", &self.status)
            .field("has_team", &self.team_id.is_some())
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 已持久化的企业团队快照。
#[derive(Clone, PartialEq)]
pub struct OrganizationTeamRecord {
    pub(super) id: OrganizationTeamId,
    pub(super) organization_id: OrganizationId,
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) status: OrganizationTeamStatus,
    pub(super) version: i64,
    pub(super) created_at: i64,
    pub(super) updated_at: i64,
    pub(super) deleted_at: Option<i64>,
}

/// 企业与活动成员的成对快照；两者必须来自同一企业路径查询。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationAccessRecord {
    organization: OrganizationRecord,
    membership: OrganizationMembershipRecord,
}

impl OrganizationAccessRecord {
    pub(super) const fn new(
        organization: OrganizationRecord,
        membership: OrganizationMembershipRecord,
    ) -> Self {
        Self {
            organization,
            membership,
        }
    }

    /// 返回企业快照。
    #[must_use]
    pub fn organization(&self) -> &OrganizationRecord {
        &self.organization
    }

    /// 返回活动成员快照。
    #[must_use]
    pub fn membership(&self) -> &OrganizationMembershipRecord {
        &self.membership
    }
}

impl OrganizationTeamRecord {
    /// 返回团队内部标识。
    #[must_use]
    pub const fn id(&self) -> OrganizationTeamId {
        self.id
    }

    /// 返回团队状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationTeamStatus {
        self.status
    }

    /// 返回团队 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

impl fmt::Debug for OrganizationTeamRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationTeamRecord")
            .field("status", &self.status)
            .field("version", &self.version)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .field("deleted_at", &self.deleted_at)
            .finish_non_exhaustive()
    }
}

/// 企业创建后必须同时存在的企业和初始 Owner 快照。
#[derive(Debug, PartialEq)]
pub struct OrganizationCreatedRecord {
    organization: OrganizationRecord,
    owner_membership: OrganizationMembershipRecord,
}

impl OrganizationCreatedRecord {
    pub(super) const fn new(
        organization: OrganizationRecord,
        owner_membership: OrganizationMembershipRecord,
    ) -> Self {
        Self {
            organization,
            owner_membership,
        }
    }

    /// 拆出企业和初始 Owner 快照。
    #[must_use]
    pub fn into_parts(self) -> (OrganizationRecord, OrganizationMembershipRecord) {
        (self.organization, self.owner_membership)
    }
}

/// 所有权转移完成后的原 Owner 和新 Owner 快照。
#[derive(Debug, PartialEq)]
pub struct OrganizationOwnershipTransferRecord {
    previous_owner: OrganizationMembershipRecord,
    current_owner: OrganizationMembershipRecord,
}

impl OrganizationOwnershipTransferRecord {
    pub(super) const fn new(
        previous_owner: OrganizationMembershipRecord,
        current_owner: OrganizationMembershipRecord,
    ) -> Self {
        Self {
            previous_owner,
            current_owner,
        }
    }

    /// 拆出转移后的原 Owner 和新 Owner 快照。
    #[must_use]
    pub fn into_parts(self) -> (OrganizationMembershipRecord, OrganizationMembershipRecord) {
        (self.previous_owner, self.current_owner)
    }
}

/// 组织仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationRepositoryConfigError {
    /// 零超时无法形成有效的数据库截止边界。
    #[error("组织仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 组织仓储错误；不携带企业、成员、团队或用户的原始标识与文本。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationRepositoryError {
    /// 名称、短标识、说明或版本不满足持久化边界。
    #[error("组织仓储输入无效")]
    InvalidInput,
    /// 目标企业、成员或团队在给定企业范围内不存在。
    #[error("组织资源不存在")]
    NotFound,
    /// 目标全局用户不存在或已停用。
    #[error("组织成员用户不可用")]
    UserUnavailable,
    /// 唯一键或 CAS 版本与当前状态冲突。
    #[error("组织资源状态冲突")]
    Conflict,
    /// 目标生命周期转换不允许执行。
    #[error("组织资源状态转换无效")]
    InvalidTransition,
    /// 操作将导致企业失去最后一个活动 Owner。
    #[error("企业必须保留至少一个活动所有者")]
    LastOwner,
    /// 团队仍有活动成员归属，不能禁用。
    #[error("团队仍有活动成员")]
    TeamHasActiveMembers,
    /// 系统随机源无法生成企业公开标识。
    #[error("企业公开标识生成失败")]
    EntropyUnavailable,
    /// 获取连接、事务或执行数据库语句失败。
    #[error("组织仓储数据库操作失败")]
    Query,
    /// 单次数据库操作超过硬截止时间。
    #[error("组织仓储数据库操作超时")]
    Timeout,
    /// 持久化数据违反组织边界或闭合状态约束。
    #[error("组织仓储持久化状态损坏")]
    Invariant,
}
