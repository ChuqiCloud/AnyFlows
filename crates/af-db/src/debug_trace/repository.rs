use std::{fmt, sync::Arc, time::Duration};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{debug_trace_attempts, debug_trace_settings, debug_traces},
};

use super::snapshots::{insert_snapshot, prune_access_audits, read_snapshots};
use super::types::{
    DebugTraceAttemptOutcome, DebugTraceAttemptRecord, DebugTraceDetailRecord,
    DebugTraceFailureKind, DebugTraceListQuery, DebugTraceOperation, DebugTraceOutcome,
    DebugTracePageRecord, DebugTraceProtocol, DebugTraceSettingsRecord, DebugTraceSettingsWrite,
    DebugTraceSnapshotCipher, DebugTraceSnapshotContext, DebugTraceSnapshotKind,
    DebugTraceSnapshotRecord, DebugTraceSnapshotScope, DebugTraceSummaryRecord, DebugTraceWrite,
    DebugTraceWriteError, MAX_DEBUG_TRACE_RETENTION_HOURS, valid_method, valid_model, valid_path,
    valid_request_id,
};

const DEBUG_TRACE_SETTINGS_ID: i16 = 1;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DebugTraceRepositoryConfigError {
    #[error("调试追踪数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DebugTraceRepositoryError {
    #[error("调试追踪数据库操作失败")]
    Query,
    #[error("调试追踪数据库操作超时")]
    Timeout,
    #[error("调试追踪持久化状态损坏")]
    Invariant,
    #[error("调试追踪输入无效")]
    InvalidInput,
    #[error("调试追踪记录不存在")]
    NotFound,
    #[error("调试追踪快照解密失败")]
    Decrypt,
}

#[derive(Clone)]
pub struct DebugTraceRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    snapshot_cipher: Arc<dyn DebugTraceSnapshotCipher>,
}

