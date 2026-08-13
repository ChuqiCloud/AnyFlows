use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminUsageLogRecord, AdminUsageLogRepository, AdminUsageLogRepositoryError,
    MAX_ADMIN_USAGE_LOG_PAGE_SIZE, UsageLogVideoResolution,
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
    /// 按请求次数或视频时长等业务维度直接计费。
    PerCall,
}

impl AdminUsageLogBillingMode {
    fn from_database(value: i16) -> Result<Self, AdminUsageLogReadError> {
        match value {
            1 => Ok(Self::PerToken),
            2 => Ok(Self::Free),
            3 => Ok(Self::PerCall),
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

/// 管理 API 使用的稳定视频分辨率档位。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AdminUsageLogVideoResolution {
    /// 480p 输出。
    #[serde(rename = "480p")]
    P480,
    /// 720p 输出。
    #[serde(rename = "720p")]
    P720,
    /// 1080p 输出。
    #[serde(rename = "1080p")]
    P1080,
}

/// 调用日志公开的客户端入口协议。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageLogProtocol {
    OpenAiChat,
    OpenAiResponses,
    OpenAiEmbeddings,
    OpenAiImages,
    OpenAiAudio,
    OpenAiSpeech,
    JinaRerank,
    CohereRerank,
    XaiVideo,
    Anthropic,
    Gemini,
}

impl UsageLogProtocol {
    fn from_database(value: i16) -> Result<Self, AdminUsageLogReadError> {
        match value {
            1 => Ok(Self::OpenAiChat),
            2 => Ok(Self::OpenAiResponses),
            3 => Ok(Self::OpenAiEmbeddings),
            4 => Ok(Self::OpenAiImages),
            5 => Ok(Self::OpenAiAudio),
            6 => Ok(Self::OpenAiSpeech),
            7 => Ok(Self::JinaRerank),
            8 => Ok(Self::CohereRerank),
            9 => Ok(Self::XaiVideo),
            10 => Ok(Self::Anthropic),
            11 => Ok(Self::Gemini),
            _ => Err(AdminUsageLogReadError::Internal),
        }
    }
}

/// 调用日志公开的规范化操作类型。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageLogOperation {
    Chat,
    Responses,
    ResponsesCompact,
    Embedding,
    Image,
    Audio,
    Rerank,
    Video,
    CountTokens,
}

impl UsageLogOperation {
    fn from_database(value: i16) -> Result<Self, AdminUsageLogReadError> {
        match value {
            1 => Ok(Self::Chat),
            2 => Ok(Self::Responses),
            3 => Ok(Self::ResponsesCompact),
            4 => Ok(Self::Embedding),
            5 => Ok(Self::Image),
            6 => Ok(Self::Audio),
            7 => Ok(Self::Rerank),
            8 => Ok(Self::Video),
            9 => Ok(Self::CountTokens),
            _ => Err(AdminUsageLogReadError::Internal),
        }
    }
}

/// 调用日志公开的规范化思考等级。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageLogReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    ExtraHigh,
    Max,
}

impl UsageLogReasoningEffort {
    fn from_database(value: i16) -> Result<Self, AdminUsageLogReadError> {
        match value {
            1 => Ok(Self::None),
            2 => Ok(Self::Minimal),
            3 => Ok(Self::Low),
            4 => Ok(Self::Medium),
            5 => Ok(Self::High),
            6 => Ok(Self::ExtraHigh),
            7 => Ok(Self::Max),
            _ => Err(AdminUsageLogReadError::Internal),
        }
    }
}

