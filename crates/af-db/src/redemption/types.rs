use std::{collections::HashSet, fmt};

use af_domain::{
    Quota, RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId, UserId, WalletEventId,
};
use thiserror::Error;

use super::{PresentedRedemptionCode, RedemptionCodeDefinition};

/// 兑换码批次名称允许的最大 UTF-8 字节数。
pub const MAX_REDEMPTION_BATCH_NAME_BYTES: usize = 80;
/// 单次批次创建允许写入的最大兑换码数量。
pub const MAX_REDEMPTION_BATCH_CODES: usize = 1_000;
/// 管理端单页允许读取的最大兑换码批次数量。
pub const MAX_REDEMPTION_BATCH_PAGE_SIZE: usize = 100;

/// 创建兑换码批次的不可变业务事实。
pub struct RedemptionBatchWrite {
    pub(super) batch_id: RedemptionBatchId,
    pub(super) name: String,
    pub(super) created_by_user_id: UserId,
    pub(super) quota_amount: Quota,
    pub(super) codes: Vec<RedemptionCodeDefinition>,
    pub(super) expires_at: Option<u64>,
    pub(super) created_at: u64,
}

impl RedemptionBatchWrite {
    /// 校验名称、正额度、唯一摘要集合和时间边界。
    pub fn new(
        batch_id: RedemptionBatchId,
        name: String,
        created_by_user_id: UserId,
        quota_amount: Quota,
        codes: Vec<RedemptionCodeDefinition>,
        expires_at: Option<u64>,
        created_at: u64,
    ) -> Result<Self, RedemptionInputError> {
        if !valid_name(&name) {
            return Err(RedemptionInputError::InvalidName);
        }
        if quota_amount.is_zero() {
            return Err(RedemptionInputError::InvalidQuota);
        }
        if !(1..=MAX_REDEMPTION_BATCH_CODES).contains(&codes.len()) {
            return Err(RedemptionInputError::InvalidCodeCount);
        }
        validate_time(created_at)?;
        if let Some(expires_at) = expires_at {
            validate_time(expires_at)?;
            if expires_at <= created_at {
                return Err(RedemptionInputError::InvalidTiming);
            }
        }

        let mut code_ids = HashSet::with_capacity(codes.len());
        let mut digests = HashSet::with_capacity(codes.len());
        for definition in &codes {
            let wallet_event = WalletEventId::new(definition.code_id().bytes())
                .map_err(|_| RedemptionInputError::InvalidCodeDefinition)?;
            if wallet_event.is_system_opening()
                || !code_ids.insert(definition.code_id())
                || !digests.insert(definition.digest())
            {
                return Err(RedemptionInputError::InvalidCodeDefinition);
            }
        }

        Ok(Self {
            batch_id,
            name,
            created_by_user_id,
            quota_amount,
            codes,
            expires_at,
            created_at,
        })
    }
}

impl fmt::Debug for RedemptionBatchWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionBatchWrite(<redacted>)")
    }
}

/// 通过 CAS 整体禁用一个兑换码批次的命令。
pub struct RedemptionBatchDisable {
    pub(super) batch_id: RedemptionBatchId,
    pub(super) expected_version: i64,
    pub(super) disabled_at: u64,
}

impl RedemptionBatchDisable {
    /// 校验可递增正版本和服务端禁用时间。
    pub fn new(
        batch_id: RedemptionBatchId,
        expected_version: u64,
        disabled_at: u64,
    ) -> Result<Self, RedemptionInputError> {
        let expected_version =
            i64::try_from(expected_version).map_err(|_| RedemptionInputError::InvalidVersion)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(RedemptionInputError::InvalidVersion);
        }
        validate_time(disabled_at)?;
        Ok(Self {
            batch_id,
            expected_version,
            disabled_at,
        })
    }
}

impl fmt::Debug for RedemptionBatchDisable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionBatchDisable(<redacted>)")
    }
}

/// 已完成格式校验的一次用户兑换尝试。
pub struct RedemptionAttempt {
    pub(super) user_id: UserId,
    pub(super) code: PresentedRedemptionCode,
    pub(super) redeemed_at: u64,
}

impl RedemptionAttempt {
    /// 固化目标用户、规范兑换码和受信服务端兑换时间。
    pub fn new(
        user_id: UserId,
        code: PresentedRedemptionCode,
        redeemed_at: u64,
    ) -> Result<Self, RedemptionInputError> {
        validate_time(redeemed_at)?;
        Ok(Self {
            user_id,
            code,
            redeemed_at,
        })
    }
}

impl fmt::Debug for RedemptionAttempt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionAttempt(<redacted>)")
    }
}

