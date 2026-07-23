use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use af_domain::{BillingReservationId, GatewayPrincipal, Quota};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, Set, SqlErr,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{BillingReservationKey, usage_logs},
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const CONSUME_EVENT_TYPE: i16 = 1;

/// 用量日志持久化使用的计费模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageLogBillingMode {
    /// 按 token 计费。
    PerToken,
    /// 显式免费，仍保留用量事实。
    Free,
}

impl UsageLogBillingMode {
    const fn database_value(self) -> i16 {
        match self {
            Self::PerToken => 1,
            Self::Free => 2,
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
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct UsageLogWrite {
    event_id: BillingReservationId,
    principal: GatewayPrincipal,
    billing_mode: UsageLogBillingMode,
    usage: UsageLogUsage,
    source: UsageLogSource,
    semantics: UsageLogSemantics,
    quota: Quota,
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
        }
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
            #[cfg(test)]
            outcome_unknown_after_insert: Arc::new(AtomicBool::new(false)),
        })
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
        let event_key = event_key(write)?;
        let usage = write.usage;
        let model = usage_logs::ActiveModel {
            event_id: Set(event_key),
            event_type: Set(CONSUME_EVENT_TYPE),
            user_id: Set(write.principal.user_id().get()),
            token_id: Set(write.principal.token_id().get()),
            group_id: Set(write.principal.group_id().get()),
            billing_mode: Set(write.billing_mode.database_value()),
            input_tokens: Set(usage.input_tokens),
            output_tokens: Set(usage.output_tokens),
            cache_read: Set(usage.cache_read),
            cache_creation_5m: Set(usage.cache_creation_5m),
            cache_creation_1h: Set(usage.cache_creation_1h),
            reasoning_tokens: Set(usage.reasoning_tokens),
            audio_input_tokens: Set(usage.audio_input_tokens),
            audio_output_tokens: Set(usage.audio_output_tokens),
            usage_source: Set(write.source.database_value()),
            usage_semantics: Set(write.semantics.database_value()),
            quota: Set(write.quota.units()),
            created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        match model.insert(self.pool.connection()).await {
            Ok(_) => Ok(UsageLogWriteOutcome::Applied),
            Err(error) if is_unique_violation(&error) => self.classify_existing(write).await,
            Err(_) => Err(UsageLogRepositoryError::OutcomeUnknown),
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
            Ok(UsageLogWriteOutcome::Existing)
        } else {
            Err(UsageLogRepositoryError::Conflict)
        }
    }
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
        && existing.billing_mode == write.billing_mode.database_value()
        && existing.input_tokens == usage.input_tokens
        && existing.output_tokens == usage.output_tokens
        && existing.cache_read == usage.cache_read
        && existing.cache_creation_5m == usage.cache_creation_5m
        && existing.cache_creation_1h == usage.cache_creation_1h
        && existing.reasoning_tokens == usage.reasoning_tokens
        && existing.audio_input_tokens == usage.audio_input_tokens
        && existing.audio_output_tokens == usage.audio_output_tokens
        && existing.usage_source == write.source.database_value()
        && existing.usage_semantics == write.semantics.database_value()
        && existing.quota == write.quota.units()
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
