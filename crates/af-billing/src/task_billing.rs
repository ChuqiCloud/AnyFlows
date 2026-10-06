use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use af_domain::{BillingReservationId, GatewayPrincipal, Quota};
use thiserror::Error;

mod ports;
mod release_consumer;
mod release_queue;

pub use ports::{
    TaskBillingReleaseSignalOutcome, TaskBillingReleaseSignalPort, TaskBillingReserveError,
    TaskBillingReserveFuture, TaskBillingReservePort, TaskBillingSettlementError,
    TaskBillingSettlementFuture, TaskBillingSettlementPort,
};
pub use release_consumer::{
    TaskBillingReleaseConsumer, TaskBillingReleaseConsumerError, TaskBillingReleaseSink,
    TaskBillingReleaseSinkError, TaskBillingReleaseSinkFuture,
};
pub use release_queue::{
    TaskBillingReleaseDelivery, TaskBillingReleaseQueue, TaskBillingReleaseQueueError,
    TaskBillingReleaseReceiver,
};

/// 任务计费的提交前计划。
///
/// 计划固化预留标识、认证主体和严格正数上界；结果未知时调用方必须保留本值，并用
/// 同一端口重放 [`Self::reserve`]，禁止换键或按差额补偿。
#[must_use = "任务计费计划必须完成唯一预留或用同一参数重放"]
pub struct TaskBillingPlan {
    reservation_id: BillingReservationId,
    principal: GatewayPrincipal,
    upper_bound: Quota,
}

impl TaskBillingPlan {
    /// 创建任务计费计划；显式免费任务应由后续免费入口承载，不能伪造零额预留。
    pub fn new(
        reservation_id: BillingReservationId,
        principal: GatewayPrincipal,
        upper_bound: Quota,
    ) -> Result<Self, TaskBillingError> {
        if upper_bound.is_zero() {
            return Err(TaskBillingError::ZeroUpperBound);
        }
        Ok(Self {
            reservation_id,
            principal,
            upper_bound,
        })
    }

    /// 返回任务冻结额度的严格上界。
    #[must_use]
    pub const fn upper_bound(&self) -> Quota {
        self.upper_bound
    }

    /// 使用批量任务专用端口执行或重放持久化冻结。
    pub async fn reserve(&self, port: &dyn TaskBillingReservePort) -> Result<(), TaskBillingError> {
        port.reserve(self.reservation_id, self.principal, self.upper_bound)
            .await?;
        Ok(())
    }

    /// 从已经明确确认的持久化冻结启动三阶段生命周期。
    pub fn start_reserved(
        self,
        release_port: Arc<dyn TaskBillingReleaseSignalPort>,
        settlement_port: Arc<dyn TaskBillingSettlementPort>,
    ) -> TaskBillingLifecycle {
        TaskBillingLifecycle {
            reservation_id: self.reservation_id,
            upper_bound: self.upper_bound,
            state: LifecycleState::Reserved,
            release_port,
            settlement_port,
        }
    }
}

impl fmt::Debug for TaskBillingPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskBillingPlan")
            .field("reservation_id", &"<脱敏>")
            .field("principal", &"<脱敏>")
            .field("upper_bound", &"<脱敏>")
            .finish()
    }
}

/// 已固化且只能用相同额度重放的任务结算请求。
#[must_use = "任务结算请求必须交给批量任务专用持久化端口"]
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct TaskBillingSettlementRequest {
    reservation_id: BillingReservationId,
    actual_quota: Quota,
}

impl TaskBillingSettlementRequest {
    /// 从持久化任务事实恢复待结算请求，并在执行 IO 前校验冻结上界。
    pub fn new(
        reservation_id: BillingReservationId,
        actual_quota: Quota,
        upper_bound: Quota,
    ) -> Result<Self, TaskBillingError> {
        if actual_quota > upper_bound {
            return Err(TaskBillingError::ActualExceedsUpperBound);
        }
        Ok(Self {
            reservation_id,
            actual_quota,
        })
    }

