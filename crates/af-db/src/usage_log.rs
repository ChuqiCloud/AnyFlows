use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{BillingReservationId, GatewayPrincipal, Operation, Protocol, Quota};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AnalyticsExportFactKind, AnalyticsExportRepository, DatabasePool,
    entity::{BillingReservationKey, usage_logs},
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const CONSUME_EVENT_TYPE: i16 = 1;
const MAX_AUDIO_DURATION_NANOSECONDS: i64 = 24 * 60 * 60 * 1_000_000_000;
const MAX_VIDEO_DURATION_SECONDS: i64 = 24 * 60 * 60;

/// 用量日志持久化使用的计费模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageLogBillingMode {
    /// 按 token 计费。
    PerToken,
    /// 显式免费，仍保留用量事实。
    Free,
    /// 按请求次数或已校验业务维度直接计费。
    PerCall,
}

impl UsageLogBillingMode {
    const fn database_value(self) -> i16 {
        match self {
            Self::PerToken => 1,
            Self::Free => 2,
            Self::PerCall => 3,
        }
    }
}

/// 用量数据的持久化来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageLogSource {
    /// 上游协议明确返回。
    Upstream,
    /// 本地令牌器估算。
    Estimated,
}

impl UsageLogSource {
    const fn database_value(self) -> i16 {
        match self {
            Self::Upstream => 1,
            Self::Estimated => 2,
        }
    }
}

/// 输入总量与缓存明细的持久化计入口径。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageLogSemantics {
    /// 输入总量已经包含缓存明细。
    Inclusive,
    /// 缓存明细需要独立计入。
    CacheSeparated,
}

impl UsageLogSemantics {
    const fn database_value(self) -> i16 {
        match self {
            Self::Inclusive => 1,
            Self::CacheSeparated => 2,
        }
    }
}

/// 用量审计使用的闭合视频分辨率档位。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageLogVideoResolution {
    /// 480p 输出。
    P480,
    /// 720p 输出。
    P720,
    /// 1080p 输出。
    P1080,
}

impl UsageLogVideoResolution {
    const fn database_value(self) -> i16 {
        match self {
            Self::P480 => 1,
            Self::P720 => 2,
            Self::P1080 => 3,
        }
    }

    pub(crate) const fn from_database(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::P480),
            2 => Some(Self::P720),
            3 => Some(Self::P1080),
            _ => None,
        }
    }

    /// 返回管理 API 使用的稳定分辨率文本。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::P480 => "480p",
            Self::P720 => "720p",
            Self::P1080 => "1080p",
        }
    }
}

/// `usage_logs` 当前能够保存的完整 token 事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UsageLogUsage {
    input_tokens: i64,
    output_tokens: i64,
    cache_read: i64,
    cache_creation_5m: i64,
    cache_creation_1h: i64,
    reasoning_tokens: i64,
    audio_input_tokens: i64,
    audio_output_tokens: i64,
}

impl UsageLogUsage {
    /// 校验并构造全部非负 token 维度。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        input_tokens: i64,
        output_tokens: i64,
        cache_read: i64,
        cache_creation_5m: i64,
        cache_creation_1h: i64,
        reasoning_tokens: i64,
        audio_input_tokens: i64,
        audio_output_tokens: i64,
    ) -> Result<Self, UsageLogWriteError> {
        if [
            input_tokens,
            output_tokens,
            cache_read,
            cache_creation_5m,
            cache_creation_1h,
            reasoning_tokens,
            audio_input_tokens,
            audio_output_tokens,
        ]
        .into_iter()
        .any(|value| value < 0)
        {
            return Err(UsageLogWriteError::NegativeTokenCount);
        }
        Ok(Self {
            input_tokens,
            output_tokens,
            cache_read,
            cache_creation_5m,
            cache_creation_1h,
            reasoning_tokens,
            audio_input_tokens,
            audio_output_tokens,
        })
    }
}

/// 一条只追加用量日志的已校验写入事实。
#[derive(Clone, Eq, PartialEq)]
pub struct UsageLogWrite {
    event_id: BillingReservationId,
    principal: GatewayPrincipal,
    billing_mode: UsageLogBillingMode,
    usage: UsageLogUsage,
    source: UsageLogSource,
    semantics: UsageLogSemantics,
    quota: Quota,
    audio_duration_nanoseconds: Option<i64>,
    video_duration_seconds: Option<i64>,
    video_resolution: Option<UsageLogVideoResolution>,
    request_id: Option<String>,
    model: Option<String>,
    protocol: Option<Protocol>,
    operation: Option<Operation>,
    is_stream: Option<bool>,
    reasoning_effort: Option<i16>,
    reasoning_budget_tokens: Option<i64>,
    first_token_ms: Option<i64>,
    duration_ms: Option<i64>,
}

