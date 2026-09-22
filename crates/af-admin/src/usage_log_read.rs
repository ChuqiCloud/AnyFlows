use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminUsageLogRecord, AdminUsageLogRepository, AdminUsageLogRepositoryError,
    MAX_ADMIN_USAGE_LOG_PAGE_SIZE, RequestFailureKind, RequestOutcomePageRecord,
    RequestOutcomeRecord, RequestOutcomeRepository, RequestOutcomeRepositoryError,
    UsageLogVideoResolution,
};
use af_domain::{
    ChannelId, GroupId, Operation, OrganizationId, OrganizationTeamId, Protocol, TokenId, UserId,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理用量日志列表默认页大小。
pub const DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE: usize = 50;

/// 已校验的管理用量日志列表查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminUsageLogListQuery {
    before: Option<i64>,
    failed_before: Option<i64>,
    limit: usize,
}

impl AdminUsageLogListQuery {
    /// 校验倒序 ID 游标和固定页大小边界。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, AdminUsageLogReadError> {
        Self::new_with_failed_before(before, None, limit)
    }

    /// 同时校验成功和失败事实各自使用的倒序游标。
    pub fn new_with_failed_before(
        before: Option<i64>,
        failed_before: Option<i64>,
        limit: usize,
    ) -> Result<Self, AdminUsageLogReadError> {
        if before.is_some_and(|value| value <= 0)
            || failed_before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ADMIN_USAGE_LOG_PAGE_SIZE).contains(&limit)
        {
            return Err(AdminUsageLogReadError::InvalidPagination);
        }
        Ok(Self {
            before,
            failed_before,
            limit,
        })
    }

    /// 返回上一页末尾日志 ID。
    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }

    /// 返回失败事实上一页末尾的日志 ID。
    #[must_use]
    pub const fn failed_before(self) -> Option<i64> {
        self.failed_before
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
            failed_before: None,
            limit: DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE,
        }
    }
}

