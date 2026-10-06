use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

use af_domain::{BillingReservationId, Quota};
use thiserror::Error;

/// 计费会话的本地单调状态。
///
/// `SettlementPending` 只表示结算责任已经从 RAII 退款路径移交给持久化结算流程，
/// 不保证数据库记录此刻已经进入同名状态。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingSessionState {
    /// 持久化预扣已确认，尚未发起结算；Drop 必须尝试发送退款信号。
    Reserved,
    /// 已固化结算请求，之后只能用同一请求重放或确认结算完成。
    SettlementPending,
    /// 持久化层已经明确确认结算终态。
    Settled,
    /// 退款信号已经被后台端口接受，不表示数据库已经完成退款。
    RefundQueued,
}

/// 后台退款信号端口的非阻塞接收结果。
#[must_use = "退款信号结果必须用于决定是否解除 RAII 退款责任"]
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RefundSignalOutcome {
    /// 后台端口已经接收该幂等退款提示。
    Accepted,
    /// 有界队列已满，本次提示未被接收。
    Saturated,
    /// 后台端口已永久关闭，本次提示未被接收。
    Closed,
}

/// 只接收计费预留标识的后台退款信号端口。
///
/// 实现必须保持 O(1)、非阻塞且不得执行网络或数据库 IO。信号只表示后台端口是否接收
/// 提示，不表示退款已经完成；端口实现与实际退款流程不属于本类型职责。
pub trait RefundSignalPort: Send + Sync + 'static {
    /// 尝试把一个预留标识交给后台退款流程。
    fn try_signal_refund(&self, reservation_id: BillingReservationId) -> RefundSignalOutcome;
}

/// 计费会话状态转换或退款信号错误；不保留预留标识和额度。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingSessionError {
    /// 当前状态不允许请求的单调转换。
    #[error("当前计费会话状态不允许该转换")]
    InvalidTransition,
    /// 重放结算时使用了不同的实际额度。
    #[error("结算额度与已固化请求不一致")]
    SettlementQuotaConflict,
    /// 后台退款信号队列已满。
    #[error("后台退款信号队列已满")]
    RefundSignalSaturated,
    /// 后台退款信号端口已关闭。
    #[error("后台退款信号端口已关闭")]
    RefundSignalClosed,
    /// 后台退款信号端口违反了不得 panic 的契约。
    #[error("后台退款信号端口异常")]
    RefundSignalPanicked,
}

/// 已固化且可使用同一参数安全重放的结算请求。
#[must_use = "结算请求必须交给持久化仓储执行或重放"]
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SettlementRequest {
    reservation_id: BillingReservationId,
    actual_quota: Quota,
}

impl SettlementRequest {
    /// 返回持久化结算使用的同一预留标识。
    #[must_use]
    pub const fn reservation_id(self) -> BillingReservationId {
        self.reservation_id
    }

    /// 返回首次发起结算时固化的实际额度。
    #[must_use]
    pub const fn actual_quota(self) -> Quota {
        self.actual_quota
    }
}

impl fmt::Debug for SettlementRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SettlementRequest(<redacted>)")
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SessionState {
    Reserved,
    SettlementPending { actual_quota: Quota },
    Settled,
    RefundQueued,
}

/// 持有一次持久化预留退款责任的 RAII 计费会话。
///
/// 只能在数据库已经确认预扣后构造。免费请求不创建该类型。调用方必须在执行持久化
/// 结算及任何相关 `await` 前先调用 [`Self::begin_settlement`]，从而在取消、超时或结果
/// 未知时禁止 Drop 与结算流程竞态退款。
///
/// 会话刻意不实现 `Clone`，避免多个所有者在 Drop 时重复发送退款信号。
///
/// ```compile_fail
/// use af_billing::BillingSession;
///
/// fn duplicate(session: BillingSession) {
///     let _copy = session.clone();
/// }
/// ```
#[must_use = "计费会话必须保留到结算完成或退款信号已入队"]
pub struct BillingSession {
    reservation_id: BillingReservationId,
    state: SessionState,
    refund_port: Arc<dyn RefundSignalPort>,
}

impl BillingSession {
    /// 从已经确认成功的持久化预留构造退款责任处于激活状态的会话。
    pub fn from_reserved(
        reservation_id: BillingReservationId,
        refund_port: Arc<dyn RefundSignalPort>,
    ) -> Self {
        Self {
            reservation_id,
            state: SessionState::Reserved,
            refund_port,
        }
    }

