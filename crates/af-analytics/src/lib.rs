//! 用量记录、聚合与分析投影。

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{AdminDashboardRecord, AdminDashboardRepository, AdminDashboardRepositoryError};
use thiserror::Error;

/// 管理看板固定使用最近 24 小时的持久化用量窗口。
pub const ADMIN_DASHBOARD_PERIOD_SECONDS: i64 = 24 * 60 * 60;

/// 调用看板分析服务时携带的管理权限等级。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminDashboardAccess {
    /// 普通管理会话，不允许读取全局经营指标。
    User,
    /// 管理员会话，可以读取全局经营指标。
    Admin,
}

/// 管理员看板公开的最近 24 小时快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminDashboard {
    period_start: i64,
    period_end: i64,
    request_count: i64,
    quota_consumed: i64,
    upstream_usage_count: i64,
    estimated_usage_count: i64,
    per_token_request_count: i64,
    free_request_count: i64,
    enabled_channel_count: i64,
    disabled_channel_count: i64,
    auto_disabled_channel_count: i64,
}

impl AdminDashboard {
    /// 返回统计窗口起点的 Unix 秒数。
    #[must_use]
    pub const fn period_start(self) -> i64 {
        self.period_start
    }

    /// 返回统计窗口终点的 Unix 秒数。
    #[must_use]
    pub const fn period_end(self) -> i64 {
        self.period_end
    }

    /// 返回窗口内已经确认并落库的请求数。
    #[must_use]
    pub const fn request_count(self) -> i64 {
        self.request_count
    }

    /// 返回窗口内最终消耗的额度单位数。
    #[must_use]
    pub const fn quota_consumed(self) -> i64 {
        self.quota_consumed
    }

    /// 返回由上游响应确认用量的请求数。
    #[must_use]
    pub const fn upstream_usage_count(self) -> i64 {
        self.upstream_usage_count
    }

    /// 返回由本地估算用量的请求数。
    #[must_use]
    pub const fn estimated_usage_count(self) -> i64 {
        self.estimated_usage_count
    }

    /// 返回按 token 计费的请求数。
    #[must_use]
    pub const fn per_token_request_count(self) -> i64 {
        self.per_token_request_count
    }

    /// 返回免费请求数。
    #[must_use]
    pub const fn free_request_count(self) -> i64 {
        self.free_request_count
    }

    /// 返回当前启用的未删除渠道数。
    #[must_use]
    pub const fn enabled_channel_count(self) -> i64 {
        self.enabled_channel_count
    }

    /// 返回当前手动禁用的未删除渠道数。
    #[must_use]
    pub const fn disabled_channel_count(self) -> i64 {
        self.disabled_channel_count
    }

    /// 返回当前自动禁用的未删除渠道数。
    #[must_use]
    pub const fn auto_disabled_channel_count(self) -> i64 {
        self.auto_disabled_channel_count
    }

    fn from_record(period_start: i64, period_end: i64, record: AdminDashboardRecord) -> Self {
        Self {
            period_start,
            period_end,
            request_count: record.request_count(),
            quota_consumed: record.quota_consumed(),
            upstream_usage_count: record.upstream_usage_count(),
            estimated_usage_count: record.estimated_usage_count(),
            per_token_request_count: record.per_token_request_count(),
            free_request_count: record.free_request_count(),
            enabled_channel_count: record.enabled_channel_count(),
            disabled_channel_count: record.disabled_channel_count(),
            auto_disabled_channel_count: record.auto_disabled_channel_count(),
        }
    }
}

/// 管理看板读取错误，不向边界层泄露数据库和统计细节。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminDashboardReadError {
    /// 当前会话不是管理员。
    #[error("需要管理员权限")]
    Forbidden,
    /// 时钟、数据库查询或持久化不变量校验失败。
    #[error("管理看板读取失败")]
    Internal,
}

/// 管理看板异步读取结果。
pub type AdminDashboardReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminDashboard, AdminDashboardReadError>> + Send + 'a>>;

