use std::{future::Future, pin::Pin};

use af_db::{QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationStatus};
use af_domain::{BillingReservationId, GatewayPrincipal, Quota};
use thiserror::Error;

use crate::SettlementRequest;

use super::BillingUsageRecord;

/// 持久化结算端口的一次异步调用结果。
pub type BillingSettlementFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BillingSettlementError>> + Send + 'a>>;

/// 只接收已经由 `BillingSession` 固化的幂等结算请求。
pub trait BillingSettlementPort: Send + Sync + 'static {
    /// 使用同一预留标识和实际额度执行或重放持久化结算。
    fn settle<'a>(&'a self, request: SettlementRequest) -> BillingSettlementFuture<'a>;
}

/// 持久化预扣端口的一次异步调用结果。
pub type BillingPrechargeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), BillingPrechargeError>> + Send + 'a>>;

/// 只接收已经由请求计费计划固化的正额度预扣请求。
pub trait BillingPrechargePort: Send + Sync + 'static {
    /// 使用同一预留标识执行或重放持久化预扣。
    fn precharge<'a>(
        &'a self,
        reservation_id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
    ) -> BillingPrechargeFuture<'a>;
}

/// 持久化预扣的闭合结果；不携带主体、预留标识或额度。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingPrechargeError {
    /// 预扣时用户或有限令牌额度不足。
    #[error("预扣额度不足")]
    InsufficientQuota,
    /// 超时或提交失败导致终态未知，只能重放同一请求。
    #[error("持久化预扣结果未知")]
    OutcomeUnknown,
    /// 幂等参数冲突、预留已处于其他状态或预留不存在。
    #[error("持久化预扣状态冲突")]
    Conflict,
    /// 数据库当前不可完成预扣。
    #[error("持久化预扣暂不可用")]
    Unavailable,
    /// 持久化数据或返回状态违反计费不变量。
    #[error("持久化预扣状态损坏")]
    Invariant,
}

impl BillingPrechargePort for QuotaRepository {
    fn precharge<'a>(
        &'a self,
        reservation_id: BillingReservationId,
        principal: GatewayPrincipal,
        amount: Quota,
    ) -> BillingPrechargeFuture<'a> {
        Box::pin(async move {
            let outcome = QuotaRepository::precharge(self, reservation_id, principal, amount)
                .await
                .map_err(map_quota_repository_precharge_error)?;
            match outcome {
                QuotaMutationOutcome::Applied
                | QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved) => Ok(()),
                QuotaMutationOutcome::Existing(
                    QuotaReservationStatus::SettlementPending
                    | QuotaReservationStatus::Settled
                    | QuotaReservationStatus::Refunded,
                ) => Err(BillingPrechargeError::Conflict),
            }
        })
    }
}

/// 持久化结算的闭合结果；不携带主体、预留标识或额度。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingSettlementError {
    /// 结算补扣时用户或有限令牌额度不足，持久化记录保持待结算。
    #[error("结算补扣额度不足")]
    InsufficientQuota,
    /// 超时或提交失败导致终态未知，只能重放同一请求。
    #[error("持久化结算结果未知")]
    OutcomeUnknown,
    /// 预留不存在、幂等参数冲突或状态不允许结算。
    #[error("持久化结算状态冲突")]
    Conflict,
    /// 数据库当前不可完成结算。
    #[error("持久化结算暂不可用")]
    Unavailable,
    /// 持久化数据或返回状态违反计费不变量。
    #[error("持久化结算状态损坏")]
    Invariant,
}

impl BillingSettlementPort for QuotaRepository {
    fn settle<'a>(&'a self, request: SettlementRequest) -> BillingSettlementFuture<'a> {
        Box::pin(async move {
            let outcome =
                QuotaRepository::settle(self, request.reservation_id(), request.actual_quota())
                    .await
                    .map_err(map_quota_repository_error)?;
            match outcome {
                QuotaMutationOutcome::Applied
                | QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled) => Ok(()),
                QuotaMutationOutcome::Existing(
                    QuotaReservationStatus::Reserved
                    | QuotaReservationStatus::SettlementPending
                    | QuotaReservationStatus::Refunded,
                ) => Err(BillingSettlementError::Invariant),
            }
        })
    }
}

// af-db 的错误枚举允许后续扩展；当前版本穷尽时仍保留脱敏兜底分类。
#[allow(unreachable_patterns)]
const fn map_quota_repository_error(error: QuotaRepositoryError) -> BillingSettlementError {
    match error {
        QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient => BillingSettlementError::InsufficientQuota,
        QuotaRepositoryError::OutcomeUnknown => BillingSettlementError::OutcomeUnknown,
        QuotaRepositoryError::NotFound | QuotaRepositoryError::Conflict => {
            BillingSettlementError::Conflict
        }
        QuotaRepositoryError::Query | QuotaRepositoryError::InvalidConfiguration => {
            BillingSettlementError::Unavailable
        }
        QuotaRepositoryError::ZeroAmount | QuotaRepositoryError::Invariant => {
            BillingSettlementError::Invariant
        }
        _ => BillingSettlementError::Invariant,
    }
}

// 预扣与结算共享底层错误分类，但保持各自端口的业务语义独立。
#[allow(unreachable_patterns)]
const fn map_quota_repository_precharge_error(
    error: QuotaRepositoryError,
) -> BillingPrechargeError {
    match error {
        QuotaRepositoryError::UserQuotaInsufficient
        | QuotaRepositoryError::TokenQuotaInsufficient => BillingPrechargeError::InsufficientQuota,
        QuotaRepositoryError::OutcomeUnknown => BillingPrechargeError::OutcomeUnknown,
        QuotaRepositoryError::NotFound | QuotaRepositoryError::Conflict => {
            BillingPrechargeError::Conflict
        }
        QuotaRepositoryError::Query | QuotaRepositoryError::InvalidConfiguration => {
            BillingPrechargeError::Unavailable
        }
        QuotaRepositoryError::ZeroAmount | QuotaRepositoryError::Invariant => {
            BillingPrechargeError::Invariant
        }
        _ => BillingPrechargeError::Invariant,
    }
}

/// 非阻塞用量记录端口的接收结果。
#[must_use = "用量记录结果必须决定是否重试同一记录"]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageRecordOutcome {
    /// 记录已经被可靠队列或存储边界接收。
    Accepted,
    /// 有界队列已满，本次记录未被接收。
    Saturated,
    /// 记录端口已经永久关闭。
    Closed,
    /// 相同幂等标识已经绑定了不同的记录内容。
    Conflict,
}

/// 免费与按量请求共用的非阻塞用量记录端口。
///
/// 实现必须以 [`BillingUsageRecord::event_id`] 作为幂等键；返回 `Accepted` 后，即使调用方
/// 无法确认后续进程状态，相同记录重放也不得产生第二条用量事实。
pub trait UsageRecordPort: Send + Sync + 'static {
    /// 尝试接收一条不含模型名或请求内容的规范化用量记录。
    fn try_record(&self, record: BillingUsageRecord) -> UsageRecordOutcome;
}
