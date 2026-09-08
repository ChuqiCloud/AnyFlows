use std::{fmt, str::FromStr, time::Duration};

use af_domain::{ChannelId, Operation, Protocol};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, Set, SqlErr, TransactionTrait,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AnalyticsExportFactKind, AnalyticsExportRepository, DatabasePool, entity::request_outcome_logs,
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REQUEST_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 255;
const SUCCEEDED_OUTCOME: i16 = 1;
const FAILED_OUTCOME: i16 = 2;

/// 同步模型请求失败的低基数、无原始文本分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RequestFailureKind {
    InvalidRequest,
    ModelNotAllowed,
    InsufficientQuota,
    QuotaLimited,
    ConcurrencyLimited,
    OutcomeUnknown,
    UpstreamRateLimited,
    UpstreamOverloaded,
    UpstreamAuthentication,
    UpstreamQuota,
    UpstreamModel,
    UpstreamProtocol,
    UpstreamServer,
    UpstreamNetwork,
    Internal,
}

impl RequestFailureKind {
    /// 返回数据库和管理 API 共用的稳定分类标识。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::ModelNotAllowed => "model_not_allowed",
            Self::InsufficientQuota => "insufficient_quota",
            Self::QuotaLimited => "quota_limited",
            Self::ConcurrencyLimited => "concurrency_limited",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::UpstreamRateLimited => "upstream_rate_limited",
            Self::UpstreamOverloaded => "upstream_overloaded",
            Self::UpstreamAuthentication => "upstream_authentication",
            Self::UpstreamQuota => "upstream_quota",
            Self::UpstreamModel => "upstream_model",
            Self::UpstreamProtocol => "upstream_protocol",
            Self::UpstreamServer => "upstream_server",
            Self::UpstreamNetwork => "upstream_network",
            Self::Internal => "internal",
        }
    }
}

impl FromStr for RequestFailureKind {
    type Err = RequestOutcomeWriteError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "invalid_request" => Ok(Self::InvalidRequest),
            "model_not_allowed" => Ok(Self::ModelNotAllowed),
            "insufficient_quota" => Ok(Self::InsufficientQuota),
            "quota_limited" => Ok(Self::QuotaLimited),
            "concurrency_limited" => Ok(Self::ConcurrencyLimited),
            "outcome_unknown" => Ok(Self::OutcomeUnknown),
            "upstream_rate_limited" => Ok(Self::UpstreamRateLimited),
            "upstream_overloaded" => Ok(Self::UpstreamOverloaded),
            "upstream_authentication" => Ok(Self::UpstreamAuthentication),
            "upstream_quota" => Ok(Self::UpstreamQuota),
            "upstream_model" => Ok(Self::UpstreamModel),
            "upstream_protocol" => Ok(Self::UpstreamProtocol),
            "upstream_server" => Ok(Self::UpstreamServer),
            "upstream_network" => Ok(Self::UpstreamNetwork),
            "internal" => Ok(Self::Internal),
            _ => Err(RequestOutcomeWriteError::InvalidFailureKind),
        }
    }
}

/// 一条请求终态事实的已校验写入值。
#[derive(Clone, Eq, PartialEq)]
pub struct RequestOutcomeWrite {
    request_id: String,
    protocol: Protocol,
    operation: Operation,
    model: String,
    error_kind: Option<RequestFailureKind>,
    channel_id: Option<ChannelId>,
    duration_ms: i64,
}

impl RequestOutcomeWrite {
    /// 构造成功请求事实；最终渠道在直连兼容实现中允许为空。
    pub fn succeeded(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        Self::new(
            request_id, protocol, operation, model, None, channel_id, duration,
        )
    }

