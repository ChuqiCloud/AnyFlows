use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminUsageLogRecord, AdminUsageLogRepository, AdminUsageLogRepositoryError,
    MAX_ADMIN_USAGE_LOG_PAGE_SIZE,
};
use af_domain::{GroupId, TokenId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理用量日志列表默认页大小。
pub const DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE: usize = 50;

/// 已校验的管理用量日志列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUsageLogListQuery {
    before: Option<i64>,
    limit: usize,
}

impl AdminUsageLogListQuery {
    /// 校验倒序 ID 游标和固定页大小边界。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, AdminUsageLogReadError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ADMIN_USAGE_LOG_PAGE_SIZE).contains(&limit)
        {
            return Err(AdminUsageLogReadError::InvalidPagination);
        }
        Ok(Self { before, limit })
    }

    /// 返回上一页末尾日志 ID。
    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }

    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for AdminUsageLogListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE,
        }
    }
}

/// 管理 API 使用的稳定计费模式。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminUsageLogBillingMode {
    /// 按 token 计费。
    PerToken,
    /// 显式免费。
    Free,
}

impl AdminUsageLogBillingMode {
    fn from_database(value: i16) -> Result<Self, AdminUsageLogReadError> {
        match value {
            1 => Ok(Self::PerToken),
            2 => Ok(Self::Free),
            _ => Err(AdminUsageLogReadError::Internal),
        }
    }
}

/// 管理 API 使用的稳定用量来源。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminUsageLogSource {
    /// 上游协议明确返回。
    Upstream,
    /// 本地令牌器估算。
    Estimated,
}

impl AdminUsageLogSource {
    fn from_database(value: i16) -> Result<Self, AdminUsageLogReadError> {
        match value {
            1 => Ok(Self::Upstream),
            2 => Ok(Self::Estimated),
            _ => Err(AdminUsageLogReadError::Internal),
        }
    }
}

/// 管理 API 使用的稳定 token 统计口径。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminUsageLogSemantics {
    /// 输入总量已经包含缓存明细。
    Inclusive,
    /// 缓存明细需要独立计入。
    CacheSeparated,
}

impl AdminUsageLogSemantics {
    fn from_database(value: i16) -> Result<Self, AdminUsageLogReadError> {
        match value {
            1 => Ok(Self::Inclusive),
            2 => Ok(Self::CacheSeparated),
            _ => Err(AdminUsageLogReadError::Internal),
        }
    }
}

/// 管理 API 可读取的规范化 token 用量明细。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUsageLogUsage {
    input_tokens: i64,
    output_tokens: i64,
    cache_read: i64,
    cache_creation_5m: i64,
    cache_creation_1h: i64,
    reasoning_tokens: i64,
    audio_input_tokens: i64,
    audio_output_tokens: i64,
}

impl AdminUsageLogUsage {
    /// 返回输入 token 数。
    #[must_use]
    pub const fn input_tokens(self) -> i64 {
        self.input_tokens
    }

    /// 返回输出 token 数。
    #[must_use]
    pub const fn output_tokens(self) -> i64 {
        self.output_tokens
    }

    /// 返回缓存读取 token 数。
    #[must_use]
    pub const fn cache_read(self) -> i64 {
        self.cache_read
    }

    /// 返回 5 分钟缓存创建 token 数。
    #[must_use]
    pub const fn cache_creation_5m(self) -> i64 {
        self.cache_creation_5m
    }

    /// 返回 1 小时缓存创建 token 数。
    #[must_use]
    pub const fn cache_creation_1h(self) -> i64 {
        self.cache_creation_1h
    }

    /// 返回推理 token 数。
    #[must_use]
    pub const fn reasoning_tokens(self) -> i64 {
        self.reasoning_tokens
    }

    /// 返回音频输入 token 数。
    #[must_use]
    pub const fn audio_input_tokens(self) -> i64 {
        self.audio_input_tokens
    }

    /// 返回音频输出 token 数。
    #[must_use]
    pub const fn audio_output_tokens(self) -> i64 {
        self.audio_output_tokens
    }
}

/// 管理 API 可读取的一条用量日志。
pub struct AdminUsageLog {
    id: i64,
    event_id: String,
    user_id: UserId,
    token_id: TokenId,
    group_id: GroupId,
    billing_mode: AdminUsageLogBillingMode,
    usage: AdminUsageLogUsage,
    source: AdminUsageLogSource,
    semantics: AdminUsageLogSemantics,
    quota: i64,
    created_at: i64,
}

