use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    OrganizationApprovalBudgetExecutionOutcome, OrganizationApprovalBudgetExecutionRecord,
    OrganizationApprovalBudgetExecutionWrite, OrganizationApprovalContractPriceExecutionOutcome,
    OrganizationApprovalContractPriceExecutionRecord,
    OrganizationApprovalContractPriceExecutionWrite, OrganizationApprovalExecutionRepository,
    OrganizationApprovalExecutionRepositoryError, OrganizationApprovalFundsExecutionOutcome,
    OrganizationApprovalFundsExecutionRecord, OrganizationApprovalFundsExecutionWrite,
    OrganizationApprovalMemberExecutionOutcome, OrganizationApprovalMemberExecutionRecord,
    OrganizationApprovalMemberExecutionWrite, OrganizationApprovalServiceAccountExecutionAction,
    OrganizationApprovalServiceAccountExecutionOutcome,
    OrganizationApprovalServiceAccountExecutionRecord,
    OrganizationApprovalServiceAccountExecutionWrite, OrganizationBudgetScope,
    organization_approval_budget_resource_summary, organization_approval_funds_resource_summary,
    organization_approval_member_resource_summary,
    organization_approval_service_account_budget_binding_resource_summary,
    organization_approval_service_account_status_resource_summary,
};
use af_domain::{
    OrganizationApprovalRequestId, OrganizationBudgetLimit, OrganizationBudgetPeriod,
    OrganizationBudgetPolicyId, OrganizationDepartmentId, OrganizationMembershipId,
    OrganizationMembershipStatus, OrganizationPermission, OrganizationRole,
    OrganizationServiceAccountId, OrganizationServiceAccountStatus, OrganizationTeamId, Protocol,
    QuotaDelta, WalletEventId,
};
use rust_decimal::Decimal;
use thiserror::Error;

use crate::OrganizationContext;

/// 成员审批执行命令；所有可变成员字段都使用完整目标快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalMemberChangeCommand {
    membership_id: OrganizationMembershipId,
    role: OrganizationRole,
    status: OrganizationMembershipStatus,
    team_id: Option<OrganizationTeamId>,
    expected_version: i64,
}

impl OrganizationApprovalMemberChangeCommand {
    /// 构造带 CAS 版本的成员审批目标。
    pub fn new(
        membership_id: OrganizationMembershipId,
        role: OrganizationRole,
        status: OrganizationMembershipStatus,
        team_id: Option<OrganizationTeamId>,
        expected_version: i64,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if expected_version <= 0 {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            membership_id,
            role,
            status,
            team_id,
            expected_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        organization_approval_member_resource_summary(
            context.organization_id(),
            self.membership_id,
            self.role,
            self.status,
            self.team_id,
            self.expected_version,
        )
    }
}

/// 应用层成员审批执行结果，明确标记是否为幂等恢复。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalMemberExecutionResult {
    record: OrganizationApprovalMemberExecutionRecord,
    replayed: bool,
}

/// 应用层预算审批执行结果，明确标记是否为幂等恢复。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalBudgetExecutionResult {
    record: OrganizationApprovalBudgetExecutionRecord,
    replayed: bool,
}

/// 应用层服务账号审批执行结果，明确标记是否为幂等恢复。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalServiceAccountExecutionResult {
    record: OrganizationApprovalServiceAccountExecutionRecord,
    replayed: bool,
}

/// 应用层合同价审批执行结果，明确标记是否为幂等恢复。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationApprovalContractPriceExecutionResult {
    record: OrganizationApprovalContractPriceExecutionRecord,
    replayed: bool,
}

/// 应用层资金审批执行结果，明确标记是否为幂等恢复。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalFundsExecutionResult {
    record: OrganizationApprovalFundsExecutionRecord,
    replayed: bool,
}

impl OrganizationApprovalFundsExecutionResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationApprovalFundsExecutionRecord {
        &self.record
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl OrganizationApprovalContractPriceExecutionResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationApprovalContractPriceExecutionRecord {
        &self.record
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl OrganizationApprovalServiceAccountExecutionResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationApprovalServiceAccountExecutionRecord {
        &self.record
    }

    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl OrganizationApprovalBudgetExecutionResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationApprovalBudgetExecutionRecord {
        &self.record
    }
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

impl OrganizationApprovalMemberExecutionResult {
    #[must_use]
    pub const fn record(&self) -> &OrganizationApprovalMemberExecutionRecord {
        &self.record
    }
    #[must_use]
    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

/// 审批执行应用层错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationApprovalExecutionError {
    #[error("审批执行请求无效")]
    InvalidRequest,
    #[error("审批执行权限不足")]
    Forbidden,
    #[error("审批事实或资源不存在")]
    NotFound,
    #[error("审批申请资源类型不匹配")]
    ResourceTypeMismatch,
    #[error("审批申请未获批准")]
    NotApproved,
    #[error("审批申请已过期")]
    Expired,
    #[error("审批执行版本或命令冲突")]
    Conflict,
    #[error("成员目标状态转换无效")]
    InvalidTransition,
    #[error("企业必须保留至少一个活动所有者")]
    LastOwner,
    #[error("企业商业授权不允许当前操作")]
    EntitlementRestricted,
    #[error("企业席位容量已用尽")]
    SeatLimitReached,
    #[error("企业钱包余额不足")]
    InsufficientQuota,
    #[error("企业钱包额度溢出")]
    QuotaOverflow,
    #[error("企业钱包审批执行结果未知")]
    OutcomeUnknown,
    #[error("审批执行内部错误")]
    Internal,
}

pub type OrganizationApprovalMemberExecutionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalMemberExecutionResult,
                    OrganizationApprovalExecutionError,
                >,
            > + Send
            + 'a,
    >,
>;

pub type OrganizationApprovalBudgetExecutionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalBudgetExecutionResult,
                    OrganizationApprovalExecutionError,
                >,
            > + Send
            + 'a,
    >,
>;

pub type OrganizationApprovalServiceAccountExecutionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalServiceAccountExecutionResult,
                    OrganizationApprovalExecutionError,
                >,
            > + Send
            + 'a,
    >,
>;

pub type OrganizationApprovalContractPriceExecutionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalContractPriceExecutionResult,
                    OrganizationApprovalExecutionError,
                >,
            > + Send
            + 'a,
    >,
>;

pub type OrganizationApprovalFundsExecutionFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationApprovalFundsExecutionResult,
                    OrganizationApprovalExecutionError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 预算审批执行命令；策略字段与版本均来自申请时的完整快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalBudgetPolicyCommand {
    scope: OrganizationBudgetScope,
    period: OrganizationBudgetPeriod,
    limit: OrganizationBudgetLimit,
    effective_from: i64,
    expected_policy_version: i64,
}

impl OrganizationApprovalBudgetPolicyCommand {
    /// 构造带策略版本 CAS 的预算审批目标。
    pub fn new(
        scope: OrganizationBudgetScope,
        period: OrganizationBudgetPeriod,
        limit: OrganizationBudgetLimit,
        effective_from: i64,
        expected_policy_version: i64,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if effective_from < 0 || expected_policy_version < 0 {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            scope,
            period,
            limit,
            effective_from,
            expected_policy_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        organization_approval_budget_resource_summary(
            context.organization_id(),
            self.scope,
            self.period,
            self.limit,
            self.effective_from,
            self.expected_policy_version,
        )
    }
}

/// 服务账号生命周期审批命令；部门和主体版本来自申请时快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalServiceAccountStatusCommand {
    service_account_id: OrganizationServiceAccountId,
    department_id: Option<OrganizationDepartmentId>,
    target_status: OrganizationServiceAccountStatus,
    expected_account_version: i64,
}

impl OrganizationApprovalServiceAccountStatusCommand {
    pub fn new(
        service_account_id: OrganizationServiceAccountId,
        department_id: Option<OrganizationDepartmentId>,
        target_status: OrganizationServiceAccountStatus,
        expected_account_version: i64,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if expected_account_version <= 0 || expected_account_version == i64::MAX {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            service_account_id,
            department_id,
            target_status,
            expected_account_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        organization_approval_service_account_status_resource_summary(
            context.organization_id(),
            self.service_account_id,
            self.department_id,
            self.target_status,
            self.expected_account_version,
        )
    }
}

/// 服务账号预算绑定审批命令；主体、策略和绑定版本都必须复验。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalServiceAccountBudgetBindingCommand {
    service_account_id: OrganizationServiceAccountId,
    department_id: Option<OrganizationDepartmentId>,
    policy_id: OrganizationBudgetPolicyId,
    expected_policy_version: i64,
    expected_binding_version: Option<i64>,
    expected_account_version: i64,
}

impl OrganizationApprovalServiceAccountBudgetBindingCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service_account_id: OrganizationServiceAccountId,
        department_id: Option<OrganizationDepartmentId>,
        policy_id: OrganizationBudgetPolicyId,
        expected_policy_version: i64,
        expected_binding_version: Option<i64>,
        expected_account_version: i64,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if expected_policy_version <= 0
            || expected_account_version <= 0
            || expected_account_version == i64::MAX
            || expected_binding_version.is_some_and(|version| version <= 0 || version == i64::MAX)
        {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            service_account_id,
            department_id,
            policy_id,
            expected_policy_version,
            expected_binding_version,
            expected_account_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        organization_approval_service_account_budget_binding_resource_summary(
            context.organization_id(),
            self.service_account_id,
            self.department_id,
            self.policy_id,
            self.expected_policy_version,
            self.expected_binding_version,
            self.expected_account_version,
        )
    }
}

/// 合同价版本审批命令；模型、协议、价格和生效区间均是不可变目标快照。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationApprovalContractPriceCommand {
    model: String,
    protocol: Protocol,
    prices: [Decimal; 5],
    effective_from: i64,
    effective_until: Option<i64>,
    expected_version: Option<i64>,
}