impl DebugTraceRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
        snapshot_cipher: Arc<dyn DebugTraceSnapshotCipher>,
    ) -> Result<Self, DebugTraceRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(DebugTraceRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
            snapshot_cipher,
        })
    }

    pub async fn settings(&self) -> Result<DebugTraceSettingsRecord, DebugTraceRepositoryError> {
        self.with_timeout(self.settings_inner()).await
    }

    pub async fn update_settings(
        &self,
        write: DebugTraceSettingsWrite,
    ) -> Result<DebugTraceSettingsRecord, DebugTraceRepositoryError> {
        self.with_timeout(self.update_settings_inner(write)).await
    }

    pub async fn insert(
        &self,
        write: DebugTraceWrite,
    ) -> Result<DebugTraceSummaryRecord, DebugTraceRepositoryError> {
        self.with_timeout(self.insert_inner(write)).await
    }

    pub async fn list(
        &self,
        query: DebugTraceListQuery,
    ) -> Result<DebugTracePageRecord, DebugTraceRepositoryError> {
        self.with_timeout(self.list_inner(query)).await
    }

    pub async fn detail(
        &self,
        trace_id: i64,
    ) -> Result<DebugTraceDetailRecord, DebugTraceRepositoryError> {
        if trace_id < 1 {
            return Err(DebugTraceRepositoryError::InvalidInput);
        }
        self.with_timeout(self.detail_inner(trace_id)).await
    }

    /// 在同一事务内解密指定范围并追加读取审计；审计提交失败时不返回明文。
    pub async fn snapshots(
        &self,
        trace_id: i64,
        actor_user_id: i64,
        scope: DebugTraceSnapshotScope,
    ) -> Result<DebugTraceSnapshotRecord, DebugTraceRepositoryError> {
        if trace_id < 1 || actor_user_id < 1 {
            return Err(DebugTraceRepositoryError::InvalidInput);
        }
        self.with_timeout(self.snapshots_inner(trace_id, actor_user_id, scope))
            .await
    }

    pub async fn prune(&self, retention_hours: i32) -> Result<u64, DebugTraceRepositoryError> {
        if !(1..=MAX_DEBUG_TRACE_RETENTION_HOURS).contains(&retention_hours) {
            return Err(DebugTraceRepositoryError::InvalidInput);
        }
        self.with_timeout(self.prune_inner(retention_hours)).await
    }

    async fn with_timeout<T>(
        &self,
        future: impl Future<Output = Result<T, DebugTraceRepositoryError>>,
    ) -> Result<T, DebugTraceRepositoryError> {
        timeout(self.operation_timeout, future)
            .await
            .unwrap_or_else(|_| Err(internal_error(DebugTraceRepositoryError::Timeout)))
    }

    async fn settings_inner(&self) -> Result<DebugTraceSettingsRecord, DebugTraceRepositoryError> {
        let model = debug_trace_settings::Entity::find_by_id(DEBUG_TRACE_SETTINGS_ID)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_settings_read"))?
            .ok_or_else(|| internal_error(DebugTraceRepositoryError::Invariant))?;
        settings_from_model(model)
    }

    async fn update_settings_inner(
        &self,
        write: DebugTraceSettingsWrite,
    ) -> Result<DebugTraceSettingsRecord, DebugTraceRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_settings_begin"))?;
        let existing = lock_settings(&transaction).await?;
        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| internal_error(DebugTraceRepositoryError::Invariant))?;
        let saved = debug_trace_settings::ActiveModel {
            id: Set(DEBUG_TRACE_SETTINGS_ID),
            enabled: Set(write.enabled),
            sample_per_million: Set(write.sample_per_million),
            retention_hours: Set(write.retention_hours),
            capture_headers: Set(write.capture_headers),
            capture_bodies: Set(write.capture_bodies),
            max_body_bytes: Set(write.max_body_bytes),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        }
        .update(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("debug_trace_settings_write"))?;
        let saved = settings_from_model(saved)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_settings_commit"))?;
        Ok(saved)
    }

    async fn insert_inner(
        &self,
        write: DebugTraceWrite,
    ) -> Result<DebugTraceSummaryRecord, DebugTraceRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_begin"))?;
        let selected = write.selected_target();
        let attempt_count = i32::try_from(write.attempts.len())
            .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?;
        let created_at = TimeDateTimeWithTimeZone::now_utc();
        let (downstream_method, downstream_path, downstream_headers_json, downstream_body_json) =
            write
                .downstream_diagnostic
                .map_or((None, None, None, None), |diagnostic| {
                    (
                        Some(diagnostic.method),
                        Some(diagnostic.path),
                        diagnostic.headers_json,
                        diagnostic.body_json,
                    )
                });
        let model = debug_traces::ActiveModel {
            id: sea_orm::NotSet,
            request_id: Set(write.request_id),
            user_id: Set(write.user_id),
            token_id: Set(write.token_id),
            group_id: Set(write.group_id),
            requested_model: Set(write.requested_model),
            downstream_protocol: Set(write.downstream_protocol.as_i16()),
            upstream_protocol: Set(write.upstream_protocol.as_i16()),
            operation: Set(write.operation.as_i16()),
            outcome: Set(write.outcome.as_i16()),
            selected_channel_id: Set(selected.map(|target| target.0)),
            selected_credential_id: Set(selected.map(|target| target.1)),
            routing_elapsed_ms: Set(write.routing_elapsed_ms),
            attempt_count: Set(attempt_count),
            downstream_method: Set(downstream_method),
            downstream_path: Set(downstream_path),
            // 旧列只保留滚动升级结构兼容，新版本永不再写入诊断明文。
            downstream_headers_json: Set(None),
            downstream_body_json: Set(None),
            created_at: Set(created_at),
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("debug_trace_insert"))?;
        if let Some(plaintext) = downstream_headers_json.as_deref() {
            insert_snapshot(
                &transaction,
                self.snapshot_cipher.as_ref(),
                DebugTraceSnapshotContext::new(
                    model.id,
                    None,
                    DebugTraceSnapshotKind::DownstreamHeaders,
                )
                .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?,
                plaintext,
                created_at,
            )
            .await?;
        }
        if let Some(plaintext) = downstream_body_json.as_deref() {
            insert_snapshot(
                &transaction,
                self.snapshot_cipher.as_ref(),
                DebugTraceSnapshotContext::new(
                    model.id,
                    None,
                    DebugTraceSnapshotKind::DownstreamBody,
                )
                .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?,
                plaintext,
                created_at,
            )
            .await?;
        }
        for attempt in write.attempts {
            let (
                request_method,
                request_url,
                request_headers_json,
                request_body_json,
                response_status,
                response_headers_json,
                response_body_json,
                response_streamed,
            ) = attempt.diagnostic.map_or(
                (None, None, None, None, None, None, None, false),
                |diagnostic| {
                    (
                        Some(diagnostic.request_method),
                        Some(diagnostic.request_url),
                        diagnostic.request_headers_json,
                        diagnostic.request_body_json,
                        diagnostic.response_status,
                        diagnostic.response_headers_json,
                        diagnostic.response_body_json,
                        diagnostic.response_streamed,
                    )
                },
            );
            let attempt_model = debug_trace_attempts::ActiveModel {
                id: sea_orm::NotSet,
                trace_id: Set(model.id),
                candidate_index: Set(attempt.candidate_index),
                channel_id: Set(attempt.channel_id),
                credential_id: Set(attempt.credential_id),
                outcome: Set(attempt.outcome.as_i16()),
                failure_kind: Set(attempt.failure_kind.map(DebugTraceFailureKind::as_i16)),
                upstream_status: Set(attempt.upstream_status),
                retry_decision: Set(attempt.retry_decision),
                elapsed_ms: Set(attempt.elapsed_ms),
                client_simulation_profile: Set(attempt
                    .client_simulation_profile
                    .map(|profile| profile.as_str().to_owned())),
                client_simulation_result: Set(attempt
                    .client_simulation_result
                    .map(|result| result.as_str().to_owned())),
                client_simulation_body_profile: Set(attempt
                    .client_simulation_body_profile
                    .map(|profile| profile.as_str().to_owned())),
                client_simulation_body_result: Set(attempt
                    .client_simulation_body_result
                    .map(|result| result.as_str().to_owned())),
                request_method: Set(request_method),
                request_url: Set(request_url),
                request_headers_json: Set(None),
                request_body_json: Set(None),
                response_status: Set(response_status),
                response_headers_json: Set(None),
                response_body_json: Set(None),
                response_streamed: Set(response_streamed),
                created_at: Set(created_at),
            }
            .insert(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_attempt_insert"))?;
            for (kind, plaintext) in [
                (
                    DebugTraceSnapshotKind::AttemptRequestHeaders,
                    request_headers_json.as_deref(),
                ),
                (
                    DebugTraceSnapshotKind::AttemptRequestBody,
                    request_body_json.as_deref(),
                ),
                (
                    DebugTraceSnapshotKind::AttemptResponseHeaders,
                    response_headers_json.as_deref(),
                ),
                (
                    DebugTraceSnapshotKind::AttemptResponseBody,
                    response_body_json.as_deref(),
                ),
            ] {
                if let Some(plaintext) = plaintext {
                    insert_snapshot(
                        &transaction,
                        self.snapshot_cipher.as_ref(),
                        DebugTraceSnapshotContext::new(model.id, Some(attempt_model.id), kind)
                            .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?,
                        plaintext,
                        created_at,
                    )
                    .await?;
                }
            }
        }
        let record = summary_from_model(model)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_commit"))?;
        Ok(record)
    }

    async fn list_inner(
        &self,
        query: DebugTraceListQuery,
    ) -> Result<DebugTracePageRecord, DebugTraceRepositoryError> {
        let mut select = debug_traces::Entity::find().order_by_desc(debug_traces::Column::Id);
        if let Some(before) = query.before {
            select = select.filter(debug_traces::Column::Id.lt(before));
        }
        if let Some(outcome) = query.outcome {
            select = select.filter(debug_traces::Column::Outcome.eq(outcome.as_i16()));
        }
        if let Some(model) = query.requested_model {
            select = select.filter(debug_traces::Column::RequestedModel.eq(model));
        }
        if let Some(request_id) = query.request_id {
            select = select.filter(debug_traces::Column::RequestId.eq(request_id));
        }
        let mut traces = select
            .limit((query.limit + 1) as u64)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_list"))?
            .into_iter()
            .map(summary_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let has_next = traces.len() > query.limit;
        traces.truncate(query.limit);
        let next_cursor = has_next
            .then(|| traces.last().map(DebugTraceSummaryRecord::id))
            .flatten();
        Ok(DebugTracePageRecord::new(traces, next_cursor))
    }

    async fn detail_inner(
        &self,
        trace_id: i64,
    ) -> Result<DebugTraceDetailRecord, DebugTraceRepositoryError> {
        let model = debug_traces::Entity::find_by_id(trace_id)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_detail"))?
            .ok_or(DebugTraceRepositoryError::NotFound)?;
        let summary = summary_from_model(model)?;
        let attempts = debug_trace_attempts::Entity::find()
            .filter(debug_trace_attempts::Column::TraceId.eq(trace_id))
            .order_by_asc(debug_trace_attempts::Column::CandidateIndex)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_attempt_list"))?
            .into_iter()
            .map(attempt_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        if i32::try_from(attempts.len()).ok() != Some(summary.attempt_count) {
            return Err(internal_error(DebugTraceRepositoryError::Invariant));
        }
        Ok(DebugTraceDetailRecord::new(summary, attempts))
    }

    async fn snapshots_inner(
        &self,
        trace_id: i64,
        actor_user_id: i64,
        scope: DebugTraceSnapshotScope,
    ) -> Result<DebugTraceSnapshotRecord, DebugTraceRepositoryError> {
        read_snapshots(
            &self.pool,
            self.snapshot_cipher.as_ref(),
            trace_id,
            actor_user_id,
            scope,
        )
        .await
    }

    async fn prune_inner(&self, retention_hours: i32) -> Result<u64, DebugTraceRepositoryError> {
        let seconds = i64::from(retention_hours)
            .checked_mul(3_600)
            .ok_or(DebugTraceRepositoryError::InvalidInput)?;
        let cutoff_epoch = TimeDateTimeWithTimeZone::now_utc()
            .unix_timestamp()
            .checked_sub(seconds)
            .ok_or(DebugTraceRepositoryError::InvalidInput)?;
        let cutoff = TimeDateTimeWithTimeZone::from_unix_timestamp(cutoff_epoch)
            .map_err(|_| DebugTraceRepositoryError::InvalidInput)?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_prune_begin"))?;
        prune_access_audits(&transaction, cutoff).await?;
        let deleted = debug_traces::Entity::delete_many()
            .filter(debug_traces::Column::CreatedAt.lt(cutoff))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_prune"))?
            .rows_affected;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_prune_commit"))?;
        Ok(deleted)
    }
}

impl fmt::Debug for DebugTraceRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DebugTraceRepository")
            .field("pool", &self.pool)
            .field("operation_timeout", &self.operation_timeout)
            .field("snapshot_cipher", &"<已脱敏>")
            .finish()
    }
}