impl UsageLogWrite {
    /// 组合请求幂等键、认证主体、规范化 usage 与最终计费结果。
    #[must_use]
    pub const fn new(
        event_id: BillingReservationId,
        principal: GatewayPrincipal,
        billing_mode: UsageLogBillingMode,
        usage: UsageLogUsage,
        source: UsageLogSource,
        semantics: UsageLogSemantics,
        quota: Quota,
    ) -> Self {
        Self {
            event_id,
            principal,
            billing_mode,
            usage,
            source,
            semantics,
            quota,
            audio_duration_nanoseconds: None,
            video_duration_seconds: None,
            video_resolution: None,
            request_id: None,
            model: None,
            protocol: None,
            operation: None,
            is_stream: None,
            reasoning_effort: None,
            reasoning_budget_tokens: None,
            first_token_ms: None,
            duration_ms: None,
        }
    }

    /// 附加可选音频时长事实；缺失与真实零值保持可区分。
    pub fn with_audio_duration_nanoseconds(
        mut self,
        audio_duration_nanoseconds: Option<i64>,
    ) -> Result<Self, UsageLogWriteError> {
        if audio_duration_nanoseconds
            .is_some_and(|value| !(0..=MAX_AUDIO_DURATION_NANOSECONDS).contains(&value))
        {
            return Err(UsageLogWriteError::InvalidAudioDuration);
        }
        self.audio_duration_nanoseconds = audio_duration_nanoseconds;
        Ok(self)
    }

    /// 附加可选视频时长与分辨率事实；缺失值保持 `NULL`，不得猜测上游默认值。
    pub fn with_video_dimensions(
        mut self,
        video_duration_seconds: Option<i64>,
        video_resolution: Option<UsageLogVideoResolution>,
    ) -> Result<Self, UsageLogWriteError> {
        if video_duration_seconds
            .is_some_and(|value| !(1..=MAX_VIDEO_DURATION_SECONDS).contains(&value))
        {
            return Err(UsageLogWriteError::InvalidVideoDuration);
        }
        self.video_duration_seconds = video_duration_seconds;
        self.video_resolution = video_resolution;
        Ok(self)
    }

    /// 返回可选的视频真实时长，供幂等写入与回归验证使用。
    #[must_use]
    pub const fn video_duration_seconds(&self) -> Option<i64> {
        self.video_duration_seconds
    }

    /// 返回可选的视频分辨率档位，供幂等写入与回归验证使用。
    #[must_use]
    pub const fn video_resolution(&self) -> Option<UsageLogVideoResolution> {
        self.video_resolution
    }

    /// 附加已经通过计费层容量校验的模型调用观测。
    #[allow(clippy::too_many_arguments)]
    pub fn with_call_observation(
        mut self,
        request_id: &str,
        model: &str,
        protocol: Protocol,
        operation: Operation,
        is_stream: bool,
        reasoning_effort: Option<i16>,
        reasoning_budget_tokens: Option<i64>,
        first_token_ms: Option<i64>,
        duration_ms: i64,
    ) -> Result<Self, UsageLogWriteError> {
        if request_id.is_empty()
            || request_id.len() > 128
            || model.is_empty()
            || model.len() > 255
            || reasoning_effort.is_some_and(|value| !(1..=7).contains(&value))
            || reasoning_budget_tokens.is_some_and(|value| value < 0)
            || first_token_ms.is_some_and(|value| value < 0 || value > duration_ms)
            || duration_ms < 0
        {
            return Err(UsageLogWriteError::InvalidCallObservation);
        }
        self.request_id = Some(request_id.to_owned());
        self.model = Some(model.to_owned());
        self.protocol = Some(protocol);
        self.operation = Some(operation);
        self.is_stream = Some(is_stream);
        self.reasoning_effort = reasoning_effort;
        self.reasoning_budget_tokens = reasoning_budget_tokens;
        self.first_token_ms = first_token_ms;
        self.duration_ms = Some(duration_ms);
        Ok(self)
    }
}

