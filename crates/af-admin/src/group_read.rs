use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminGroupLookupOutcome, AdminGroupRecord, AdminGroupRepository, AdminGroupRepositoryError,
    MAX_ADMIN_GROUP_PAGE_SIZE,
};
use af_domain::GroupId;
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理分组列表默认页大小。
pub const DEFAULT_ADMIN_GROUP_PAGE_SIZE: usize = 50;

/// 已校验的管理分组列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminGroupListQuery {
    after: Option<GroupId>,
    limit: usize,
}

impl AdminGroupListQuery {
    /// 校验单调 ID 游标和固定页大小边界。
    pub fn new(after: Option<GroupId>, limit: usize) -> Result<Self, AdminGroupReadError> {
        if !(1..=MAX_ADMIN_GROUP_PAGE_SIZE).contains(&limit) {
            return Err(AdminGroupReadError::InvalidPagination);
        }
        Ok(Self { after, limit })
    }

    /// 返回上一页最后一个分组 ID。
    #[must_use]
    pub const fn after(self) -> Option<GroupId> {
        self.after
    }

    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminGroupListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_GROUP_PAGE_SIZE,
        }
    }
}

/// 管理 API 公开的分组高峰倍率窗口。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminGroupPeak {
    ratio_micros: i64,
    start_second: u32,
    end_second: u32,
}

impl AdminGroupPeak {
    /// 组合已经持久化校验的高峰倍率字段。
    #[must_use]
    pub const fn from_parts(ratio_micros: i64, start_second: u32, end_second: u32) -> Self {
        Self {
            ratio_micros,
            start_second,
            end_second,
        }
    }

    /// 返回高峰倍率的百万分比整数值。
    #[must_use]
    pub const fn ratio_micros(self) -> i64 {
        self.ratio_micros
    }

    /// 返回午夜起始的窗口起点秒数。
    #[must_use]
    pub const fn start_second(self) -> u32 {
        self.start_second
    }

    /// 返回午夜起始的窗口终点秒数。
    #[must_use]
    pub const fn end_second(self) -> u32 {
        self.end_second
    }
}

/// 管理 API 可读取的单个 UTC 日历共享额度窗口。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminGroupWindow {
    usage: i64,
    started_at: i64,
    resets_at: i64,
}

impl AdminGroupWindow {
    /// 组合已经由持久化层校验的窗口字段。
    #[must_use]
    pub const fn from_parts(usage: i64, started_at: i64, resets_at: i64) -> Self {
        Self {
            usage,
            started_at,
            resets_at,
        }
    }

    /// 返回当前窗口内已结算的共享额度。
    #[must_use]
    pub const fn usage(self) -> i64 {
        self.usage
    }

    /// 返回当前窗口的 UTC Unix 秒起点。
    #[must_use]
    pub const fn started_at(self) -> i64 {
        self.started_at
    }

    /// 返回当前窗口的 UTC Unix 秒重置时间。
    #[must_use]
    pub const fn resets_at(self) -> i64 {
        self.resets_at
    }
}

/// 管理 API 可读取的分组快照。
pub struct AdminGroup {
    group_id: GroupId,
    name: String,
    display_name: String,
    ratio_micros: i64,
    peak: Option<AdminGroupPeak>,
    is_exclusive: bool,
    daily_limit: Option<i64>,
    weekly_limit: Option<i64>,
    monthly_limit: Option<i64>,
    daily_window: AdminGroupWindow,
    weekly_window: AdminGroupWindow,
    monthly_window: AdminGroupWindow,
    rpm_limit: Option<i32>,
    fallback_group_id: Option<GroupId>,
    flags: serde_json::Value,
}