fn settings_from_model(
    model: debug_trace_settings::Model,
) -> Result<DebugTraceSettingsRecord, DebugTraceRepositoryError> {
    if model.id != DEBUG_TRACE_SETTINGS_ID {
        return Err(internal_error(DebugTraceRepositoryError::Invariant));
    }
    DebugTraceSettingsRecord::new(
        model.enabled,
        model.sample_per_million,
        model.retention_hours,
        model.capture_headers,
        model.capture_bodies,
        model.max_body_bytes,
        model.version,
    )
    .map_err(map_write_error)
}

fn summary_from_model(
    model: debug_traces::Model,
) -> Result<DebugTraceSummaryRecord, DebugTraceRepositoryError> {
    if model.id < 1
        || model.user_id < 1
        || model.token_id < 1
        || model.group_id < 1
        || !valid_request_id(&model.request_id)
        || !valid_model(&model.requested_model)
        || model.routing_elapsed_ms < 0
        || !(0..=64).contains(&model.attempt_count)
        || model.selected_channel_id.is_some() != model.selected_credential_id.is_some()
        || model.selected_channel_id.is_some_and(|value| value < 1)
        || model.selected_credential_id.is_some_and(|value| value < 1)
        || model.downstream_method.is_some() != model.downstream_path.is_some()
        || model
            .downstream_method
            .as_deref()
            .is_some_and(|value| !valid_method(value))
        || model
            .downstream_path
            .as_deref()
            .is_some_and(|value| !valid_path(value))
    {
        return Err(internal_error(DebugTraceRepositoryError::Invariant));
    }
    let outcome = DebugTraceOutcome::from_i16(model.outcome).map_err(map_write_error)?;
    if (outcome == DebugTraceOutcome::Succeeded) != model.selected_channel_id.is_some() {
        return Err(internal_error(DebugTraceRepositoryError::Invariant));
    }
    Ok(DebugTraceSummaryRecord {
        id: model.id,
        request_id: model.request_id,
        user_id: model.user_id,
        token_id: model.token_id,
        group_id: model.group_id,
        requested_model: model.requested_model,
        downstream_protocol: DebugTraceProtocol::from_i16(model.downstream_protocol)
            .map_err(map_write_error)?,
        upstream_protocol: DebugTraceProtocol::from_i16(model.upstream_protocol)
            .map_err(map_write_error)?,
        operation: DebugTraceOperation::from_i16(model.operation).map_err(map_write_error)?,
        outcome,
        selected_channel_id: model.selected_channel_id,
        selected_credential_id: model.selected_credential_id,
        routing_elapsed_ms: model.routing_elapsed_ms,
        attempt_count: model.attempt_count,
        downstream_method: model.downstream_method,
        downstream_path: model.downstream_path,
        created_at: model.created_at,
    })
}

