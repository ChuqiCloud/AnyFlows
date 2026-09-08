use std::{future::Future, pin::Pin};

use af_db::{QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationStatus};
use af_domain::{BillingReservationId, GatewayPrincipal, Quota};
use thiserror::Error;

use super::TaskBillingSettlementRequest;

/// 任务冻结端口的一次异步调用结果。
pub type TaskBillingReserveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), TaskBillingReserveError>> + Send + 'a>>;

/// 只允许批量任务语义的持久化冻结端口。
pub trait TaskBillingReservePort: Send + Sync + 'static {
    /// 使用同一预留标识、主体和上界执行或重放冻结。
    fn reserve<'a>(
        &'a self,
        reservation_id: BillingReservationId,
        principal: GatewayPrincipal,
        upper_bound: Quota,
    ) -> TaskBillingReserveFuture<'a>;
}

/// 任务成功结算端口的一次异步调用结果。
pub type TaskBillingSettlementFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), TaskBillingSettlementError>> + Send + 'a>>;

/// 只允许批量任务语义的持久化结算端口。
pub trait TaskBillingSettlementPort: Send + Sync + 'static {
    /// 使用已经固化的相同预留标识和实际额度执行或重放结算。
    fn settle<'a>(
        &'a self,
        request: TaskBillingSettlementRequest,
    ) -> TaskBillingSettlementFuture<'a>;
}

/// 非阻塞任务释放信号的接收结果。
#[must_use = "任务释放信号结果必须决定是否解除本地释放责任"]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskBillingReleaseSignalOutcome {
    /// 后台端口已经接收该幂等释放提示。
    Accepted,
    /// 有界队列已满，本次提示未被接收。
    Saturated,
    /// 后台端口已永久关闭，本次提示未被接收。
    Closed,
}

/// 任务额度释放的非阻塞信号端口。
///
/// 实现必须保持 O(1)、不得执行网络或数据库 IO，并最终调用
/// `QuotaRepository::release_batch_task` 的等价专用 sink，禁止转入普通请求退款端口。
pub trait TaskBillingReleaseSignalPort: Send + Sync + 'static {
    /// 尝试把预留标识交给任务额度释放后台流程。
    fn try_signal_release(
        &self,
        reservation_id: BillingReservationId,
    ) -> TaskBillingReleaseSignalOutcome;
}

/// 提交前任务冻结的闭合错误分类。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskBillingReserveError {
    /// 钱包或有限令牌无法容纳冻结上界。
    #[error("任务冻结额度不足")]
    InsufficientQuota,
    /// 超时或提交失败导致持久化终态未知，只能原样重放。
    #[error("任务冻结结果未知")]
    OutcomeUnknown,
    /// 幂等参数冲突或预留已进入其他状态。
    #[error("任务冻结状态冲突")]
    Conflict,
    /// 数据库当前无法完成任务冻结。
    #[error("任务冻结暂不可用")]
    Unavailable,
    /// 持久化状态或端口返回违反任务计费不变量。
    #[error("任务冻结状态损坏")]
    Invariant,
}

/// 成功任务结算的闭合错误分类。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TaskBillingSettlementError {
    /// 实际额度超过冻结上界，记录保持未结算。
    #[error("任务实际额度超过冻结上界")]
    ActualExceedsReservation,
    /// 超时或提交失败导致持久化终态未知，只能原样重放。
    #[error("任务结算结果未知")]
    OutcomeUnknown,
    /// 预留不存在、幂等参数冲突或状态不允许结算。
    #[error("任务结算状态冲突")]
    Conflict,
    /// 数据库当前无法完成任务结算。
    #[error("任务结算暂不可用")]
    Unavailable,
    /// 持久化状态或端口返回违反任务计费不变量。
    #[error("任务结算状态损坏")]
    Invariant,
}