    /// 构造失败请求事实，只接受闭合分类而不接收原始错误文本。
    pub fn failed(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        error_kind: RequestFailureKind,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        Self::new(
            request_id,
            protocol,
            operation,
            model,
            Some(error_kind),
            channel_id,
            duration,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        request_id: &str,
        protocol: Protocol,
        operation: Operation,
        model: &str,
        error_kind: Option<RequestFailureKind>,
        channel_id: Option<ChannelId>,
        duration: Duration,
    ) -> Result<Self, RequestOutcomeWriteError> {
        if !valid_text(request_id, MAX_REQUEST_ID_BYTES) {
            return Err(RequestOutcomeWriteError::InvalidRequestId);
        }
        if !valid_text(model, MAX_MODEL_BYTES) {
            return Err(RequestOutcomeWriteError::InvalidModel);
        }
        let duration_ms = i64::try_from(duration.as_millis())
            .map_err(|_| RequestOutcomeWriteError::InvalidDuration)?;
        Ok(Self {
            request_id: request_id.to_owned(),
            protocol,
            operation,
            model: model.to_owned(),
            error_kind,
            channel_id,
            duration_ms,
        })
    }

    /// 返回当前事实是否表示成功终态。
    #[must_use]
    pub const fn succeeded_outcome(&self) -> bool {
        self.error_kind.is_none()
    }
}

impl fmt::Debug for RequestOutcomeWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestOutcomeWrite")
            .field("protocol", &self.protocol)
            .field("operation", &self.operation)
            .field("error_kind", &self.error_kind)
            .field("has_channel", &self.channel_id.is_some())
            .field("duration_ms", &self.duration_ms)
            .finish_non_exhaustive()
    }
}

/// 请求终态写入值校验错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RequestOutcomeWriteError {
    #[error("请求终态请求标识无效")]
    InvalidRequestId,
    #[error("请求终态模型无效")]
    InvalidModel,
    #[error("请求终态耗时无效")]
    InvalidDuration,
    #[error("请求终态失败分类无效")]
    InvalidFailureKind,
}

/// 幂等追加请求终态事实的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestOutcomeWriteOutcome {
    Applied,
    Existing,
}

/// 请求终态事实仓储错误，不携带请求或数据库细节。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RequestOutcomeRepositoryError {
    #[error("请求终态数据库查询失败")]
    Query,
    #[error("请求终态数据库操作超时")]
    Timeout,
    #[error("请求终态幂等事实冲突")]
    Conflict,
    #[error("请求终态持久化状态损坏")]
    Invariant,
}

/// 请求终态事实的只追加、幂等仓储。
#[derive(Clone)]
pub struct RequestOutcomeRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    analytics_export: Option<AnalyticsExportRepository>,
}

