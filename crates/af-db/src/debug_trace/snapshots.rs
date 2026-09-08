use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone,
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{
        EncryptedJson, debug_trace_attempts, debug_trace_snapshot_access_audits,
        debug_trace_snapshots, debug_traces,
    },
};

use super::{
    repository::{DebugTraceRepositoryError, internal_error, query_error},
    types::{
        DebugTraceAttemptSnapshotRecord, DebugTraceSnapshotCipher, DebugTraceSnapshotContext,
        DebugTraceSnapshotKind, DebugTraceSnapshotPlaintext, DebugTraceSnapshotRecord,
        DebugTraceSnapshotScope, valid_snapshot,
    },
};

/// 使用真实追踪与 Attempt ID 加密单个永久脱敏字段。
pub(super) async fn insert_snapshot(
    transaction: &DatabaseTransaction,
    cipher: &dyn DebugTraceSnapshotCipher,
    context: DebugTraceSnapshotContext,
    plaintext: &str,
    created_at: TimeDateTimeWithTimeZone,
) -> Result<(), DebugTraceRepositoryError> {
    if !valid_snapshot(Some(plaintext)) {
        return Err(internal_error(DebugTraceRepositoryError::Invariant));
    }
    let envelope = cipher
        .encrypt(context, plaintext)
        .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))?;
    debug_trace_snapshots::ActiveModel {
        id: sea_orm::NotSet,
        trace_id: Set(context.trace_id()),
        attempt_id: Set(context.attempt_id()),
        kind: Set(context.kind().as_i16()),
        encrypted_payload: Set(encrypted_json_from_envelope(envelope)?),
        created_at: Set(created_at),
    }
    .insert(transaction)
    .with_subscriber(NoSubscriber::default())
    .await
    .map(|_| ())
    .map_err(|_| query_error("debug_trace_snapshot_insert"))
}

/// 在同一事务内完成读取、解密和审计提交，审计失败时不返回明文。
pub(super) async fn read_snapshots(
    pool: &DatabasePool,
    cipher: &dyn DebugTraceSnapshotCipher,
    trace_id: i64,
    actor_user_id: i64,
    scope: DebugTraceSnapshotScope,
) -> Result<DebugTraceSnapshotRecord, DebugTraceRepositoryError> {
    let transaction = pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("debug_trace_snapshot_read_begin"))?;
    let trace_exists = debug_traces::Entity::find_by_id(trace_id)
        .select_only()
        .column(debug_traces::Column::Id)
        .into_tuple::<i64>()
        .one(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("debug_trace_snapshot_trace_read"))?
        .is_some();
    if !trace_exists {
        audit_and_commit(
            transaction,
            trace_id,
            actor_user_id,
            scope,
            SnapshotAccessOutcome::NotFound,
        )
        .await?;
        return Err(DebugTraceRepositoryError::NotFound);
    }

    let attempts = debug_trace_attempts::Entity::find()
        .filter(debug_trace_attempts::Column::TraceId.eq(trace_id))
        .all(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("debug_trace_snapshot_attempt_read"))?;
    let attempt_positions = attempts
        .iter()
        .map(|attempt| (attempt.id, attempt.candidate_index))
        .collect::<BTreeMap<_, _>>();
    if attempt_positions.len() != attempts.len() {
        return Err(internal_error(DebugTraceRepositoryError::Invariant));
    }
    let kinds = scope_kinds(scope);
    let models = debug_trace_snapshots::Entity::find()
        .filter(debug_trace_snapshots::Column::TraceId.eq(trace_id))
        .filter(debug_trace_snapshots::Column::Kind.is_in(kinds))
        .order_by_asc(debug_trace_snapshots::Column::Kind)
        .order_by_asc(debug_trace_snapshots::Column::AttemptId)
        .all(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await;
    let models = match models {
        Ok(models) => models,
        Err(_) => {
            audit_and_commit(
                transaction,
                trace_id,
                actor_user_id,
                scope,
                SnapshotAccessOutcome::DecryptFailed,
            )
            .await?;
            return Err(internal_error(DebugTraceRepositoryError::Decrypt));
        }
    };
    if models.is_empty() {
        audit_and_commit(
            transaction,
            trace_id,
            actor_user_id,
            scope,
            SnapshotAccessOutcome::NotFound,
        )
        .await?;
        return Err(DebugTraceRepositoryError::NotFound);
    }

    let record = match decrypt_snapshot_models(cipher, trace_id, scope, &attempt_positions, models)
    {
        Ok(record) => record,
        Err(()) => {
            audit_and_commit(
                transaction,
                trace_id,
                actor_user_id,
                scope,
                SnapshotAccessOutcome::DecryptFailed,
            )
            .await?;
            return Err(internal_error(DebugTraceRepositoryError::Decrypt));
        }
    };
    audit_and_commit(
        transaction,
        trace_id,
        actor_user_id,
        scope,
        SnapshotAccessOutcome::Succeeded,
    )
    .await?;
    Ok(record)
}

/// 按追踪保留期清理独立审计，避免未知 ID 探测记录无限增长。
pub(super) async fn prune_access_audits(
    transaction: &DatabaseTransaction,
    cutoff: TimeDateTimeWithTimeZone,
) -> Result<(), DebugTraceRepositoryError> {
    debug_trace_snapshot_access_audits::Entity::delete_many()
        .filter(debug_trace_snapshot_access_audits::Column::CreatedAt.lt(cutoff))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|_| ())
        .map_err(|_| query_error("debug_trace_snapshot_access_prune"))
}