impl OrganizationApprovalContractPriceCommand {
    /// 构造带合同价范围版本 CAS 的审批目标。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        model: String,
        protocol: Protocol,
        prices: [Decimal; 5],
        effective_from: i64,
        effective_until: Option<i64>,
        expected_version: Option<i64>,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if effective_from < 0
            || effective_until.is_some_and(|until| until <= effective_from)
            || expected_version.is_some_and(|version| !(0..i64::MAX).contains(&version))
            || af_db::DatabaseTimestamp::from_unix_timestamp(effective_from).is_err()
            || effective_until
                .is_some_and(|until| af_db::DatabaseTimestamp::from_unix_timestamp(until).is_err())
        {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            model,
            protocol,
            prices,
            effective_from,
            effective_until,
            expected_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        af_db::organization_approval_contract_price_resource_summary(
            context.organization_id(),
            &self.model,
            self.protocol,
            self.prices,
            af_db::DatabaseTimestamp::from_unix_timestamp(self.effective_from)
                .expect("构造命令时已校验生效时间"),
            self.effective_until.map(|value| {
                af_db::DatabaseTimestamp::from_unix_timestamp(value)
                    .expect("构造命令时已校验失效时间")
            }),
            self.expected_version,
        )
    }
}

/// 企业钱包管理员调账审批命令；仅描述钱包账本调账，不代表支付或退款。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationApprovalFundsAdjustmentCommand {
    event_id: WalletEventId,
    quota_delta: QuotaDelta,
    reason: String,
    expected_wallet_version: i64,
}

impl OrganizationApprovalFundsAdjustmentCommand {
    /// 构造带钱包版本 CAS 的资金审批目标。
    pub fn new(
        event_id: WalletEventId,
        quota_delta: QuotaDelta,
        reason: String,
        expected_wallet_version: i64,
    ) -> Result<Self, OrganizationApprovalExecutionError> {
        if event_id.is_system_opening()
            || quota_delta.is_zero()
            || expected_wallet_version <= 0
            || expected_wallet_version == i64::MAX
            || reason.is_empty()
            || reason.len() > af_db::MAX_ORGANIZATION_WALLET_ADJUSTMENT_REASON_BYTES
            || reason.chars().any(char::is_control)
        {
            return Err(OrganizationApprovalExecutionError::InvalidRequest);
        }
        Ok(Self {
            event_id,
            quota_delta,
            reason,
            expected_wallet_version,
        })
    }

    /// 生成创建审批申请时必须使用的命令摘要。
    #[must_use]
    pub fn resource_summary(&self, context: &OrganizationContext) -> String {
        organization_approval_funds_resource_summary(
            context.organization_id(),
            self.event_id,
            self.quota_delta,
            &self.reason,
            self.expected_wallet_version,
        )
    }
}

/// 企业审批资源执行端口；按资源类型提供受控适配器，不接受宽泛资源载荷。
pub trait OrganizationApprovalExecutionService: Send + Sync {
    fn execute_member_change<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalMemberChangeCommand,
    ) -> OrganizationApprovalMemberExecutionFuture<'a>;

    fn execute_budget_policy<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalBudgetPolicyCommand,
    ) -> OrganizationApprovalBudgetExecutionFuture<'a>;

    fn execute_service_account_status<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalServiceAccountStatusCommand,
    ) -> OrganizationApprovalServiceAccountExecutionFuture<'a>;

    fn execute_service_account_budget_binding<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalServiceAccountBudgetBindingCommand,
    ) -> OrganizationApprovalServiceAccountExecutionFuture<'a>;

    fn execute_contract_price<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalContractPriceCommand,
    ) -> OrganizationApprovalContractPriceExecutionFuture<'a>;

    fn execute_funds_adjustment<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalFundsAdjustmentCommand,
    ) -> OrganizationApprovalFundsExecutionFuture<'a>;
}