impl RequestOutcomeRepository {
    /// 使用共享连接池和固定两秒写入截止时间构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
            analytics_export: None,
        }
    }

    /// 开启事实 outbox；请求终态与投递指针会在同一数据库事务提交。
    #[must_use]
    pub fn with_analytics_export(mut self, repository: AnalyticsExportRepository) -> Self {
        self.analytics_export = Some(repository);
        self
    }

    /// 按网关请求标识幂等追加一条终态事实。
    pub async fn record(
        &self,
        write: &RequestOutcomeWrite,
    ) -> Result<RequestOutcomeWriteOutcome, RequestOutcomeRepositoryError> {
        let operation = self
            .record_inner(write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(RequestOutcomeRepositoryError::Timeout)),
        }
    }

    async fn record_inner(
        &self,
        write: &RequestOutcomeWrite,
    ) -> Result<RequestOutcomeWriteOutcome, RequestOutcomeRepositoryError> {
        let model = request_outcome_logs::ActiveModel {
            request_id: Set(write.request_id.clone()),
            protocol: Set(write.protocol.as_str().to_owned()),
            operation: Set(write.operation.as_str().to_owned()),
            model: Set(write.model.clone()),
            outcome: Set(if write.succeeded_outcome() {
                SUCCEEDED_OUTCOME
            } else {
                FAILED_OUTCOME
            }),
            error_kind: Set(write
                .error_kind
                .map(RequestFailureKind::as_str)
                .map(str::to_owned)),
            channel_id: Set(write.channel_id.map(ChannelId::get)),
            duration_ms: Set(write.duration_ms),
            created_at: Set(sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        if self.analytics_export.is_some() {
            let transaction = self
                .pool
                .connection()
                .begin()
                .await
                .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query))?;
            match model.insert(&transaction).await {
                Ok(saved) => {
                    let result = AnalyticsExportRepository::enqueue_in_transaction(
                        &transaction,
                        AnalyticsExportFactKind::RequestOutcome,
                        saved.id,
                        saved.created_at,
                    )
                    .await;
                    match result {
                        Ok(()) => transaction
                            .commit()
                            .await
                            .map(|_| RequestOutcomeWriteOutcome::Applied)
                            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query)),
                        Err(_) => {
                            let _ = transaction.rollback().await;
                            Err(internal_error(RequestOutcomeRepositoryError::Query))
                        }
                    }
                }
                Err(error) if is_unique_violation(&error) => {
                    let _ = transaction.rollback().await;
                    self.classify_existing(write).await
                }
                Err(_) => {
                    let _ = transaction.rollback().await;
                    Err(internal_error(RequestOutcomeRepositoryError::Query))
                }
            }
        } else {
            match model.insert(self.pool.connection()).await {
                Ok(_) => Ok(RequestOutcomeWriteOutcome::Applied),
                Err(error) if is_unique_violation(&error) => self.classify_existing(write).await,
                Err(_) => Err(internal_error(RequestOutcomeRepositoryError::Query)),
            }
        }
    }

    async fn classify_existing(
        &self,
        write: &RequestOutcomeWrite,
    ) -> Result<RequestOutcomeWriteOutcome, RequestOutcomeRepositoryError> {
        let existing = request_outcome_logs::Entity::find()
            .filter(request_outcome_logs::Column::RequestId.eq(&write.request_id))
            .one(self.pool.connection())
            .await
            .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query))?
            .ok_or_else(|| internal_error(RequestOutcomeRepositoryError::Invariant))?;
        if matches_write(&existing, write) {
            if let Some(exporter) = self.analytics_export.as_ref() {
                exporter
                    .ensure_pointer(AnalyticsExportFactKind::RequestOutcome, existing.id)
                    .await
                    .map_err(|_| internal_error(RequestOutcomeRepositoryError::Query))?;
            }
            Ok(RequestOutcomeWriteOutcome::Existing)
        } else {
            Err(RequestOutcomeRepositoryError::Conflict)
        }
    }
}

impl fmt::Debug for RequestOutcomeRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestOutcomeRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

fn matches_write(existing: &request_outcome_logs::Model, write: &RequestOutcomeWrite) -> bool {
    existing.request_id == write.request_id
        && existing.protocol == write.protocol.as_str()
        && existing.operation == write.operation.as_str()
        && existing.model == write.model
        && existing.outcome
            == if write.succeeded_outcome() {
                SUCCEEDED_OUTCOME
            } else {
                FAILED_OUTCOME
            }
        && existing.error_kind.as_deref() == write.error_kind.map(RequestFailureKind::as_str)
        && existing.channel_id == write.channel_id.map(ChannelId::get)
        && existing.duration_ms == write.duration_ms
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn internal_error(error: RequestOutcomeRepositoryError) -> RequestOutcomeRepositoryError {
    let error_kind = match error {
        RequestOutcomeRepositoryError::Query => "request_outcome_query",
        RequestOutcomeRepositoryError::Timeout => "request_outcome_timeout",
        RequestOutcomeRepositoryError::Conflict => return error,
        RequestOutcomeRepositoryError::Invariant => "request_outcome_invariant",
    };
    tracing::error!(
        target: "af_db::request_outcome",
        error_kind,
        "请求终态事实仓储发生内部错误"
    );
    error
}
