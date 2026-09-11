use std::fmt;

use crate::EncryptedCredentialEnvelope;
use af_domain::{
    OrganizationCustomRoleId, OrganizationCustomRoleKey, OrganizationEntitlementPreflightError,
    OrganizationEntitlementSnapshot, OrganizationId, OrganizationInvitationKey,
    OrganizationInvitationStatus, OrganizationKey, OrganizationMembershipId,
    OrganizationMembershipStatus, OrganizationPermission, OrganizationRole, OrganizationStatus,
    OrganizationTeamId, OrganizationTeamStatus, UserId,
};
use thiserror::Error;
use zeroize::Zeroizing;

/// 企业显示名最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_NAME_BYTES: usize = 128;
/// 企业短标识最大 ASCII 字节数。
pub const MAX_ORGANIZATION_SLUG_BYTES: usize = 64;
/// 企业团队名称最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_TEAM_NAME_BYTES: usize = 128;
/// 企业团队说明最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_TEAM_DESCRIPTION_BYTES: usize = 2_000;
/// 企业成员和团队目录单页最大记录数。
pub const MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE: usize = 100;
/// 授权来源最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_ENTITLEMENT_SOURCE_BYTES: usize = 64;
/// 授权外部引用最大 UTF-8 字节数。
pub const MAX_ORGANIZATION_ENTITLEMENT_REFERENCE_BYTES: usize = 128;
/// 单批企业邀请最多允许提交的行数。
pub const MAX_ORGANIZATION_INVITATION_BATCH_SIZE: usize = 200;

/// 创建企业时由服务端验证的基础资料和初始 Owner。
pub struct OrganizationCreate {
    pub(super) name: String,
    pub(super) slug: String,
    pub(super) owner_user_id: UserId,
    pub(super) entitlement: OrganizationEntitlementWrite,
}

impl OrganizationCreate {
    /// 组装企业基础资料；公开标识和 Owner 关系由仓储原子生成。
    #[must_use]
    pub fn new(
        name: String,
        slug: String,
        owner_user_id: UserId,
        entitlement: OrganizationEntitlementWrite,
    ) -> Self {
        Self {
            name,
            slug,
            owner_user_id,
            entitlement,
        }
    }
}

impl fmt::Debug for OrganizationCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationCreate(<redacted>)")
    }
}

/// 按企业版本更新显示名称和稳定短标识。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationProfileChange {
    pub(super) name: String,
    pub(super) slug: String,
    pub(super) expected_version: i64,
}

impl OrganizationProfileChange {
    /// 组装企业资料的完整目标快照；字段边界由仓储统一校验。
    #[must_use]
    pub fn new(name: String, slug: String, expected_version: i64) -> Self {
        Self {
            name,
            slug,
            expected_version,
        }
    }
}

/// 仅供受信订单、合同或许可证流程提交的完整授权快照。
#[derive(Clone)]
pub struct OrganizationEntitlementWrite {
    pub(super) snapshot: OrganizationEntitlementSnapshot,
    pub(super) source: String,
    pub(super) external_reference: Option<String>,
    pub(super) expected_version: Option<i64>,
}

impl OrganizationEntitlementWrite {
    /// 构造完整授权写入；首次写入不带版本，替换时必须携带当前版本。
    pub fn new(
        snapshot: OrganizationEntitlementSnapshot,
        source: String,
        external_reference: Option<String>,
        expected_version: Option<u64>,
    ) -> Result<Self, OrganizationRepositoryError> {
        if !valid_entitlement_text(&source, MAX_ORGANIZATION_ENTITLEMENT_SOURCE_BYTES)
            || external_reference.as_ref().is_some_and(|value| {
                !valid_entitlement_text(value, MAX_ORGANIZATION_ENTITLEMENT_REFERENCE_BYTES)
            })
        {
            return Err(OrganizationRepositoryError::InvalidInput);
        }
        let expected_version = expected_version
            .map(i64::try_from)
            .transpose()
            .map_err(|_| OrganizationRepositoryError::InvalidInput)?;
        if expected_version.is_some_and(|value| value <= 0 || value == i64::MAX) {
            return Err(OrganizationRepositoryError::InvalidInput);
        }
        Ok(Self {
            snapshot,
            source,
            external_reference,
            expected_version,
        })
    }

