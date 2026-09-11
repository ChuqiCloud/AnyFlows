use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationId, OrganizationMembershipId, OrganizationSsoProviderKey, OrganizationTeamId,
    UserId,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::DatabasePool;

mod invitation;
pub(crate) mod mutation;
pub(crate) mod read;
mod types;

pub use types::{
    MAX_ORGANIZATION_DIRECTORY_PAGE_SIZE, MAX_ORGANIZATION_ENTITLEMENT_REFERENCE_BYTES,
    MAX_ORGANIZATION_ENTITLEMENT_SOURCE_BYTES, MAX_ORGANIZATION_INVITATION_BATCH_SIZE,
    MAX_ORGANIZATION_NAME_BYTES, MAX_ORGANIZATION_SLUG_BYTES,
    MAX_ORGANIZATION_TEAM_DESCRIPTION_BYTES, MAX_ORGANIZATION_TEAM_NAME_BYTES,
    OrganizationAccessRecord, OrganizationCreate, OrganizationCreatedRecord,
    OrganizationEntitlementPreflightOutcome, OrganizationEntitlementReadRecord,
    OrganizationEntitlementRecord, OrganizationEntitlementWrite,
    OrganizationInvitationAcceptanceContext, OrganizationInvitationBatchRecord,
    OrganizationInvitationCreate, OrganizationInvitationPreviewItem,
    OrganizationInvitationPreviewRow, OrganizationInvitationPreviewSnapshot,
    OrganizationInvitationRecord, OrganizationMembershipChange, OrganizationMembershipCreate,
    OrganizationMembershipPageRecord, OrganizationMembershipRecord, OrganizationOwnershipTransfer,
    OrganizationOwnershipTransferRecord, OrganizationProfileChange, OrganizationRecord,
    OrganizationRepositoryConfigError, OrganizationRepositoryError, OrganizationTeamChange,
    OrganizationTeamCreate, OrganizationTeamPageRecord, OrganizationTeamRecord,
    PlatformOrganizationDirectoryPageRecord, PlatformOrganizationDirectoryRecord,
    PlatformOrganizationMemberPageRecord, PlatformOrganizationMemberRecord,
    PlatformUserOrganizationPageRecord, PlatformUserOrganizationRecord,
};

/// 企业主体、真实成员和扁平团队的事务仓储。
#[derive(Clone)]
pub struct OrganizationRepository {
    pub(super) pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationRepository {
    /// 使用共享连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 原子创建企业和第一个活动 Owner，禁止留下无 Owner 企业。
    pub async fn create(
        &self,
        write: OrganizationCreate,
    ) -> Result<OrganizationCreatedRecord, OrganizationRepositoryError> {
        self.run(mutation::create(self, write)).await
    }

    /// 按企业版本 CAS 更新基础资料，并保持短标识全局唯一。
    pub async fn update_profile(
        &self,
        organization_id: OrganizationId,
        change: OrganizationProfileChange,
    ) -> Result<OrganizationRecord, OrganizationRepositoryError> {
        self.run(mutation::update_profile(self, organization_id, change))
            .await
    }

    /// 由受信订单、合同或许可证流程按 CAS 替换当前商业授权。
    pub async fn replace_entitlement(
        &self,
        organization_id: OrganizationId,
        write: OrganizationEntitlementWrite,
    ) -> Result<OrganizationEntitlementRecord, OrganizationRepositoryError> {
        self.run(mutation::replace_entitlement(self, organization_id, write))
            .await
    }

    /// 在企业事务锁内重新统计资源占用并预检定时授权计划，不改变数据库事实。
    pub async fn preflight_entitlement_change(
        &self,
        organization_id: OrganizationId,
        plan: af_domain::OrganizationEntitlementChangePlan,
    ) -> Result<OrganizationEntitlementPreflightOutcome, OrganizationRepositoryError> {
        self.run(mutation::preflight_entitlement_change(
            self,
            organization_id,
            plan,
        ))
        .await
    }

    /// 按企业路径读取生命周期与完整授权事实。
    pub async fn entitlement(
        &self,
        organization_id: OrganizationId,
    ) -> Result<OrganizationEntitlementReadRecord, OrganizationRepositoryError> {
        self.run(read::entitlement(self, organization_id)).await
    }

    /// 在指定企业范围内创建一条唯一的活动成员关系。
    pub async fn add_membership(
        &self,
        organization_id: OrganizationId,
        write: OrganizationMembershipCreate,
    ) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
        self.run(mutation::add_membership(self, organization_id, write))
            .await
    }