impl fmt::Debug for UsageLogWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UsageLogWrite")
            .field("billing_mode", &self.billing_mode)
            .field("source", &self.source)
            .field("semantics", &self.semantics)
            .finish_non_exhaustive()
    }
}

/// 用量日志写入参数错误；不携带原始 token 数。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UsageLogWriteError {
    /// 任一 token 维度为负数。
    #[error("用量日志 token 数不能为负数")]
    NegativeTokenCount,
    /// 音频时长为负数或超过 24 小时固定边界。
    #[error("用量日志音频时长无效")]
    InvalidAudioDuration,
    /// 视频时长不是正整数或超过 24 小时固定边界。
    #[error("用量日志视频时长无效")]
    InvalidVideoDuration,
    /// 调用日志的文本、枚举或延迟观测不满足闭合边界。
    #[error("用量日志调用观测无效")]
    InvalidCallObservation,
}

/// 幂等写入用量日志后的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageLogWriteOutcome {
    /// 首次写入该事件事实。
    Applied,
    /// 数据库已经保存完全相同的事件事实。
    Existing,
}

/// 用量日志仓储错误；不回显幂等键、主体、token 数或额度。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UsageLogRepositoryError {
    /// 操作截止时间配置无效。
    #[error("用量日志仓储配置无效")]
    InvalidConfiguration,
    /// 数据库当前无法读取现有事实。
    #[error("用量日志仓储查询失败")]
    Query,
    /// 插入超时或连接异常导致提交终态未知，只能重放同一事实。
    #[error("用量日志写入结果未知")]
    OutcomeUnknown,
    /// 同一事件幂等键已经绑定不同的用量事实。
    #[error("用量日志事件冲突")]
    Conflict,
    /// 持久化记录违反用量日志不变量。
    #[error("用量日志持久化状态损坏")]
    Invariant,
}

/// 以 `event_id` 幂等追加最小用量事实的数据库仓储。
#[derive(Clone)]
pub struct UsageLogRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    analytics_export: Option<AnalyticsExportRepository>,
    #[cfg(test)]
    outcome_unknown_after_insert: Arc<AtomicBool>,
}

impl UsageLogRepository {
    /// 使用默认五秒操作截止时间构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
            analytics_export: None,
            #[cfg(test)]
            outcome_unknown_after_insert: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 使用显式非零操作截止时间构造仓储。
    pub fn with_operation_timeout(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, UsageLogRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(UsageLogRepositoryError::InvalidConfiguration);
        }
        Ok(Self {
            pool,
            operation_timeout,
            analytics_export: None,
            #[cfg(test)]
            outcome_unknown_after_insert: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 开启事实 outbox；业务事实与投递指针会在同一数据库事务提交。
    #[must_use]
    pub fn with_analytics_export(mut self, repository: AnalyticsExportRepository) -> Self {
        self.analytics_export = Some(repository);
        self
    }

    /// 幂等追加一条用量事实；相同事实重放不会产生第二行。
    pub async fn record(
        &self,
        write: &UsageLogWrite,
    ) -> Result<UsageLogWriteOutcome, UsageLogRepositoryError> {
        let operation = self
            .record_inner(write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => self.finish_operation(result),
            Err(_) => Err(record_internal_error(
                UsageLogRepositoryError::OutcomeUnknown,
            )),
        }
    }

    /// 仅供回归测试模拟插入成功但调用方未收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_insert(&self) {
        self.outcome_unknown_after_insert
            .store(true, Ordering::Release);
    }

    fn finish_operation(
        &self,
        result: Result<UsageLogWriteOutcome, UsageLogRepositoryError>,
    ) -> Result<UsageLogWriteOutcome, UsageLogRepositoryError> {
        let result = result.map_err(record_internal_error)?;
        #[cfg(test)]
        if result == UsageLogWriteOutcome::Applied
            && self
                .outcome_unknown_after_insert
                .swap(false, Ordering::AcqRel)
        {
            return Err(record_internal_error(
                UsageLogRepositoryError::OutcomeUnknown,
            ));
        }
        Ok(result)
    }

    async fn record_inner(
        &self,
        write: &UsageLogWrite,
    ) -> Result<UsageLogWriteOutcome, UsageLogRepositoryError> {
        let model = usage_log_active_model(write)?;
        if self.analytics_export.is_some() {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| UsageLogRepositoryError::OutcomeUnknown)?;
            match model.insert(&transaction).await {
                Ok(saved) => {
                    let result = AnalyticsExportRepository::enqueue_in_transaction(
                        &transaction,
                        AnalyticsExportFactKind::UsageLog,
                        saved.id,
                        saved.created_at,
                    )
                    .await;
                    match result {
                        Ok(()) => transaction
                            .commit()
                            .await
                            .map(|_| UsageLogWriteOutcome::Applied)
                            .map_err(|_| UsageLogRepositoryError::OutcomeUnknown),
                        Err(_) => {
                            let _ = transaction.rollback().await;
                            Err(UsageLogRepositoryError::OutcomeUnknown)
                        }
                    }
                }
                Err(error) if is_unique_violation(&error) => {
                    let _ = transaction.rollback().await;
                    self.classify_existing(write).await
                }
                Err(_) => {
                    let _ = transaction.rollback().await;
                    Err(UsageLogRepositoryError::OutcomeUnknown)
                }
            }
        } else {
            match model.insert(self.pool.connection()).await {
                Ok(_) => Ok(UsageLogWriteOutcome::Applied),
                Err(error) if is_unique_violation(&error) => self.classify_existing(write).await,
                Err(_) => Err(UsageLogRepositoryError::OutcomeUnknown),
            }
        }
    }