/// 已持久化兑换码批次的非敏感状态快照。
pub struct RedemptionBatchRecord {
    pub(super) database_id: i64,
    pub(super) batch_id: RedemptionBatchId,
    pub(super) name: String,
    pub(super) created_by_user_id: UserId,
    pub(super) status: RedemptionBatchStatus,
    pub(super) quota_amount: Quota,
    pub(super) code_count: usize,
    pub(super) version: u64,
    pub(super) expires_at: Option<u64>,
    pub(super) disabled_at: Option<u64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl RedemptionBatchRecord {
    /// 返回数据库内部单调主键，仅用于稳定分页游标。
    #[must_use]
    pub const fn database_id(&self) -> i64 {
        self.database_id
    }

    /// 返回稳定批次标识。
    #[must_use]
    pub const fn batch_id(&self) -> RedemptionBatchId {
        self.batch_id
    }

    /// 返回批次名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回创建该批次的管理员用户。
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }

    /// 返回批次当前闭合状态。
    #[must_use]
    pub const fn status(&self) -> RedemptionBatchStatus {
        self.status
    }

    /// 返回每个兑换码到账的整数额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回批次固化的兑换码数量。
    #[must_use]
    pub const fn code_count(&self) -> usize {
        self.code_count
    }

    /// 返回当前 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回可选过期时间的 Unix 秒数。
    #[must_use]
    pub const fn expires_at(&self) -> Option<u64> {
        self.expires_at
    }

    /// 返回可选禁用时间的 Unix 秒数。
    #[must_use]
    pub const fn disabled_at(&self) -> Option<u64> {
        self.disabled_at
    }

    /// 返回批次创建时间 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回保持单调的批次审计更新时间 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub(super) fn matches_write(&self, write: &RedemptionBatchWrite) -> bool {
        self.batch_id == write.batch_id
            && self.name == write.name
            && self.created_by_user_id == write.created_by_user_id
            && self.quota_amount == write.quota_amount
            && self.code_count == write.codes.len()
            && self.expires_at == write.expires_at
            && self.created_at == write.created_at
    }
}

impl fmt::Debug for RedemptionBatchRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionBatchRecord(<redacted>)")
    }
}

/// 管理端批次列表中的非敏感汇总记录。
pub struct RedemptionBatchListRecord {
    batch: RedemptionBatchRecord,
    redeemed_count: usize,
}

impl RedemptionBatchListRecord {
    pub(super) const fn new(batch: RedemptionBatchRecord, redeemed_count: usize) -> Self {
        Self {
            batch,
            redeemed_count,
        }
    }

    /// 返回批次不可变事实与启停状态。
    #[must_use]
    pub const fn batch(&self) -> &RedemptionBatchRecord {
        &self.batch
    }

    /// 返回已经原子到账的兑换码数量。
    #[must_use]
    pub const fn redeemed_count(&self) -> usize {
        self.redeemed_count
    }
}

impl fmt::Debug for RedemptionBatchListRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionBatchListRecord(<redacted>)")
    }
}

/// 一页按数据库主键倒序排列的兑换码批次。
pub struct RedemptionBatchPageRecord {
    batches: Vec<RedemptionBatchListRecord>,
    next_cursor: Option<i64>,
}

impl RedemptionBatchPageRecord {
    pub(super) fn new(batches: Vec<RedemptionBatchListRecord>, next_cursor: Option<i64>) -> Self {
        Self {
            batches,
            next_cursor,
        }
    }

    /// 消费页面并返回批次汇总和下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<RedemptionBatchListRecord>, Option<i64>) {
        (self.batches, self.next_cursor)
    }
}

impl fmt::Debug for RedemptionBatchPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionBatchPageRecord(<redacted>)")
    }
}

/// 幂等创建兑换码批次后的闭合结果。
pub enum RedemptionBatchCreateOutcome {
    /// 本次调用创建了新批次及全部摘要记录。
    Created(RedemptionBatchRecord),
    /// 相同批次标识和完整不可变事实已经存在。
    Existing(RedemptionBatchRecord),
    /// 批次创建者不存在或已经软删除。
    CreatorNotFound,
}

/// CAS 禁用兑换码批次后的闭合结果。
pub enum RedemptionBatchDisableOutcome {
    /// 本次调用把 Active 批次推进为 Disabled。
    Applied(RedemptionBatchRecord),
    /// 相同版本迁移事实已经提交。
    Existing(RedemptionBatchRecord),
    /// 批次不存在。
    NotFound,
}

/// 一次兑换成功后可公开给已鉴权调用方的结果。
pub struct RedemptionRecord {
    code_id: RedemptionCodeId,
    batch_id: RedemptionBatchId,
    user_id: UserId,
    quota_amount: Quota,
    balance_after: Quota,
    redeemed_at: u64,
}

impl RedemptionRecord {
    pub(super) const fn new(
        code_id: RedemptionCodeId,
        batch_id: RedemptionBatchId,
        user_id: UserId,
        quota_amount: Quota,
        balance_after: Quota,
        redeemed_at: u64,
    ) -> Self {
        Self {
            code_id,
            batch_id,
            user_id,
            quota_amount,
            balance_after,
            redeemed_at,
        }
    }