/// 使用数据库原子复验仓储实现成员审批执行。
pub struct DatabaseOrganizationApprovalExecutionService {
    repository: Arc<OrganizationApprovalExecutionRepository>,
}

impl DatabaseOrganizationApprovalExecutionService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationApprovalExecutionRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationApprovalExecutionService for DatabaseOrganizationApprovalExecutionService {
    fn execute_member_change<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalMemberChangeCommand,
    ) -> OrganizationApprovalMemberExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::MembersManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let write = OrganizationApprovalMemberExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.membership_id,
                command.role,
                command.status,
                command.team_id,
                command.expected_version,
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            let (record, replayed) = match self
                .repository
                .execute_member_change(write)
                .await
                .map_err(map_repository_error)?
            {
                OrganizationApprovalMemberExecutionOutcome::Applied(record) => (record, false),
                OrganizationApprovalMemberExecutionOutcome::Existing(record) => (record, true),
            };
            Ok(OrganizationApprovalMemberExecutionResult { record, replayed })
        })
    }

    fn execute_budget_policy<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalBudgetPolicyCommand,
    ) -> OrganizationApprovalBudgetExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::BudgetsManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let effective_from =
                af_db::DatabaseTimestamp::from_unix_timestamp(command.effective_from)
                    .map_err(|_| OrganizationApprovalExecutionError::InvalidRequest)?;
            let write = OrganizationApprovalBudgetExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.scope,
                command.period,
                command.limit,
                effective_from,
                command.expected_policy_version,
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            let (record, replayed) = match self
                .repository
                .execute_budget_policy(write)
                .await
                .map_err(map_repository_error)?
            {
                OrganizationApprovalBudgetExecutionOutcome::Applied(record) => (record, false),
                OrganizationApprovalBudgetExecutionOutcome::Existing(record) => (record, true),
            };
            Ok(OrganizationApprovalBudgetExecutionResult { record, replayed })
        })
    }

    fn execute_service_account_status<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalServiceAccountStatusCommand,
    ) -> OrganizationApprovalServiceAccountExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::ServiceAccountsManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let write = OrganizationApprovalServiceAccountExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.service_account_id,
                command.department_id,
                OrganizationApprovalServiceAccountExecutionAction::SetStatus {
                    target_status: command.target_status,
                    expected_account_version: command.expected_account_version,
                },
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            execute_service_account(&self.repository, write).await
        })
    }

    fn execute_service_account_budget_binding<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalServiceAccountBudgetBindingCommand,
    ) -> OrganizationApprovalServiceAccountExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::ServiceAccountsManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let write = OrganizationApprovalServiceAccountExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.service_account_id,
                command.department_id,
                OrganizationApprovalServiceAccountExecutionAction::SetBudgetBinding {
                    policy_id: command.policy_id,
                    expected_policy_version: command.expected_policy_version,
                    expected_binding_version: command.expected_binding_version,
                    expected_account_version: command.expected_account_version,
                },
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            execute_service_account(&self.repository, write).await
        })
    }

    fn execute_contract_price<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalContractPriceCommand,
    ) -> OrganizationApprovalContractPriceExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::ContractPricesManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let effective_from =
                af_db::DatabaseTimestamp::from_unix_timestamp(command.effective_from)
                    .map_err(|_| OrganizationApprovalExecutionError::InvalidRequest)?;
            let effective_until = command
                .effective_until
                .map(af_db::DatabaseTimestamp::from_unix_timestamp)
                .transpose()
                .map_err(|_| OrganizationApprovalExecutionError::InvalidRequest)?;
            let write = OrganizationApprovalContractPriceExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.model,
                command.protocol,
                command.prices,
                effective_from,
                effective_until,
                command.expected_version,
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            let (record, replayed) = match self
                .repository
                .execute_contract_price(write)
                .await
                .map_err(map_repository_error)?
            {
                OrganizationApprovalContractPriceExecutionOutcome::Applied(record) => {
                    (record, false)
                }
                OrganizationApprovalContractPriceExecutionOutcome::Existing(record) => {
                    (record, true)
                }
            };
            Ok(OrganizationApprovalContractPriceExecutionResult { record, replayed })
        })
    }

    fn execute_funds_adjustment<'a>(
        &'a self,
        context: OrganizationContext,
        request_id: OrganizationApprovalRequestId,
        command: OrganizationApprovalFundsAdjustmentCommand,
    ) -> OrganizationApprovalFundsExecutionFuture<'a> {
        Box::pin(async move {
            if !context.allows(OrganizationPermission::WalletManage) {
                return Err(OrganizationApprovalExecutionError::Forbidden);
            }
            let write = OrganizationApprovalFundsExecutionWrite::new(
                context.organization_id(),
                request_id,
                command.event_id,
                command.quota_delta,
                command.reason,
                command.expected_wallet_version,
                context.membership_id(),
                context.user_id(),
            )
            .map_err(map_repository_error)?;
            let (record, replayed) = match self
                .repository
                .execute_funds_adjustment(write)
                .await
                .map_err(map_repository_error)?
            {
                OrganizationApprovalFundsExecutionOutcome::Applied(record) => (record, false),
                OrganizationApprovalFundsExecutionOutcome::Existing(record) => (record, true),
            };
            Ok(OrganizationApprovalFundsExecutionResult { record, replayed })
        })
    }
}

