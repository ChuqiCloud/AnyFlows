use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use af_domain::{
    BillingContractPriceSnapshot, BillingReservationId, GatewayPrincipal,
    OrganizationServiceAccountRuntimeIdentity, Quota,
};
use af_protocol::Usage;
use thiserror::Error;

use crate::{
    BillingMode, BillingSession, BillingSessionError, BillingSessionState, PricingContext,
    PricingError, PricingResolver, RefundSignalPort,
};

mod ports;
mod records;

pub use ports::{
    BillingPrechargeError, BillingPrechargeFuture, BillingPrechargePort, BillingSettlementError,
    BillingSettlementFuture, BillingSettlementPort, UsageRecordOutcome, UsageRecordPort,
};
pub use records::{
    BillingCompletion, BillingUsageContext, BillingUsageDimensions, BillingUsageObservation,
    BillingUsageObservationError, BillingUsageRecord, BillingUsageTiming, MAX_USAGE_MODEL_BYTES,
    MAX_USAGE_REQUEST_ID_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlanMode {
    Free,
    Metered {
        billing_mode: BillingMode,
        precharge: Quota,
    },
}

/// 使用同一不可变定价解析器准备的一次请求计费计划。
///
/// 计划只计算预扣额度，不执行 IO。调用方必须先完成唯一一次持久化预扣，才能通过
/// [`Self::start_reserved`] 把同一计划交给覆盖完整 Relay 重试循环的生命周期。
#[must_use = "计费计划必须启动免费生命周期或完成预扣后启动按量生命周期"]
pub struct BillingRequestPlan {
    resolver: Arc<dyn PricingResolver>,
    mode: PlanMode,
    contract_price: Option<BillingContractPriceSnapshot>,
}

impl BillingRequestPlan {
    /// 根据上界用量准备免费或按 token 计费计划。
    ///
    /// PerToken 即使舍入后为零也预留最小 1 quota；实际结算仍使用真实零额度，避免
    /// 把零价配置或微额舍入误判为显式免费，并满足数据库预扣必须为正数的约束。
    pub fn prepare(
        resolver: Arc<dyn PricingResolver>,
        estimated_usage: &Usage,
    ) -> Result<Self, BillingLifecycleError> {
        let estimated = resolver.resolve(&PricingContext::new(estimated_usage))?;
        let mode = match estimated.billing_mode() {
            BillingMode::Free => PlanMode::Free,
            BillingMode::PerToken | BillingMode::PerCall => PlanMode::Metered {
                billing_mode: estimated.billing_mode(),
                precharge: if estimated.quota().is_zero() {
                    Quota::new(1).expect("1 必须始终是合法的最小正额度")
                } else {
                    estimated.quota()
                },
            },
        };
        Ok(Self {
            resolver,
            mode,
            contract_price: None,
        })
    }

    /// 绑定请求快照命中的企业合同价，随后只能随本计划进入预扣。
    #[must_use = "必须保留绑定合同价后的计费计划"]
    pub const fn with_contract_price(
        mut self,
        contract_price: Option<BillingContractPriceSnapshot>,
    ) -> Self {
        self.contract_price = contract_price;
        self
    }

    /// 返回本计划固定的合同价快照。
    #[must_use]
    pub const fn contract_price(&self) -> Option<BillingContractPriceSnapshot> {
        self.contract_price
    }

    /// 返回计划固定的计费模式。
    #[must_use]
    pub const fn billing_mode(&self) -> BillingMode {
        match self.mode {
            PlanMode::Free => BillingMode::Free,
            PlanMode::Metered { billing_mode, .. } => billing_mode,
        }
    }

    /// 返回必须持久化预扣的正额度；显式免费返回 `None`。
    #[must_use]
    pub const fn precharge_quota(&self) -> Option<Quota> {
        match self.mode {
            PlanMode::Free => None,
            PlanMode::Metered { precharge, .. } => Some(precharge),
        }
    }

    /// 启动不创建 `BillingSession` 的显式免费生命周期。
    ///
    /// `event_id` 只作为用量记录幂等键，不得为免费请求创建持久化额度预留。
    pub fn start_free(
        self,
        principal: GatewayPrincipal,
        event_id: BillingReservationId,
        usage_port: Arc<dyn UsageRecordPort>,
    ) -> Result<BillingRequestLifecycle, BillingLifecycleError> {
        if self.mode != PlanMode::Free {
            return Err(BillingLifecycleError::InvalidActivation);
        }
        Ok(BillingRequestLifecycle {
            principal,
            service_account_identity: None,
            event_id,
            resolver: self.resolver,
            state: LifecycleState::Free,
            settlement_port: None,
            usage_port,
        })
    }

    /// 从已经明确确认成功的唯一持久化预留启动按量生命周期。
    pub fn start_reserved(
        self,
        principal: GatewayPrincipal,
        reservation_id: BillingReservationId,
        refund_port: Arc<dyn RefundSignalPort>,
        settlement_port: Arc<dyn BillingSettlementPort>,
        usage_port: Arc<dyn UsageRecordPort>,
    ) -> Result<BillingRequestLifecycle, BillingLifecycleError> {
        let PlanMode::Metered { billing_mode, .. } = self.mode else {
            return Err(BillingLifecycleError::InvalidActivation);
        };
        Ok(BillingRequestLifecycle {
            principal,
            service_account_identity: None,
            event_id: reservation_id,
            resolver: self.resolver,
            state: LifecycleState::Metered {
                billing_mode,
                session: BillingSession::from_reserved(reservation_id, refund_port),
                pending_completion: None,
            },
            settlement_port: Some(settlement_port),
            usage_port,
        })
    }
}

impl fmt::Debug for BillingRequestPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingRequestPlan")
            .field("billing_mode", &self.billing_mode())
            .finish_non_exhaustive()
    }
}