fn scope_kinds(scope: DebugTraceSnapshotScope) -> [i16; 3] {
    match scope {
        DebugTraceSnapshotScope::Headers => [
            DebugTraceSnapshotKind::DownstreamHeaders.as_i16(),
            DebugTraceSnapshotKind::AttemptRequestHeaders.as_i16(),
            DebugTraceSnapshotKind::AttemptResponseHeaders.as_i16(),
        ],
        DebugTraceSnapshotScope::Bodies => [
            DebugTraceSnapshotKind::DownstreamBody.as_i16(),
            DebugTraceSnapshotKind::AttemptRequestBody.as_i16(),
            DebugTraceSnapshotKind::AttemptResponseBody.as_i16(),
        ],
    }
}

fn encrypted_json_from_envelope(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, DebugTraceRepositoryError> {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| internal_error(DebugTraceRepositoryError::Invariant))
}

fn envelope_from_encrypted_json(
    encrypted: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, ()> {
    let (key_id, nonce, ciphertext) = encrypted.envelope_parts().map_err(|_| ())?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext).map_err(|_| ())
}

#[derive(Clone, Copy)]
enum SnapshotAccessOutcome {
    Succeeded,
    NotFound,
    DecryptFailed,
}

impl SnapshotAccessOutcome {
    const fn as_i16(self) -> i16 {
        match self {
            Self::Succeeded => 1,
            Self::NotFound => 2,
            Self::DecryptFailed => 3,
        }
    }
}

async fn audit_and_commit(
    transaction: DatabaseTransaction,
    trace_id: i64,
    actor_user_id: i64,
    scope: DebugTraceSnapshotScope,
    outcome: SnapshotAccessOutcome,
) -> Result<(), DebugTraceRepositoryError> {
    debug_trace_snapshot_access_audits::ActiveModel {
        id: sea_orm::NotSet,
        trace_id: Set(trace_id),
        actor_user_id: Set(actor_user_id),
        scope: Set(scope.as_i16()),
        outcome: Set(outcome.as_i16()),
        created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
    }
    .insert(&transaction)
    .with_subscriber(NoSubscriber::default())
    .await
    .map_err(|_| query_error("debug_trace_snapshot_access_insert"))?;
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("debug_trace_snapshot_access_commit"))
}

fn decrypt_snapshot_models(
    cipher: &dyn DebugTraceSnapshotCipher,
    trace_id: i64,
    scope: DebugTraceSnapshotScope,
    attempt_positions: &BTreeMap<i64, i16>,
    models: Vec<debug_trace_snapshots::Model>,
) -> Result<DebugTraceSnapshotRecord, ()> {
    let mut downstream = None;
    let mut attempts = BTreeMap::<i16, AttemptSnapshotParts>::new();
    for model in models {
        if model.id < 1 || model.trace_id != trace_id {
            return Err(());
        }
        let kind = DebugTraceSnapshotKind::from_i16(model.kind).map_err(|_| ())?;
        if kind.scope() != scope || model.attempt_id.is_some() != kind.requires_attempt() {
            return Err(());
        }
        let context =
            DebugTraceSnapshotContext::new(trace_id, model.attempt_id, kind).map_err(|_| ())?;
        let envelope = envelope_from_encrypted_json(model.encrypted_payload)?;
        let plaintext = cipher.decrypt(context, &envelope).map_err(|_| ())?;
        match kind {
            DebugTraceSnapshotKind::DownstreamHeaders | DebugTraceSnapshotKind::DownstreamBody => {
                if downstream.replace(plaintext).is_some() {
                    return Err(());
                }
            }
            DebugTraceSnapshotKind::AttemptRequestHeaders
            | DebugTraceSnapshotKind::AttemptRequestBody => {
                let attempt_id = model.attempt_id.ok_or(())?;
                let candidate_index = *attempt_positions.get(&attempt_id).ok_or(())?;
                if attempts
                    .entry(candidate_index)
                    .or_default()
                    .request
                    .replace(plaintext)
                    .is_some()
                {
                    return Err(());
                }
            }
            DebugTraceSnapshotKind::AttemptResponseHeaders
            | DebugTraceSnapshotKind::AttemptResponseBody => {
                let attempt_id = model.attempt_id.ok_or(())?;
                let candidate_index = *attempt_positions.get(&attempt_id).ok_or(())?;
                if attempts
                    .entry(candidate_index)
                    .or_default()
                    .response
                    .replace(plaintext)
                    .is_some()
                {
                    return Err(());
                }
            }
        }
    }
    Ok(DebugTraceSnapshotRecord::new(
        scope,
        downstream,
        attempts
            .into_iter()
            .map(|(candidate_index, parts)| {
                DebugTraceAttemptSnapshotRecord::new(candidate_index, parts.request, parts.response)
            })
            .collect(),
    ))
}

#[derive(Default)]
struct AttemptSnapshotParts {
    request: Option<DebugTraceSnapshotPlaintext>,
    response: Option<DebugTraceSnapshotPlaintext>,
}
