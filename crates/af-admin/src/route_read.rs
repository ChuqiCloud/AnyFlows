use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminRouteChannelRecord, AdminRouteLookupOutcome, AdminRouteRecord, AdminRouteRepository,
    AdminRouteRepositoryError, MAX_ADMIN_ROUTE_PAGE_SIZE,
};
use af_domain::{ChannelId, CredentialId, RouteChannelId, RouteId, RouteMode, RouteStrategy};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理路由列表默认页大小。
pub const DEFAULT_ADMIN_ROUTE_PAGE_SIZE: usize = 50;

/// 已校验的管理路由列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRouteListQuery {
    after: Option<RouteId>,
    limit: usize,
}

impl AdminRouteListQuery {
    /// 校验稳定 ID 游标与有界页大小。
    pub fn new(after: Option<RouteId>, limit: usize) -> Result<Self, AdminRouteReadError> {
        if !(1..=MAX_ADMIN_ROUTE_PAGE_SIZE).contains(&limit) {
            return Err(AdminRouteReadError::InvalidPagination);
        }
        Ok(Self { after, limit })
    }

    /// 返回上一页最后一个路由 ID。
    #[must_use]
    pub const fn after(self) -> Option<RouteId> {
        self.after
    }

    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminRouteListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ADMIN_ROUTE_PAGE_SIZE,
        }
    }
}

/// 管理 API 可读取的路由候选快照。
pub struct AdminRouteChannel {
    id: RouteChannelId,
    channel_id: ChannelId,
    credential_id: CredentialId,
    priority: i32,
    weight: i32,
    enabled: bool,
    success_count: i64,
    fail_count: i64,
    total_latency: i64,
    cooldown_level: i16,
    cooldown_until_epoch_seconds: Option<i64>,
    last_selected_at_epoch_seconds: Option<i64>,
    last_failure_at_epoch_seconds: Option<i64>,
}

impl AdminRouteChannel {
    /// 返回候选 ID。
    #[must_use]
    pub const fn id(&self) -> RouteChannelId {
        self.id
    }
    /// 返回渠道 ID。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
    /// 返回凭据 ID。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }
    /// 返回优先级。
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }
    /// 返回权重。
    #[must_use]
    pub const fn weight(&self) -> i32 {
        self.weight
    }
    /// 返回启用状态。
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    /// 返回成功次数。
    #[must_use]
    pub const fn success_count(&self) -> i64 {
        self.success_count
    }
    /// 返回失败次数。
    #[must_use]
    pub const fn fail_count(&self) -> i64 {
        self.fail_count
    }
    /// 返回累计延迟。
    #[must_use]
    pub const fn total_latency(&self) -> i64 {
        self.total_latency
    }
    /// 返回冷却级别。
    #[must_use]
    pub const fn cooldown_level(&self) -> i16 {
        self.cooldown_level
    }
    /// 返回冷却截止时间（Unix 秒）。
    #[must_use]
    pub const fn cooldown_until_epoch_seconds(&self) -> Option<i64> {
        self.cooldown_until_epoch_seconds
    }
    /// 返回最近选中时间（Unix 秒）。
    #[must_use]
    pub const fn last_selected_at_epoch_seconds(&self) -> Option<i64> {
        self.last_selected_at_epoch_seconds
    }
    /// 返回最近失败时间（Unix 秒）。
    #[must_use]
    pub const fn last_failure_at_epoch_seconds(&self) -> Option<i64> {
        self.last_failure_at_epoch_seconds
    }

    pub(super) fn from_record(record: AdminRouteChannelRecord) -> Self {
        Self {
            id: record.id(),
            channel_id: record.channel_id(),
            credential_id: record.credential_id(),
            priority: record.priority(),
            weight: record.weight(),
            enabled: record.enabled(),
            success_count: record.success_count(),
            fail_count: record.fail_count(),
            total_latency: record.total_latency(),
            cooldown_level: record.cooldown_level(),
            cooldown_until_epoch_seconds: record
                .cooldown_until()
                .map(|value| value.unix_timestamp()),
            last_selected_at_epoch_seconds: record
                .last_selected_at()
                .map(|value| value.unix_timestamp()),
            last_failure_at_epoch_seconds: record
                .last_failure_at()
                .map(|value| value.unix_timestamp()),
        }
    }
}

impl fmt::Debug for AdminRouteChannel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRouteChannel(<redacted>)")
    }
}

/// 管理 API 可读取的路由规则快照。
pub struct AdminRoute {
    route_id: RouteId,
    name: String,
    model_pattern: String,
    mode: RouteMode,
    strategy: RouteStrategy,
    model_mapping: serde_json::Value,
    enabled: bool,
    channels: Vec<AdminRouteChannel>,
}