async fn lock_settings(
    transaction: &DatabaseTransaction,
) -> Result<debug_trace_settings::Model, DebugTraceRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不支持 FOR UPDATE，先执行无变化写入以取得数据库写锁。
        let result = debug_trace_settings::Entity::update_many()
            .filter(debug_trace_settings::Column::Id.eq(DEBUG_TRACE_SETTINGS_ID))
            .col_expr(
                debug_trace_settings::Column::Version,
                Expr::col(debug_trace_settings::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("debug_trace_settings_lock"))?;
        if result.rows_affected != 1 {
            return Err(internal_error(DebugTraceRepositoryError::Invariant));
        }
    }

    let mut query = debug_trace_settings::Entity::find_by_id(DEBUG_TRACE_SETTINGS_ID);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("debug_trace_settings_read_for_update"))?
        .ok_or_else(|| internal_error(DebugTraceRepositoryError::Invariant))
}

fn attempt_from_model(
    model: debug_trace_attempts::Model,
) -> Result<DebugTraceAttemptRecord, DebugTraceRepositoryError> {
    let outcome = DebugTraceAttemptOutcome::from_i16(model.outcome).map_err(map_write_error)?;
    let failure_kind = model
        .failure_kind
        .map(DebugTraceFailureKind::from_i16)
        .transpose()
        .map_err(map_write_error)?;
    let client_simulation_profile = model
        .client_simulation_profile
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?;
    let client_simulation_result = model
        .client_simulation_result
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?;
    let client_simulation_body_profile = model
        .client_simulation_body_profile
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?;
    let client_simulation_body_result = model
        .client_simulation_body_result
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?;
    let shape_valid = match outcome {
        DebugTraceAttemptOutcome::Succeeded => {
            failure_kind.is_none() && model.upstream_status.is_none() && !model.retry_decision
        }
        DebugTraceAttemptOutcome::Failed => failure_kind.is_some(),
    };
    if !(0..64).contains(&model.candidate_index)
        || model.channel_id < 1
        || model.credential_id < 1
        || model.elapsed_ms < 0
        || !shape_valid
        || model
            .upstream_status
            .is_some_and(|status| !(500..=599).contains(&status))
        || model.request_method.is_some() != model.request_url.is_some()
        || model
            .request_method
            .as_deref()
            .is_some_and(|value| !valid_method(value))
        || model
            .request_url
            .as_deref()
            .is_some_and(|value| !valid_path(value))
        || model
            .response_status
            .is_some_and(|status| !(100..=599).contains(&status))
        || client_simulation_profile.is_some() != client_simulation_result.is_some()
        || client_simulation_body_profile.is_some() != client_simulation_body_result.is_some()
    {
        return Err(internal_error(DebugTraceRepositoryError::Invariant));
    }
    Ok(DebugTraceAttemptRecord {
        candidate_index: model.candidate_index,
        channel_id: model.channel_id,
        credential_id: model.credential_id,
        outcome,
        failure_kind,
        upstream_status: model.upstream_status,
        retry_decision: model.retry_decision,
        elapsed_ms: model.elapsed_ms,
        client_simulation_profile,
        client_simulation_result,
        client_simulation_body_profile,
        client_simulation_body_result,
        request_method: model.request_method,
        request_url: model.request_url,
        response_status: model.response_status,
        response_streamed: model.response_streamed,
    })
}