impl AdminGroup {
    /// 组合已完成持久化校验的分组字段，供仓储适配器和测试实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定管理 API 响应一一对应")]
    #[must_use]
    pub fn from_parts(
        group_id: GroupId,
        name: String,
        display_name: String,
        ratio_micros: i64,
        peak: Option<AdminGroupPeak>,
        is_exclusive: bool,
        daily_limit: Option<i64>,
        weekly_limit: Option<i64>,
        monthly_limit: Option<i64>,
        daily_window: AdminGroupWindow,
        weekly_window: AdminGroupWindow,
        monthly_window: AdminGroupWindow,
        rpm_limit: Option<i32>,
        fallback_group_id: Option<GroupId>,
        flags: serde_json::Value,
    ) -> Self {
        Self {
            group_id,
            name,
            display_name,
            ratio_micros,
            peak,
            is_exclusive,
            daily_limit,
            weekly_limit,
            monthly_limit,
            daily_window,
            weekly_window,
            monthly_window,
            rpm_limit,
            fallback_group_id,
            flags,
        }
    }

    /// 返回分组标识。
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }

    /// 返回稳定分组名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回分组显示名。
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 返回基础倍率的百万分比整数值。
    #[must_use]
    pub const fn ratio_micros(&self) -> i64 {
        self.ratio_micros
    }

    /// 返回可选高峰倍率窗口。
    #[must_use]
    pub const fn peak(&self) -> Option<AdminGroupPeak> {
        self.peak
    }

    /// 返回是否为独占分组。
    #[must_use]
    pub const fn is_exclusive(&self) -> bool {
        self.is_exclusive
    }

    /// 返回每日额度上限。
    #[must_use]
    pub const fn daily_limit(&self) -> Option<i64> {
        self.daily_limit
    }

    /// 返回每周额度上限。
    #[must_use]
    pub const fn weekly_limit(&self) -> Option<i64> {
        self.weekly_limit
    }

    /// 返回每月额度上限。
    #[must_use]
    pub const fn monthly_limit(&self) -> Option<i64> {
        self.monthly_limit
    }

    /// 返回 UTC 自然日共享额度窗口。
    #[must_use]
    pub const fn daily_window(&self) -> AdminGroupWindow {
        self.daily_window
    }

    /// 返回 UTC 周一开始自然周共享额度窗口。
    #[must_use]
    pub const fn weekly_window(&self) -> AdminGroupWindow {
        self.weekly_window
    }

    /// 返回 UTC 自然月共享额度窗口。
    #[must_use]
    pub const fn monthly_window(&self) -> AdminGroupWindow {
        self.monthly_window
    }

    /// 返回分组级 RPM 限制。
    #[must_use]
    pub const fn rpm_limit(&self) -> Option<i32> {
        self.rpm_limit
    }

    /// 返回客户端限制降级目标。
    #[must_use]
    pub const fn fallback_group_id(&self) -> Option<GroupId> {
        self.fallback_group_id
    }

    /// 返回分组开关对象。
    #[must_use]
    pub const fn flags(&self) -> &serde_json::Value {
        &self.flags
    }

    pub(super) fn from_record(record: AdminGroupRecord) -> Self {
        let peak = record.peak().map(|peak| {
            AdminGroupPeak::from_parts(peak.ratio_micros(), peak.start_second(), peak.end_second())
        });
        let daily_window = record.daily_window();
        let weekly_window = record.weekly_window();
        let monthly_window = record.monthly_window();
        Self::from_parts(
            record.group_id(),
            record.name().to_owned(),
            record.display_name().to_owned(),
            record.ratio_micros(),
            peak,
            record.is_exclusive(),
            record.daily_limit(),
            record.weekly_limit(),
            record.monthly_limit(),
            AdminGroupWindow::from_parts(
                daily_window.usage(),
                daily_window.started_at(),
                daily_window.resets_at(),
            ),
            AdminGroupWindow::from_parts(
                weekly_window.usage(),
                weekly_window.started_at(),
                weekly_window.resets_at(),
            ),
            AdminGroupWindow::from_parts(
                monthly_window.usage(),
                monthly_window.started_at(),
                monthly_window.resets_at(),
            ),
            record.rpm_limit(),
            record.fallback_group_id(),
            record.flags().clone(),
        )
    }
}

impl fmt::Debug for AdminGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroup(<redacted>)")
    }
}

/// 一页管理分组响应。
pub struct AdminGroupPage {
    groups: Vec<AdminGroup>,
    next_cursor: Option<GroupId>,
}

