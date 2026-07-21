use af_domain::{GatewayPrincipal, Quota};
use sea_orm::{QueryResult, entity::prelude::TimeDateTimeWithTimeZone};
use thiserror::Error;

/// 计费预留的持久化状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum QuotaReservationStatus {
    /// 已冻结预估额度，等待请求终态。
    Reserved = 1,
    /// 已固化实际额度，等待余额调整成功。
    SettlementPending = 2,
    /// 余额与累计用量已完成最终结算。
    Settled = 3,
    /// 预留额度已完整退还。
    Refunded = 4,
}

impl QuotaReservationStatus {
    pub(crate) const fn code(self) -> i16 {
        self as i16
    }

    fn from_code(code: i16) -> Result<Self, QuotaRepositoryError> {
        match code {
            1 => Ok(Self::Reserved),
            2 => Ok(Self::SettlementPending),
            3 => Ok(Self::Settled),
            4 => Ok(Self::Refunded),
            _ => Err(QuotaRepositoryError::Invariant),
        }
    }
}

/// 幂等额度变更的执行结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaMutationOutcome {
    /// 本次调用完成了新的持久化状态转换。
    Applied,
    /// 相同幂等调用已经存在，未再次调整额度。
    Existing(QuotaReservationStatus),
}

/// 原子额度仓储错误；不携带主体标识、额度数值或底层数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum QuotaRepositoryError {
    /// 免费请求不得创建持久化预留。
    #[error("预扣额度必须大于零")]
    ZeroAmount,
    /// 仓储超时或预留期限配置无效。
    #[error("额度仓储配置无效")]
    InvalidConfiguration,
    /// 指定的计费预留不存在。
    #[error("计费预留不存在")]
    NotFound,
    /// 幂等键已绑定不同参数，或当前状态不允许该操作。
    #[error("计费预留状态冲突")]
    Conflict,
    /// 用户钱包不足以完成预扣或结算补扣。
    #[error("用户额度不足")]
    UserQuotaInsufficient,
    /// 有限令牌不足以完成预扣或结算补扣。
    #[error("令牌额度不足")]
    TokenQuotaInsufficient,
    /// 获取连接或执行非提交数据库操作失败。
    #[error("额度仓储数据库操作失败")]
    Query,
    /// 超时或提交失败使调用方无法确定事务终态。
    #[error("额度仓储操作结果未知")]
    OutcomeUnknown,
    /// 持久化记录、关联关系或计数器违反计费不变量。
    #[error("额度仓储持久化状态损坏")]
    Invariant,
}

pub(crate) struct ReservationState {
    pub(crate) user_id: i64,
    pub(crate) token_id: i64,
    pub(crate) group_id: i64,
    pub(crate) status: QuotaReservationStatus,
    pub(crate) reserved_quota: i64,
    pub(crate) token_reserved_quota: i64,
    pub(crate) actual_quota: Option<i64>,
}

impl ReservationState {
    pub(crate) fn try_from_result(result: &QueryResult) -> Result<Self, QuotaRepositoryError> {
        let status = QuotaReservationStatus::from_code(get(result, "status")?)?;
        let state = Self {
            user_id: get(result, "user_id")?,
            token_id: get(result, "token_id")?,
            group_id: get(result, "group_id")?,
            status,
            reserved_quota: get(result, "reserved_quota")?,
            token_reserved_quota: get(result, "token_reserved_quota")?,
            actual_quota: get(result, "actual_quota")?,
        };
        let finalized_at: Option<TimeDateTimeWithTimeZone> = get(result, "finalized_at")?;
        state.validate(finalized_at.is_some())?;
        Ok(state)
    }

    pub(crate) fn matches_precharge(&self, principal: GatewayPrincipal, amount: Quota) -> bool {
        self.user_id == principal.user_id().get()
            && self.token_id == principal.token_id().get()
            && self.group_id == principal.group_id().get()
            && self.reserved_quota == amount.units()
    }

    pub(crate) fn matches_actual(&self, actual: Quota) -> bool {
        self.actual_quota == Some(actual.units())
    }

    /// 校验无锁预读期间 reservation 的主体与预扣快照没有被替换。
    pub(crate) fn matches_immutable_snapshot(&self, observed: &Self) -> bool {
        self.user_id == observed.user_id
            && self.token_id == observed.token_id
            && self.group_id == observed.group_id
            && self.reserved_quota == observed.reserved_quota
            && self.token_reserved_quota == observed.token_reserved_quota
    }

    fn validate(&self, finalized: bool) -> Result<(), QuotaRepositoryError> {
        if self.user_id <= 0
            || self.token_id <= 0
            || self.group_id <= 0
            || self.reserved_quota <= 0
            || (self.token_reserved_quota != 0 && self.token_reserved_quota != self.reserved_quota)
            || self.actual_quota.is_some_and(|value| value < 0)
        {
            return Err(QuotaRepositoryError::Invariant);
        }

        let valid_state = match self.status {
            QuotaReservationStatus::Reserved => self.actual_quota.is_none() && !finalized,
            QuotaReservationStatus::SettlementPending => self.actual_quota.is_some() && !finalized,
            QuotaReservationStatus::Settled => self.actual_quota.is_some() && finalized,
            QuotaReservationStatus::Refunded => self.actual_quota.is_none() && finalized,
        };
        valid_state
            .then_some(())
            .ok_or(QuotaRepositoryError::Invariant)
    }
}

pub(crate) struct UserQuotaState {
    pub(crate) quota: i64,
    pub(crate) used_quota: i64,
    pub(crate) frozen_quota: i64,
    pub(crate) request_count: i64,
}

impl UserQuotaState {
    pub(crate) fn try_from_result(result: &QueryResult) -> Result<Self, QuotaRepositoryError> {
        let state = Self {
            quota: get(result, "quota")?,
            used_quota: get(result, "used_quota")?,
            frozen_quota: get(result, "frozen_quota")?,
            request_count: get(result, "request_count")?,
        };
        if state.quota < 0
            || state.used_quota < 0
            || state.frozen_quota < 0
            || state.request_count < 0
        {
            return Err(QuotaRepositoryError::Invariant);
        }
        Ok(state)
    }
}

pub(crate) struct TokenQuotaState {
    pub(crate) user_id: i64,
    pub(crate) remain_quota: i64,
    pub(crate) unlimited_quota: bool,
    pub(crate) used_quota: i64,
}

impl TokenQuotaState {
    pub(crate) fn try_from_result(result: &QueryResult) -> Result<Self, QuotaRepositoryError> {
        let state = Self {
            user_id: get(result, "user_id")?,
            remain_quota: get(result, "remain_quota")?,
            unlimited_quota: get(result, "unlimited_quota")?,
            used_quota: get(result, "used_quota")?,
        };
        if state.user_id <= 0 || state.remain_quota < 0 || state.used_quota < 0 {
            return Err(QuotaRepositoryError::Invariant);
        }
        Ok(state)
    }
}

fn get<T>(result: &QueryResult, column: &str) -> Result<T, QuotaRepositoryError>
where
    T: sea_orm::TryGetable,
{
    result
        .try_get("", column)
        .map_err(|_| QuotaRepositoryError::Invariant)
}