    pub(crate) const fn snapshot(&self) -> OrganizationEntitlementSnapshot {
        self.snapshot
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn external_reference(&self) -> Option<&str> {
        self.external_reference.as_deref()
    }

    pub(crate) const fn expected_version(&self) -> Option<i64> {
        self.expected_version
    }
}

impl fmt::Debug for OrganizationEntitlementWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationEntitlementWrite")
            .field("snapshot", &self.snapshot)
            .field("source", &self.source)
            .field("has_external_reference", &self.external_reference.is_some())
            .field("expected_version", &self.expected_version)
            .finish_non_exhaustive()
    }
}

/// 已持久化的企业授权快照。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationEntitlementRecord {
    pub(super) organization_id: OrganizationId,
    pub(super) snapshot: OrganizationEntitlementSnapshot,
    pub(super) source: String,
    pub(super) external_reference: Option<String>,
    pub(super) version: i64,
    pub(super) updated_at: i64,
}

/// 企业授权变更的事务预检结果；拒绝只返回原因，不触碰任何企业资源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationEntitlementPreflightOutcome {
    /// 当前占用满足目标授权。
    Passed,
    /// 当前占用不满足目标授权，调用方应保留原授权和全部资源。
    Rejected(OrganizationEntitlementPreflightError),
}

impl OrganizationEntitlementRecord {
    /// 返回授权所属企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回授权能力、容量和时间边界。
    #[must_use]
    pub const fn snapshot(&self) -> OrganizationEntitlementSnapshot {
        self.snapshot
    }

    /// 返回授权来源。
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// 返回脱敏外部引用。
    #[must_use]
    pub fn external_reference(&self) -> Option<&str> {
        self.external_reference.as_deref()
    }

    /// 返回 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// 返回授权事实最近更新时间。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
}

/// 企业生命周期与完整授权事实的同次读取结果。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationEntitlementReadRecord {
    pub(super) organization_status: OrganizationStatus,
    pub(super) entitlement: OrganizationEntitlementRecord,
}

impl OrganizationEntitlementReadRecord {
    /// 返回企业生命周期状态，用于推导当前授权状态。
    #[must_use]
    pub const fn organization_status(&self) -> OrganizationStatus {
        self.organization_status
    }

    /// 返回完整授权事实。
    #[must_use]
    pub const fn entitlement(&self) -> &OrganizationEntitlementRecord {
        &self.entitlement
    }
}

fn valid_entitlement_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
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

/// 带 CAS 版本的企业团队资料目标快照。
pub struct OrganizationTeamChange {
    pub(super) name: String,
    pub(super) description: Option<String>,
    pub(super) expected_version: i64,
}

impl OrganizationTeamChange {
    /// 组装团队资料更新，字段边界由仓储统一校验。
    #[must_use]
    pub fn new(name: String, description: Option<String>, expected_version: i64) -> Self {
        Self {
            name,
            description,
            expected_version,
        }
    }
}

impl OrganizationTeamCreate {
    /// 组装企业内的扁平团队资料。
    #[must_use]
    pub fn new(name: String, description: Option<String>) -> Self {
        Self { name, description }
    }
}

/// 已完成邮箱规范化、摘要和密文封套的单条邀请材料。
pub struct OrganizationInvitationCreate {
    pub(super) invitation_key: OrganizationInvitationKey,
    pub(super) normalized_email: Zeroizing<String>,
    pub(super) email_fingerprint: [u8; 32],
    pub(super) email_secret: EncryptedCredentialEnvelope,
    pub(super) token_digest: [u8; 32],
    pub(super) role: OrganizationRole,
    pub(super) team_id: Option<OrganizationTeamId>,
    pub(super) invited_by_user_id: UserId,
    pub(super) batch_key: Option<OrganizationInvitationKey>,
    pub(super) batch_digest: Option<[u8; 32]>,
    pub(super) batch_size: Option<usize>,
    pub(super) expires_at: i64,
}

