use std::{future::Future, pin::Pin};

use af_db::{QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationStatus};
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
}

/// 免费与按量请求共用的非阻塞用量记录端口。
///
/// 实现必须以 [`BillingUsageRecord::event_id`] 作为幂等键；返回 `Accepted` 后，即使调用方
/// 无法确认后续进程状态，相同记录重放也不得产生第二条用量事实。
pub trait UsageRecordPort: Send + Sync + 'static {
    /// 尝试接收一条不含模型名或请求内容的规范化用量记录。
    fn try_record(&self, record: BillingUsageRecord) -> UsageRecordOutcome;
}