    /// 返回后续结算和退款重放必须复用的预留标识。
    #[must_use]
    pub const fn reservation_id(&self) -> BillingReservationId {
        self.reservation_id
    }

    /// 返回不包含预留标识或额度的本地状态。
    #[must_use]
    pub const fn state(&self) -> BillingSessionState {
        match self.state {
            SessionState::Reserved => BillingSessionState::Reserved,
            SessionState::SettlementPending { .. } => BillingSessionState::SettlementPending,
            SessionState::Settled => BillingSessionState::Settled,
            SessionState::RefundQueued => BillingSessionState::RefundQueued,
        }
    }

    /// 在任何持久化结算 IO 前固化实际额度并永久解除自动退款责任。
    ///
    /// 相同额度的重复调用返回同一请求，便于结果未知时安全重放；不同额度会被拒绝。
    pub fn begin_settlement(
        &mut self,
        actual_quota: Quota,
    ) -> Result<SettlementRequest, BillingSessionError> {
        match self.state {
            SessionState::Reserved => {
                self.state = SessionState::SettlementPending { actual_quota };
                Ok(self.settlement_request(actual_quota))
            }
            SessionState::SettlementPending {
                actual_quota: pending,
            } if pending == actual_quota => Ok(self.settlement_request(pending)),
            SessionState::SettlementPending { .. } => {
                Err(BillingSessionError::SettlementQuotaConflict)
            }
            SessionState::Settled | SessionState::RefundQueued => {
                Err(BillingSessionError::InvalidTransition)
            }
        }
    }

    /// 返回首次固化的结算请求，供超时或结果未知后的同参数重放。
    #[must_use]
    pub const fn pending_settlement(&self) -> Option<SettlementRequest> {
        match self.state {
            SessionState::SettlementPending { actual_quota } => {
                Some(self.settlement_request(actual_quota))
            }
            SessionState::Reserved | SessionState::Settled | SessionState::RefundQueued => None,
        }
    }

    /// 在持久化层明确确认已结算后进入终态；同态重放是幂等空操作。
    pub fn mark_settled(&mut self) -> Result<(), BillingSessionError> {
        match self.state {
            SessionState::SettlementPending { .. } => {
                self.state = SessionState::Settled;
                Ok(())
            }
            SessionState::Settled => Ok(()),
            SessionState::Reserved | SessionState::RefundQueued => {
                Err(BillingSessionError::InvalidTransition)
            }
        }
    }

    /// 显式尝试入队退款提示；成功后 Drop 不会重复发送。
    ///
    /// 队列拒绝或端口异常时保持 `Reserved`，调用方仍可重试，Drop 也会再做一次兜底尝试。
    pub fn request_refund(&mut self) -> Result<(), BillingSessionError> {
        match self.state {
            SessionState::Reserved => {
                self.try_signal_refund()?;
                self.state = SessionState::RefundQueued;
                Ok(())
            }
            SessionState::RefundQueued => Ok(()),
            SessionState::SettlementPending { .. } | SessionState::Settled => {
                Err(BillingSessionError::InvalidTransition)
            }
        }
    }

    const fn settlement_request(&self, actual_quota: Quota) -> SettlementRequest {
        SettlementRequest {
            reservation_id: self.reservation_id,
            actual_quota,
        }
    }

    fn try_signal_refund(&self) -> Result<(), BillingSessionError> {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.refund_port.try_signal_refund(self.reservation_id)
        }))
        .map_err(|_| BillingSessionError::RefundSignalPanicked)?;
        match outcome {
            RefundSignalOutcome::Accepted => Ok(()),
            RefundSignalOutcome::Saturated => Err(BillingSessionError::RefundSignalSaturated),
            RefundSignalOutcome::Closed => Err(BillingSessionError::RefundSignalClosed),
        }
    }
}

impl Drop for BillingSession {
    fn drop(&mut self) {
        if self.state == SessionState::Reserved {
            // Drop 不能传播端口错误，也不能让端口 panic 覆盖原始业务 panic。
            let _ = self.try_signal_refund();
        }
    }
}

impl fmt::Debug for BillingSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingSession")
            .field("state", &self.state())
            .finish_non_exhaustive()
    }
}