impl OrganizationInvitationCreate {
    /// 组装一条已在服务层完成密码学处理的邀请。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        invitation_key: OrganizationInvitationKey,
        normalized_email: String,
        email_fingerprint: [u8; 32],
        email_secret: EncryptedCredentialEnvelope,
        token_digest: [u8; 32],
        role: OrganizationRole,
        team_id: Option<OrganizationTeamId>,
        invited_by_user_id: UserId,
        batch_key: Option<OrganizationInvitationKey>,
        batch_digest: Option<[u8; 32]>,
        batch_size: Option<usize>,
        expires_at: i64,
    ) -> Result<Self, OrganizationRepositoryError> {
        if normalized_email.is_empty()
            || normalized_email.len() > 320
            || normalized_email.trim() != normalized_email
            || normalized_email.bytes().any(|byte| byte.is_ascii_control())
            || normalized_email
                .bytes()
                .any(|byte| byte.is_ascii_whitespace())
            || normalized_email
                .bytes()
                .any(|byte| byte.is_ascii_uppercase())
            || role.is_owner()
            || expires_at <= 0
            || batch_key.is_none() != batch_digest.is_none()
            || batch_key.is_none() != batch_size.is_none()
            || batch_size
                .is_some_and(|size| !(1..=MAX_ORGANIZATION_INVITATION_BATCH_SIZE).contains(&size))
        {
            return Err(OrganizationRepositoryError::InvalidInput);
        }
        Ok(Self {
            invitation_key,
            normalized_email: Zeroizing::new(normalized_email),
            email_fingerprint,
            email_secret,
            token_digest,
            role,
            team_id,
            invited_by_user_id,
            batch_key,
            batch_digest,
            batch_size,
            expires_at,
        })
    }
}

impl fmt::Debug for OrganizationInvitationCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationInvitationCreate")
            .field("role", &self.role)
            .field("has_team", &self.team_id.is_some())
            .field("has_batch", &self.batch_key.is_some())
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// 已持久化企业邀请的安全投影；不包含邮箱、摘要或密文。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationInvitationRecord {
    pub(super) id: i64,
    pub(super) invitation_key: OrganizationInvitationKey,
    pub(super) organization_id: OrganizationId,
    pub(super) role: OrganizationRole,
    pub(super) team_id: Option<OrganizationTeamId>,
    pub(super) status: OrganizationInvitationStatus,
    pub(super) invited_by_user_id: UserId,
    pub(super) accepted_by_user_id: Option<UserId>,
    pub(super) batch_key: Option<OrganizationInvitationKey>,
    pub(super) version: i64,
    pub(super) created_at: i64,
    pub(super) expires_at: i64,
    pub(super) finalized_at: Option<i64>,
}

/// 批量邀请提交结果；重放只返回已持久化事实，不重新生成投递令牌。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationInvitationBatchRecord {
    records: Vec<OrganizationInvitationRecord>,
    replayed: bool,
}

impl OrganizationInvitationBatchRecord {
    pub(super) const fn new(records: Vec<OrganizationInvitationRecord>, replayed: bool) -> Self {
        Self { records, replayed }
    }

    /// 返回批次内邀请安全投影。
    #[must_use]
    pub fn records(&self) -> &[OrganizationInvitationRecord] {
        &self.records
    }

    /// 判断本次是否命中了同一批次的幂等重放。
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

/// 邀请批量预览输入；邮箱已由服务层规范化，仓储只用它做受控匹配。
#[derive(Clone)]
pub struct OrganizationInvitationPreviewItem {
    pub(super) normalized_email: String,
    pub(super) email_fingerprint: [u8; 32],
    pub(super) team_id: Option<OrganizationTeamId>,
}

impl OrganizationInvitationPreviewItem {
    /// 组装一条预览匹配输入。
    pub fn new(
        normalized_email: String,
        email_fingerprint: [u8; 32],
        team_id: Option<OrganizationTeamId>,
    ) -> Self {
        Self {
            normalized_email,
            email_fingerprint,
            team_id,
        }
    }
}

impl fmt::Debug for OrganizationInvitationPreviewItem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationInvitationPreviewItem")
            .field("has_team", &self.team_id.is_some())
            .finish_non_exhaustive()
    }
}

