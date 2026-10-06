use std::{fmt, future::Future, pin::Pin};

use af_db::{
    MAX_REDEMPTION_AUDIT_PAGE_SIZE, RedemptionAuditBatchRecord, RedemptionAuditPageRecord,
    RedemptionAuditQuery, RedemptionAuditStatus, RedemptionAuditSummaryRecord,
};
use af_domain::RedemptionBatchId;

use super::types::{AdminRedemptionBatch, RedemptionServiceError};

/// 管理端兑换码审计报表的默认分页大小。
pub const DEFAULT_ADMIN_REDEMPTION_AUDIT_PAGE_SIZE: usize = 25;

/// 管理端报表支持的闭合状态筛选。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminRedemptionAuditStatus {
    /// 当前仍可兑换的批次。
    Active,
    /// 已到期且没有被整体停用的批次。
    Expired,
    /// 已被管理员整体停用的批次。
    Disabled,
    /// 至少存在一条到账事实的批次。
    Redeemed,
}

impl From<AdminRedemptionAuditStatus> for RedemptionAuditStatus {
    fn from(value: AdminRedemptionAuditStatus) -> Self {
        match value {
            AdminRedemptionAuditStatus::Active => Self::Active,
            AdminRedemptionAuditStatus::Expired => Self::Expired,
            AdminRedemptionAuditStatus::Disabled => Self::Disabled,
            AdminRedemptionAuditStatus::Redeemed => Self::Redeemed,
        }
    }
}

/// 管理员提交的兑换码运营报表筛选条件。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRedemptionAuditQuery {
    before: Option<i64>,
    limit: usize,
    batch_id: Option<RedemptionBatchId>,
    status: Option<AdminRedemptionAuditStatus>,
    redeemed_after: Option<u64>,
    redeemed_before: Option<u64>,
}

impl AdminRedemptionAuditQuery {
    /// 校验批次、状态、兑换时间窗口和稳定游标。
    pub fn new(
        before: Option<i64>,
        limit: usize,
        batch_id: Option<RedemptionBatchId>,
        status: Option<AdminRedemptionAuditStatus>,
        redeemed_after: Option<i64>,
        redeemed_before: Option<i64>,
    ) -> Result<Self, RedemptionServiceError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_REDEMPTION_AUDIT_PAGE_SIZE).contains(&limit)
            || redeemed_after.is_some_and(|value| value <= 0)
            || redeemed_before.is_some_and(|value| value <= 0)
            || matches!((redeemed_after, redeemed_before), (Some(start), Some(end)) if start >= end)
        {
            return Err(RedemptionServiceError::InvalidInput);
        }
        Ok(Self {
            before,
            limit,
            batch_id,
            status,
            redeemed_after: redeemed_after
                .map(u64::try_from)
                .transpose()
                .map_err(|_| RedemptionServiceError::InvalidInput)?,
            redeemed_before: redeemed_before
                .map(u64::try_from)
                .transpose()
                .map_err(|_| RedemptionServiceError::InvalidInput)?,
        })
    }

    pub(super) fn into_repository_query(
        self,
        now: u64,
    ) -> Result<RedemptionAuditQuery, RedemptionServiceError> {
        RedemptionAuditQuery::new(
            self.before,
            self.limit,
            self.batch_id,
            self.status.map(Into::into),
            self.redeemed_after,
            self.redeemed_before,
            now,
        )
        .map_err(|_| RedemptionServiceError::InvalidInput)
    }
}

impl Default for AdminRedemptionAuditQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_ADMIN_REDEMPTION_AUDIT_PAGE_SIZE,
            batch_id: None,
            status: None,
            redeemed_after: None,
            redeemed_before: None,
        }
    }
}

/// 管理员可读的单批次运营统计。
pub struct AdminRedemptionAuditBatch {
    batch: AdminRedemptionBatch,
    issued_count: usize,
    remaining_count: usize,
    expired_count: usize,
    disabled_count: usize,
    last_redeemed_at: Option<u64>,
}

impl AdminRedemptionAuditBatch {
    fn from_record(record: &RedemptionAuditBatchRecord) -> Self {
        Self {
            batch: AdminRedemptionBatch::from_record(record.batch(), record.redeemed_count()),
            issued_count: record.issued_count(),
            remaining_count: record.remaining_count(),
            expired_count: record.expired_count(),
            disabled_count: record.disabled_count(),
            last_redeemed_at: record.last_redeemed_at(),
        }
    }

    /// 返回批次固化事实和已兑换数量。
    #[must_use]
    pub const fn batch(&self) -> &AdminRedemptionBatch {
        &self.batch
    }

    /// 返回签发总数。
    #[must_use]
    pub const fn issued_count(&self) -> usize {
        self.issued_count
    }

    /// 返回当前仍可兑换数量。
    #[must_use]
    pub const fn remaining_count(&self) -> usize {
        self.remaining_count
    }

    /// 返回到期失效数量。
    #[must_use]
    pub const fn expired_count(&self) -> usize {
        self.expired_count
    }

    /// 返回停用失效数量。
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

impl fmt::Debug for AdminRedemptionAuditBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRedemptionAuditBatch(<redacted>)")
    }
}

/// 当前返回页的兑换码运营闭合汇总。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRedemptionAuditSummary {
    issued_count: usize,
    redeemed_count: usize,
    remaining_count: usize,
    expired_count: usize,
    disabled_count: usize,
}

impl From<RedemptionAuditSummaryRecord> for AdminRedemptionAuditSummary {
    fn from(record: RedemptionAuditSummaryRecord) -> Self {
        Self {
            issued_count: record.issued_count(),
            redeemed_count: record.redeemed_count(),
            remaining_count: record.remaining_count(),
            expired_count: record.expired_count(),
            disabled_count: record.disabled_count(),
        }
    }
}

impl AdminRedemptionAuditSummary {
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

/// 一页管理员兑换码运营报表。
pub struct AdminRedemptionAuditPage {
    batches: Vec<AdminRedemptionAuditBatch>,
    summary: AdminRedemptionAuditSummary,
    next_cursor: Option<i64>,
}

impl AdminRedemptionAuditPage {
    /// 从仓储页构造已裁剪的管理员视图。
    #[must_use]
    pub fn from_record(record: RedemptionAuditPageRecord) -> Self {
        let (records, summary, next_cursor) = record.into_parts();
        Self {
            batches: records
                .iter()
                .map(AdminRedemptionAuditBatch::from_record)
                .collect(),
            summary: summary.into(),
            next_cursor,
        }
    }

    /// 返回当前页批次统计。
    #[must_use]
    pub fn batches(&self) -> &[AdminRedemptionAuditBatch] {
        &self.batches
    }

    /// 返回当前页闭合汇总。
    #[must_use]
    pub const fn summary(&self) -> AdminRedemptionAuditSummary {
        self.summary
    }

    /// 返回下一页稳定游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminRedemptionAuditPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRedemptionAuditPage(<redacted>)")
    }
}

/// 管理员读取兑换码运营报表的对象安全 Future。
pub type RedemptionAuditFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminRedemptionAuditPage, RedemptionServiceError>> + Send + 'a>,
>;
