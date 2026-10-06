use std::fmt;

use af_domain::RedemptionBatchId;
use thiserror::Error;

use super::{MAX_REDEMPTION_BATCH_PAGE_SIZE, RedemptionBatchRecord};

/// 兑换码运营报表允许的最大分页大小。
pub const MAX_REDEMPTION_AUDIT_PAGE_SIZE: usize = 100;

/// 管理端报表使用的有效批次状态筛选。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedemptionAuditStatus {
    /// 当前仍可兑换的批次。
    Active,
    /// 已到期但没有被管理动作停用的批次。
    Expired,
    /// 被管理员整体停用的批次。
    Disabled,
    /// 至少包含一条已兑换事实的批次。
    Redeemed,
}

/// 已校验的兑换码运营报表查询条件。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RedemptionAuditQuery {
    before: Option<i64>,
    limit: usize,
    batch_id: Option<RedemptionBatchId>,
    status: Option<RedemptionAuditStatus>,
    redeemed_after: Option<u64>,
    redeemed_before: Option<u64>,
    now: u64,
}

impl RedemptionAuditQuery {
    /// 校验游标、时间窗口和分页边界，并固化本次查询使用的当前时间。
    pub fn new(
        before: Option<i64>,
        limit: usize,
        batch_id: Option<RedemptionBatchId>,
        status: Option<RedemptionAuditStatus>,
        redeemed_after: Option<u64>,
        redeemed_before: Option<u64>,
        now: u64,
    ) -> Result<Self, RedemptionAuditQueryError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_REDEMPTION_AUDIT_PAGE_SIZE).contains(&limit)
            || now > i64::MAX as u64
            || redeemed_after.is_some_and(|value| value > i64::MAX as u64)
            || redeemed_before.is_some_and(|value| value > i64::MAX as u64)
            || matches!((redeemed_after, redeemed_before), (Some(start), Some(end)) if start >= end)
        {
            return Err(RedemptionAuditQueryError::InvalidInput);
        }
        Ok(Self {
            before,
            limit,
            batch_id,
            status,
            redeemed_after,
            redeemed_before,
            now,
        })
    }

    /// 返回稳定分页游标。
    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }

    /// 返回本页大小。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }

    /// 返回可选批次筛选。
    #[must_use]
    pub const fn batch_id(self) -> Option<RedemptionBatchId> {
        self.batch_id
    }

    /// 返回批次状态筛选。
    #[must_use]
    pub const fn status(self) -> Option<RedemptionAuditStatus> {
        self.status
    }

    /// 返回兑换事实时间窗口起点（含）。
    #[must_use]
    pub const fn redeemed_after(self) -> Option<u64> {
        self.redeemed_after
    }

    /// 返回兑换事实时间窗口终点（不含）。
    #[must_use]
    pub const fn redeemed_before(self) -> Option<u64> {
        self.redeemed_before
    }

    /// 返回本次查询固化的当前时间。
    #[must_use]
    pub const fn now(self) -> u64 {
        self.now
    }
}

impl Default for RedemptionAuditQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: MAX_REDEMPTION_BATCH_PAGE_SIZE,
            batch_id: None,
            status: None,
            redeemed_after: None,
            redeemed_before: None,
            now: 0,
        }
    }
}

impl fmt::Debug for RedemptionAuditQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedemptionAuditQuery")
            .field("before", &self.before)
            .field("limit", &self.limit)
            .field("status", &self.status)
            .field("has_batch_id", &self.batch_id.is_some())
            .field("has_redeemed_after", &self.redeemed_after.is_some())
            .field("has_redeemed_before", &self.redeemed_before.is_some())
            .finish()
    }
}

/// 兑换码运营报表查询参数错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionAuditQueryError {
    /// 游标、分页或时间窗口不满足公开契约。
    #[error("兑换码审计报表查询参数无效")]
    InvalidInput,
}

/// 单个批次的兑换码运营统计，所有数量均按互斥状态归类。
pub struct RedemptionAuditBatchRecord {
    batch: RedemptionBatchRecord,
    issued_count: usize,
    redeemed_count: usize,
    remaining_count: usize,
    expired_count: usize,
    disabled_count: usize,
    last_redeemed_at: Option<u64>,
}

