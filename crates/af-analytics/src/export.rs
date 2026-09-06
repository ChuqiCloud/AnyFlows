use std::{future::Future, pin::Pin};

use thiserror::Error;

/// ClickHouse 异步事实 outbox 的只读健康快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalyticsExportQueueSnapshot {
    pending_count: u64,
    leased_count: u64,
    published_count: u64,
}

impl AnalyticsExportQueueSnapshot {
    /// 组合数据库返回的三类状态计数。
    #[must_use]
    pub const fn new(pending_count: u64, leased_count: u64, published_count: u64) -> Self {
        Self {
            pending_count,
            leased_count,
            published_count,
        }
    }

    #[must_use]
    pub const fn pending_count(self) -> u64 {
        self.pending_count
    }

    #[must_use]
    pub const fn leased_count(self) -> u64 {
        self.leased_count
    }

    #[must_use]
    pub const fn published_count(self) -> u64 {
        self.published_count
    }

    /// 返回尚未发布的积压总量。
    #[must_use]
    pub const fn backlog_count(self) -> u64 {
        self.pending_count.saturating_add(self.leased_count)
    }
}

/// 分析导出运维端口的闭合错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AnalyticsExportControlError {
    /// 主库不可读或操作超时。
    #[error("分析导出状态暂不可用")]
    Unavailable,
    /// outbox 状态违反闭合不变量。
    #[error("分析导出状态损坏")]
    Invariant,
}

/// 分析导出状态读取异步结果。
pub type AnalyticsExportStatusFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AnalyticsExportQueueSnapshot, AnalyticsExportControlError>>
            + Send
            + 'a,
    >,
>;

/// 分析导出重放异步结果。
pub type AnalyticsExportReplayFuture<'a> =
    Pin<Box<dyn Future<Output = Result<u64, AnalyticsExportControlError>> + Send + 'a>>;

/// 管理端使用的分析导出运维端口；实现不得直接暴露凭据或 ClickHouse 原始响应。
pub trait AnalyticsExportControl: Send + Sync {
    /// 读取 outbox 状态计数。
    fn status(&self) -> AnalyticsExportStatusFuture<'_>;

    /// 有界重排待处理或已过期租约，并返回本次推进数量。
    fn replay(&self, limit: u16) -> AnalyticsExportReplayFuture<'_>;
}