enum LifecycleState {
    Free,
    Metered {
        billing_mode: BillingMode,
        session: BillingSession,
        pending_completion: Option<BillingCompletion>,
    },
    UsagePending(BillingCompletion),
    Completed(BillingCompletion),
}

/// 对外可观察且不包含额度或主体的请求计费生命周期状态。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingLifecycleState {
    /// 显式免费，等待最终 usage。
    Free,
    /// 持久化预扣已确认，尚未发起结算。
    Reserved,
    /// 结算请求和完整用量事实已经固化，只能使用相同参数重放。
    SettlementPending,
    /// 额度结算已确认，等待同一 usage 记录被端口接收。
    UsagePending,
    /// 结算和用量记录均已完成。
    Completed,
    /// 退款信号已经入队，不再允许结算。
    RefundQueued,
}

/// 覆盖一次完整 Relay 重试循环的计费生命周期。
///
/// 流式与非流式只负责以不同方式取得最终 [`Usage`]，之后都必须调用唯一的
/// [`Self::complete`]。方法会先固化结算请求，再执行持久化 IO，最后只记录一次 usage。
#[must_use = "请求计费生命周期必须保留到完成结算或触发 RAII 退款责任"]
pub struct BillingRequestLifecycle {
    principal: GatewayPrincipal,
    service_account_identity: Option<OrganizationServiceAccountRuntimeIdentity>,
    event_id: BillingReservationId,
    resolver: Arc<dyn PricingResolver>,
    state: LifecycleState,
    settlement_port: Option<Arc<dyn BillingSettlementPort>>,
    usage_port: Arc<dyn UsageRecordPort>,
}

