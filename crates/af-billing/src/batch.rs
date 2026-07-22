//! 崩溃安全的计费增量分桶与文件 WAL 内核。

use std::{
    fmt,
    path::Path,
    sync::{Arc, Mutex},
};

mod buckets;
mod types;
mod wal;

pub use types::{
    BillingBatch, BillingBatchApplyOutcome, BillingBatchError, BillingBatchEvent,
    BillingBatchFlushOutcome, BillingBatchRecordOutcome, BillingBatchSink, BillingBatchSinkError,
    BillingBatchSinkFuture, BillingWriterId, ChannelBillingDelta, TokenBillingDelta,
    UserBillingDelta,
};
use wal::WalStore;

/// 文件 WAL 批量器；记录路径使用 `spawn_blocking` 隔离同步写入与 `fsync`。
#[derive(Clone)]
pub struct FileBillingBatcher {
    state: Arc<Mutex<WalStore>>,
    sink: Arc<dyn BillingBatchSink>,
    flush_gate: Arc<tokio::sync::Mutex<()>>,
}

impl FileBillingBatcher {
    /// 打开或创建 WAL 目录，并在返回前重放全部完整分段与活动记录。
    pub fn open(
        directory: impl AsRef<Path>,
        sink: Arc<dyn BillingBatchSink>,
    ) -> Result<Self, BillingBatchError> {
        Ok(Self {
            state: Arc::new(Mutex::new(WalStore::open(directory.as_ref())?)),
            sink,
            flush_gate: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    /// 返回当前 WAL writer 标识。
    pub fn writer_id(&self) -> Result<BillingWriterId, BillingBatchError> {
        self.state
            .lock()
            .map(|state| state.writer_id())
            .map_err(|_| BillingBatchError::WalPoisoned)
    }

    /// 返回尚未由 sink 确认的事件数量。
    pub fn pending_event_count(&self) -> Result<usize, BillingBatchError> {
        self.state
            .lock()
            .map(|state| state.pending_event_count())
            .map_err(|_| BillingBatchError::WalPoisoned)
    }

    /// checked 汇总后持久化单个事件；只有记录完成 `fsync` 才返回 `Applied`。
    ///
    /// Future 被取消时，隔离的文件任务仍可能完成；调用方必须复用同一 `event_id`
    /// 重试，不能构造新的补偿事件。
    pub async fn record(
        &self,
        event: BillingBatchEvent,
    ) -> Result<BillingBatchRecordOutcome, BillingBatchError> {
        let state = Arc::clone(&self.state);
        tokio::task::spawn_blocking(move || {
            state
                .lock()
                .map_err(|_| BillingBatchError::WalPoisoned)?
                .append(event)
        })
        .await
        .map_err(|_| BillingBatchError::FileTaskFailed)?
    }

    /// 封存活动分段并尝试落盘最早的连续批次。
    ///
    /// sink 失败、Future 被取消或结果未知时不会删除分段；下次调用只会重放同一
    /// writer 与序列范围。
    pub async fn flush_once(&self) -> Result<BillingBatchFlushOutcome, BillingBatchError> {
        let _guard = self.flush_gate.lock().await;
        let state = Arc::clone(&self.state);
        let prepared = tokio::task::spawn_blocking(move || {
            state
                .lock()
                .map_err(|_| BillingBatchError::WalPoisoned)?
                .prepare_flush()
        })
        .await
        .map_err(|_| BillingBatchError::FileTaskFailed)??;
        let Some(prepared) = prepared else {
            return Ok(BillingBatchFlushOutcome::Empty);
        };

        let sink_outcome = self.sink.apply(&prepared.batch).await?;
        let event_count = prepared.batch.event_count();
        let segment_key = prepared.segment_key;
        let state = Arc::clone(&self.state);
        tokio::task::spawn_blocking(move || {
            state
                .lock()
                .map_err(|_| BillingBatchError::WalPoisoned)?
                .confirm_flush(&segment_key)
        })
        .await
        .map_err(|_| BillingBatchError::FileTaskFailed)??;

        Ok(match sink_outcome {
            BillingBatchApplyOutcome::Applied => BillingBatchFlushOutcome::Applied { event_count },
            BillingBatchApplyOutcome::Existing => {
                BillingBatchFlushOutcome::Existing { event_count }
            }
        })
    }
}

impl fmt::Debug for FileBillingBatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileBillingBatcher")
            .finish_non_exhaustive()
    }
}