impl AdminRoute {
    /// 返回路由 ID。
    #[must_use]
    pub const fn route_id(&self) -> RouteId {
        self.route_id
    }
    /// 返回路由名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// 返回模型匹配表达式。
    #[must_use]
    pub fn model_pattern(&self) -> &str {
        &self.model_pattern
    }
    /// 返回路由模式。
    #[must_use]
    pub const fn mode(&self) -> RouteMode {
        self.mode
    }
    /// 返回选路策略。
    #[must_use]
    pub const fn strategy(&self) -> RouteStrategy {
        self.strategy
    }
    /// 返回模型映射对象。
    #[must_use]
    pub fn model_mapping(&self) -> &serde_json::Value {
        &self.model_mapping
    }
    /// 返回启用状态。
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    /// 返回有序候选。
    #[must_use]
    pub fn channels(&self) -> &[AdminRouteChannel] {
        &self.channels
    }

    /// 组合测试适配器使用的管理快照。
    #[allow(clippy::too_many_arguments, reason = "字段与稳定管理契约一一对应")]
    #[must_use]
    pub fn from_parts(
        route_id: RouteId,
        name: String,
        model_pattern: String,
        mode: RouteMode,
        strategy: RouteStrategy,
        model_mapping: serde_json::Value,
        enabled: bool,
        channels: Vec<AdminRouteChannel>,
    ) -> Self {
        Self {
            route_id,
            name,
            model_pattern,
            mode,
            strategy,
            model_mapping,
            enabled,
            channels,
        }
    }

    pub(super) fn from_record(record: AdminRouteRecord) -> Self {
        Self::from_parts(
            record.route_id(),
            record.name().to_owned(),
            record.model_pattern().to_owned(),
            record.mode(),
            record.strategy(),
            record.model_mapping().clone(),
            record.enabled(),
            record
                .channels()
                .iter()
                .map(|channel| AdminRouteChannel::from_record(channel.clone()))
                .collect(),
        )
    }
}

impl fmt::Debug for AdminRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRoute(<redacted>)")
    }
}

/// 一页管理路由响应。
pub struct AdminRoutePage {
    routes: Vec<AdminRoute>,
    next_cursor: Option<RouteId>,
}

impl AdminRoutePage {
    /// 组合路由列表与下一游标。
    #[must_use]
    pub fn from_parts(routes: Vec<AdminRoute>, next_cursor: Option<RouteId>) -> Self {
        Self {
            routes,
            next_cursor,
        }
    }
    /// 返回本页路由。
    #[must_use]
    pub fn routes(&self) -> &[AdminRoute] {
        &self.routes
    }
    /// 返回下一游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<RouteId> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminRoutePage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRoutePage(<redacted>)")
    }
}

/// 管理路由读取失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminRouteReadError {
    /// 分页参数无效。
    #[error("管理路由分页参数无效")]
    InvalidPagination,
    /// 当前会话不是管理员。
    #[error("管理路由读取权限不足")]
    Forbidden,
    /// 路由不存在或已删除。
    #[error("管理路由不存在")]
    NotFound,
    /// 数据库失败或状态损坏。
    #[error("管理路由读取内部失败")]
    Internal,
}

pub type AdminRouteListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminRoutePage, AdminRouteReadError>> + Send + 'a>>;
pub type AdminRouteGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminRoute, AdminRouteReadError>> + Send + 'a>>;

/// 管理路由只读应用端口。
pub trait AdminRouteReader: Send + Sync {
    /// 读取一页路由。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRouteListQuery,
    ) -> AdminRouteListFuture<'a>;
    /// 读取一个路由及候选。
    fn get<'a>(&'a self, principal: SessionPrincipal, route_id: RouteId)
    -> AdminRouteGetFuture<'a>;
}

/// 数据库路由只读应用端口实现。
pub struct DatabaseAdminRouteReader {
    repository: AdminRouteRepository,
}

impl DatabaseAdminRouteReader {
    /// 绑定路由仓储。
    #[must_use]
    pub const fn new(repository: AdminRouteRepository) -> Self {
        Self { repository }
    }
}

impl AdminRouteReader for DatabaseAdminRouteReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRouteListQuery,
    ) -> AdminRouteListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list(query.after(), query.limit())
                .await
                .map_err(map_error)?;
            let (records, next_cursor) = page.into_parts();
            Ok(AdminRoutePage::from_parts(
                records.into_iter().map(AdminRoute::from_record).collect(),
                next_cursor,
            ))
        })
    }

    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        route_id: RouteId,
    ) -> AdminRouteGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self.repository.get(route_id).await.map_err(map_error)? {
                AdminRouteLookupOutcome::Found(record) => Ok(AdminRoute::from_record(record)),
                AdminRouteLookupOutcome::NotFound => Err(AdminRouteReadError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminRouteReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminRouteReader(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminRouteReadError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(AdminRouteReadError::Forbidden)
}

fn map_error(error: AdminRouteRepositoryError) -> AdminRouteReadError {
    let _ = error;
    AdminRouteReadError::Internal
}

// 管理快照只读使用，避免把数据库实体暴露到应用层。