impl BillingRequestLifecycle {
    /// 返回不包含主体、预留标识、额度或 token 数的当前状态。
    #[must_use]
    pub const fn state(&self) -> BillingLifecycleState {
        match &self.state {
            LifecycleState::Free => BillingLifecycleState::Free,
            LifecycleState::Metered { session, .. } => match session.state() {
                BillingSessionState::Reserved => BillingLifecycleState::Reserved,
                BillingSessionState::SettlementPending => BillingLifecycleState::SettlementPending,
                BillingSessionState::Settled => BillingLifecycleState::UsagePending,
                BillingSessionState::RefundQueued => BillingLifecycleState::RefundQueued,
            },
            LifecycleState::UsagePending(_) => BillingLifecycleState::UsagePending,
            LifecycleState::Completed(_) => BillingLifecycleState::Completed,
        }
    }

    /// 绑定已由运行时鉴权固定的服务账号身份，后续完成时写入企业审计投影。
    ///
    /// 该方法只设置脱敏身份，不改变现有个人/API Key 主体；应在首次完成请求前调用。
    pub const fn with_service_account_identity(
        mut self,
        identity: OrganizationServiceAccountRuntimeIdentity,
    ) -> Self {
        self.service_account_identity = Some(identity);
        self
    }

    /// 用最终 usage 完成免费记录或按量结算；相同 usage 的终态重放不会重复副作用。
    pub async fn complete(
        &mut self,
        usage: Usage,
    ) -> Result<BillingCompletion, BillingLifecycleError> {
        self.complete_with_dimensions(usage, BillingUsageDimensions::empty())
            .await
    }

    /// 用最终 usage 和可选多模态事实完成结算；终态重放必须同时匹配两者。
    pub async fn complete_with_dimensions(
        &mut self,
        usage: Usage,
        dimensions: BillingUsageDimensions,
    ) -> Result<BillingCompletion, BillingLifecycleError> {
        self.complete_with_observation(usage, dimensions, BillingUsageObservation::default())
            .await
    }

    /// 用最终用量、多模态维度和调用观测完成同一条结算事实。
    pub async fn complete_with_observation(
        &mut self,
        usage: Usage,
        dimensions: BillingUsageDimensions,
        observation: BillingUsageObservation,
    ) -> Result<BillingCompletion, BillingLifecycleError> {
        match self.state {
            LifecycleState::UsagePending(completion) => {
                ensure_same_completion(completion, usage, dimensions, observation)?;
                return self.try_record_usage();
            }
            LifecycleState::Completed(completion) => {
                ensure_same_completion(completion, usage, dimensions, observation)?;
                return Ok(completion);
            }
            LifecycleState::Free | LifecycleState::Metered { .. } => {}
        }

        let frozen_completion = match self.state {
            LifecycleState::Metered {
                pending_completion: Some(completion),
                ..
            } => {
                ensure_same_completion(completion, usage, dimensions, observation)?;
                Some(completion)
            }
            LifecycleState::Free
            | LifecycleState::Metered {
                pending_completion: None,
                ..
            } => None,
            LifecycleState::UsagePending(_) | LifecycleState::Completed(_) => {
                unreachable!("终态与待记录状态已在定价前返回")
            }
        };
        let completion = if let Some(completion) = frozen_completion {
            completion
        } else {
            let price = self.resolver.resolve(&PricingContext::new(&usage))?;
            BillingCompletion::new(
                BillingUsageRecord::new_with_dimensions(
                    self.event_id,
                    self.principal,
                    usage,
                    dimensions,
                    price.billing_mode(),
                    price.quota(),
                )
                .with_service_account_identity_opt(self.service_account_identity)
                .with_observation(observation),
            )
        };
        let settlement_port = self.settlement_port.clone();
        let settlement = match &mut self.state {
            LifecycleState::Free if completion.billing_mode() == BillingMode::Free => None,
            LifecycleState::Metered {
                billing_mode,
                session,
                pending_completion,
            } if completion.billing_mode() == *billing_mode => {
                let settlement_port = settlement_port
                    .as_ref()
                    .cloned()
                    .ok_or(BillingLifecycleError::InvalidActivation)?;
                let request = session.begin_settlement(completion.quota())?;
                // 在任何持久化 IO 前冻结额度、usage 与多模态维度，结果未知时只能原样重放。
                *pending_completion = Some(completion);
                Some((request, settlement_port))
            }
            LifecycleState::Free | LifecycleState::Metered { .. } => {
                return Err(BillingLifecycleError::BillingModeChanged);
            }
            LifecycleState::UsagePending(_) | LifecycleState::Completed(_) => {
                unreachable!("终态与待记录状态已在定价前返回")
            }
        };

        if let Some((request, settlement_port)) = settlement {
            settlement_port.settle(request).await?;
            let LifecycleState::Metered {
                session,
                pending_completion: Some(pending_completion),
                ..
            } = &mut self.state
            else {
                return Err(BillingLifecycleError::InvalidActivation);
            };
            if *pending_completion != completion {
                return Err(BillingLifecycleError::InvalidActivation);
            }
            session.mark_settled()?;
        }

        self.state = LifecycleState::UsagePending(completion);
        self.try_record_usage()
    }

