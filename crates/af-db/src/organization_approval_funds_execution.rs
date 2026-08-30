use std::fmt;

use af_domain::{
    OrganizationApprovalDecisionKind, OrganizationApprovalRequestId,
    OrganizationApprovalResourceType, OrganizationId, OrganizationMembershipId,
    OrganizationPermission, OrganizationStatus, Quota, QuotaDelta, UserId, WalletEventId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};
use sha2::{Digest as _, Sha256};

use crate::{
    MAX_ORGANIZATION_WALLET_ADJUSTMENT_REASON_BYTES, OrganizationApprovalExecutionRepositoryError,
    OrganizationWalletAdjustmentOutcome, OrganizationWalletAdjustmentWrite,
    entity::organization_approval_funds_executions,
    organization::mutation,
    organization_approval_execution::{
        lock_decision, lock_request, require_current_member_permission,
    },
    organization_wallet::apply_adjustment_with_expected_version,
};

const FUNDS_SUMMARY_PREFIX: &str = "funds-wallet-adjustment-sha256:";

/// 企业钱包调账审批执行的完整结构化目标快照。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationApprovalFundsExecutionWrite {
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    event_id: WalletEventId,
    quota_delta: QuotaDelta,
    reason: String,
    expected_wallet_version: i64,
    executed_by_membership_id: OrganizationMembershipId,
    executed_by_user_id: UserId,
}

impl OrganizationApprovalFundsExecutionWrite {
    /// 构造带钱包版本 CAS 和当前操作者双重身份的资金执行命令。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        request_id: OrganizationApprovalRequestId,
        event_id: WalletEventId,
        quota_delta: QuotaDelta,
        reason: String,
        expected_wallet_version: i64,
        executed_by_membership_id: OrganizationMembershipId,
        executed_by_user_id: UserId,
    ) -> Result<Self, OrganizationApprovalExecutionRepositoryError> {
        if event_id.is_system_opening()
            || quota_delta.is_zero()
            || expected_wallet_version <= 0
            || expected_wallet_version == i64::MAX
            || !valid_reason(&reason)
        {
            return Err(OrganizationApprovalExecutionRepositoryError::InvalidInput);
        }
        Ok(Self {
            organization_id,
            request_id,
            event_id,
            quota_delta,
            reason,
            expected_wallet_version,
            executed_by_membership_id,
            executed_by_user_id,
        })
    }

    /// 返回创建审批申请时必须保存的规范化命令摘要。
    #[must_use]
    pub fn resource_summary(&self) -> String {
        organization_approval_funds_resource_summary(
            self.organization_id,
            self.event_id,
            self.quota_delta,
            &self.reason,
            self.expected_wallet_version,
        )
    }

    #[must_use]
    pub const fn event_id(&self) -> WalletEventId {
        self.event_id
    }

    #[must_use]
    pub const fn quota_delta(&self) -> QuotaDelta {
        self.quota_delta
    }

    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    #[must_use]
    pub const fn expected_wallet_version(&self) -> i64 {
        self.expected_wallet_version
    }
}

/// 已追加的企业钱包调账审批执行事实。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalFundsExecutionRecord {
    id: i64,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    decision_id: i64,
    event_id: WalletEventId,
    quota_delta: QuotaDelta,
    reason: String,
    expected_wallet_version: i64,
    applied_wallet_version: i64,
    balance_before: Quota,
    balance_after: Quota,
    ledger_entry_id: i64,
    executed_by_membership_id: OrganizationMembershipId,
    executed_at: i64,
}

impl OrganizationApprovalFundsExecutionRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn request_id(&self) -> OrganizationApprovalRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn decision_id(&self) -> i64 {
        self.decision_id
    }
    #[must_use]
    pub const fn event_id(&self) -> WalletEventId {
        self.event_id
    }
    #[must_use]
    pub const fn quota_delta(&self) -> QuotaDelta {
        self.quota_delta
    }
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    #[must_use]
    pub const fn expected_wallet_version(&self) -> i64 {
        self.expected_wallet_version
    }
    #[must_use]
    pub const fn applied_wallet_version(&self) -> i64 {
        self.applied_wallet_version
    }
    #[must_use]
    pub const fn balance_before(&self) -> Quota {
        self.balance_before
    }
    #[must_use]
    pub const fn balance_after(&self) -> Quota {
        self.balance_after
    }
    #[must_use]
    pub const fn ledger_entry_id(&self) -> i64 {
        self.ledger_entry_id
    }
    #[must_use]
    pub const fn executed_by_membership_id(&self) -> OrganizationMembershipId {
        self.executed_by_membership_id
    }
    #[must_use]
    pub const fn executed_at(&self) -> i64 {
        self.executed_at
    }
}