/// 管理 API 使用的稳定计费模式。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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
    username: String,
    token_id: TokenId,
    group_id: GroupId,
    organization_id: Option<OrganizationId>,
    organization_team_id: Option<OrganizationTeamId>,
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

    /// 返回调用所属用户的当前用户名。
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
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

    /// 返回调用固化的企业归属；个人调用为空。
    #[must_use]
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }

    /// 返回调用固化的企业团队归属。
    #[must_use]
    pub const fn organization_team_id(&self) -> Option<OrganizationTeamId> {
        self.organization_team_id
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

    pub(crate) fn from_record(record: AdminUsageLogRecord) -> Result<Self, AdminUsageLogReadError> {
        let usage = record.usage();
        Ok(Self {
            id: record.id(),
            event_id: record.event_id().to_owned(),
            user_id: record.user_id(),
            username: record.username().to_owned(),
            token_id: record.token_id(),
            group_id: record.group_id(),
            organization_id: record.organization_id(),
            organization_team_id: record.organization_team_id(),
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

/// 管理员或企业管理角色可读取的一条失败调用明细。
pub struct AdminFailedCallLog {
    id: i64,
    request_id: String,
    model: String,
    protocol: UsageLogProtocol,
    operation: UsageLogOperation,
    error_kind: RequestFailureKind,
    error_code: String,
    error_message: String,
    user_id: Option<UserId>,
    username: Option<String>,
    token_id: Option<TokenId>,
    group_id: Option<GroupId>,
    organization_id: Option<OrganizationId>,
    organization_team_id: Option<OrganizationTeamId>,
    channel_id: Option<ChannelId>,
    duration_ms: i64,
    created_at: i64,
}

impl AdminFailedCallLog {
    pub(crate) fn from_record(
        record: RequestOutcomeRecord,
    ) -> Result<Self, AdminUsageLogReadError> {
        Ok(Self {
            id: record.id(),
            request_id: record.request_id().to_owned(),
            model: record.model().to_owned(),
            protocol: protocol_from_domain(record.protocol())?,
            operation: operation_from_domain(record.operation())?,
            error_kind: record.failure_kind(),
            error_code: record.public_error_code().to_owned(),
            error_message: record.public_error_message().to_owned(),
            user_id: record.user_id(),
            username: record.username().map(str::to_owned),
            token_id: record.token_id(),
            group_id: record.group_id(),
            organization_id: record.organization_id(),
            organization_team_id: record.organization_team_id(),
            channel_id: record.channel_id(),
            duration_ms: record.duration_ms(),
            created_at: record.created_at(),
        })
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
    #[must_use]
    pub const fn protocol(&self) -> UsageLogProtocol {
        self.protocol
    }
    #[must_use]
    pub const fn operation(&self) -> UsageLogOperation {
        self.operation
    }
    #[must_use]
    pub const fn error_kind(&self) -> RequestFailureKind {
        self.error_kind
    }
    #[must_use]
    pub fn error_code(&self) -> &str {
        &self.error_code
    }
    #[must_use]
    pub fn error_message(&self) -> &str {
        &self.error_message
    }
    #[must_use]
    pub const fn user_id(&self) -> Option<UserId> {
        self.user_id
    }
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }
    #[must_use]
    pub const fn token_id(&self) -> Option<TokenId> {
        self.token_id
    }
    #[must_use]
    pub const fn group_id(&self) -> Option<GroupId> {
        self.group_id
    }
    #[must_use]
    pub const fn organization_id(&self) -> Option<OrganizationId> {
        self.organization_id
    }
    #[must_use]
    pub const fn organization_team_id(&self) -> Option<OrganizationTeamId> {
        self.organization_team_id
    }
    #[must_use]
    pub const fn channel_id(&self) -> Option<ChannelId> {
        self.channel_id
    }
    #[must_use]
    pub const fn duration_ms(&self) -> i64 {
        self.duration_ms
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for AdminFailedCallLog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminFailedCallLog(<redacted>)")
    }
}

/// 一页失败调用管理明细。
pub struct AdminFailedCallLogPage {
    logs: Vec<AdminFailedCallLog>,
    next_cursor: Option<i64>,
}

impl AdminFailedCallLogPage {
    #[must_use]
    pub fn from_parts(logs: Vec<AdminFailedCallLog>, next_cursor: Option<i64>) -> Self {
        Self { logs, next_cursor }
    }

    #[must_use]
    pub fn logs(&self) -> &[AdminFailedCallLog] {
        &self.logs
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }

    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminFailedCallLog>, Option<i64>) {
        (self.logs, self.next_cursor)
    }
}

impl fmt::Debug for AdminFailedCallLogPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminFailedCallLogPage(<redacted>)")
    }
}

/// 普通用户可见的失败调用明细，不包含主体、渠道和内部分类。
pub struct UserFailedCallLog {
    id: i64,
    request_id: String,
    model: String,
    protocol: UsageLogProtocol,
    operation: UsageLogOperation,
    error_code: String,
    error_message: String,
    duration_ms: i64,
    created_at: i64,
}

impl UserFailedCallLog {
    pub(crate) fn from_admin(log: &AdminFailedCallLog) -> Self {
        Self {
            id: log.id,
            request_id: log.request_id.clone(),
            model: log.model.clone(),
            protocol: log.protocol,
            operation: log.operation,
            // Re-derive the public view from the closed category so a malformed
            // or legacy stored detail can never expose internal wording here.
            error_code: log.error_kind.public_error_code().to_owned(),
            error_message: log.error_kind.public_error_message().to_owned(),
            duration_ms: log.duration_ms,
            created_at: log.created_at,
        }
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
    #[must_use]
    pub const fn protocol(&self) -> UsageLogProtocol {
        self.protocol
    }
    #[must_use]
    pub const fn operation(&self) -> UsageLogOperation {
        self.operation
    }
    #[must_use]
    pub fn error_code(&self) -> &str {
        &self.error_code
    }
    #[must_use]
    pub fn error_message(&self) -> &str {
        &self.error_message
    }
    #[must_use]
    pub const fn duration_ms(&self) -> i64 {
        self.duration_ms
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl fmt::Debug for UserFailedCallLog {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserFailedCallLog(<redacted>)")
    }
}

/// 普通用户失败调用列表页。
pub struct UserFailedCallLogPage {
    logs: Vec<UserFailedCallLog>,
    next_cursor: Option<i64>,
}

impl UserFailedCallLogPage {
    #[must_use]
    pub fn from_admin_page(page: AdminFailedCallLogPage) -> Self {
        let AdminFailedCallLogPage { logs, next_cursor } = page;
        Self {
            logs: logs.iter().map(UserFailedCallLog::from_admin).collect(),
            next_cursor,
        }
    }

    #[must_use]
    pub fn logs(&self) -> &[UserFailedCallLog] {
        &self.logs
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for UserFailedCallLogPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserFailedCallLogPage(<redacted>)")
    }
}

fn protocol_from_domain(value: Protocol) -> Result<UsageLogProtocol, AdminUsageLogReadError> {
    match value {
        Protocol::OpenAiChat => Ok(UsageLogProtocol::OpenAiChat),
        Protocol::OpenAiResponses => Ok(UsageLogProtocol::OpenAiResponses),
        Protocol::OpenAiEmbeddings => Ok(UsageLogProtocol::OpenAiEmbeddings),
        Protocol::OpenAiImages => Ok(UsageLogProtocol::OpenAiImages),
        Protocol::OpenAiAudio => Ok(UsageLogProtocol::OpenAiAudio),
        Protocol::OpenAiSpeech => Ok(UsageLogProtocol::OpenAiSpeech),
        Protocol::JinaRerank => Ok(UsageLogProtocol::JinaRerank),
        Protocol::CohereRerank => Ok(UsageLogProtocol::CohereRerank),
        Protocol::XaiVideo => Ok(UsageLogProtocol::XaiVideo),
        Protocol::Anthropic => Ok(UsageLogProtocol::Anthropic),
        Protocol::Gemini => Ok(UsageLogProtocol::Gemini),
    }
}

fn operation_from_domain(value: Operation) -> Result<UsageLogOperation, AdminUsageLogReadError> {
    match value {
        Operation::Chat => Ok(UsageLogOperation::Chat),
        Operation::Responses => Ok(UsageLogOperation::Responses),
        Operation::ResponsesCompact => Ok(UsageLogOperation::ResponsesCompact),
        Operation::Embedding => Ok(UsageLogOperation::Embedding),
        Operation::Image => Ok(UsageLogOperation::Image),
        Operation::Audio => Ok(UsageLogOperation::Audio),
        Operation::Rerank => Ok(UsageLogOperation::Rerank),
        Operation::Video => Ok(UsageLogOperation::Video),
        Operation::CountTokens => Ok(UsageLogOperation::CountTokens),
    }
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

/// 管理端失败调用列表的对象安全 Future。
pub type AdminFailedCallLogListFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminFailedCallLogPage, AdminUsageLogReadError>> + Send + 'a>,
>;

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

    /// 读取管理员可见的失败调用明细；未接入失败仓储的兼容实现返回空页。
    fn list_failed<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminUsageLogListQuery,
    ) -> AdminFailedCallLogListFuture<'a> {
        Box::pin(async { Ok(AdminFailedCallLogPage::from_parts(Vec::new(), None)) })
    }

    /// 读取当前会话用户自己的失败调用明细。
    fn list_own_failed<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminUsageLogListQuery,
    ) -> AdminFailedCallLogListFuture<'a> {
        Box::pin(async { Ok(AdminFailedCallLogPage::from_parts(Vec::new(), None)) })
    }
}