    /// 返回冻结、结算和结果未知重放共用的标识。
    #[must_use]
    pub const fn reservation_id(self) -> BillingReservationId {
        self.reservation_id
    }

    /// 返回首次成功终态固化的实际额度。
    #[must_use]
    pub const fn actual_quota(self) -> Quota {
        self.actual_quota
    }
}

impl fmt::Debug for TaskBillingSettlementRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TaskBillingSettlementRequest(<脱敏>)")
    }
}

/// 任务结算完成后的最小计费事实。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct TaskBillingCompletion {
    actual_quota: Quota,
    used_submission_fallback: bool,
}

impl TaskBillingCompletion {
    /// 返回最终结算额度。
    #[must_use]
    pub const fn actual_quota(self) -> Quota {
        self.actual_quota
    }

    /// 返回本次结算是否因缺少最终事实而使用提交阶段 fallback。
    #[must_use]
    pub const fn used_submission_fallback(self) -> bool {
        self.used_submission_fallback
    }
}

impl fmt::Debug for TaskBillingCompletion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskBillingCompletion")
            .field("used_submission_fallback", &self.used_submission_fallback)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LifecycleState {
    Reserved,
    Submitted {
        fallback_quota: Quota,
    },
    SettlementPending {
        fallback_quota: Quota,
        actual_quota: Quota,
        used_submission_fallback: bool,
    },
    Settled {
        fallback_quota: Quota,
        actual_quota: Quota,
        used_submission_fallback: bool,
    },
    ReleaseQueued,
}

/// 对外可观察且不包含标识或额度的任务计费状态。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskBillingState {
    /// 提交前上界已经冻结，Drop 仍承担失败释放责任。
    Reserved,
    /// 上游已接受任务，提交阶段 fallback 已固化。
    Submitted,
    /// 成功结算参数已固化，只能按相同事实重放。
    SettlementPending,
    /// 批量任务额度已经完成最终结算。
    Settled,
    /// 失败释放信号已由后台端口接收。
    ReleaseQueued,
}

/// 覆盖提交、轮询到终态的任务计费生命周期。
///
/// `Reserved` 在提前返回或 panic 时会非阻塞发送一次任务专用释放信号。调用
/// [`Self::accept_submission`] 表示上游已经可能产生费用，因此会解除提交前 Drop 释放；
/// 此后调用方必须把任务句柄和 fallback 可靠持久化，并在终态显式结算或释放。
#[must_use = "任务计费生命周期必须保留到结算或失败释放"]
pub struct TaskBillingLifecycle {
    reservation_id: BillingReservationId,
    upper_bound: Quota,
    state: LifecycleState,
    release_port: Arc<dyn TaskBillingReleaseSignalPort>,
    settlement_port: Arc<dyn TaskBillingSettlementPort>,
}

impl TaskBillingLifecycle {
    /// 返回不包含任务标识和额度的当前状态。
    #[must_use]
    pub const fn state(&self) -> TaskBillingState {
        match self.state {
            LifecycleState::Reserved => TaskBillingState::Reserved,
            LifecycleState::Submitted { .. } => TaskBillingState::Submitted,
            LifecycleState::SettlementPending { .. } => TaskBillingState::SettlementPending,
            LifecycleState::Settled { .. } => TaskBillingState::Settled,
            LifecycleState::ReleaseQueued => TaskBillingState::ReleaseQueued,
        }
    }

    /// 在确认上游接受后固化提交阶段 fallback。
    ///
    /// fallback 必须不超过提交前冻结上界。相同值可以幂等重放，不同值会失败关闭。
    pub fn accept_submission(&mut self, fallback_quota: Quota) -> Result<(), TaskBillingError> {
        self.ensure_within_upper_bound(
            fallback_quota,
            TaskBillingError::FallbackExceedsUpperBound,
        )?;
        match self.state {
            LifecycleState::Reserved => {
                self.state = LifecycleState::Submitted { fallback_quota };
                Ok(())
            }
            LifecycleState::Submitted {
                fallback_quota: existing,
            }
            | LifecycleState::SettlementPending {
                fallback_quota: existing,
                ..
            }
            | LifecycleState::Settled {
                fallback_quota: existing,
                ..
            } if existing == fallback_quota => Ok(()),
            LifecycleState::Submitted { .. }
            | LifecycleState::SettlementPending { .. }
            | LifecycleState::Settled { .. } => Err(TaskBillingError::SubmissionFallbackConflict),
            LifecycleState::ReleaseQueued => Err(TaskBillingError::InvalidTransition),
        }
    }

