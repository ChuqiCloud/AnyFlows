use std::fmt;

use af_domain::{Quota, UserId, WalletEventId};
use thiserror::Error;

/// 一次邀请返利到账的不可变业务事实。
pub struct InviteRebateGrant {
    pub(super) event_id: WalletEventId,
    pub(super) invitee_user_id: UserId,
    pub(super) quota_amount: Quota,
    pub(super) credited_at: u64,
}

impl InviteRebateGrant {
    /// 校验返利事件键、正整数额度和受信到账时间。
    pub fn new(
        event_id: WalletEventId,
        invitee_user_id: UserId,
        quota_amount: Quota,
        credited_at: u64,
    ) -> Result<Self, InviteRebateInputError> {
        if event_id.is_system_opening() {
            return Err(InviteRebateInputError::ReservedEventId);
        }
        if quota_amount.is_zero() {
            return Err(InviteRebateInputError::InvalidQuota);
        }
        validate_time(credited_at)?;
        Ok(Self {
            event_id,
            invitee_user_id,
            quota_amount,
            credited_at,
        })
    }
}

impl fmt::Debug for InviteRebateGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InviteRebateGrant(<redacted>)")
    }
}

/// 邀请返利到账后可公开给已授权调用方的结果。
pub struct InviteRebateRecord {
    event_id: WalletEventId,
    inviter_user_id: UserId,
    invitee_user_id: UserId,
    quota_amount: Quota,
    balance_after: Quota,
    credited_at: u64,
}

impl InviteRebateRecord {
    pub(super) const fn new(
        event_id: WalletEventId,
        inviter_user_id: UserId,
        invitee_user_id: UserId,
        quota_amount: Quota,
        balance_after: Quota,
        credited_at: u64,
    ) -> Self {
        Self {
            event_id,
            inviter_user_id,
            invitee_user_id,
            quota_amount,
            balance_after,
            credited_at,
        }
    }

    /// 返回稳定返利事件标识。
    #[must_use]
    pub const fn event_id(&self) -> WalletEventId {
        self.event_id
    }

    /// 返回获得返利的邀请人。
    #[must_use]
    pub const fn inviter_user_id(&self) -> UserId {
        self.inviter_user_id
    }

    /// 返回触发返利的被邀请用户。
    #[must_use]
    pub const fn invitee_user_id(&self) -> UserId {
        self.invitee_user_id
    }

    /// 返回本次到账额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回事务提交后的邀请人钱包余额。
    #[must_use]
    pub const fn balance_after(&self) -> Quota {
        self.balance_after
    }

    /// 返回到账时间 Unix 秒数。
    #[must_use]
    pub const fn credited_at(&self) -> u64 {
        self.credited_at
    }
}

impl fmt::Debug for InviteRebateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InviteRebateRecord(<redacted>)")
    }
}

/// 邀请返利无法到账的闭合业务原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InviteRebateRejection {
    /// 被邀请用户不存在或已经软删除。
    InviteeNotFound,
    /// 被邀请用户没有绑定邀请人。
    NoInviter,
    /// 邀请人不存在或已经软删除。
    InviterNotFound,
    /// 该被邀请用户已经产生过返利到账事件。
    AlreadyCredited,
    /// 到账会超过邀请人钱包或返利累计字段的整数上界。
    CreditOverflow,
}

/// 邀请返利到账后的闭合结果。
pub enum InviteRebateGrantOutcome {
    /// 本次调用首次完成返利到账。
    Applied(InviteRebateRecord),
    /// 相同事件键已经按同一事实完成到账。
    Existing(InviteRebateRecord),
    /// 返利规则拒绝到账。
    Rejected(InviteRebateRejection),
}

/// 邀请返利输入构造错误；不携带主体或事件明文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InviteRebateInputError {
    /// 返利额度必须为正整数。
    #[error("邀请返利额度无效")]
    InvalidQuota,
    /// 到账时间无法持久化。
    #[error("邀请返利时间边界无效")]
    InvalidTiming,
    /// 返利事件占用了钱包 opening 保留命名空间。
    #[error("邀请返利事件标识命名空间无效")]
    ReservedEventId,
}

/// 邀请返利仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InviteRebateRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("邀请返利仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 邀请返利仓储错误；不携带事件、主体或余额。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InviteRebateRepositoryError {
    /// 相同事件键绑定了不同事实，或数据库返利事实损坏。
    #[error("邀请返利事件冲突")]
    Conflict,
    /// 获取连接或执行确定未提交的数据库操作失败。
    #[error("邀请返利数据库操作失败")]
    Query,
    /// 写入超时或提交失败，调用方必须复用同一事件键查询或重试。
    #[error("邀请返利操作结果未知")]
    OutcomeUnknown,
    /// 持久化事件、钱包或用户累计字段违反不变量。
    #[error("邀请返利持久化状态损坏")]
    Invariant,
}

impl fmt::Debug for InviteRebateGrantOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => {
                formatter.write_str("InviteRebateGrantOutcome::Applied(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("InviteRebateGrantOutcome::Existing(<redacted>)")
            }
            Self::Rejected(reason) => formatter
                .debug_tuple("InviteRebateGrantOutcome::Rejected")
                .field(reason)
                .finish(),
        }
    }
}

pub(super) fn validate_time(value: u64) -> Result<(), InviteRebateInputError> {
    if value > i64::MAX as u64 {
        return Err(InviteRebateInputError::InvalidTiming);
    }
    Ok(())
}