impl TaskBillingReservePort for QuotaRepository {
    fn reserve<'a>(
        &'a self,
        reservation_id: BillingReservationId,
        principal: GatewayPrincipal,
        upper_bound: Quota,
    ) -> TaskBillingReserveFuture<'a> {
        Box::pin(async move {
            let outcome =
                QuotaRepository::reserve_batch_task(self, reservation_id, principal, upper_bound)
                    .await
                    .map_err(map_reserve_error)?;
            match outcome {
                QuotaMutationOutcome::Applied
                | QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved) => Ok(()),
                QuotaMutationOutcome::Existing(
                    QuotaReservationStatus::SettlementPending
                    | QuotaReservationStatus::Settled
                    | QuotaReservationStatus::Refunded,
                ) => Err(TaskBillingReserveError::Conflict),
            }
        })
    }
}

impl TaskBillingSettlementPort for QuotaRepository {
    fn settle<'a>(
        &'a self,
        request: TaskBillingSettlementRequest,
    ) -> TaskBillingSettlementFuture<'a> {
        Box::pin(async move {
            let outcome = QuotaRepository::settle_batch_task(
                self,
                request.reservation_id(),
                request.actual_quota(),
            )
            .await
            .map_err(map_settlement_error)?;
            match outcome {
                QuotaMutationOutcome::Applied
                | QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled) => Ok(()),
                QuotaMutationOutcome::Existing(
                    QuotaReservationStatus::Reserved
                    | QuotaReservationStatus::SettlementPending
                    | QuotaReservationStatus::Refunded,
                ) => Err(TaskBillingSettlementError::Invariant),
            }
        })
    }
}

#[allow(unreachable_patterns)]
const fn map_reserve_error(error: QuotaRepositoryError) -> TaskBillingReserveError {
    match error {
        QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::OrganizationQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient => {
            TaskBillingReserveError::InsufficientQuota
        }
        QuotaRepositoryError::OutcomeUnknown => TaskBillingReserveError::OutcomeUnknown,
        QuotaRepositoryError::NotFound | QuotaRepositoryError::Conflict => {
            TaskBillingReserveError::Conflict
        }
        QuotaRepositoryError::Query | QuotaRepositoryError::InvalidConfiguration => {
            TaskBillingReserveError::Unavailable
        }
        QuotaRepositoryError::ZeroAmount
        | QuotaRepositoryError::ActualExceedsReservation
        | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::Invariant => TaskBillingReserveError::Invariant,
        _ => TaskBillingReserveError::Invariant,
    }
}

#[allow(unreachable_patterns)]
const fn map_settlement_error(error: QuotaRepositoryError) -> TaskBillingSettlementError {
    match error {
        QuotaRepositoryError::ActualExceedsReservation => {
            TaskBillingSettlementError::ActualExceedsReservation
        }
        QuotaRepositoryError::OutcomeUnknown => TaskBillingSettlementError::OutcomeUnknown,
        QuotaRepositoryError::NotFound | QuotaRepositoryError::Conflict => {
            TaskBillingSettlementError::Conflict
        }
        QuotaRepositoryError::Query | QuotaRepositoryError::InvalidConfiguration => {
            TaskBillingSettlementError::Unavailable
        }
        QuotaRepositoryError::ZeroAmount
        | QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::OrganizationQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient
        | QuotaRepositoryError::GroupWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::TokenWindowQuotaInsufficient { .. }
        | QuotaRepositoryError::Invariant => TaskBillingSettlementError::Invariant,
        _ => TaskBillingSettlementError::Invariant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_errors_map_without_crossing_request_semantics() {
        assert_eq!(
            map_reserve_error(QuotaRepositoryError::GroupWindowQuotaInsufficient {
                retry_after: None,
            }),
            TaskBillingReserveError::Invariant
        );
        assert_eq!(
            map_settlement_error(QuotaRepositoryError::ActualExceedsReservation),
            TaskBillingSettlementError::ActualExceedsReservation
        );
        assert_eq!(
            map_settlement_error(QuotaRepositoryError::UserQuotaInsufficient),
            TaskBillingSettlementError::Invariant
        );
    }
}