    fn try_record_usage(&mut self) -> Result<BillingCompletion, BillingLifecycleError> {
        let LifecycleState::UsagePending(completion) = self.state else {
            return Err(BillingLifecycleError::InvalidActivation);
        };
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.usage_port.try_record(completion.usage_record())
        }))
        .map_err(|_| BillingLifecycleError::UsageRecordPanicked)?;
        match outcome {
            UsageRecordOutcome::Accepted => {
                self.state = LifecycleState::Completed(completion);
                Ok(completion)
            }
            UsageRecordOutcome::Saturated => Err(BillingLifecycleError::UsageRecordSaturated),
            UsageRecordOutcome::Closed => Err(BillingLifecycleError::UsageRecordClosed),
            UsageRecordOutcome::Conflict => Err(BillingLifecycleError::UsageRecordConflict),
        }
    }
}

impl fmt::Debug for BillingRequestLifecycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingRequestLifecycle")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

fn ensure_same_completion(
    completion: BillingCompletion,
    usage: Usage,
    dimensions: BillingUsageDimensions,
    observation: BillingUsageObservation,
) -> Result<(), BillingLifecycleError> {
    if completion.usage() == usage
        && completion.dimensions() == dimensions
        && completion.observation() == observation
    {
        Ok(())
    } else {
        Err(BillingLifecycleError::CompletionConflict)
    }
}

/// 请求计费计划、状态转换、结算或用量记录错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingLifecycleError {
    /// 免费与按量计划使用了错误的启动入口或缺少必需端口。
    #[error("计费计划启动方式无效")]
    InvalidActivation,
    /// 同一不可变解析器在预估和实际阶段返回了不同计费模式。
    #[error("请求生命周期内计费模式发生变化")]
    BillingModeChanged,
    /// 结果未知或终态重放携带了不同的最终 usage 或审计维度。
    #[error("计费完成重放的用量不一致")]
    CompletionConflict,
    /// 定价解析失败。
    #[error(transparent)]
    Pricing(#[from] PricingError),
    /// 本地会话拒绝了状态转换或不同额度重放。
    #[error(transparent)]
    Session(#[from] BillingSessionError),
    /// 持久化结算未获得明确终态。
    #[error(transparent)]
    Settlement(#[from] BillingSettlementError),
    /// 用量记录队列已满，可用同一 completion 重试。
    #[error("用量记录队列已满")]
    UsageRecordSaturated,
    /// 用量记录端口已经关闭。
    #[error("用量记录端口已关闭")]
    UsageRecordClosed,
    /// 用量记录幂等键已经绑定了不同的内容。
    #[error("用量记录幂等键冲突")]
    UsageRecordConflict,
    /// 用量记录端口违反了不得 panic 的契约。
    #[error("用量记录端口异常")]
    UsageRecordPanicked,
}
