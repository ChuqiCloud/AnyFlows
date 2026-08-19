use std::{future::Future, pin::Pin};

use thiserror::Error;

use crate::{
    AdminDashboardChannelFlow, AdminDashboardFailure, AdminDashboardHourlyPoint,
    AdminDashboardPerformance,
};

/// 分析存储返回的已确认用量汇总。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardUsageSnapshot {
    pub(crate) request_count: i64,
    pub(crate) quota_consumed: i64,
    pub(crate) upstream_usage_count: i64,
    pub(crate) estimated_usage_count: i64,
    pub(crate) per_token_request_count: i64,
    pub(crate) per_call_request_count: i64,
    pub(crate) free_request_count: i64,
}

impl AdminDashboardUsageSnapshot {
    /// 组合分析存储在同一半开窗口内计算的用量汇总。
    #[must_use]
    pub const fn new(
        request_count: i64,
        quota_consumed: i64,
        upstream_usage_count: i64,
        estimated_usage_count: i64,
        per_token_request_count: i64,
        per_call_request_count: i64,
        free_request_count: i64,
    ) -> Self {
        Self {
            request_count,
            quota_consumed,
            upstream_usage_count,
            estimated_usage_count,
            per_token_request_count,
            per_call_request_count,
            free_request_count,
        }
    }
}

/// 主库返回的当前渠道状态汇总。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboardChannelSnapshot {
    pub(crate) enabled_channel_count: i64,
    pub(crate) disabled_channel_count: i64,
    pub(crate) auto_disabled_channel_count: i64,
}

impl AdminDashboardChannelSnapshot {
    /// 组合当前未删除渠道的三类闭合状态计数。
    #[must_use]
    pub const fn new(
        enabled_channel_count: i64,
        disabled_channel_count: i64,
        auto_disabled_channel_count: i64,
    ) -> Self {
        Self {
            enabled_channel_count,
            disabled_channel_count,
            auto_disabled_channel_count,
        }
    }
}

/// 分析存储返回的请求终态汇总。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardOutcomeSnapshot {
    pub(crate) request_count: i64,
    pub(crate) successful_request_count: i64,
    pub(crate) failed_request_count: i64,
    pub(crate) other_success_count: i64,
    pub(crate) failures: Vec<AdminDashboardFailure>,
    pub(crate) channel_flows: Vec<AdminDashboardChannelFlow>,
}

impl AdminDashboardOutcomeSnapshot {
    /// 组合请求终态、失败分类和可见成功流向。
    #[must_use]
    pub fn new(
        request_count: i64,
        successful_request_count: i64,
        failed_request_count: i64,
        other_success_count: i64,
        failures: Vec<AdminDashboardFailure>,
        channel_flows: Vec<AdminDashboardChannelFlow>,
    ) -> Self {
        Self {
            request_count,
            successful_request_count,
            failed_request_count,
            other_success_count,
            failures,
            channel_flows,
        }
    }
}

/// 一个时间窗口内的完整分析读取快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminDashboardStorageSnapshot {
    pub(crate) usage: AdminDashboardUsageSnapshot,
    pub(crate) channels: AdminDashboardChannelSnapshot,
    pub(crate) outcomes: AdminDashboardOutcomeSnapshot,
    pub(crate) hourly: Vec<AdminDashboardHourlyPoint>,
    pub(crate) performance: AdminDashboardPerformance,
}

impl AdminDashboardStorageSnapshot {
    /// 组合用量、终态、当前渠道状态和性能观测。
    #[must_use]
    pub fn new(
        usage: AdminDashboardUsageSnapshot,
        channels: AdminDashboardChannelSnapshot,
        outcomes: AdminDashboardOutcomeSnapshot,
        hourly: Vec<AdminDashboardHourlyPoint>,
        performance: AdminDashboardPerformance,
    ) -> Self {
        Self {
            usage,
            channels,
            outcomes,
            hourly,
            performance,
        }
    }
}

/// 分析读取存储端口的闭合错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDashboardStorageError {
    /// 分析存储或主库当前不可读。
    #[error("管理看板存储暂不可用")]
    Unavailable,
    /// 存储返回的聚合事实违反闭合不变量。
    #[error("管理看板存储快照损坏")]
    Invariant,
}

/// 分析读取存储端口的异步结果。
pub type AdminDashboardStorageFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminDashboardStorageSnapshot, AdminDashboardStorageError>>
            + Send
            + 'a,
    >,
>;

/// 管理看板使用的可替换分析读取存储端口。
pub trait AdminDashboardStorage: Send + Sync {
    /// 读取指定半开时间窗口；实现必须同时提供主库权威的当前渠道状态。
    fn snapshot(&self, period_start: i64, period_end: i64) -> AdminDashboardStorageFuture<'_>;
}