/// 单条预览匹配事实；不返回邮箱或任何密码学材料。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationInvitationPreviewRow {
    existing_member: bool,
    pending_invitation: bool,
    valid_team: bool,
}

impl OrganizationInvitationPreviewRow {
    pub(super) const fn new(
        existing_member: bool,
        pending_invitation: bool,
        valid_team: bool,
    ) -> Self {
        Self {
            existing_member,
            pending_invitation,
            valid_team,
        }
    }

    #[must_use]
    pub const fn existing_member(self) -> bool {
        self.existing_member
    }

    #[must_use]
    pub const fn pending_invitation(self) -> bool {
        self.pending_invitation
    }

    #[must_use]
    pub const fn valid_team(self) -> bool {
        self.valid_team
    }
}

/// 在同一组织事务中读取的批量预览快照。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationInvitationPreviewSnapshot {
    rows: Vec<OrganizationInvitationPreviewRow>,
    available_seats: i64,
}

impl OrganizationInvitationPreviewSnapshot {
    pub(super) const fn new(
        rows: Vec<OrganizationInvitationPreviewRow>,
        available_seats: i64,
    ) -> Self {
        Self {
            rows,
            available_seats,
        }
    }

    /// 返回与输入顺序对应的成员/邀请/团队事实。
    #[must_use]
    pub fn rows(&self) -> &[OrganizationInvitationPreviewRow] {
        &self.rows
    }

    /// 返回当前已扣除活动成员和待接受邀请后的席位数。
    #[must_use]
    pub const fn available_seats(&self) -> i64 {
        self.available_seats
    }
}

impl OrganizationInvitationRecord {
    /// 返回邀请内部标识。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回不暴露数据库顺序的邀请标识。
    #[must_use]
    pub const fn invitation_key(&self) -> OrganizationInvitationKey {
        self.invitation_key
    }

    /// 返回所属企业。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    /// 返回接受后使用的固定角色。
    #[must_use]
    pub const fn role(&self) -> OrganizationRole {
        self.role
    }

    /// 返回接受后使用的主团队。
    #[must_use]
    pub const fn team_id(&self) -> Option<OrganizationTeamId> {
        self.team_id
    }

    /// 返回邀请状态。
    #[must_use]
    pub const fn status(&self) -> OrganizationInvitationStatus {
        self.status
    }

    /// 返回邀请操作者。
    #[must_use]
    pub const fn invited_by_user_id(&self) -> UserId {
        self.invited_by_user_id
    }

    /// 返回接受用户（若已消费）。
    #[must_use]
    pub const fn accepted_by_user_id(&self) -> Option<UserId> {
        self.accepted_by_user_id
    }

    /// 返回批次幂等标识。
    #[must_use]
    pub const fn batch_key(&self) -> Option<OrganizationInvitationKey> {
        self.batch_key
    }

    /// 返回邀请生命周期 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// 返回创建时间。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回过期时间。
    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }

    /// 返回最终化时间。
    #[must_use]
    pub const fn finalized_at(&self) -> Option<i64> {
        self.finalized_at
    }
}

/// 接受事务的受限上下文；仅向受控服务暴露邮箱密文。
#[derive(Clone)]
pub struct OrganizationInvitationAcceptanceContext {
    pub(super) record: OrganizationInvitationRecord,
    pub(super) email_secret: EncryptedCredentialEnvelope,
}

impl OrganizationInvitationAcceptanceContext {
    /// 拆出安全邀请投影与密文封套。
    #[must_use]
    pub fn into_parts(self) -> (OrganizationInvitationRecord, EncryptedCredentialEnvelope) {
        (self.record, self.email_secret)
    }
}

