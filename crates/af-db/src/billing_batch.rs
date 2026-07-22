use std::{fmt, time::Duration};

#[cfg(test)]
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use sea_orm::{
    ConnectionTrait, DatabaseTransaction, DbBackend, DbErr, QueryResult, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::SelectStatement,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::DatabasePool;

mod sql;
mod state;
mod types;

use state::CheckpointState;
pub use types::{
    BillingBatchRepositoryError, BillingBatchWrite, BillingBatchWriteError,
    BillingBatchWriteOutcome, ChannelBillingWrite, TokenBillingWrite, UserBillingWrite,
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// 使用 writer/sequence checkpoint 原子应用计费聚合增量的数据库仓储。
#[derive(Clone)]
pub struct BillingBatchRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
    #[cfg(test)]
    outcome_unknown_after_commit: Arc<AtomicBool>,
}

impl BillingBatchRepository {
    /// 使用默认五秒操作截止时间构造仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 使用显式非零操作截止时间构造仓储。
    pub fn with_operation_timeout(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, BillingBatchRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(BillingBatchRepositoryError::InvalidConfiguration);
        }
        Ok(Self {
            pool,
            operation_timeout,
            #[cfg(test)]
            outcome_unknown_after_commit: Arc::new(AtomicBool::new(false)),
        })
    }

    /// 原子应用完整批次；相同 writer、范围与指纹重放时不会重复修改主体计数器。
    pub async fn apply(
        &self,
        batch: &BillingBatchWrite,
    ) -> Result<BillingBatchWriteOutcome, BillingBatchRepositoryError> {
        let operation = self
            .apply_inner(batch)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => self.finish_operation(result),
            Err(_) => Err(record_internal_error(
                BillingBatchRepositoryError::OutcomeUnknown,
            )),
        }
    }

    /// 仅供回归测试模拟事务已提交但调用方未收到确定结果。
    #[cfg(test)]
    pub(crate) fn inject_outcome_unknown_after_commit(&self) {
        self.outcome_unknown_after_commit
            .store(true, Ordering::Release);
    }

    fn finish_operation(
        &self,
        result: Result<BillingBatchWriteOutcome, BillingBatchRepositoryError>,
    ) -> Result<BillingBatchWriteOutcome, BillingBatchRepositoryError> {
        let result = result.map_err(record_internal_error)?;
        #[cfg(test)]
        if result == BillingBatchWriteOutcome::Applied
            && self
                .outcome_unknown_after_commit
                .swap(false, Ordering::AcqRel)
        {
            return Err(record_internal_error(
                BillingBatchRepositoryError::OutcomeUnknown,
            ));
        }
        Ok(result)
    }

    async fn apply_inner(
        &self,
        batch: &BillingBatchWrite,
    ) -> Result<BillingBatchWriteOutcome, BillingBatchRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| BillingBatchRepositoryError::Query)?;
        let backend = transaction.get_database_backend();
        let writer_key = batch.writer_key();

        if backend == DbBackend::Sqlite
            && transaction
                .execute(backend.build(&sql::sqlite_lock_checkpoint(writer_key.clone())))
                .await
                .is_err()
        {
            return rollback_with_error(transaction, BillingBatchRepositoryError::Query).await;
        }

        let checkpoint = match load_checkpoint(&transaction, writer_key.clone(), true).await {
            Ok(checkpoint) => checkpoint,
            Err(error) => return rollback_with_error(transaction, error).await,
        };
        match checkpoint.as_ref() {
            Some(checkpoint) if checkpoint.matches(batch) => {
                return rollback_with_outcome(transaction, BillingBatchWriteOutcome::Existing)
                    .await;
            }
            Some(checkpoint) if !checkpoint.accepts_next(batch) => {
                return rollback_with_error(
                    transaction,
                    BillingBatchRepositoryError::SequenceConflict,
                )
                .await;
            }
            None if batch.start_sequence() != 1 => {
                return rollback_with_error(
                    transaction,
                    BillingBatchRepositoryError::SequenceConflict,
                )
                .await;
            }
            Some(_) | None => {}
        }

        let now = TimeDateTimeWithTimeZone::now_utc();
        if let Err(error) = apply_subject_deltas(&transaction, backend, batch, now).await {
            return rollback_with_error(transaction, error).await;
        }

        let checkpoint_result = match checkpoint.as_ref() {
            Some(previous) => {
                transaction
                    .execute(backend.build(&sql::update_checkpoint(
                        batch,
                        previous.last_start_sequence,
                        previous.last_end_sequence,
                        previous.last_event_count,
                        previous.last_fingerprint.clone(),
                        now,
                    )))
                    .await
            }
            None => {
                transaction
                    .execute(backend.build(&sql::insert_checkpoint(batch, now)))
                    .await
            }
        };
        match checkpoint_result {
            Ok(result) if result.rows_affected() == 1 => commit_applied(transaction).await,
            Ok(_) => rollback_with_error(transaction, BillingBatchRepositoryError::Invariant).await,
            Err(error) if checkpoint.is_none() && is_unique_violation(&error) => {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| BillingBatchRepositoryError::OutcomeUnknown)?;
                self.classify_existing(batch).await
            }
            Err(_) => rollback_with_error(transaction, BillingBatchRepositoryError::Query).await,
        }
    }

    async fn classify_existing(
        &self,
        batch: &BillingBatchWrite,
    ) -> Result<BillingBatchWriteOutcome, BillingBatchRepositoryError> {
        let checkpoint = load_checkpoint(self.pool.connection(), batch.writer_key(), false)
            .await?
            .ok_or(BillingBatchRepositoryError::Invariant)?;
        if checkpoint.matches(batch) {
            Ok(BillingBatchWriteOutcome::Existing)
        } else {
            Err(BillingBatchRepositoryError::SequenceConflict)
        }
    }
}