    /// 按 CAS 版本更新成员角色、状态和主团队，并保护最后一个 Owner。
    pub async fn change_membership(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
        change: OrganizationMembershipChange,
    ) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
        self.run(mutation::change_membership(
            self,
            organization_id,
            membership_id,
            change,
        ))
        .await
    }

    /// 在企业路径内读取单个成员生命周期快照。
    pub async fn membership(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
    ) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
        self.run(read::membership(self, organization_id, membership_id))
            .await
    }

    /// 按稳定成员 ID 游标列出企业成员。
    pub async fn list_memberships(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationMembershipId>,
        limit: usize,
    ) -> Result<OrganizationMembershipPageRecord, OrganizationRepositoryError> {
        self.run(read::list_memberships(self, organization_id, after, limit))
            .await
    }

    /// 按企业 ID 游标读取平台企业目录安全摘要。
    pub async fn list_platform_directory(
        &self,
        after: Option<OrganizationId>,
        limit: usize,
    ) -> Result<PlatformOrganizationDirectoryPageRecord, OrganizationRepositoryError> {
        self.run(read::list_platform_directory(self, after, limit))
            .await
    }

    /// 按成员关系 ID 游标读取指定用户的全部企业归属。
    pub async fn list_user_organizations(
        &self,
        user_id: UserId,
        after: Option<OrganizationMembershipId>,
        limit: usize,
    ) -> Result<PlatformUserOrganizationPageRecord, OrganizationRepositoryError> {
        self.run(read::list_user_organizations(self, user_id, after, limit))
            .await
    }

    /// 按成员关系 ID 游标读取指定企业的成员身份摘要。
    pub async fn list_platform_members(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationMembershipId>,
        limit: usize,
    ) -> Result<PlatformOrganizationMemberPageRecord, OrganizationRepositoryError> {
        self.run(read::list_platform_members(
            self,
            organization_id,
            after,
            limit,
        ))
        .await
    }

    /// 按企业和成员双重范围读取单个成员身份摘要。
    pub async fn platform_member(
        &self,
        organization_id: OrganizationId,
        membership_id: OrganizationMembershipId,
    ) -> Result<PlatformOrganizationMemberRecord, OrganizationRepositoryError> {
        self.run(read::platform_member(self, organization_id, membership_id))
            .await
    }

    /// 在同一事务内提升新 Owner 并降级原 Owner。
    pub async fn transfer_ownership(
        &self,
        organization_id: OrganizationId,
        transfer: OrganizationOwnershipTransfer,
    ) -> Result<OrganizationOwnershipTransferRecord, OrganizationRepositoryError> {
        self.run(mutation::transfer_ownership(
            self,
            organization_id,
            transfer,
        ))
        .await
    }

    /// 在指定企业内创建活动团队。
    pub async fn create_team(
        &self,
        organization_id: OrganizationId,
        write: OrganizationTeamCreate,
    ) -> Result<OrganizationTeamRecord, OrganizationRepositoryError> {
        self.run(mutation::create_team(self, organization_id, write))
            .await
    }

    /// 按 CAS 更新活动团队资料。
    pub async fn update_team(
        &self,
        organization_id: OrganizationId,
        team_id: OrganizationTeamId,
        change: OrganizationTeamChange,
    ) -> Result<OrganizationTeamRecord, OrganizationRepositoryError> {
        self.run(mutation::update_team(
            self,
            organization_id,
            team_id,
            change,
        ))
        .await
    }

    /// 按稳定团队 ID 游标列出企业团队。
    pub async fn list_teams(
        &self,
        organization_id: OrganizationId,
        after: Option<OrganizationTeamId>,
        limit: usize,
    ) -> Result<OrganizationTeamPageRecord, OrganizationRepositoryError> {
        self.run(read::list_teams(self, organization_id, after, limit))
            .await
    }

    /// 禁用没有活动成员归属的团队，并使用 CAS 防止覆盖并发修改。
    pub async fn disable_team(
        &self,
        organization_id: OrganizationId,
        team_id: OrganizationTeamId,
        expected_version: i64,
    ) -> Result<OrganizationTeamRecord, OrganizationRepositoryError> {
        self.run(mutation::disable_team(
            self,
            organization_id,
            team_id,
            expected_version,
        ))
        .await
    }

    /// 按企业路径和当前用户同时解析活动成员，禁止只凭成员或团队标识进入企业。
    pub async fn authorized_access(
        &self,
        organization_id: OrganizationId,
        user_id: UserId,
    ) -> Result<OrganizationAccessRecord, OrganizationRepositoryError> {
        self.run(read::authorized_access(self, organization_id, user_id))
            .await
    }

    /// 列出当前用户可进入的活动企业及其活动成员角色。
    pub async fn list_authorized_access(
        &self,
        user_id: UserId,
    ) -> Result<Vec<OrganizationAccessRecord>, OrganizationRepositoryError> {
        self.run(read::list_authorized_access(self, user_id)).await
    }

    /// 在企业事务锁内创建一条邀请；有效邀请会占用一个待接受席位。
    pub async fn create_invitation(
        &self,
        organization_id: OrganizationId,
        write: OrganizationInvitationCreate,
    ) -> Result<OrganizationInvitationRecord, OrganizationRepositoryError> {
        self.run(invitation::create(self, organization_id, write))
            .await
    }

    /// 在企业事务锁内幂等创建最多 200 条批量邀请。
    pub async fn create_invitation_batch(
        &self,
        organization_id: OrganizationId,
        writes: Vec<OrganizationInvitationCreate>,
    ) -> Result<OrganizationInvitationBatchRecord, OrganizationRepositoryError> {
        self.run(invitation::create_batch(self, organization_id, writes))
            .await
    }

    /// 在组织事务内读取批量邀请的成员、待处理邀请、团队和席位事实。
    pub async fn preview_invitations(
        &self,
        organization_id: OrganizationId,
        items: Vec<OrganizationInvitationPreviewItem>,
    ) -> Result<OrganizationInvitationPreviewSnapshot, OrganizationRepositoryError> {
        self.run(invitation::preview(self, organization_id, items))
            .await
    }

    /// 列出企业邀请安全投影，不读取邮箱明文或密码学材料。
    pub async fn list_invitations(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Vec<OrganizationInvitationRecord>, OrganizationRepositoryError> {
        self.run(invitation::list(self, organization_id)).await
    }

    /// 按邀请标识读取接受事务所需的企业范围和邮箱密文。
    pub async fn invitation_acceptance_context(
        &self,
        invitation_key: af_domain::OrganizationInvitationKey,
    ) -> Result<OrganizationInvitationAcceptanceContext, OrganizationRepositoryError> {
        self.run(invitation::acceptance_context(self, invitation_key))
            .await
    }

    /// 锁定企业、邀请和目标用户，校验邮箱后单次创建成员关系并消费邀请。
    pub async fn accept_invitation(
        &self,
        organization_id: OrganizationId,
        invitation_key: af_domain::OrganizationInvitationKey,
        token_digest: [u8; 32],
        email_fingerprint: [u8; 32],
        user_id: UserId,
        normalized_email: String,
    ) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
        self.run(invitation::accept(
            self,
            organization_id,
            invitation_key,
            token_digest,
            email_fingerprint,
            user_id,
            normalized_email,
        ))
        .await
    }

    /// 仅凭受控 IdP claim 的企业隔离邮箱指纹消费有效邀请，并原子建立 JIT 用户、成员和 SSO 身份。
    ///
    /// 调用方必须先验证协议签名和 claim 语义；此入口不接受邮箱、域名或用户名等可推测关联材料。
    pub async fn bind_invited_sso_identity(
        &self,
        organization_id: OrganizationId,
        provider_key: OrganizationSsoProviderKey,
        subject_digest: String,
        invitation_email_fingerprint: [u8; 32],
    ) -> Result<OrganizationMembershipRecord, OrganizationRepositoryError> {
        self.run(invitation::bind_invited_sso_identity(
            self,
            organization_id,
            provider_key,
            subject_digest,
            invitation_email_fingerprint,
        ))
        .await
    }

    /// 撤销仍待接受的邀请；已消费邀请不可逆转。
    pub async fn revoke_invitation(
        &self,
        organization_id: OrganizationId,
        invitation_key: af_domain::OrganizationInvitationKey,
    ) -> Result<OrganizationInvitationRecord, OrganizationRepositoryError> {
        self.run(invitation::revoke(self, organization_id, invitation_key))
            .await
    }

    /// 以同一事务撤销旧邀请并写入新的令牌版本。
    pub async fn resend_invitation(
        &self,
        organization_id: OrganizationId,
        invitation_key: af_domain::OrganizationInvitationKey,
        write: OrganizationInvitationCreate,
    ) -> Result<OrganizationInvitationRecord, OrganizationRepositoryError> {
        self.run(invitation::resend(
            self,
            organization_id,
            invitation_key,
            write,
        ))
        .await
    }

    async fn run<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationRepositoryError>>,
    ) -> Result<T, OrganizationRepositoryError> {
        match timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(OrganizationRepositoryError::Timeout)),
        }
    }
}