impl AdminGroupPage {
    /// 组合分组列表和可选下一游标。
    #[must_use]
    pub fn from_parts(groups: Vec<AdminGroup>, next_cursor: Option<GroupId>) -> Self {
        Self {
            groups,
            next_cursor,
        }
    }

    /// 返回当前页分组。
    #[must_use]
    pub fn groups(&self) -> &[AdminGroup] {
        &self.groups
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<GroupId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminGroupPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminGroupPage(<redacted>)")
    }
}

/// 管理分组读取失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminGroupReadError {
    /// 游标或页大小不满足公开边界。
    #[error("管理分组分页参数无效")]
    InvalidPagination,
    /// 当前会话不是管理员。
    #[error("管理分组读取权限不足")]
    Forbidden,
    /// 分组不存在或已经软删除。
    #[error("管理分组不存在")]
    NotFound,
    /// 数据库失败或持久化状态损坏。
    #[error("管理分组读取内部失败")]
    Internal,
}

/// 管理分组列表调用的对象安全 Future。
pub type AdminGroupListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminGroupPage, AdminGroupReadError>> + Send + 'a>>;

/// 管理分组详情调用的对象安全 Future。
pub type AdminGroupGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminGroup, AdminGroupReadError>> + Send + 'a>>;

/// 管理分组只读应用端口；角色校验必须在进入仓储前完成。
pub trait AdminGroupReader: Send + Sync {
    /// 读取一页分组。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a>;

    /// 按分组 ID 读取详情。
    fn get<'a>(&'a self, principal: SessionPrincipal, group_id: GroupId)
    -> AdminGroupGetFuture<'a>;
}

/// 使用数据库仓储实现管理员分组读取。
pub struct DatabaseAdminGroupReader {
    repository: AdminGroupRepository,
}

impl DatabaseAdminGroupReader {
    /// 绑定已配置查询截止时间的分组仓储。
    #[must_use]
    pub const fn new(repository: AdminGroupRepository) -> Self {
        Self { repository }
    }
}

impl AdminGroupReader for DatabaseAdminGroupReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list(query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            let groups = records.into_iter().map(AdminGroup::from_record).collect();
            Ok(AdminGroupPage::from_parts(groups, next_cursor))
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> AdminGroupGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .get(group_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminGroupLookupOutcome::Found(record) => Ok(AdminGroup::from_record(*record)),
                AdminGroupLookupOutcome::NotFound => Err(AdminGroupReadError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminGroupReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminGroupReader(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminGroupReadError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminGroupReadError::Forbidden)
    }
}

fn map_repository_error(error: AdminGroupRepositoryError) -> AdminGroupReadError {
    let _ = error;
    AdminGroupReadError::Internal
}

#[cfg(test)]
mod tests {
    use af_domain::UserId;

    use super::*;

    #[test]
    fn pagination_and_sensitive_debug_contract_are_closed() {
        assert_eq!(AdminGroupListQuery::default().limit(), 50);
        assert_eq!(
            AdminGroupListQuery::new(None, 0),
            Err(AdminGroupReadError::InvalidPagination)
        );
        assert_eq!(
            AdminGroupListQuery::new(None, MAX_ADMIN_GROUP_PAGE_SIZE + 1),
            Err(AdminGroupReadError::InvalidPagination)
        );
        let group = AdminGroup::from_parts(
            GroupId::new(1).unwrap(),
            "private-group".to_owned(),
            "Private Group".to_owned(),
            1_000_000,
            None,
            false,
            None,
            None,
            None,
            AdminGroupWindow::from_parts(0, 0, 1),
            AdminGroupWindow::from_parts(0, 0, 1),
            AdminGroupWindow::from_parts(0, 0, 1),
            None,
            None,
            serde_json::json!({"internal_switch": true}),
        );
        assert_eq!(format!("{group:?}"), "AdminGroup(<redacted>)");
    }

    #[test]
    fn normal_user_is_rejected_before_repository_access() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminGroupReadError::Forbidden)
        );
    }
}
