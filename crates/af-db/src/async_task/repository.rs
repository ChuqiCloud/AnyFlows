use std::{fmt, str::FromStr, time::Duration};

use af_domain::{
    AsyncTaskId, AsyncTaskRequestId, ChannelId, CredentialId, GatewayPrincipal, GroupId, Protocol,
    TokenId, UpstreamTaskId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseTransaction, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{SensitiveString, async_tasks, credentials, groups, tokens},
};

use super::status::{
    allowed_transition, same_persisted_status, state_code, status_from_persistence,
    status_to_persistence,
};
use super::types::{
    AsyncTaskCreate, AsyncTaskCreateOutcome, AsyncTaskPageCursor, AsyncTaskPageRecord,
    AsyncTaskRecord, AsyncTaskRepositoryConfigError, AsyncTaskRepositoryError, AsyncTaskTransition,
    AsyncTaskTransitionOutcome,
};

/// 单次异步任务历史读取的最大页容量。
pub const MAX_ASYNC_TASK_PAGE_SIZE: usize = 100;

/// 原子维护异步任务幂等创建与前向状态迁移的数据库仓储。
#[derive(Clone)]
pub struct AsyncTaskRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl AsyncTaskRepository {
    /// 使用共享连接池和单次数据库操作截止时间构造仓储。
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