impl fmt::Debug for OrganizationInvitationAcceptanceContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationInvitationAcceptanceContext")
            .field("record", &self.record)
            .field("email_secret", &"<redacted>")
            .finish()
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
    pub(super) custom_role_id: Option<OrganizationCustomRoleId>,
    pub(super) custom_role_key: Option<OrganizationCustomRoleKey>,
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

    /// 返回当前绑定的自定义角色内部标识。
    #[must_use]
    pub const fn custom_role_id(&self) -> Option<OrganizationCustomRoleId> {
        self.custom_role_id
    }

    /// 返回成员当前绑定的自定义角色公开标识。
    #[must_use]
    pub const fn custom_role_key(&self) -> Option<OrganizationCustomRoleKey> {
        self.custom_role_key
    }

    pub(super) const fn with_custom_role_key(
        mut self,
        custom_role_key: Option<OrganizationCustomRoleKey>,
    ) -> Self {
        self.custom_role_key = custom_role_key;
        self
    }

    /// 返回成员 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// 返回加入企业的时间。
    #[must_use]
    pub const fn joined_at(&self) -> i64 {
        self.joined_at
    }

    /// 返回首次停用时间。
    #[must_use]
    pub const fn suspended_at(&self) -> Option<i64> {
        self.suspended_at
    }

    /// 返回移除时间。
    #[must_use]
    pub const fn removed_at(&self) -> Option<i64> {
        self.removed_at
    }

    /// 返回可选邀请操作人。
    #[must_use]
    pub const fn invited_by_user_id(&self) -> Option<UserId> {
        self.invited_by_user_id
    }
}

impl fmt::Debug for OrganizationMembershipRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationMembershipRecord")
            .field("role", &self.role)
            .field("status", &self.status)
            .field("has_team", &self.team_id.is_some())
            .field("has_custom_role", &self.custom_role_id.is_some())
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
    entitlement: OrganizationEntitlementSnapshot,
    custom_role_permissions: Option<Vec<OrganizationPermission>>,
    active_seats: i64,
    active_teams: i64,
}

/// 平台企业目录使用的最小安全快照，不包含资金、密钥或认证配置。
#[derive(Clone, Debug, PartialEq)]
pub struct PlatformOrganizationDirectoryRecord {
    organization: OrganizationRecord,
    entitlement: OrganizationEntitlementSnapshot,
    owner_membership: OrganizationMembershipRecord,
    owner_username: String,
    active_member_count: i64,
}

impl PlatformOrganizationDirectoryRecord {
    pub(super) fn new(
        organization: OrganizationRecord,
        entitlement: OrganizationEntitlementSnapshot,
        owner_membership: OrganizationMembershipRecord,
        owner_username: String,
        active_member_count: i64,
    ) -> Self {
        Self {
            organization,
            entitlement,
            owner_membership,
            owner_username,
            active_member_count,
        }
    }

    /// 返回企业基础事实。
    #[must_use]
    pub const fn organization(&self) -> &OrganizationRecord {
        &self.organization
    }

    /// 返回企业当前授权快照。
    #[must_use]
    pub const fn entitlement(&self) -> OrganizationEntitlementSnapshot {
        self.entitlement
    }

    /// 返回当前活动 Owner 成员关系。
    #[must_use]
    pub const fn owner_membership(&self) -> &OrganizationMembershipRecord {
        &self.owner_membership
    }

    /// 返回当前活动 Owner 的非敏感用户名。
    #[must_use]
    pub fn owner_username(&self) -> &str {
        &self.owner_username
    }

    /// 返回企业当前活动成员数。
    #[must_use]
    pub const fn active_member_count(&self) -> i64 {
        self.active_member_count
    }
}

/// 平台企业目录稳定游标页。
#[derive(Debug, PartialEq)]
pub struct PlatformOrganizationDirectoryPageRecord {
    records: Vec<PlatformOrganizationDirectoryRecord>,
    next_cursor: Option<OrganizationId>,
}