/// 企业钱包调账审批执行结果；`Existing` 表示命令未被再次应用。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationApprovalFundsExecutionOutcome {
    Applied(OrganizationApprovalFundsExecutionRecord),
    Existing(OrganizationApprovalFundsExecutionRecord),
}

/// 在目标钱包事务内复验审批事实、权限和钱包版本，并追加独立执行事实。
pub(crate) async fn execute_funds_adjustment_transaction(
    transaction: &DatabaseTransaction,
    write: OrganizationApprovalFundsExecutionWrite,
) -> Result<OrganizationApprovalFundsExecutionOutcome, OrganizationApprovalExecutionRepositoryError>
{
    let organization = mutation::lock_organization(transaction, write.organization_id)
        .await
        .map_err(map_organization_error)?;
    if organization.status != OrganizationStatus::Active.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::Forbidden);
    }
    let request = lock_request(transaction, write.organization_id, write.request_id).await?;
    if request.resource_type != OrganizationApprovalResourceType::Funds.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch);
    }
    if request.resource_summary.as_str() != write.resource_summary() {
        return Err(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    let decision = lock_decision(transaction, write.organization_id, write.request_id).await?;
    if decision.decision_kind != OrganizationApprovalDecisionKind::Approved.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::NotApproved);
    }
    let actor = mutation::lock_membership(
        transaction,
        write.organization_id,
        write.executed_by_membership_id,
    )
    .await
    .map_err(map_organization_error)?;
    require_current_member_permission(
        transaction,
        &actor,
        write.executed_by_user_id,
        OrganizationPermission::WalletManage,
    )
    .await?;

    if let Some(existing) =
        find_execution(transaction, write.organization_id, write.request_id).await?
    {
        let record = execution_record(existing)?;
        return exact_replay(&record, &write, decision.id)
            .then_some(OrganizationApprovalFundsExecutionOutcome::Existing(record))
            .ok_or(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    if request.expires_at <= TimeDateTimeWithTimeZone::now_utc() {
        return Err(OrganizationApprovalExecutionRepositoryError::Expired);
    }

    let adjustment = OrganizationWalletAdjustmentWrite::new(
        write.event_id,
        write.organization_id,
        write.executed_by_user_id,
        write.quota_delta,
        write.reason.clone(),
    )
    .map_err(map_wallet_error)?;
    let entry = match apply_adjustment_with_expected_version(
        transaction,
        &adjustment,
        Some(write.expected_wallet_version),
    )
    .await
    .map_err(map_wallet_error)?
    {
        OrganizationWalletAdjustmentOutcome::Applied(entry) => entry,
        OrganizationWalletAdjustmentOutcome::Existing(_) => {
            return Err(OrganizationApprovalExecutionRepositoryError::Conflict);
        }
        OrganizationWalletAdjustmentOutcome::NotFound => {
            return Err(OrganizationApprovalExecutionRepositoryError::NotFound);
        }
    };
    let applied_wallet_version = write
        .expected_wallet_version
        .checked_add(1)
        .ok_or_else(invariant)?;
    let executed_at = TimeDateTimeWithTimeZone::now_utc();
    let model = organization_approval_funds_executions::ActiveModel {
        organization_id: Set(write.organization_id.get()),
        request_id: Set(write.request_id.get()),
        decision_id: Set(decision.id),
        event_key: Set(write.event_id.persistence_key()),
        quota_delta: Set(write.quota_delta.units()),
        reason: Set(crate::entity::SensitiveString::from(write.reason)),
        expected_wallet_version: Set(write.expected_wallet_version),
        applied_wallet_version: Set(applied_wallet_version),
        balance_before: Set(entry.balance_before().units()),
        balance_after: Set(entry.balance_after().units()),
        ledger_entry_id: Set(entry.id()),
        executed_by_membership_id: Set(write.executed_by_membership_id.get()),
        executed_at: Set(executed_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|_| OrganizationApprovalExecutionRepositoryError::Query)?;
    Ok(OrganizationApprovalFundsExecutionOutcome::Applied(
        execution_record(model)?,
    ))
}

async fn find_execution(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<
    Option<organization_approval_funds_executions::Model>,
    OrganizationApprovalExecutionRepositoryError,
> {
    let mut query = organization_approval_funds_executions::Entity::find()
        .filter(
            organization_approval_funds_executions::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(organization_approval_funds_executions::Column::RequestId.eq(request_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(|_| OrganizationApprovalExecutionRepositoryError::Query)
}

fn exact_replay(
    record: &OrganizationApprovalFundsExecutionRecord,
    write: &OrganizationApprovalFundsExecutionWrite,
    decision_id: i64,
) -> bool {
    record.organization_id == write.organization_id
        && record.request_id == write.request_id
        && record.decision_id == decision_id
        && record.event_id == write.event_id
        && record.quota_delta == write.quota_delta
        && record.reason == write.reason
        && record.expected_wallet_version == write.expected_wallet_version
        && write
            .expected_wallet_version
            .checked_add(1)
            .is_some_and(|version| record.applied_wallet_version == version)
        && record.executed_by_membership_id == write.executed_by_membership_id
}

fn execution_record(
    model: organization_approval_funds_executions::Model,
) -> Result<OrganizationApprovalFundsExecutionRecord, OrganizationApprovalExecutionRepositoryError>
{
    let event_id =
        WalletEventId::from_persistence_key(&model.event_key).map_err(|_| invariant())?;
    let quota_delta = QuotaDelta::new(model.quota_delta).map_err(|_| invariant())?;
    let balance_before = Quota::new(model.balance_before).map_err(|_| invariant())?;
    let balance_after = Quota::new(model.balance_after).map_err(|_| invariant())?;
    if model.id <= 0
        || model.decision_id <= 0
        || quota_delta.is_zero()
        || model.reason.as_str().is_empty()
        || !valid_reason(model.reason.as_str())
        || model.expected_wallet_version <= 0
        || model.expected_wallet_version == i64::MAX
        || model.expected_wallet_version.checked_add(1) != Some(model.applied_wallet_version)
        || balance_before.checked_apply(quota_delta) != Ok(balance_after)
        || model.ledger_entry_id <= 0
        || model.executed_at.unix_timestamp() < 0
    {
        return Err(invariant());
    }
    Ok(OrganizationApprovalFundsExecutionRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        request_id: OrganizationApprovalRequestId::new(model.request_id)
            .map_err(|_| invariant())?,
        decision_id: model.decision_id,
        event_id,
        quota_delta,
        reason: model.reason.as_str().to_owned(),
        expected_wallet_version: model.expected_wallet_version,
        applied_wallet_version: model.applied_wallet_version,
        balance_before,
        balance_after,
        ledger_entry_id: model.ledger_entry_id,
        executed_by_membership_id: OrganizationMembershipId::new(model.executed_by_membership_id)
            .map_err(|_| invariant())?,
        executed_at: model.executed_at.unix_timestamp(),
    })
}

/// 为申请创建和执行复验生成同一份企业钱包调账目标摘要。
#[must_use]
pub fn organization_approval_funds_resource_summary(
    organization_id: OrganizationId,
    event_id: WalletEventId,
    quota_delta: QuotaDelta,
    reason: &str,
    expected_wallet_version: i64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows:organization-approval:funds-wallet-adjustment:v1\0");
    hasher.update(organization_id.get().to_be_bytes());
    hasher.update(event_id.bytes());
    hasher.update(quota_delta.units().to_be_bytes());
    hasher.update((reason.len() as u64).to_be_bytes());
    hasher.update(reason.as_bytes());
    hasher.update(expected_wallet_version.to_be_bytes());
    let digest = hasher.finalize();
    let mut summary = String::with_capacity(FUNDS_SUMMARY_PREFIX.len() + digest.len() * 2);
    summary.push_str(FUNDS_SUMMARY_PREFIX);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut summary, "{byte:02x}").expect("写入 String 不会失败");
    }
    summary
}

fn valid_reason(reason: &str) -> bool {
    !reason.is_empty()
        && reason.len() <= MAX_ORGANIZATION_WALLET_ADJUSTMENT_REASON_BYTES
        && !reason.chars().any(char::is_control)
}

fn map_wallet_error(
    error: crate::OrganizationWalletError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        crate::OrganizationWalletError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        crate::OrganizationWalletError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        crate::OrganizationWalletError::InsufficientQuota => {
            OrganizationApprovalExecutionRepositoryError::InsufficientQuota
        }
        crate::OrganizationWalletError::Overflow => {
            OrganizationApprovalExecutionRepositoryError::QuotaOverflow
        }
        crate::OrganizationWalletError::OutcomeUnknown => {
            OrganizationApprovalExecutionRepositoryError::OutcomeUnknown
        }
        crate::OrganizationWalletError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        crate::OrganizationWalletError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        crate::OrganizationWalletError::Invariant => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
    }
}

fn map_organization_error(
    error: crate::OrganizationRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        crate::OrganizationRepositoryError::NotFound
        | crate::OrganizationRepositoryError::UserUnavailable => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        crate::OrganizationRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        crate::OrganizationRepositoryError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        crate::OrganizationRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        _ => OrganizationApprovalExecutionRepositoryError::Invariant,
    }
}

fn invariant() -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Invariant
}

impl fmt::Debug for OrganizationApprovalFundsExecutionWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationApprovalFundsExecutionWrite")
            .field("organization_id", &self.organization_id)
            .field("request_id", &self.request_id)
            .field("quota_delta", &self.quota_delta)
            .field("expected_wallet_version", &self.expected_wallet_version)
            .finish_non_exhaustive()
    }
}