    async fn classify_existing(
        &self,
        write: &UsageLogWrite,
    ) -> Result<UsageLogWriteOutcome, UsageLogRepositoryError> {
        let event_key = event_key(write)?;
        let existing = usage_logs::Entity::find()
            .filter(usage_logs::Column::EventId.eq(event_key))
            .one(self.pool.connection())
            .await
            .map_err(|_| UsageLogRepositoryError::Query)?
            .ok_or(UsageLogRepositoryError::Invariant)?;
        if matches_write(&existing, write) {
            if let Some(exporter) = self.analytics_export.as_ref() {
                exporter
                    .ensure_pointer(AnalyticsExportFactKind::UsageLog, existing.id)
                    .await
                    .map_err(|_| UsageLogRepositoryError::OutcomeUnknown)?;
            }
            Ok(UsageLogWriteOutcome::Existing)
        } else {
            Err(UsageLogRepositoryError::Conflict)
        }
    }
}

fn usage_log_active_model(
    write: &UsageLogWrite,
) -> Result<usage_logs::ActiveModel, UsageLogRepositoryError> {
    let event_key = event_key(write)?;
    let usage = write.usage;
    let organization = write.principal.organization_principal();
    Ok(usage_logs::ActiveModel {
        event_id: Set(event_key),
        event_type: Set(CONSUME_EVENT_TYPE),
        user_id: Set(write.principal.user_id().get()),
        token_id: Set(write.principal.token_id().get()),
        group_id: Set(write.principal.group_id().get()),
        organization_id: Set(organization.map(|value| value.organization_id().get())),
        organization_team_id: Set(organization
            .and_then(|value| value.team_id())
            .map(|value| value.get())),
        billing_mode: Set(write.billing_mode.database_value()),
        input_tokens: Set(usage.input_tokens),
        output_tokens: Set(usage.output_tokens),
        cache_read: Set(usage.cache_read),
        cache_creation_5m: Set(usage.cache_creation_5m),
        cache_creation_1h: Set(usage.cache_creation_1h),
        reasoning_tokens: Set(usage.reasoning_tokens),
        audio_input_tokens: Set(usage.audio_input_tokens),
        audio_output_tokens: Set(usage.audio_output_tokens),
        audio_duration_nanoseconds: Set(write.audio_duration_nanoseconds),
        video_duration_seconds: Set(write.video_duration_seconds),
        video_resolution: Set(write
            .video_resolution
            .map(UsageLogVideoResolution::database_value)),
        request_id: Set(write.request_id.clone()),
        model: Set(write.model.clone()),
        protocol: Set(write.protocol.map(protocol_database_value)),
        operation: Set(write.operation.map(operation_database_value)),
        is_stream: Set(write.is_stream),
        reasoning_effort: Set(write.reasoning_effort),
        reasoning_budget_tokens: Set(write.reasoning_budget_tokens),
        first_token_ms: Set(write.first_token_ms),
        duration_ms: Set(write.duration_ms),
        usage_source: Set(write.source.database_value()),
        usage_semantics: Set(write.semantics.database_value()),
        quota: Set(write.quota.units()),
        created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        ..Default::default()
    })
}