impl PlatformOrganizationDirectoryPageRecord {
    pub(super) const fn new(
        records: Vec<PlatformOrganizationDirectoryRecord>,
        next_cursor: Option<OrganizationId>,
    ) -> Self {
        Self {
            records,
            next_cursor,
        }
    }

    /// 拆出本页企业摘要和下一游标。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<PlatformOrganizationDirectoryRecord>,
        Option<OrganizationId>,
    ) {
        (self.records, self.next_cursor)
    }
}

/// 平台按用户查询到的企业成员关系安全快照。
#[derive(Clone, Debug, PartialEq)]
pub struct PlatformUserOrganizationRecord {
    organization: OrganizationRecord,
    membership: OrganizationMembershipRecord,
    entitlement: OrganizationEntitlementSnapshot,
}

impl PlatformUserOrganizationRecord {
    pub(super) const fn new(
        organization: OrganizationRecord,
        membership: OrganizationMembershipRecord,
        entitlement: OrganizationEntitlementSnapshot,
    ) -> Self {
        Self {
            organization,
            membership,
            entitlement,
        }
    }

    /// 返回企业基础事实。
    #[must_use]
    pub const fn organization(&self) -> &OrganizationRecord {
        &self.organization
    }

    /// 返回用户在该企业中的成员关系。
    #[must_use]
    pub const fn membership(&self) -> &OrganizationMembershipRecord {
        &self.membership
    }

    /// 返回企业当前授权快照。
    #[must_use]
    pub const fn entitlement(&self) -> OrganizationEntitlementSnapshot {
        self.entitlement
    }
}

/// 平台用户企业归属稳定游标页。
#[derive(Debug, PartialEq)]
pub struct PlatformUserOrganizationPageRecord {
    records: Vec<PlatformUserOrganizationRecord>,
    next_cursor: Option<OrganizationMembershipId>,
}

/// 平台跨企业成员管理使用的身份与成员关系安全快照。
#[derive(Clone, PartialEq)]
pub struct PlatformOrganizationMemberRecord {
    membership: OrganizationMembershipRecord,
    username: String,
    email: Option<String>,
}

impl PlatformOrganizationMemberRecord {
    pub(super) fn new(
        membership: OrganizationMembershipRecord,
        username: String,
        email: Option<String>,
    ) -> Self {
        Self {
            membership,
            username,
            email,
        }
    }

    /// 返回企业范围内的成员关系事实。
    #[must_use]
    pub const fn membership(&self) -> &OrganizationMembershipRecord {
        &self.membership
    }

    /// 返回成员对应的全局用户名。
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 返回成员用于身份确认的全局邮箱。
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }
}

impl fmt::Debug for PlatformOrganizationMemberRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlatformOrganizationMemberRecord")
            .field("membership", &self.membership)
            .field("has_email", &self.email.is_some())
            .finish_non_exhaustive()
    }
}

/// 平台企业成员目录稳定游标页。
#[derive(Debug, PartialEq)]
pub struct PlatformOrganizationMemberPageRecord {
    records: Vec<PlatformOrganizationMemberRecord>,
    next_cursor: Option<OrganizationMembershipId>,
}

impl PlatformOrganizationMemberPageRecord {
    pub(super) const fn new(
        records: Vec<PlatformOrganizationMemberRecord>,
        next_cursor: Option<OrganizationMembershipId>,
    ) -> Self {
        Self {
            records,
            next_cursor,
        }
    }

    /// 拆出成员身份摘要与下一页游标。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<PlatformOrganizationMemberRecord>,
        Option<OrganizationMembershipId>,
    ) {
        (self.records, self.next_cursor)
    }
}

impl PlatformUserOrganizationPageRecord {
    pub(super) const fn new(
        records: Vec<PlatformUserOrganizationRecord>,
        next_cursor: Option<OrganizationMembershipId>,
    ) -> Self {
        Self {
            records,
            next_cursor,
        }
    }