    /// 按用户范围幂等创建异步任务，并验证令牌和凭据的归属关系。
    pub async fn create(
        &self,
        write: AsyncTaskCreate,
    ) -> Result<AsyncTaskCreateOutcome, AsyncTaskRepositoryError> {
        let operation = self
            .create_inner(&write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::OutcomeUnknown)),
        }
    }

    /// 只在指定用户范围内读取任务，避免公开键泄露跨用户状态。
    pub async fn find(
        &self,
        user_id: UserId,
        task_id: AsyncTaskId,
    ) -> Result<Option<AsyncTaskRecord>, AsyncTaskRepositoryError> {
        let operation = load_by_owner(self.pool.connection(), user_id, task_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::Timeout)),
        }
    }

    /// 按用户与客户端幂等标识读取已经完成绑定的任务。
    pub async fn find_by_request(
        &self,
        user_id: UserId,
        request_id: AsyncTaskRequestId,
    ) -> Result<Option<AsyncTaskRecord>, AsyncTaskRepositoryError> {
        let operation = load_by_request(self.pool.connection(), user_id, request_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::Timeout)),
        }
    }

    /// 按用户和协议从新到旧读取持久化任务，不访问任何上游服务。
    pub async fn list(
        &self,
        user_id: UserId,
        protocol: Protocol,
        before: Option<AsyncTaskPageCursor>,
        limit: usize,
    ) -> Result<AsyncTaskPageRecord, AsyncTaskRepositoryError> {
        if !(1..=MAX_ASYNC_TASK_PAGE_SIZE).contains(&limit) {
            return Err(internal(AsyncTaskRepositoryError::Invariant));
        }
        let operation = list_by_owner(self.pool.connection(), user_id, protocol, before, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::Timeout)),
        }
    }

    /// 使用预期版本执行一次严格前向状态迁移；相同命令允许安全重放。
    pub async fn transition(
        &self,
        write: AsyncTaskTransition,
    ) -> Result<AsyncTaskTransitionOutcome, AsyncTaskRepositoryError> {
        let operation = self
            .transition_inner(&write)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(internal),
            Err(_) => Err(internal(AsyncTaskRepositoryError::OutcomeUnknown)),
        }
    }

    async fn create_inner(
        &self,
        write: &AsyncTaskCreate,
    ) -> Result<AsyncTaskCreateOutcome, AsyncTaskRepositoryError> {
        if let Some(existing) = load_create_collision(self.pool.connection(), write).await? {
            return classify_create_collision(existing, write);
        }

        let transaction = begin(&self.pool).await?;
        let result = create_in_transaction(&transaction, write).await;
        match result {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(TransactionWriteError::UniqueConflict) => {
                rollback(transaction).await?;
                let existing = load_create_collision(self.pool.connection(), write)
                    .await?
                    .ok_or(AsyncTaskRepositoryError::Invariant)?;
                classify_create_collision(existing, write)
            }
            Err(TransactionWriteError::Concurrent) => {
                rollback(transaction).await?;
                Err(AsyncTaskRepositoryError::Conflict)
            }
            Err(TransactionWriteError::Repository(error)) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }

    async fn transition_inner(
        &self,
        write: &AsyncTaskTransition,
    ) -> Result<AsyncTaskTransitionOutcome, AsyncTaskRepositoryError> {
        let transaction = begin(&self.pool).await?;
        match transition_in_transaction(&transaction, write).await {
            Ok(outcome) => {
                commit(transaction).await?;
                Ok(outcome)
            }
            Err(TransactionWriteError::Concurrent) => {
                rollback(transaction).await?;
                let Some(existing) =
                    load_by_owner(self.pool.connection(), write.user_id, write.task_id).await?
                else {
                    return Ok(AsyncTaskTransitionOutcome::NotFound);
                };
                classify_transition_replay(existing, write)
            }
            Err(TransactionWriteError::UniqueConflict) => {
                rollback(transaction).await?;
                Err(AsyncTaskRepositoryError::Invariant)
            }
            Err(TransactionWriteError::Repository(error)) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }
}

impl fmt::Debug for AsyncTaskRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncTaskRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

enum TransactionWriteError {
    UniqueConflict,
    Concurrent,
    Repository(AsyncTaskRepositoryError),
}

impl From<AsyncTaskRepositoryError> for TransactionWriteError {
    fn from(error: AsyncTaskRepositoryError) -> Self {
        Self::Repository(error)
    }
}

async fn create_in_transaction(
    transaction: &DatabaseTransaction,
    write: &AsyncTaskCreate,
) -> Result<AsyncTaskCreateOutcome, TransactionWriteError> {
    if let Some(existing) = load_create_collision(transaction, write).await? {
        return classify_create_collision(existing, write).map_err(Into::into);
    }
    if !references_match(transaction, write).await? {
        return Ok(AsyncTaskCreateOutcome::NotFound);
    }

    let created_at = to_database_time(write.created_at)?;
    let observed_at = to_database_time(write.observed_at)?;
    let (status, progress_basis_points, failure_kind) = status_to_persistence(&write.status);
    let terminal_at = write.status.is_terminal().then_some(observed_at);
    let inserted = async_tasks::ActiveModel {
        task_key: Set(SensitiveString::from(write.task_id.persistence_key())),
        user_id: Set(write.principal.user_id().get()),
        token_id: Set(write.principal.token_id().get()),
        group_id: Set(write.principal.group_id().get()),
        idempotency_key: Set(SensitiveString::from(write.request_id.persistence_key())),
        protocol: Set(write.protocol.as_str().to_owned()),
        requested_model: Set(SensitiveString::from(write.requested_model.clone())),
        upstream_model: Set(SensitiveString::from(write.upstream_model.clone())),
        channel_id: Set(write.channel_id.get()),
        credential_id: Set(write.credential_id.get()),
        credential_revision: Set(SensitiveString::from(credential_revision_key(
            write.credential_revision,
        ))),
        upstream_task_id: Set(SensitiveString::from(
            write.upstream_task_id.as_str().to_owned(),
        )),
        status: Set(status),
        progress_basis_points: Set(progress_basis_points),
        failure_kind: Set(failure_kind),
        version: Set(1),
        terminal_at: Set(terminal_at),
        created_at: Set(created_at),
        updated_at: Set(observed_at),
        ..Default::default()
    }
    .insert(transaction)
    .await;
    match inserted {
        Ok(model) => Ok(AsyncTaskCreateOutcome::Created(task_record(model)?)),
        Err(error) if is_unique_conflict(&error) => Err(TransactionWriteError::UniqueConflict),
        Err(_) => Err(AsyncTaskRepositoryError::Query.into()),
    }
}

async fn transition_in_transaction(
    transaction: &DatabaseTransaction,
    write: &AsyncTaskTransition,
) -> Result<AsyncTaskTransitionOutcome, TransactionWriteError> {
    let Some(current) = load_by_owner(transaction, write.user_id, write.task_id).await? else {
        return Ok(AsyncTaskTransitionOutcome::NotFound);
    };
    let current_version =
        i64::try_from(current.version).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    if (current_version == write.expected_version || current_version == write.expected_version + 1)
        && same_persisted_status(&current.status, &write.status)
    {
        return Ok(AsyncTaskTransitionOutcome::Existing(current));
    }
    if current_version != write.expected_version
        || !allowed_transition(&current.status, &write.status)
        || write.observed_at < current.updated_at
    {
        return Err(AsyncTaskRepositoryError::Conflict.into());
    }

    let next_version = write
        .expected_version
        .checked_add(1)
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    let observed_at = to_database_time(write.observed_at)?;
    let (status, progress_basis_points, failure_kind) = status_to_persistence(&write.status);
    let update = async_tasks::Entity::update_many()
        .filter(async_tasks::Column::Id.eq(current.database_id))
        .filter(async_tasks::Column::UserId.eq(write.user_id.get()))
        .filter(async_tasks::Column::Version.eq(write.expected_version))
        .filter(async_tasks::Column::Status.eq(state_code(current.status.state())))
        .col_expr(async_tasks::Column::Status, Expr::value(status))
        .col_expr(
            async_tasks::Column::ProgressBasisPoints,
            Expr::value(progress_basis_points),
        )
        .col_expr(async_tasks::Column::FailureKind, Expr::value(failure_kind))
        .col_expr(async_tasks::Column::Version, Expr::value(next_version))
        .col_expr(
            async_tasks::Column::TerminalAt,
            Expr::value(write.status.is_terminal().then_some(observed_at)),
        )
        .col_expr(async_tasks::Column::UpdatedAt, Expr::value(observed_at))
        .exec(transaction)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    if update.rows_affected != 1 {
        return Err(TransactionWriteError::Concurrent);
    }
    let updated = load_by_database_id(transaction, current.database_id)
        .await?
        .ok_or(AsyncTaskRepositoryError::Invariant)?;
    Ok(AsyncTaskTransitionOutcome::Applied(updated))
}

fn classify_transition_replay(
    existing: AsyncTaskRecord,
    write: &AsyncTaskTransition,
) -> Result<AsyncTaskTransitionOutcome, AsyncTaskRepositoryError> {
    let expected_version =
        u64::try_from(write.expected_version).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    if same_persisted_status(&existing.status, &write.status)
        && (existing.version == expected_version
            || existing.version == expected_version.saturating_add(1))
    {
        Ok(AsyncTaskTransitionOutcome::Existing(existing))
    } else {
        Err(AsyncTaskRepositoryError::Conflict)
    }
}

async fn references_match(
    transaction: &DatabaseTransaction,
    write: &AsyncTaskCreate,
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
    let credential_matches = credentials::Entity::find_by_id(write.credential_id.get())
        .filter(credentials::Column::ChannelId.eq(write.channel_id.get()))
        .one(transaction)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .is_some();
    Ok(token_matches && group_exists && credential_matches)
}

async fn load_by_owner<C>(
    connection: &C,
    user_id: UserId,
    task_id: AsyncTaskId,
) -> Result<Option<AsyncTaskRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    async_tasks::Entity::find()
        .filter(async_tasks::Column::TaskKey.eq(SensitiveString::from(task_id.persistence_key())))
        .filter(async_tasks::Column::UserId.eq(user_id.get()))
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .map(task_record)
        .transpose()
}