/// 使用数据库仓储实现管理员用量日志读取。
pub struct DatabaseAdminUsageLogReader {
    repository: AdminUsageLogRepository,
    request_outcomes: Option<RequestOutcomeRepository>,
}

impl DatabaseAdminUsageLogReader {
    /// 绑定已经配置查询截止时间的用量日志仓储。
    #[must_use]
    pub const fn new(repository: AdminUsageLogRepository) -> Self {
        Self {
            repository,
            request_outcomes: None,
        }
    }

    /// 注入失败调用事实仓储；保留旧构造函数以兼容轻量测试实现。
    #[must_use]
    pub fn with_request_outcomes(mut self, repository: RequestOutcomeRepository) -> Self {
        self.request_outcomes = Some(repository);
        self
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

    fn list_failed<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUsageLogListQuery,
    ) -> AdminFailedCallLogListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let Some(repository) = self.request_outcomes.as_ref() else {
                return Ok(AdminFailedCallLogPage::from_parts(Vec::new(), None));
            };
            map_failed_page(
                repository
                    .list_failed(query.failed_before(), query.limit())
                    .await
                    .map_err(map_outcome_repository_error)?,
            )
        })
    }

    fn list_own_failed<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminUsageLogListQuery,
    ) -> AdminFailedCallLogListFuture<'a> {
        Box::pin(async move {
            let Some(repository) = self.request_outcomes.as_ref() else {
                return Ok(AdminFailedCallLogPage::from_parts(Vec::new(), None));
            };
            let page = repository
                .list_failed_for_user(principal.user_id(), query.failed_before(), query.limit())
                .await
                .map_err(map_outcome_repository_error)?;
            map_failed_page(page)
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

fn map_failed_page(
    page: RequestOutcomePageRecord,
) -> Result<AdminFailedCallLogPage, AdminUsageLogReadError> {
    let (records, next_cursor) = page.into_parts();
    let logs = records
        .into_iter()
        .map(AdminFailedCallLog::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AdminFailedCallLogPage::from_parts(logs, next_cursor))
}

fn map_outcome_repository_error(error: RequestOutcomeRepositoryError) -> AdminUsageLogReadError {
    let _ = error;
    AdminUsageLogReadError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_failed_calls_do_not_reuse_internal_error_text() {
        let internal = AdminFailedCallLog {
            id: 1,
            request_id: "request-safe-view".to_owned(),
            model: "test-model".to_owned(),
            protocol: UsageLogProtocol::OpenAiChat,
            operation: UsageLogOperation::Chat,
            error_kind: RequestFailureKind::UpstreamAuthentication,
            error_code: "upstream_authentication".to_owned(),
            error_message: "private upstream credential detail".to_owned(),
            user_id: Some(UserId::new(1).unwrap()),
            username: Some("owner".to_owned()),
            token_id: Some(TokenId::new(2).unwrap()),
            group_id: Some(GroupId::new(3).unwrap()),
            organization_id: Some(OrganizationId::new(4).unwrap()),
            organization_team_id: Some(OrganizationTeamId::new(5).unwrap()),
            channel_id: Some(ChannelId::new(6).unwrap()),
            duration_ms: 17,
            created_at: 1,
        };
        let public = UserFailedCallLog::from_admin(&internal);
        assert_eq!(public.request_id(), internal.request_id());
        assert_eq!(
            public.error_code(),
            RequestFailureKind::UpstreamAuthentication.public_error_code()
        );
        assert_eq!(
            public.error_message(),
            RequestFailureKind::UpstreamAuthentication.public_error_message()
        );
        assert!(!public.error_code().contains("upstream"));
        assert!(!public.error_message().contains("upstream"));
        assert!(!public.error_message().contains("credential"));
    }

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