impl AdminUsageLog {
    /// 返回日志主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回计费事件标识。
    #[must_use]
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    /// 返回用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回令牌标识。
    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.token_id
    }

    /// 返回实际计费分组标识。
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }

    /// 返回计费模式。
    #[must_use]
    pub const fn billing_mode(&self) -> AdminUsageLogBillingMode {
        self.billing_mode
    }

    /// 返回完整 token 用量明细。
    #[must_use]
    pub const fn usage(&self) -> AdminUsageLogUsage {
        self.usage
    }

    /// 返回用量来源。
    #[must_use]
    pub const fn source(&self) -> AdminUsageLogSource {
        self.source
    }

    /// 返回 token 统计口径。
    #[must_use]
    pub const fn semantics(&self) -> AdminUsageLogSemantics {
        self.semantics
    }

    /// 返回最终扣减额度单位数。
    #[must_use]
    pub const fn quota(&self) -> i64 {
        self.quota
    }

    /// 返回创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    fn from_record(record: AdminUsageLogRecord) -> Result<Self, AdminUsageLogReadError> {
        let usage = record.usage();
        Ok(Self {
            id: record.id(),
            event_id: record.event_id().to_owned(),
            user_id: record.user_id(),
            token_id: record.token_id(),
            group_id: record.group_id(),
            billing_mode: AdminUsageLogBillingMode::from_database(record.billing_mode())?,
            usage: AdminUsageLogUsage {
                input_tokens: usage.input_tokens(),
                output_tokens: usage.output_tokens(),
                cache_read: usage.cache_read(),
                cache_creation_5m: usage.cache_creation_5m(),
                cache_creation_1h: usage.cache_creation_1h(),
                reasoning_tokens: usage.reasoning_tokens(),
                audio_input_tokens: usage.audio_input_tokens(),
                audio_output_tokens: usage.audio_output_tokens(),
            },
            source: AdminUsageLogSource::from_database(record.usage_source())?,
            semantics: AdminUsageLogSemantics::from_database(record.usage_semantics())?,
            quota: record.quota(),
            created_at: record.created_at(),
        })
    }
}

impl fmt::Debug for AdminUsageLog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUsageLog(<redacted>)")
    }
}

/// 一页管理用量日志响应。
pub struct AdminUsageLogPage {
    logs: Vec<AdminUsageLog>,
    next_cursor: Option<i64>,
}

impl AdminUsageLogPage {
    /// 组装日志列表和可选下一游标。
    #[must_use]
    pub fn from_parts(logs: Vec<AdminUsageLog>, next_cursor: Option<i64>) -> Self {
        Self { logs, next_cursor }
    }

    /// 返回当前页日志。
    #[must_use]
    pub fn logs(&self) -> &[AdminUsageLog] {
        &self.logs
    }

    /// 返回下一页游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminUsageLogPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminUsageLogPage(<redacted>)")
    }
}

/// 管理用量日志读取失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUsageLogReadError {
    /// 游标或页大小不满足公开接口。
    #[error("管理用量日志分页参数无效")]
    InvalidPagination,
    /// 当前会话不是管理员。
    #[error("管理用量日志读取权限不足")]
    Forbidden,
    /// 数据库失败或持久化状态损坏。
    #[error("管理用量日志读取内部失败")]
    Internal,
}

/// 管理用量日志列表调用的对象安全 Future。
pub type AdminUsageLogListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminUsageLogPage, AdminUsageLogReadError>> + Send + 'a>>;

/// 管理用量日志只读应用端口；角色校验必须在进入仓储前完成。
pub trait AdminUsageLogReader: Send + Sync {
    /// 读取一页最新用量日志。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUsageLogListQuery,
    ) -> AdminUsageLogListFuture<'a>;
}

/// 使用数据库仓储实现管理员用量日志读取。
pub struct DatabaseAdminUsageLogReader {
    repository: AdminUsageLogRepository,
}

impl DatabaseAdminUsageLogReader {
    /// 绑定已经配置查询截止时间的用量日志仓储。
    #[must_use]
    pub const fn new(repository: AdminUsageLogRepository) -> Self {
        Self { repository }
    }
}

impl AdminUsageLogReader for DatabaseAdminUsageLogReader {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUsageLogListQuery,
    ) -> AdminUsageLogListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list(query.before(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            let logs = records
                .into_iter()
                .map(AdminUsageLog::from_record)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(AdminUsageLogPage::from_parts(logs, next_cursor))
        })
    }
}

impl fmt::Debug for DatabaseAdminUsageLogReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminUsageLogReader(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminUsageLogReadError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminUsageLogReadError::Forbidden)
    }
}

fn map_repository_error(error: AdminUsageLogRepositoryError) -> AdminUsageLogReadError {
    let _ = error;
    AdminUsageLogReadError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pagination_and_role_boundaries_are_closed() {
        assert_eq!(AdminUsageLogListQuery::default().limit(), 50);
        assert_eq!(
            AdminUsageLogListQuery::new(Some(0), 1),
            Err(AdminUsageLogReadError::InvalidPagination)
        );
        assert_eq!(
            AdminUsageLogListQuery::new(None, 0),
            Err(AdminUsageLogReadError::InvalidPagination)
        );
        assert_eq!(
            AdminUsageLogListQuery::new(None, MAX_ADMIN_USAGE_LOG_PAGE_SIZE + 1),
            Err(AdminUsageLogReadError::InvalidPagination)
        );
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminUsageLogReadError::Forbidden)
        );
    }
}