fn map_write_error(error: DebugTraceWriteError) -> DebugTraceRepositoryError {
    match error {
        DebugTraceWriteError::InvalidInput => DebugTraceRepositoryError::InvalidInput,
        DebugTraceWriteError::Invariant => internal_error(DebugTraceRepositoryError::Invariant),
    }
}

pub(super) fn query_error(kind: &'static str) -> DebugTraceRepositoryError {
    tracing::error!(target: "af_db::debug_trace", error_kind = kind, "调试追踪数据库操作失败");
    DebugTraceRepositoryError::Query
}

pub(super) fn internal_error(error: DebugTraceRepositoryError) -> DebugTraceRepositoryError {
    let kind = match error {
        DebugTraceRepositoryError::Query => "debug_trace_query",
        DebugTraceRepositoryError::Timeout => "debug_trace_timeout",
        DebugTraceRepositoryError::Invariant => "debug_trace_invariant",
        DebugTraceRepositoryError::InvalidInput => "debug_trace_invalid_input",
        DebugTraceRepositoryError::NotFound => "debug_trace_not_found",
        DebugTraceRepositoryError::Decrypt => "debug_trace_decrypt",
    };
    tracing::error!(target: "af_db::debug_trace", error_kind = kind, "调试追踪仓储拒绝损坏状态");
    error
}