    /// 拆出本页归属摘要和下一游标。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<PlatformUserOrganizationRecord>,
        Option<OrganizationMembershipId>,
    ) {
        (self.records, self.next_cursor)
    }
}

impl OrganizationAccessRecord {
    pub(super) fn new(
        organization: OrganizationRecord,
        membership: OrganizationMembershipRecord,
        entitlement: OrganizationEntitlementSnapshot,
        custom_role_permissions: Option<Vec<OrganizationPermission>>,
        active_seats: i64,
        active_teams: i64,
    ) -> Self {
        Self {
            organization,
            membership,
            entitlement,
            custom_role_permissions,
            active_seats,
            active_teams,
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

    /// 返回当前企业的商业授权快照。
    #[must_use]
    pub const fn entitlement(&self) -> OrganizationEntitlementSnapshot {
        self.entitlement
    }

    /// 返回已经过开关、角色状态和白名单闭合的自定义权限投影。
    #[must_use]
    pub fn custom_role_permissions(&self) -> Option<&[OrganizationPermission]> {
        self.custom_role_permissions.as_deref()
    }

    /// 返回当前活动成员数。
    #[must_use]
    pub const fn active_seats(&self) -> i64 {
        self.active_seats
    }

    /// 返回当前活动团队数。
    #[must_use]
    pub const fn active_teams(&self) -> i64 {
        self.active_teams
    }
}

impl OrganizationTeamRecord {
    /// 返回团队所属企业标识。
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

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

    /// 返回团队名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回可选团队说明。
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// 返回创建时间。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// 返回最近更新时间。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }

    /// 返回禁用时间。
    #[must_use]
    pub const fn deleted_at(&self) -> Option<i64> {
        self.deleted_at
    }
}

/// 按稳定成员 ID 游标返回的企业成员目录页。
#[derive(Debug, PartialEq)]
pub struct OrganizationMembershipPageRecord {
    records: Vec<OrganizationMembershipRecord>,
    next_cursor: Option<OrganizationMembershipId>,
}

impl OrganizationMembershipPageRecord {
    pub(super) const fn new(
        records: Vec<OrganizationMembershipRecord>,
        next_cursor: Option<OrganizationMembershipId>,
    ) -> Self {
        Self {
            records,
            next_cursor,
        }
    }

    /// 拆出本页记录与下一页游标。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<OrganizationMembershipRecord>,
        Option<OrganizationMembershipId>,
    ) {
        (self.records, self.next_cursor)
    }
}

/// 按稳定团队 ID 游标返回的企业团队目录页。
#[derive(Debug, PartialEq)]
pub struct OrganizationTeamPageRecord {
    records: Vec<OrganizationTeamRecord>,
    next_cursor: Option<OrganizationTeamId>,
}

impl OrganizationTeamPageRecord {
    pub(super) const fn new(
        records: Vec<OrganizationTeamRecord>,
        next_cursor: Option<OrganizationTeamId>,
    ) -> Self {
        Self {
            records,
            next_cursor,
        }
    }

    /// 拆出本页记录与下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<OrganizationTeamRecord>, Option<OrganizationTeamId>) {
        (self.records, self.next_cursor)
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
    /// 当前授权不允许企业继续执行该写操作。
    #[error("企业商业授权不允许当前操作")]
    EntitlementRestricted,
    /// 企业席位容量已用尽。
    #[error("企业席位容量已用尽")]
    SeatLimitReached,
    /// 邀请已经过期或已被关闭。
    #[error("企业邀请已过期")]
    InvitationExpired,
    /// 邀请不再处于可消费状态。
    #[error("企业邀请不可消费")]
    InvitationNotPending,
    /// 当前接受用户邮箱与邀请不匹配。
    #[error("企业邀请邮箱不匹配")]
    InvitationEmailMismatch,
    /// 同一批次幂等键对应了不同请求内容。
    #[error("企业邀请批次幂等键冲突")]
    InvitationBatchConflict,
    /// 企业团队容量已用尽。
    #[error("企业团队容量已用尽")]
    TeamLimitReached,
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