async fn load_by_request<C>(
    connection: &C,
    user_id: UserId,
    request_id: AsyncTaskRequestId,
) -> Result<Option<AsyncTaskRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    async_tasks::Entity::find()
        .filter(async_tasks::Column::UserId.eq(user_id.get()))
        .filter(
            async_tasks::Column::IdempotencyKey
                .eq(SensitiveString::from(request_id.persistence_key())),
        )
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .map(task_record)
        .transpose()
}

async fn load_by_database_id<C>(
    connection: &C,
    database_id: i64,
) -> Result<Option<AsyncTaskRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    async_tasks::Entity::find_by_id(database_id)
        .one(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?
        .map(task_record)
        .transpose()
}

async fn list_by_owner<C>(
    connection: &C,
    user_id: UserId,
    protocol: Protocol,
    before: Option<AsyncTaskPageCursor>,
    limit: usize,
) -> Result<AsyncTaskPageRecord, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let mut query = async_tasks::Entity::find()
        .filter(async_tasks::Column::UserId.eq(user_id.get()))
        .filter(async_tasks::Column::Protocol.eq(protocol.as_str()))
        .order_by_desc(async_tasks::Column::CreatedAt)
        .order_by_desc(async_tasks::Column::Id)
        .limit((limit + 1) as u64);
    if let Some(before) = before {
        let Some(anchor) = async_tasks::Entity::find()
            .filter(async_tasks::Column::UserId.eq(user_id.get()))
            .filter(async_tasks::Column::Protocol.eq(protocol.as_str()))
            .filter(
                async_tasks::Column::TaskKey
                    .eq(SensitiveString::from(before.task_id.persistence_key())),
            )
            .one(connection)
            .await
            .map_err(|_| AsyncTaskRepositoryError::Query)?
        else {
            return Ok(AsyncTaskPageRecord {
                tasks: Vec::new(),
                next_cursor: None,
            });
        };
        query = query.filter(
            Condition::any()
                .add(async_tasks::Column::CreatedAt.lt(anchor.created_at))
                .add(
                    Condition::all()
                        .add(async_tasks::Column::CreatedAt.eq(anchor.created_at))
                        .add(async_tasks::Column::Id.lt(anchor.id)),
                ),
        );
    }
    let mut models = query
        .all(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    let has_more = models.len() > limit;
    if has_more {
        models.truncate(limit);
    }
    let tasks = models
        .into_iter()
        .map(task_record)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = has_more.then(|| tasks.last().map(page_cursor)).flatten();
    Ok(AsyncTaskPageRecord { tasks, next_cursor })
}