impl fmt::Debug for BillingBatchRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingBatchRepository")
            .finish_non_exhaustive()
    }
}

async fn apply_subject_deltas(
    transaction: &DatabaseTransaction,
    backend: DbBackend,
    batch: &BillingBatchWrite,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), BillingBatchRepositoryError> {
    // 固定 user → token → channel 且各表按主键升序更新，避免多批次交叉锁形成死锁。
    for delta in batch.users() {
        execute_single_row(transaction, backend, sql::apply_user(*delta, now)).await?;
    }
    for delta in batch.tokens() {
        execute_single_row(transaction, backend, sql::apply_token(*delta, now)).await?;
    }
    for delta in batch.channels() {
        execute_single_row(transaction, backend, sql::apply_channel(*delta, now)).await?;
    }
    Ok(())
}

async fn execute_single_row(
    transaction: &DatabaseTransaction,
    backend: DbBackend,
    statement: sea_orm::sea_query::UpdateStatement,
) -> Result<(), BillingBatchRepositoryError> {
    match transaction.execute(backend.build(&statement)).await {
        Ok(result) if result.rows_affected() == 1 => Ok(()),
        Ok(_) => Err(BillingBatchRepositoryError::Invariant),
        Err(_) => Err(BillingBatchRepositoryError::Query),
    }
}

async fn load_checkpoint<C>(
    connection: &C,
    writer_key: String,
    lock: bool,
) -> Result<Option<CheckpointState>, BillingBatchRepositoryError>
where
    C: ConnectionTrait,
{
    query_one(connection, sql::checkpoint_state(writer_key, lock))
        .await?
        .as_ref()
        .map(CheckpointState::try_from_result)
        .transpose()
}

async fn query_one<C>(
    connection: &C,
    statement: SelectStatement,
) -> Result<Option<QueryResult>, BillingBatchRepositoryError>
where
    C: ConnectionTrait,
{
    connection
        .query_one(connection.get_database_backend().build(&statement))
        .await
        .map_err(|_| BillingBatchRepositoryError::Query)
}

async fn commit_applied(
    transaction: DatabaseTransaction,
) -> Result<BillingBatchWriteOutcome, BillingBatchRepositoryError> {
    transaction
        .commit()
        .await
        .map_err(|_| BillingBatchRepositoryError::OutcomeUnknown)?;
    Ok(BillingBatchWriteOutcome::Applied)
}

async fn rollback_with_error<T>(
    transaction: DatabaseTransaction,
    error: BillingBatchRepositoryError,
) -> Result<T, BillingBatchRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| BillingBatchRepositoryError::OutcomeUnknown)?;
    Err(error)
}

async fn rollback_with_outcome(
    transaction: DatabaseTransaction,
    outcome: BillingBatchWriteOutcome,
) -> Result<BillingBatchWriteOutcome, BillingBatchRepositoryError> {
    transaction
        .rollback()
        .await
        .map_err(|_| BillingBatchRepositoryError::Query)?;
    Ok(outcome)
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

/// 只记录闭合内部分类，避免 writer、sequence、主体与额度进入日志。
fn record_internal_error(error: BillingBatchRepositoryError) -> BillingBatchRepositoryError {
    let error_kind = match error {
        BillingBatchRepositoryError::InvalidConfiguration => "billing_batch_configuration",
        BillingBatchRepositoryError::Query => "billing_batch_query",
        BillingBatchRepositoryError::OutcomeUnknown => "billing_batch_outcome_unknown",
        BillingBatchRepositoryError::Invariant => "billing_batch_invariant",
        BillingBatchRepositoryError::SequenceConflict => return error,
    };
    tracing::error!(
        target: "af_db::billing_batch",
        error_kind,
        "计费批量仓储发生内部错误"
    );
    error
}
