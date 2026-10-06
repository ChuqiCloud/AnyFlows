use std::{fmt, time::Duration};

use af_domain::{AsyncTaskId, AsyncTaskRequestId, UserId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    Set, TransactionTrait, sea_query::Expr,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    async_task::{AsyncTaskRepositoryConfigError, AsyncTaskRepositoryError},
    entity::{SensitiveString, async_task_submission_claims, credentials, groups, tokens},
};

use super::record::{
    claim_record, classify_claim_collision, credential_revision_key, expected_version,
    load_by_database_id, load_by_owner, load_by_request, load_claim_collision, next_version,
    state_code_for_claim, to_database_time,
};
use super::types::{
    AsyncTaskSubmissionAccept, AsyncTaskSubmissionBegin, AsyncTaskSubmissionClaim,
    AsyncTaskSubmissionClaimOutcome, AsyncTaskSubmissionMutationOutcome, AsyncTaskSubmissionRecord,
    AsyncTaskSubmissionRelease, AsyncTaskSubmissionState,
};
use crate::async_task::status::status_to_persistence;

/// 原子维护异步任务提交所有权、结果未知边界和恢复绑定的仓储。
#[derive(Clone)]
pub struct AsyncTaskSubmissionRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl AsyncTaskSubmissionRepository {
    /// 使用共享连接池和单次数据库操作截止时间构造提交仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, AsyncTaskRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(AsyncTaskRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 按用户幂等键创建永久 claim；同一幂等键只接受相同请求指纹。
    pub async fn claim(
        &self,
        write: AsyncTaskSubmissionClaim,
    ) -> Result<AsyncTaskSubmissionClaimOutcome, AsyncTaskRepositoryError> {
        let operation = self
            .claim_inner(&write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::OutcomeUnknown)),
        }
    }

    /// 只在指定用户范围内按本地任务标识读取提交 claim。
    pub async fn find(
        &self,
        user_id: UserId,
        task_id: AsyncTaskId,
    ) -> Result<Option<AsyncTaskSubmissionRecord>, AsyncTaskRepositoryError> {
        let operation = load_by_owner(self.pool.connection(), user_id, task_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::Timeout)),
        }
    }

    /// 按用户与客户端幂等标识读取提交 claim。
    pub async fn find_by_request(
        &self,
        user_id: UserId,
        request_id: AsyncTaskRequestId,
    ) -> Result<Option<AsyncTaskSubmissionRecord>, AsyncTaskRepositoryError> {
        let operation = load_by_request(self.pool.connection(), user_id, request_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::Timeout)),
        }
    }

    /// 由随机尝试所有者独占 claim；相同命令允许在结果未知后重放。
    pub async fn begin(
        &self,
        write: AsyncTaskSubmissionBegin,
    ) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError> {
        let operation = begin_submission(self.pool.connection(), &write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::OutcomeUnknown)),
        }
    }

    /// 只有确定未被上游接受时，原尝试所有者才能释放 claim。
    pub async fn release(
        &self,
        write: AsyncTaskSubmissionRelease,
    ) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError> {
        let operation = release_submission(self.pool.connection(), &write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::OutcomeUnknown)),
        }
    }

    /// 保存上游已接受任务的完整恢复绑定；相同绑定允许安全重放。
    pub async fn accept(
        &self,
        write: AsyncTaskSubmissionAccept,
    ) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError> {
        let operation = self
            .accept_inner(&write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::OutcomeUnknown)),
        }
    }

    async fn claim_inner(
        &self,
        write: &AsyncTaskSubmissionClaim,
    ) -> Result<AsyncTaskSubmissionClaimOutcome, AsyncTaskRepositoryError> {
        if let Some(existing) = load_claim_collision(self.pool.connection(), write).await? {
            return classify_claim_collision(existing, write);
        }
        let transaction = begin_transaction(&self.pool).await?;
        let result = claim_in_transaction(&transaction, write).await;
        match result {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(SubmissionWriteError::UniqueConflict) => {
                rollback(transaction).await?;
                let existing = load_claim_collision(self.pool.connection(), write)
                    .await?
                    .ok_or(AsyncTaskRepositoryError::Invariant)?;
                classify_claim_collision(existing, write)
            }
            Err(SubmissionWriteError::Concurrent) => {
                rollback(transaction).await?;
                Err(AsyncTaskRepositoryError::Conflict)
            }
            Err(SubmissionWriteError::Repository(error)) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }

    async fn accept_inner(
        &self,
        write: &AsyncTaskSubmissionAccept,
    ) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError> {
        let transaction = begin_transaction(&self.pool).await?;
        let result = accept_in_transaction(&transaction, write).await;
        match result {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(SubmissionWriteError::Concurrent) => {
                rollback(transaction).await?;
                let Some(existing) =
                    load_by_owner(self.pool.connection(), write.user_id, write.task_id).await?
                else {
                    return Ok(AsyncTaskSubmissionMutationOutcome::NotFound);
                };
                classify_accept_replay(existing, write)
            }
            Err(SubmissionWriteError::UniqueConflict) => {
                rollback(transaction).await?;
                Err(AsyncTaskRepositoryError::Invariant)
            }
            Err(SubmissionWriteError::Repository(error)) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }
}

impl fmt::Debug for AsyncTaskSubmissionRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncTaskSubmissionRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

enum SubmissionWriteError {
    UniqueConflict,
    Concurrent,
    Repository(AsyncTaskRepositoryError),
}

impl From<AsyncTaskRepositoryError> for SubmissionWriteError {
    fn from(error: AsyncTaskRepositoryError) -> Self {
        Self::Repository(error)
    }
}

async fn claim_in_transaction(
    transaction: &DatabaseTransaction,
    write: &AsyncTaskSubmissionClaim,
) -> Result<AsyncTaskSubmissionClaimOutcome, SubmissionWriteError> {
    if let Some(existing) = load_claim_collision(transaction, write).await? {
        return classify_claim_collision(existing, write).map_err(Into::into);
    }
    if !claim_references_match(transaction, write).await? {
        return Ok(AsyncTaskSubmissionClaimOutcome::NotFound);
    }
    let created_at = to_database_time(write.created_at)?;
    let inserted = async_task_submission_claims::ActiveModel {
        task_key: Set(SensitiveString::from(write.task_id.persistence_key())),
        user_id: Set(write.principal.user_id().get()),
        token_id: Set(write.principal.token_id().get()),
        group_id: Set(write.principal.group_id().get()),
        idempotency_key: Set(SensitiveString::from(write.request_id.persistence_key())),
        protocol: Set(write.protocol.as_str().to_owned()),
        requested_model: Set(SensitiveString::from(write.requested_model.clone())),
        request_fingerprint: Set(SensitiveString::from(
            write.request_fingerprint.persistence_key(),
        )),
        video_duration_seconds: Set(write.video_duration_seconds.map(i16::from)),
        state: Set(state_code_for_claim(AsyncTaskSubmissionState::Claimed)),
        attempt_key: Set(None),
        target_group_id: Set(None),
        upstream_model: Set(None),
        channel_id: Set(None),
        credential_id: Set(None),
        credential_revision: Set(None),
        upstream_task_id: Set(None),
        binding_fingerprint: Set(None),
        attempt_timeout_millis: Set(None),
        video_resolution: Set(None),
        status: Set(None),
        progress_basis_points: Set(None),
        failure_kind: Set(None),
        version: Set(1),
        accepted_at: Set(None),
        created_at: Set(created_at),
        updated_at: Set(created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await;
    match inserted {
        Ok(model) => Ok(AsyncTaskSubmissionClaimOutcome::Created(claim_record(
            model,
        )?)),
        Err(error) if is_unique_conflict(&error) => Err(SubmissionWriteError::UniqueConflict),
        Err(_) => Err(AsyncTaskRepositoryError::Query.into()),
    }
}

async fn begin_submission<C>(
    connection: &C,
    write: &AsyncTaskSubmissionBegin,
) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(current) = load_by_owner(connection, write.user_id, write.task_id).await? else {
        return Ok(AsyncTaskSubmissionMutationOutcome::NotFound);
    };
    let expected_version = expected_version(write.expected_version)?;
    if current.state == AsyncTaskSubmissionState::Submitting
        && current.attempt_id == Some(write.attempt_id)
        && (current.version == expected_version || current.version == expected_version + 1)
    {
        return Ok(AsyncTaskSubmissionMutationOutcome::Existing(current));
    }
    if current.state != AsyncTaskSubmissionState::Claimed
        || current.version != expected_version
        || write.observed_at < current.updated_at
    {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    let next_version = next_version(write.expected_version)?;
    let updated_at = to_database_time(write.observed_at)?;
    let update = async_task_submission_claims::Entity::update_many()
        .filter(async_task_submission_claims::Column::Id.eq(current.database_id))
        .filter(async_task_submission_claims::Column::UserId.eq(write.user_id.get()))
        .filter(async_task_submission_claims::Column::Version.eq(write.expected_version))
        .filter(async_task_submission_claims::Column::State.eq(1_i16))
        .col_expr(
            async_task_submission_claims::Column::State,
            Expr::value(2_i16),
        )
        .col_expr(
            async_task_submission_claims::Column::AttemptKey,
            Expr::value(Some(SensitiveString::from(
                write.attempt_id.persistence_key(),
            ))),
        )
        .col_expr(
            async_task_submission_claims::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            async_task_submission_claims::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .exec(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return classify_begin_after_concurrent(connection, write).await;
    }
    let updated = load_by_database_id(connection, current.database_id)
        .await?
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    Ok(AsyncTaskSubmissionMutationOutcome::Applied(updated))
}

async fn release_submission<C>(
    connection: &C,
    write: &AsyncTaskSubmissionRelease,
) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(current) = load_by_owner(connection, write.user_id, write.task_id).await? else {
        return Ok(AsyncTaskSubmissionMutationOutcome::NotFound);
    };
    let expected_version = expected_version(write.expected_version)?;
    if current.state == AsyncTaskSubmissionState::Claimed
        && current.attempt_id.is_none()
        && (current.version == expected_version || current.version == expected_version + 1)
    {
        return Ok(AsyncTaskSubmissionMutationOutcome::Existing(current));
    }
    if current.state != AsyncTaskSubmissionState::Submitting
        || current.attempt_id != Some(write.attempt_id)
        || current.version != expected_version
        || write.observed_at < current.updated_at
    {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    let next_version = next_version(write.expected_version)?;
    let updated_at = to_database_time(write.observed_at)?;
    let update = async_task_submission_claims::Entity::update_many()
        .filter(async_task_submission_claims::Column::Id.eq(current.database_id))
        .filter(async_task_submission_claims::Column::UserId.eq(write.user_id.get()))
        .filter(async_task_submission_claims::Column::Version.eq(write.expected_version))
        .filter(async_task_submission_claims::Column::State.eq(2_i16))
        .filter(
            async_task_submission_claims::Column::AttemptKey
                .eq(SensitiveString::from(write.attempt_id.persistence_key())),
        )
        .col_expr(
            async_task_submission_claims::Column::State,
            Expr::value(1_i16),
        )
        .col_expr(
            async_task_submission_claims::Column::AttemptKey,
            Expr::value(Option::<SensitiveString>::None),
        )
        .col_expr(
            async_task_submission_claims::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            async_task_submission_claims::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .exec(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return classify_release_after_concurrent(connection, write).await;
    }
    let updated = load_by_database_id(connection, current.database_id)
        .await?
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    Ok(AsyncTaskSubmissionMutationOutcome::Applied(updated))
}

async fn accept_in_transaction(
    transaction: &DatabaseTransaction,
    write: &AsyncTaskSubmissionAccept,
) -> Result<AsyncTaskSubmissionMutationOutcome, SubmissionWriteError> {
    let Some(current) = load_by_owner(transaction, write.user_id, write.task_id).await? else {
        return Ok(AsyncTaskSubmissionMutationOutcome::NotFound);
    };
    let expected_version = expected_version(write.expected_version)?;
    if current.matches_accept(write)
        && (current.version == expected_version || current.version == expected_version + 1)
    {
        return Ok(AsyncTaskSubmissionMutationOutcome::Existing(current));
    }
    if current.state != AsyncTaskSubmissionState::Submitting
        || current.attempt_id != Some(write.attempt_id)
        || current.version != expected_version
        || write.observed_at < current.updated_at
    {
        return Err(SubmissionWriteError::Concurrent);
    }
    if !binding_references_match(transaction, write).await? {
        return Ok(AsyncTaskSubmissionMutationOutcome::NotFound);
    }
    let next_version = next_version(write.expected_version)?;
    let accepted_at = to_database_time(write.observed_at)?;
    let (status, progress, failure_kind) = status_to_persistence(&write.status);
    let update = async_task_submission_claims::Entity::update_many()
        .filter(async_task_submission_claims::Column::Id.eq(current.database_id))
        .filter(async_task_submission_claims::Column::UserId.eq(write.user_id.get()))
        .filter(async_task_submission_claims::Column::Version.eq(write.expected_version))
        .filter(async_task_submission_claims::Column::State.eq(2_i16))
        .filter(
            async_task_submission_claims::Column::AttemptKey
                .eq(SensitiveString::from(write.attempt_id.persistence_key())),
        )
        .col_expr(
            async_task_submission_claims::Column::State,
            Expr::value(3_i16),
        )
        .col_expr(
            async_task_submission_claims::Column::TargetGroupId,
            Expr::value(Some(write.target_group_id.get())),
        )
        .col_expr(
            async_task_submission_claims::Column::UpstreamModel,
            Expr::value(Some(SensitiveString::from(write.upstream_model.clone()))),
        )
        .col_expr(
            async_task_submission_claims::Column::ChannelId,
            Expr::value(Some(write.channel_id.get())),
        )
        .col_expr(
            async_task_submission_claims::Column::CredentialId,
            Expr::value(Some(write.credential_id.get())),
        )
        .col_expr(
            async_task_submission_claims::Column::CredentialRevision,
            Expr::value(Some(SensitiveString::from(credential_revision_key(
                write.credential_revision,
            )))),
        )
        .col_expr(
            async_task_submission_claims::Column::UpstreamTaskId,
            Expr::value(Some(SensitiveString::from(
                write.upstream_task_id.as_str().to_owned(),
            ))),
        )
        .col_expr(
            async_task_submission_claims::Column::BindingFingerprint,
            Expr::value(Some(SensitiveString::from(
                write.binding_fingerprint.persistence_key(),
            ))),
        )
        .col_expr(
            async_task_submission_claims::Column::AttemptTimeoutMillis,
            Expr::value(Some(write.attempt_timeout_millis)),
        )
        .col_expr(
            async_task_submission_claims::Column::VideoResolution,
            Expr::value(write.video_resolution.map(|value| value.database_value())),
        )
        .col_expr(
            async_task_submission_claims::Column::Status,
            Expr::value(Some(status)),
        )
        .col_expr(
            async_task_submission_claims::Column::ProgressBasisPoints,
            Expr::value(Some(progress)),
        )
        .col_expr(
            async_task_submission_claims::Column::FailureKind,
            Expr::value(failure_kind),
        )
        .col_expr(
            async_task_submission_claims::Column::Version,
            Expr::value(next_version),
        )
        .col_expr(
            async_task_submission_claims::Column::AcceptedAt,
            Expr::value(Some(accepted_at)),
        )
        .col_expr(
            async_task_submission_claims::Column::UpdatedAt,
            Expr::value(accepted_at),
        )
        .exec(transaction)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(SubmissionWriteError::Concurrent);
    }
    let updated = load_by_database_id(transaction, current.database_id)
        .await?
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    Ok(AsyncTaskSubmissionMutationOutcome::Applied(updated))
}

async fn classify_begin_after_concurrent<C>(
    connection: &C,
    write: &AsyncTaskSubmissionBegin,
) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(existing) = load_by_owner(connection, write.user_id, write.task_id).await? else {
        return Ok(AsyncTaskSubmissionMutationOutcome::NotFound);
    };
    let expected = expected_version(write.expected_version)?;
    if existing.state == AsyncTaskSubmissionState::Submitting
        && existing.attempt_id == Some(write.attempt_id)
        && (existing.version == expected || existing.version == expected + 1)
    {
        Ok(AsyncTaskSubmissionMutationOutcome::Existing(existing))
    } else {
        Err(AsyncTaskRepositoryError::Conflict)
    }
}

async fn classify_release_after_concurrent<C>(
    connection: &C,
    write: &AsyncTaskSubmissionRelease,
) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let Some(existing) = load_by_owner(connection, write.user_id, write.task_id).await? else {
        return Ok(AsyncTaskSubmissionMutationOutcome::NotFound);
    };
    let expected = expected_version(write.expected_version)?;
    if existing.state == AsyncTaskSubmissionState::Claimed
        && existing.attempt_id.is_none()
        && (existing.version == expected || existing.version == expected + 1)
    {
        Ok(AsyncTaskSubmissionMutationOutcome::Existing(existing))
    } else {
        Err(AsyncTaskRepositoryError::Conflict)
    }
}

fn classify_accept_replay(
    existing: AsyncTaskSubmissionRecord,
    write: &AsyncTaskSubmissionAccept,
) -> Result<AsyncTaskSubmissionMutationOutcome, AsyncTaskRepositoryError> {
    let expected = expected_version(write.expected_version)?;
    if existing.matches_accept(write)
        && (existing.version == expected || existing.version == expected + 1)
    {
        Ok(AsyncTaskSubmissionMutationOutcome::Existing(existing))
    } else {
        Err(AsyncTaskRepositoryError::Conflict)
    }
}

async fn claim_references_match(
    transaction: &DatabaseTransaction,
    write: &AsyncTaskSubmissionClaim,
) -> Result<bool, AsyncTaskRepositoryError> {
    let token_matches = tokens::Entity::find_by_id(write.principal.token_id().get())
        .filter(tokens::Column::UserId.eq(write.principal.user_id().get()))
        .one(transaction)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .is_some();
    let group_exists = groups::Entity::find_by_id(write.principal.group_id().get())
        .one(transaction)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .is_some();
    Ok(token_matches && group_exists)
}

async fn binding_references_match(
    transaction: &DatabaseTransaction,
    write: &AsyncTaskSubmissionAccept,
) -> Result<bool, AsyncTaskRepositoryError> {
    let group_exists = groups::Entity::find_by_id(write.target_group_id.get())
        .one(transaction)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .is_some();
    let credential_matches = credentials::Entity::find_by_id(write.credential_id.get())
        .filter(credentials::Column::ChannelId.eq(write.channel_id.get()))
        .one(transaction)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .is_some();
    Ok(group_exists && credential_matches)
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_async_task_submission_claims_task_key")
        || rendered.contains("uq_async_task_submission_claims_owner_idempotency")
        || rendered.contains("async_task_submission_claims.task_key")
        || rendered.contains(
            "async_task_submission_claims.user_id, async_task_submission_claims.idempotency_key",
        )
        || rendered.contains("Duplicate entry")
}

async fn begin_transaction(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, AsyncTaskRepositoryError> {
    pool.connection()
        .begin()
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)
}

async fn commit(transaction: DatabaseTransaction) -> Result<(), AsyncTaskRepositoryError> {
    transaction
        .commit()
        .await
        .map_err(|_| AsyncTaskRepositoryError::OutcomeUnknown)
}

async fn rollback(transaction: DatabaseTransaction) -> Result<(), AsyncTaskRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| AsyncTaskRepositoryError::OutcomeUnknown)
}

/// 只记录闭合错误分类，避免请求指纹、任务标识和绑定事实进入日志。
fn internal(error: AsyncTaskRepositoryError) -> AsyncTaskRepositoryError {
    let error_kind = match error {
        AsyncTaskRepositoryError::Conflict => return error,
        AsyncTaskRepositoryError::Query => "async_task_submission_query",
        AsyncTaskRepositoryError::OutcomeUnknown => "async_task_submission_outcome_unknown",
        AsyncTaskRepositoryError::Timeout => "async_task_submission_timeout",
        AsyncTaskRepositoryError::Invariant => "async_task_submission_invariant",
    };
    tracing::error!(
        target: "af_db::async_task_submission",
        error_kind,
        "异步任务提交仓储发生内部错误"
    );
    error
}