    /// 使用最终计费事实或提交阶段 fallback 完成成功结算。
    ///
    /// 在任何持久化 IO 前都会先固化实际额度。端口返回结果未知时，本地状态保留为
    /// `SettlementPending`，调用方只能用完全相同的 `final_quota` 参数重放本方法。
    pub async fn complete_success(
        &mut self,
        final_quota: Option<Quota>,
    ) -> Result<TaskBillingCompletion, TaskBillingError> {
        if let LifecycleState::Settled {
            fallback_quota,
            actual_quota,
            used_submission_fallback,
        } = self.state
        {
            self.ensure_completion_replay(
                fallback_quota,
                actual_quota,
                used_submission_fallback,
                final_quota,
            )?;
            return Ok(TaskBillingCompletion {
                actual_quota,
                used_submission_fallback,
            });
        }

        let request = self.begin_settlement(final_quota)?;
        self.settlement_port.settle(request).await?;
        let LifecycleState::SettlementPending {
            fallback_quota,
            actual_quota,
            used_submission_fallback,
        } = self.state
        else {
            return Err(TaskBillingError::InvalidTransition);
        };
        self.state = LifecycleState::Settled {
            fallback_quota,
            actual_quota,
            used_submission_fallback,
        };
        Ok(TaskBillingCompletion {
            actual_quota,
            used_submission_fallback,
        })
    }

    /// 返回结果未知时必须原样重放的结算请求。
    #[must_use]
    pub const fn pending_settlement(&self) -> Option<TaskBillingSettlementRequest> {
        match self.state {
            LifecycleState::SettlementPending { actual_quota, .. } => {
                Some(TaskBillingSettlementRequest {
                    reservation_id: self.reservation_id,
                    actual_quota,
                })
            }
            LifecycleState::Reserved
            | LifecycleState::Submitted { .. }
            | LifecycleState::Settled { .. }
            | LifecycleState::ReleaseQueued => None,
        }
    }

    /// 对提交失败、任务失败、取消或超时发送任务专用释放信号。
    ///
    /// 信号被接受后重复调用是幂等空操作；端口拒绝或异常时保持原状态，允许调用方
    /// 重试，且提交前 `Reserved` 的 Drop 责任仍然保留。
    pub fn complete_failure(&mut self) -> Result<(), TaskBillingError> {
        match self.state {
            LifecycleState::Reserved | LifecycleState::Submitted { .. } => {
                self.try_signal_release()?;
                self.state = LifecycleState::ReleaseQueued;
                Ok(())
            }
            LifecycleState::ReleaseQueued => Ok(()),
            LifecycleState::SettlementPending { .. } | LifecycleState::Settled { .. } => {
                Err(TaskBillingError::InvalidTransition)
            }
        }
    }

    fn begin_settlement(
        &mut self,
        final_quota: Option<Quota>,
    ) -> Result<TaskBillingSettlementRequest, TaskBillingError> {
        let (fallback_quota, pending) = match self.state {
            LifecycleState::Submitted { fallback_quota } => (fallback_quota, None),
            LifecycleState::SettlementPending {
                fallback_quota,
                actual_quota,
                used_submission_fallback,
            } => (
                fallback_quota,
                Some((actual_quota, used_submission_fallback)),
            ),
            LifecycleState::Reserved
            | LifecycleState::Settled { .. }
            | LifecycleState::ReleaseQueued => return Err(TaskBillingError::InvalidTransition),
        };
        let used_submission_fallback = final_quota.is_none();
        let actual_quota = final_quota.unwrap_or(fallback_quota);
        self.ensure_within_upper_bound(actual_quota, TaskBillingError::ActualExceedsUpperBound)?;
        if pending.is_some_and(|existing| existing != (actual_quota, used_submission_fallback)) {
            return Err(TaskBillingError::SettlementConflict);
        }
        self.state = LifecycleState::SettlementPending {
            fallback_quota,
            actual_quota,
            used_submission_fallback,
        };
        TaskBillingSettlementRequest::new(self.reservation_id, actual_quota, self.upper_bound)
    }