/// 管理看板应用层只读端口。
pub trait AdminDashboardReader: Send + Sync {
    /// 校验管理员权限并读取最近 24 小时的真实快照。
    fn read(&self, access: AdminDashboardAccess) -> AdminDashboardReadFuture<'_>;
}

type DashboardClock = Arc<dyn Fn() -> Result<i64, AdminDashboardReadError> + Send + Sync>;

/// 使用数据库聚合仓储的管理看板读取服务。
#[derive(Clone)]
pub struct DatabaseAdminDashboardReader {
    repository: AdminDashboardRepository,
    clock: DashboardClock,
}

impl DatabaseAdminDashboardReader {
    /// 使用系统 UTC 时钟和真实聚合仓储构造读取服务。
    #[must_use]
    pub fn new(repository: AdminDashboardRepository) -> Self {
        Self {
            repository,
            clock: Arc::new(system_unix_seconds),
        }
    }

    #[cfg(test)]
    fn with_clock(repository: AdminDashboardRepository, clock: DashboardClock) -> Self {
        Self { repository, clock }
    }
}

impl AdminDashboardReader for DatabaseAdminDashboardReader {
    fn read(&self, access: AdminDashboardAccess) -> AdminDashboardReadFuture<'_> {
        Box::pin(async move {
            require_admin(access)?;
            let period_end = (self.clock)()?;
            let (period_start, period_end) = period_ending_at(period_end)?;
            let record = self
                .repository
                .snapshot(period_start, period_end)
                .await
                .map_err(map_repository_error)?;
            Ok(AdminDashboard::from_record(
                period_start,
                period_end,
                record,
            ))
        })
    }
}

impl fmt::Debug for DatabaseAdminDashboardReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminDashboardReader(<redacted>)")
    }
}

fn require_admin(access: AdminDashboardAccess) -> Result<(), AdminDashboardReadError> {
    if access == AdminDashboardAccess::Admin {
        Ok(())
    } else {
        Err(AdminDashboardReadError::Forbidden)
    }
}

fn period_ending_at(period_end: i64) -> Result<(i64, i64), AdminDashboardReadError> {
    let period_start = period_end
        .checked_sub(ADMIN_DASHBOARD_PERIOD_SECONDS)
        .filter(|start| *start >= 0)
        .ok_or(AdminDashboardReadError::Internal)?;
    Ok((period_start, period_end))
}

fn system_unix_seconds() -> Result<i64, AdminDashboardReadError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AdminDashboardReadError::Internal)?;
    i64::try_from(elapsed.as_secs()).map_err(|_| AdminDashboardReadError::Internal)
}

fn map_repository_error(_: AdminDashboardRepositoryError) -> AdminDashboardReadError {
    AdminDashboardReadError::Internal
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use af_db::{AdminDashboardRepository, DatabaseOptions};

    use super::*;

    #[test]
    fn access_and_period_boundaries_fail_closed() {
        assert_eq!(
            require_admin(AdminDashboardAccess::User),
            Err(AdminDashboardReadError::Forbidden)
        );
        assert_eq!(
            period_ending_at(ADMIN_DASHBOARD_PERIOD_SECONDS - 1),
            Err(AdminDashboardReadError::Internal)
        );
        assert_eq!(
            period_ending_at(ADMIN_DASHBOARD_PERIOD_SECONDS),
            Ok((0, ADMIN_DASHBOARD_PERIOD_SECONDS))
        );
    }

    #[tokio::test]
    async fn injected_clock_keeps_the_window_deterministic() {
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            af_db::MigrationOptions::default(),
        )
        .await
        .unwrap();
        let repository =
            AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2)).unwrap();
        let reader = DatabaseAdminDashboardReader::with_clock(
            repository,
            Arc::new(|| Ok(ADMIN_DASHBOARD_PERIOD_SECONDS + 7)),
        );
        let dashboard = reader.read(AdminDashboardAccess::Admin).await.unwrap();
        assert_eq!(dashboard.period_start(), 7);
        assert_eq!(dashboard.period_end(), ADMIN_DASHBOARD_PERIOD_SECONDS + 7);
        assert_eq!(dashboard.request_count(), 0);
        pool.close().await.unwrap();
    }
}