async fn execute_service_account(
    repository: &OrganizationApprovalExecutionRepository,
    write: OrganizationApprovalServiceAccountExecutionWrite,
) -> Result<OrganizationApprovalServiceAccountExecutionResult, OrganizationApprovalExecutionError> {
    let (record, replayed) = match repository
        .execute_service_account(write)
        .await
        .map_err(map_repository_error)?
    {
        OrganizationApprovalServiceAccountExecutionOutcome::Applied(record) => (record, false),
        OrganizationApprovalServiceAccountExecutionOutcome::Existing(record) => (record, true),
    };
    Ok(OrganizationApprovalServiceAccountExecutionResult { record, replayed })
}

impl fmt::Debug for DatabaseOrganizationApprovalExecutionService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationApprovalExecutionService(<repository>)")
    }
}

fn map_repository_error(
    error: OrganizationApprovalExecutionRepositoryError,
) -> OrganizationApprovalExecutionError {
    match error {
        OrganizationApprovalExecutionRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionError::InvalidRequest
        }
        OrganizationApprovalExecutionRepositoryError::NotFound => {
            OrganizationApprovalExecutionError::NotFound
        }
        OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch => {
            OrganizationApprovalExecutionError::ResourceTypeMismatch
        }
        OrganizationApprovalExecutionRepositoryError::NotApproved => {
            OrganizationApprovalExecutionError::NotApproved
        }
        OrganizationApprovalExecutionRepositoryError::Expired => {
            OrganizationApprovalExecutionError::Expired
        }
        OrganizationApprovalExecutionRepositoryError::Forbidden => {
            OrganizationApprovalExecutionError::Forbidden
        }
        OrganizationApprovalExecutionRepositoryError::Conflict => {
            OrganizationApprovalExecutionError::Conflict
        }
        OrganizationApprovalExecutionRepositoryError::InvalidTransition => {
            OrganizationApprovalExecutionError::InvalidTransition
        }
        OrganizationApprovalExecutionRepositoryError::LastOwner => {
            OrganizationApprovalExecutionError::LastOwner
        }
        OrganizationApprovalExecutionRepositoryError::EntitlementRestricted => {
            OrganizationApprovalExecutionError::EntitlementRestricted
        }
        OrganizationApprovalExecutionRepositoryError::SeatLimitReached => {
            OrganizationApprovalExecutionError::SeatLimitReached
        }
        OrganizationApprovalExecutionRepositoryError::InsufficientQuota => {
            OrganizationApprovalExecutionError::InsufficientQuota
        }
        OrganizationApprovalExecutionRepositoryError::QuotaOverflow => {
            OrganizationApprovalExecutionError::QuotaOverflow
        }
        OrganizationApprovalExecutionRepositoryError::OutcomeUnknown => {
            OrganizationApprovalExecutionError::OutcomeUnknown
        }
        OrganizationApprovalExecutionRepositoryError::Query
        | OrganizationApprovalExecutionRepositoryError::Timeout
        | OrganizationApprovalExecutionRepositoryError::Invariant => {
            OrganizationApprovalExecutionError::Internal
        }
    }
}
