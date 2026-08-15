use std::{fmt, future::Future, time::Duration};

use af_domain::{OrganizationId, OrganizationMembershipId, OrganizationTeamId};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::DatabasePool;

mod mutation;
mod read;
mod types;

pub use types::{
    MAX_ORGANIZATION_NAME_BYTES, MAX_ORGANIZATION_SLUG_BYTES,
    MAX_ORGANIZATION_TEAM_DESCRIPTION_BYTES, MAX_ORGANIZATION_TEAM_NAME_BYTES, OrganizationCreate,
    OrganizationCreatedRecord, OrganizationMembershipChange, OrganizationMembershipCreate,
    OrganizationMembershipRecord, OrganizationOwnershipTransfer,
    OrganizationOwnershipTransferRecord, OrganizationRecord, OrganizationRepositoryConfigError,
    OrganizationRepositoryError, OrganizationTeamCreate, OrganizationTeamRecord,
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