impl fmt::Debug for OrganizationRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

/// 只记录闭合错误分类，避免组织标识、成员身份和企业资料进入日志。
pub(super) fn record_internal_error(
    error: OrganizationRepositoryError,
) -> OrganizationRepositoryError {
    let error_kind = match error {
        OrganizationRepositoryError::InvalidInput
        | OrganizationRepositoryError::NotFound
        | OrganizationRepositoryError::UserUnavailable
        | OrganizationRepositoryError::Conflict
        | OrganizationRepositoryError::InvalidTransition
        | OrganizationRepositoryError::LastOwner
        | OrganizationRepositoryError::TeamHasActiveMembers
        | OrganizationRepositoryError::EntitlementRestricted
        | OrganizationRepositoryError::SeatLimitReached
        | OrganizationRepositoryError::InvitationExpired
        | OrganizationRepositoryError::InvitationNotPending
        | OrganizationRepositoryError::InvitationEmailMismatch
        | OrganizationRepositoryError::InvitationBatchConflict
        | OrganizationRepositoryError::TeamLimitReached
        | OrganizationRepositoryError::EntropyUnavailable => return error,
        OrganizationRepositoryError::Query => "organization_query",
        OrganizationRepositoryError::Timeout => "organization_timeout",
        OrganizationRepositoryError::Invariant => "organization_invariant",
    };
    tracing::error!(
        target: "af_db::organization",
        error_kind,
        "组织仓储发生内部错误"
    );
    error
}

pub(super) fn validate_name(
    value: &str,
    maximum_bytes: usize,
) -> Result<(), OrganizationRepositoryError> {
    if value.is_empty()
        || value.len() > maximum_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_slug(value: &str) -> Result<(), OrganizationRepositoryError> {
    if value.is_empty()
        || value.len() > MAX_ORGANIZATION_SLUG_BYTES
        || value.starts_with('-')
        || value.ends_with('-')
        || value.contains("--")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(OrganizationRepositoryError::InvalidInput);
    }
    Ok(())
}