    fn ensure_completion_replay(
        &self,
        fallback_quota: Quota,
        actual_quota: Quota,
        used_submission_fallback: bool,
        final_quota: Option<Quota>,
    ) -> Result<(), TaskBillingError> {
        let replay_uses_fallback = final_quota.is_none();
        let replay_actual = final_quota.unwrap_or(fallback_quota);
        if replay_actual == actual_quota && replay_uses_fallback == used_submission_fallback {
            Ok(())
        } else {
            Err(TaskBillingError::SettlementConflict)
        }
    }

    fn ensure_within_upper_bound(
        &self,
        quota: Quota,
        error: TaskBillingError,
    ) -> Result<(), TaskBillingError> {
        if quota <= self.upper_bound {
            Ok(())
        } else {
            Err(error)
        }
    }

    fn try_signal_release(&self) -> Result<(), TaskBillingError> {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.release_port.try_signal_release(self.reservation_id)
        }))
        .map_err(|_| TaskBillingError::ReleaseSignalPanicked)?;
        match outcome {
            TaskBillingReleaseSignalOutcome::Accepted => Ok(()),
            TaskBillingReleaseSignalOutcome::Saturated => {
                Err(TaskBillingError::ReleaseSignalSaturated)
            }
            TaskBillingReleaseSignalOutcome::Closed => Err(TaskBillingError::ReleaseSignalClosed),
        }
    }
}

impl Drop for TaskBillingLifecycle {
    fn drop(&mut self) {
        if self.state == LifecycleState::Reserved {
            // Drop 只能做非阻塞提示，且不能让端口 panic 覆盖原始业务错误。
            let _ = self.try_signal_release();
        }
    }
}

impl fmt::Debug for TaskBillingLifecycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskBillingLifecycle")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}

/// 任务三阶段计费状态转换或持久化错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskBillingError {
    /// 批量任务预留必须使用严格正数上界。
    #[error("任务计费上界必须大于零")]
    ZeroUpperBound,
    /// 提交阶段 fallback 超过冻结上界。
    #[error("任务提交计费 fallback 超过冻结上界")]
    FallbackExceedsUpperBound,
    /// 最终计费事实超过冻结上界。
    #[error("任务实际计费额度超过冻结上界")]
    ActualExceedsUpperBound,
    /// 同一提交结果重放携带了不同 fallback。
    #[error("任务提交计费 fallback 与已固化值冲突")]
    SubmissionFallbackConflict,
    /// 结果未知后的结算重放参数发生变化。
    #[error("任务结算参数与已固化请求冲突")]
    SettlementConflict,
    /// 当前任务计费状态不允许请求的转换。
    #[error("当前任务计费状态不允许该转换")]
    InvalidTransition,
    /// 任务专用释放队列已满。
    #[error("任务额度释放信号队列已满")]
    ReleaseSignalSaturated,
    /// 任务专用释放端口已关闭。
    #[error("任务额度释放信号端口已关闭")]
    ReleaseSignalClosed,
    /// 任务专用释放端口违反了不得 panic 的契约。
    #[error("任务额度释放信号端口异常")]
    ReleaseSignalPanicked,
    /// 提交前持久化冻结失败。
    #[error(transparent)]
    Reserve(#[from] TaskBillingReserveError),
    /// 成功终态的持久化结算失败。
    #[error(transparent)]
    Settlement(#[from] TaskBillingSettlementError),
}