impl RedemptionAuditBatchRecord {
    pub(super) fn new(
        batch: RedemptionBatchRecord,
        issued_count: usize,
        redeemed_count: usize,
        remaining_count: usize,
        expired_count: usize,
        disabled_count: usize,
        last_redeemed_at: Option<u64>,
    ) -> Self {
        Self {
            batch,
            issued_count,
            redeemed_count,
            remaining_count,
            expired_count,
            disabled_count,
            last_redeemed_at,
        }
    }

    /// 返回批次固化事实。
    #[must_use]
    pub const fn batch(&self) -> &RedemptionBatchRecord {
        &self.batch
    }

    /// 返回签发总数。
    #[must_use]
    pub const fn issued_count(&self) -> usize {
        self.issued_count
    }

    /// 返回已兑换数量。
    #[must_use]
    pub const fn redeemed_count(&self) -> usize {
        self.redeemed_count
    }

    /// 返回当前仍可兑换数量。
    #[must_use]
    pub const fn remaining_count(&self) -> usize {
        self.remaining_count
    }

    /// 返回因到期失效的数量。
    #[must_use]
    pub const fn expired_count(&self) -> usize {
        self.expired_count
    }

    /// 返回因批次停用失效的数量。
    #[must_use]
    pub const fn disabled_count(&self) -> usize {
        self.disabled_count
    }

    /// 返回最近一次兑换时间。
    #[must_use]
    pub const fn last_redeemed_at(&self) -> Option<u64> {
        self.last_redeemed_at
    }
}

impl fmt::Debug for RedemptionAuditBatchRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionAuditBatchRecord(<redacted>)")
    }
}

/// 当前报表页内批次统计的闭合汇总。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RedemptionAuditSummaryRecord {
    issued_count: usize,
    redeemed_count: usize,
    remaining_count: usize,
    expired_count: usize,
    disabled_count: usize,
}

impl RedemptionAuditSummaryRecord {
    pub(super) const fn new(
        issued_count: usize,
        redeemed_count: usize,
        remaining_count: usize,
        expired_count: usize,
        disabled_count: usize,
    ) -> Self {
        Self {
            issued_count,
            redeemed_count,
            remaining_count,
            expired_count,
            disabled_count,
        }
    }

    /// 返回签发总数。
    #[must_use]
    pub const fn issued_count(self) -> usize {
        self.issued_count
    }

    /// 返回已兑换总数。
    #[must_use]
    pub const fn redeemed_count(self) -> usize {
        self.redeemed_count
    }

    /// 返回当前可兑换总数。
    #[must_use]
    pub const fn remaining_count(self) -> usize {
        self.remaining_count
    }

    /// 返回到期失效总数。
    #[must_use]
    pub const fn expired_count(self) -> usize {
        self.expired_count
    }

    /// 返回停用失效总数。
    #[must_use]
    pub const fn disabled_count(self) -> usize {
        self.disabled_count
    }
}

/// 一页批次审计统计及稳定游标。
pub struct RedemptionAuditPageRecord {
    batches: Vec<RedemptionAuditBatchRecord>,
    summary: RedemptionAuditSummaryRecord,
    next_cursor: Option<i64>,
}

impl RedemptionAuditPageRecord {
    pub(super) fn new(
        batches: Vec<RedemptionAuditBatchRecord>,
        summary: RedemptionAuditSummaryRecord,
        next_cursor: Option<i64>,
    ) -> Self {
        Self {
            batches,
            summary,
            next_cursor,
        }
    }

    /// 拆出当前页批次、闭合汇总和下一页游标。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<RedemptionAuditBatchRecord>,
        RedemptionAuditSummaryRecord,
        Option<i64>,
    ) {
        (self.batches, self.summary, self.next_cursor)
    }
}

impl fmt::Debug for RedemptionAuditPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedemptionAuditPageRecord(<redacted>)")
    }
}