fn page_cursor(record: &AsyncTaskRecord) -> AsyncTaskPageCursor {
    AsyncTaskPageCursor {
        task_id: record.task_id,
    }
}

async fn load_create_collision<C>(
    connection: &C,
    write: &AsyncTaskCreate,
) -> Result<Option<AsyncTaskRecord>, AsyncTaskRepositoryError>
where
    C: ConnectionTrait,
{
    let models = async_tasks::Entity::find()
        .filter(
            Condition::any()
                .add(
                    async_tasks::Column::TaskKey
                        .eq(SensitiveString::from(write.task_id.persistence_key())),
                )
                .add(
                    Condition::all()
                        .add(async_tasks::Column::UserId.eq(write.principal.user_id().get()))
                        .add(
                            async_tasks::Column::IdempotencyKey
                                .eq(SensitiveString::from(write.request_id.persistence_key())),
                        ),
                ),
        )
        .all(connection)
        .await
        .map_err(|_| AsyncTaskRepositoryError::Query)?;
    if models.len() > 1 {
        return Err(AsyncTaskRepositoryError::Conflict);
    }
    models.into_iter().next().map(task_record).transpose()
}

fn classify_create_collision(
    existing: AsyncTaskRecord,
    write: &AsyncTaskCreate,
) -> Result<AsyncTaskCreateOutcome, AsyncTaskRepositoryError> {
    if existing.matches_create(write) {
        Ok(AsyncTaskCreateOutcome::Existing(existing))
    } else {
        Err(AsyncTaskRepositoryError::Conflict)
    }
}