impl fmt::Debug for UsageLogRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UsageLogRepository")
            .finish_non_exhaustive()
    }
}

fn event_key(write: &UsageLogWrite) -> Result<BillingReservationKey, UsageLogRepositoryError> {
    BillingReservationKey::parse(&write.event_id.persistence_key())
        .map_err(|_| UsageLogRepositoryError::Invariant)
}

fn matches_write(existing: &usage_logs::Model, write: &UsageLogWrite) -> bool {
    let usage = write.usage;
    event_key(write).is_ok_and(|event_id| existing.event_id == event_id)
        && existing.event_type == CONSUME_EVENT_TYPE
        && existing.user_id == write.principal.user_id().get()
        && existing.token_id == write.principal.token_id().get()
        && existing.group_id == write.principal.group_id().get()
        && existing.organization_id
            == write
                .principal
                .organization_principal()
                .map(|value| value.organization_id().get())
        && existing.organization_team_id
            == write
                .principal
                .organization_principal()
                .and_then(|value| value.team_id())
                .map(|value| value.get())
        && existing.billing_mode == write.billing_mode.database_value()
        && existing.input_tokens == usage.input_tokens
        && existing.output_tokens == usage.output_tokens
        && existing.cache_read == usage.cache_read
        && existing.cache_creation_5m == usage.cache_creation_5m
        && existing.cache_creation_1h == usage.cache_creation_1h
        && existing.reasoning_tokens == usage.reasoning_tokens
        && existing.audio_input_tokens == usage.audio_input_tokens
        && existing.audio_output_tokens == usage.audio_output_tokens
        && existing.audio_duration_nanoseconds == write.audio_duration_nanoseconds
        && existing.video_duration_seconds == write.video_duration_seconds
        && existing.video_resolution
            == write
                .video_resolution
                .map(UsageLogVideoResolution::database_value)
        && existing.request_id == write.request_id
        && existing.model == write.model
        && existing.protocol == write.protocol.map(protocol_database_value)
        && existing.operation == write.operation.map(operation_database_value)
        && existing.is_stream == write.is_stream
        && existing.reasoning_effort == write.reasoning_effort
        && existing.reasoning_budget_tokens == write.reasoning_budget_tokens
        && existing.first_token_ms == write.first_token_ms
        && existing.duration_ms == write.duration_ms
        && existing.usage_source == write.source.database_value()
        && existing.usage_semantics == write.semantics.database_value()
        && existing.quota == write.quota.units()
}

const fn protocol_database_value(protocol: Protocol) -> i16 {
    match protocol {
        Protocol::OpenAiChat => 1,
        Protocol::OpenAiResponses => 2,
        Protocol::OpenAiEmbeddings => 3,
        Protocol::OpenAiImages => 4,
        Protocol::OpenAiAudio => 5,
        Protocol::OpenAiSpeech => 6,
        Protocol::JinaRerank => 7,
        Protocol::CohereRerank => 8,
        Protocol::XaiVideo => 9,
        Protocol::Anthropic => 10,
        Protocol::Gemini => 11,
    }
}

const fn operation_database_value(operation: Operation) -> i16 {
    match operation {
        Operation::Chat => 1,
        Operation::Responses => 2,
        Operation::ResponsesCompact => 3,
        Operation::Embedding => 4,
        Operation::Image => 5,
        Operation::Audio => 6,
        Operation::Rerank => 7,
        Operation::Video => 8,
        Operation::CountTokens => 9,
    }
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

/// 只记录闭合内部分类，避免事件、主体、token 数与额度进入日志。
fn record_internal_error(error: UsageLogRepositoryError) -> UsageLogRepositoryError {
    let error_kind = match error {
        UsageLogRepositoryError::InvalidConfiguration => "usage_log_configuration",
        UsageLogRepositoryError::Query => "usage_log_query",
        UsageLogRepositoryError::OutcomeUnknown => "usage_log_outcome_unknown",
        UsageLogRepositoryError::Invariant => "usage_log_invariant",
        UsageLogRepositoryError::Conflict => return error,
    };
    tracing::error!(
        target: "af_db::usage_log",
        error_kind,
        "用量日志仓储发生内部错误"
    );
    error
}