    /// 返回本次消费的稳定兑换码标识。
    #[must_use]
    pub const fn code_id(&self) -> RedemptionCodeId {
        self.code_id
    }

    /// 返回所属批次标识。
    #[must_use]
    pub const fn batch_id(&self) -> RedemptionBatchId {
        self.batch_id
    }

    /// 返回到账用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回本次到账额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回事务提交后的用户余额。
    #[must_use]
    pub const fn balance_after(&self) -> Quota {
        self.balance_after
    }

    /// 返回兑换时间的 Unix 秒数。
    #[must_use]
    pub const fn redeemed_at(&self) -> u64 {
        self.redeemed_at
    }
}

impl fmt::Debug for RedemptionRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionRecord(<redacted>)")
    }
}

/// 兑换码未到账的闭合业务原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedemptionRejection {
    /// 摘要未命中任何兑换码。
    InvalidCode,
    /// 批次已经整体禁用。
    BatchDisabled,
    /// 批次在本次服务端兑换时间之前已经过期。
    Expired,
    /// 兑换码已经由其他用户消费。
    AlreadyUsed,
    /// 兑换时间早于批次或兑换码创建时间。
    TimingConflict,
    /// 到账会超过钱包额度整数上界。
    CreditOverflow,
}

/// 原子兑换后的闭合结果。
pub enum RedemptionOutcome {
    /// 本次调用首次完成消费和钱包到账。
    Applied(RedemptionRecord),
    /// 相同兑换码已经由同一用户完成到账。
    Existing(RedemptionRecord),
    /// 兑换码存在或查找失败，但未发生到账。
    Rejected(RedemptionRejection),
    /// 目标用户不存在或已经软删除。
    UserNotFound,
}

/// 兑换码输入构造错误；不携带名称、标识或明文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionInputError {
    /// 批次名称不符合长度、空白或控制字符边界。
    #[error("兑换码批次名称无效")]
    InvalidName,
    /// 批次额度必须为正整数。
    #[error("兑换码批次额度无效")]
    InvalidQuota,
    /// 批次兑换码数量超出边界。
    #[error("兑换码批次数量无效")]
    InvalidCodeCount,
    /// 兑换码标识或摘要在批次内重复或占用保留命名空间。
    #[error("兑换码批次定义无效")]
    InvalidCodeDefinition,
    /// 时间戳或时间先后关系无效。
    #[error("兑换码时间边界无效")]
    InvalidTiming,
    /// CAS 版本不是可递增的正整数。
    #[error("兑换码批次版本无效")]
    InvalidVersion,
}

/// 兑换码仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionRepositoryConfigError {
    /// 零超时无法形成有效数据库操作截止时间。
    #[error("兑换码仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 兑换码仓储错误；不携带明文、摘要、主体或余额。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionRepositoryError {
    /// 相同批次或 CAS 版本绑定了不同事实。
    #[error("兑换码批次或消费状态冲突")]
    Conflict,
    /// 获取连接或执行确定未提交的数据库操作失败。
    #[error("兑换码数据库操作失败")]
    Query,
    /// 写入超时或提交失败，调用方必须复用同一批次草稿或兑换码重试。
    #[error("兑换码操作结果未知")]
    OutcomeUnknown,
    /// 只读或确定未提交操作超过硬截止时间。
    #[error("兑换码数据库操作超时")]
    Timeout,
    /// 持久化批次、兑换码、钱包或版本违反不变量。
    #[error("兑换码持久化状态损坏")]
    Invariant,
}

impl fmt::Debug for RedemptionBatchCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("RedemptionBatchCreateOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("RedemptionBatchCreateOutcome::Existing(<redacted>)")
            }
            Self::CreatorNotFound => {
                formatter.write_str("RedemptionBatchCreateOutcome::CreatorNotFound")
            }
        }
    }
}

impl fmt::Debug for RedemptionBatchDisableOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => {
                formatter.write_str("RedemptionBatchDisableOutcome::Applied(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("RedemptionBatchDisableOutcome::Existing(<redacted>)")
            }
            Self::NotFound => formatter.write_str("RedemptionBatchDisableOutcome::NotFound"),
        }
    }
}

impl fmt::Debug for RedemptionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Applied(_) => formatter.write_str("RedemptionOutcome::Applied(<redacted>)"),
            Self::Existing(_) => formatter.write_str("RedemptionOutcome::Existing(<redacted>)"),
            Self::Rejected(reason) => formatter
                .debug_tuple("RedemptionOutcome::Rejected")
                .field(reason)
                .finish(),
            Self::UserNotFound => formatter.write_str("RedemptionOutcome::UserNotFound"),
        }
    }
}

pub(super) fn validate_time(value: u64) -> Result<(), RedemptionInputError> {
    if value > i64::MAX as u64 {
        return Err(RedemptionInputError::InvalidTiming);
    }
    Ok(())
}

pub(super) fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REDEMPTION_BATCH_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
