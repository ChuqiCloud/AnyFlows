use std::{fmt, sync::Arc};

use af_db::{
    BillingBatchRepository, BillingBatchRepositoryError, BillingBatchWrite,
    BillingBatchWriteOutcome, ChannelBillingWrite, TokenBillingWrite, UserBillingWrite,
};

use crate::{
    BillingBatch, BillingBatchApplyOutcome, BillingBatchSink, BillingBatchSinkError,
    BillingBatchSinkFuture,
};

/// 将 WAL 连续批次交给 `af-db` checkpoint 仓储的生产 sink。
#[derive(Clone)]
pub struct DatabaseBillingBatchSink {
    repository: Arc<BillingBatchRepository>,
}

impl DatabaseBillingBatchSink {
    /// 包装已配置完成的数据库批量仓储。
    #[must_use]
    pub fn new(repository: BillingBatchRepository) -> Self {
        Self {
            repository: Arc::new(repository),
        }
    }
}

impl BillingBatchSink for DatabaseBillingBatchSink {
    fn apply<'a>(&'a self, batch: &'a BillingBatch) -> BillingBatchSinkFuture<'a> {
        Box::pin(async move {
            let write = database_write(batch)?;
            match self.repository.apply(&write).await.map_err(map_error)? {
                BillingBatchWriteOutcome::Applied => Ok(BillingBatchApplyOutcome::Applied),
                BillingBatchWriteOutcome::Existing => Ok(BillingBatchApplyOutcome::Existing),
            }
        })
    }
}

impl fmt::Debug for DatabaseBillingBatchSink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseBillingBatchSink")
            .finish_non_exhaustive()
    }
}

fn database_write(batch: &BillingBatch) -> Result<BillingBatchWrite, BillingBatchSinkError> {
    let users = batch
        .users()
        .iter()
        .map(|delta| {
            UserBillingWrite::new(
                delta.user_id(),
                delta.quota_delta(),
                delta.used_quota_delta(),
                delta.request_count_delta(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| BillingBatchSinkError::Invariant)?;
    let tokens = batch
        .tokens()
        .iter()
        .map(|delta| {
            TokenBillingWrite::new(
                delta.token_id(),
                delta.remain_quota_delta(),
                delta.used_quota_delta(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| BillingBatchSinkError::Invariant)?;
    let channels = batch
        .channels()
        .iter()
        .map(|delta| ChannelBillingWrite::new(delta.channel_id(), delta.used_quota_delta()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| BillingBatchSinkError::Invariant)?;
    BillingBatchWrite::new(
        batch.writer_id().bytes(),
        batch.start_sequence(),
        batch.end_sequence(),
        batch.event_ids().to_vec(),
        users,
        tokens,
        channels,
    )
    .map_err(|_| BillingBatchSinkError::Invariant)
}

const fn map_error(error: BillingBatchRepositoryError) -> BillingBatchSinkError {
    match error {
        BillingBatchRepositoryError::InvalidConfiguration => {
            BillingBatchSinkError::InvalidConfiguration
        }
        BillingBatchRepositoryError::SequenceConflict => BillingBatchSinkError::SequenceConflict,
        BillingBatchRepositoryError::Invariant => BillingBatchSinkError::Invariant,
        BillingBatchRepositoryError::Query => BillingBatchSinkError::Query,
        BillingBatchRepositoryError::OutcomeUnknown => BillingBatchSinkError::OutcomeUnknown,
        // af-db 后续新增的闭合错误在适配层未显式审查前一律按不变量损坏拒绝确认 WAL。
        _ => BillingBatchSinkError::Invariant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_sink_implements_the_object_safe_batch_contract() {
        fn assert_sink<T: BillingBatchSink + Send + Sync + 'static>() {}

        assert_sink::<DatabaseBillingBatchSink>();
    }
}