impl From<UsageLogVideoResolution> for AdminUsageLogVideoResolution {
    fn from(value: UsageLogVideoResolution) -> Self {
        match value {
            UsageLogVideoResolution::P480 => Self::P480,
            UsageLogVideoResolution::P720 => Self::P720,
            UsageLogVideoResolution::P1080 => Self::P1080,
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
    audio_duration_nanoseconds: Option<i64>,
    video_duration_seconds: Option<i64>,
    video_resolution: Option<AdminUsageLogVideoResolution>,
    request_id: Option<String>,
    model: Option<String>,
    protocol: Option<UsageLogProtocol>,
    operation: Option<UsageLogOperation>,
    is_stream: Option<bool>,
    reasoning_effort: Option<UsageLogReasoningEffort>,
    reasoning_budget_tokens: Option<i64>,
    first_token_ms: Option<i64>,
    duration_ms: Option<i64>,
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

    /// 返回可选的音频时长纳秒事实。
    #[must_use]
    pub const fn audio_duration_nanoseconds(&self) -> Option<i64> {
        self.audio_duration_nanoseconds
    }

    /// 返回可选的视频真实时长秒数。
    #[must_use]
    pub const fn video_duration_seconds(&self) -> Option<i64> {
        self.video_duration_seconds
    }

    /// 返回可选的视频分辨率档位。
    #[must_use]
    pub const fn video_resolution(&self) -> Option<AdminUsageLogVideoResolution> {
        self.video_resolution
    }

    /// 返回公开请求标识。
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    /// 返回客户端请求模型。
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    /// 返回客户端入口协议。
    #[must_use]
    pub const fn protocol(&self) -> Option<UsageLogProtocol> {
        self.protocol
    }

    /// 返回规范化操作类型。
    #[must_use]
    pub const fn operation(&self) -> Option<UsageLogOperation> {
        self.operation
    }

    /// 返回客户端是否请求流式响应。
    #[must_use]
    pub const fn is_stream(&self) -> Option<bool> {
        self.is_stream
    }

    /// 返回规范化思考等级。
    #[must_use]
    pub const fn reasoning_effort(&self) -> Option<UsageLogReasoningEffort> {
        self.reasoning_effort
    }

    /// 返回思考 token 预算。
    #[must_use]
    pub const fn reasoning_budget_tokens(&self) -> Option<i64> {
        self.reasoning_budget_tokens
    }

    /// 返回流式首个可交付事件耗时。
    #[must_use]
    pub const fn first_token_ms(&self) -> Option<i64> {
        self.first_token_ms
    }

    /// 返回取得成功终态的总耗时。
    #[must_use]
    pub const fn duration_ms(&self) -> Option<i64> {
        self.duration_ms
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
            audio_duration_nanoseconds: record.audio_duration_nanoseconds(),
            video_duration_seconds: record.video_duration_seconds(),
            video_resolution: record.video_resolution().map(Into::into),
            request_id: record.request_id().map(str::to_owned),
            model: record.model().map(str::to_owned),
            protocol: record
                .protocol()
                .map(UsageLogProtocol::from_database)
                .transpose()?,
            operation: record
                .operation()
                .map(UsageLogOperation::from_database)
                .transpose()?,
            is_stream: record.is_stream(),
            reasoning_effort: record
                .reasoning_effort()
                .map(UsageLogReasoningEffort::from_database)
                .transpose()?,
            reasoning_budget_tokens: record.reasoning_budget_tokens(),
            first_token_ms: record.first_token_ms(),
            duration_ms: record.duration_ms(),
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

    /// 读取当前会话用户自己的最新调用日志；管理员调用时同样只返回本人。
    fn list_own<'a>(
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

    fn list_own<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUsageLogListQuery,
    ) -> AdminUsageLogListFuture<'a> {
        Box::pin(async move {
            let page = self
                .repository
                .list_for_user(principal.user_id(), query.before(), query.limit())
                .await
                .map_err(map_repository_error)?;
            map_page(page)
        })
    }
}

fn map_page(
    page: af_db::AdminUsageLogPageRecord,
) -> Result<AdminUsageLogPage, AdminUsageLogReadError> {
    let (records, next_cursor) = page.into_parts();
    let logs = records
        .into_iter()
        .map(AdminUsageLog::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AdminUsageLogPage::from_parts(logs, next_cursor))
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