fn task_record(model: async_tasks::Model) -> Result<AsyncTaskRecord, AsyncTaskRepositoryError> {
    let task_id = AsyncTaskId::from_persistence_key(model.task_key.as_str())
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let request_id = AsyncTaskRequestId::from_persistence_key(model.idempotency_key.as_str())
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let principal = GatewayPrincipal::new(
        TokenId::new(model.token_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?,
        UserId::new(model.user_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?,
        GroupId::new(model.group_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?,
    );
    let protocol =
        Protocol::from_str(&model.protocol).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let channel_id =
        ChannelId::new(model.channel_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let credential_id =
        CredentialId::new(model.credential_id).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let credential_revision = parse_credential_revision(model.credential_revision.as_str())?;
    let upstream_task_id = UpstreamTaskId::new(model.upstream_task_id.as_str().to_owned())
        .map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let status = status_from_persistence(
        model.status,
        model.progress_basis_points,
        model.failure_kind,
    )?;
    let version = u64::try_from(model.version).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    let terminal_at = optional_unix_seconds(model.terminal_at)?;
    let created_at = unix_seconds(model.created_at)?;
    let updated_at = unix_seconds(model.updated_at)?;
    let requested_model = model.requested_model.as_str().to_owned();
    let upstream_model = model.upstream_model.as_str().to_owned();
    let valid_terminal = status.is_terminal() == terminal_at.is_some();
    if model.id <= 0
        || version == 0
        || requested_model.is_empty()
        || requested_model.len() > af_domain::MAX_MODEL_NAME_BYTES
        || requested_model.trim() != requested_model
        || requested_model.chars().any(char::is_control)
        || upstream_model.is_empty()
        || upstream_model.len() > af_domain::MAX_MODEL_NAME_BYTES
        || upstream_model.trim() != upstream_model
        || upstream_model.chars().any(char::is_control)
        || updated_at < created_at
        || terminal_at.is_some_and(|value| value < created_at)
        || !valid_terminal
    {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    Ok(AsyncTaskRecord {
        database_id: model.id,
        task_id,
        request_id,
        principal,
        protocol,
        requested_model,
        upstream_model,
        channel_id,
        credential_id,
        credential_revision,
        upstream_task_id,
        status,
        version,
        terminal_at,
        created_at,
        updated_at,
    })
}

fn credential_revision_key(revision: u64) -> String {
    format!("{revision:016x}")
}

fn parse_credential_revision(value: &str) -> Result<u64, AsyncTaskRepositoryError> {
    if value.len() != 16
        || !value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(AsyncTaskRepositoryError::Invariant);
    }
    u64::from_str_radix(value, 16).map_err(|_| AsyncTaskRepositoryError::Invariant)
}

fn to_database_time(value: u64) -> Result<TimeDateTimeWithTimeZone, AsyncTaskRepositoryError> {
    let value = i64::try_from(value).map_err(|_| AsyncTaskRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| AsyncTaskRepositoryError::Invariant)
}

fn unix_seconds(value: TimeDateTimeWithTimeZone) -> Result<u64, AsyncTaskRepositoryError> {
    u64::try_from(value.unix_timestamp()).map_err(|_| AsyncTaskRepositoryError::Invariant)
}

fn optional_unix_seconds(
    value: Option<TimeDateTimeWithTimeZone>,
) -> Result<Option<u64>, AsyncTaskRepositoryError> {
    value.map(unix_seconds).transpose()
}

fn is_unique_conflict(error: &sea_orm::DbErr) -> bool {
    let rendered = error.to_string();
    rendered.contains("uq_async_tasks_task_key")
        || rendered.contains("uq_async_tasks_owner_idempotency")
        || rendered.contains("async_tasks.task_key")
        || rendered.contains("async_tasks.user_id, async_tasks.idempotency_key")
        || rendered.contains("Duplicate entry")
}

async fn begin(pool: &DatabasePool) -> Result<DatabaseTransaction, AsyncTaskRepositoryError> {
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

/// 只记录闭合内部分类，避免任务键、模型、上游标识和绑定版本进入日志。
fn internal(error: AsyncTaskRepositoryError) -> AsyncTaskRepositoryError {
    let error_kind = match error {
        AsyncTaskRepositoryError::Conflict => return error,
        AsyncTaskRepositoryError::Query => "async_task_query",
        AsyncTaskRepositoryError::OutcomeUnknown => "async_task_outcome_unknown",
        AsyncTaskRepositoryError::Timeout => "async_task_timeout",
        AsyncTaskRepositoryError::Invariant => "async_task_invariant",
    };
    tracing::error!(
        target: "af_db::async_task",
        error_kind,
        "异步任务仓储发生内部错误"
    );
    error
}
