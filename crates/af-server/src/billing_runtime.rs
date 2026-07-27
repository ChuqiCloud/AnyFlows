use std::{fmt, sync::Arc, time::Duration};

use af_billing::{
    BillingBatchError, BillingBatchEvent, BillingBatchFlushOutcome, BillingBatchRecordOutcome,
    BillingBatchSink, DatabaseBillingBatchSink, FileBillingBatcher,
};
use af_config::BillingSettings;
use af_db::{BillingBatchRepository, DatabasePool};
use thiserror::Error;
use tokio::time::{MissedTickBehavior, interval};

use crate::ShutdownController;

/// 计费运行时初始化、记录与落库错误；不会携带 WAL 路径或额度内容。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum BillingRuntimeError {
    /// 打开、加锁或恢复 WAL 失败。
    #[error("初始化计费 WAL 失败")]
    Open(#[source] BillingBatchError),
    /// 事件未能可靠写入 WAL。
    #[error("记录计费增量失败")]
    Record(#[source] BillingBatchError),
    /// WAL 批次未能获得数据库 sink 的明确确认。
    #[error("刷新计费增量失败")]
    Flush(#[source] BillingBatchError),
    /// flush 汇总计数超过进程可表示范围。
    #[error("计费刷新汇总计数溢出")]
    ReportOverflow,
}

impl BillingRuntimeError {
    /// 返回适合结构化日志的固定错误分类。
    pub(crate) const fn error_kind(self) -> &'static str {
        match self {
            Self::Open(_) => "billing_wal_open",
            Self::Record(_) => "billing_wal_record",
            Self::Flush(_) => "billing_wal_flush",
            Self::ReportOverflow => "billing_flush_report_overflow",
        }
    }
}

/// 一轮完整 flush 已由数据库确认的批次与事件数量。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BillingFlushReport {
    confirmed_batches: u64,
    confirmed_events: u64,
}

impl BillingFlushReport {
    /// 返回已确认并从 WAL 删除的批次数量。
    #[must_use]
    pub const fn confirmed_batches(self) -> u64 {
        self.confirmed_batches
    }

    /// 返回已确认并从 WAL 删除的事件数量。
    #[must_use]
    pub const fn confirmed_events(self) -> u64 {
        self.confirmed_events
    }

    fn confirm(&mut self, event_count: usize) -> Result<(), BillingRuntimeError> {
        let event_count =
            u64::try_from(event_count).map_err(|_| BillingRuntimeError::ReportOverflow)?;
        self.confirmed_batches = self
            .confirmed_batches
            .checked_add(1)
            .ok_or(BillingRuntimeError::ReportOverflow)?;
        self.confirmed_events = self
            .confirmed_events
            .checked_add(event_count)
            .ok_or(BillingRuntimeError::ReportOverflow)?;
        Ok(())
    }
}

/// 计费增量统一写入口；批量和实时模式共享同一 WAL 与 exactly-once sink。
///
/// 批量关闭时，`record` 会在返回前立即 flush；批量开启时，记录只等待 WAL `fsync`，
/// 再由受监督的周期任务落库。两种模式都要求结果未知后复用同一事件 ID。
#[derive(Clone)]
pub struct BillingRuntime {
    batcher: FileBillingBatcher,
    batch_enabled: bool,
    flush_interval: Duration,
}

impl BillingRuntime {
    /// 使用数据库 checkpoint sink 打开本实例独占的 WAL 目录。
    pub fn open(
        settings: &BillingSettings,
        database: DatabasePool,
    ) -> Result<Self, BillingRuntimeError> {
        let sink: Arc<dyn BillingBatchSink> = Arc::new(DatabaseBillingBatchSink::new(
            BillingBatchRepository::new(database),
        ));
        let batcher = FileBillingBatcher::open(settings.wal_directory(), sink)
            .map_err(BillingRuntimeError::Open)?;
        Ok(Self {
            batcher,
            batch_enabled: settings.batch_enabled(),
            flush_interval: Duration::from_secs(settings.flush_interval_secs()),
        })
    }

    /// 返回当前是否由后台任务按周期合并落库。
    #[must_use]
    pub const fn batch_enabled(&self) -> bool {
        self.batch_enabled
    }

    /// 返回尚未获得数据库 sink 确认的事件数量。
    pub fn pending_event_count(&self) -> Result<usize, BillingRuntimeError> {
        self.batcher
            .pending_event_count()
            .map_err(BillingRuntimeError::Record)
    }

    /// 可靠记录单个增量；实时模式还会在返回前清空当前 WAL 积压。
    pub async fn record(
        &self,
        event: BillingBatchEvent,
    ) -> Result<BillingBatchRecordOutcome, BillingRuntimeError> {
        let outcome = self
            .batcher
            .record(event)
            .await
            .map_err(BillingRuntimeError::Record)?;
        if !self.batch_enabled {
            self.flush_all().await?;
        }
        Ok(outcome)
    }

    /// 连续 flush 到 WAL 没有待确认分段，用于启动恢复与关闭收尾。
    pub async fn flush_all(&self) -> Result<BillingFlushReport, BillingRuntimeError> {
        let mut report = BillingFlushReport::default();
        loop {
            match self
                .batcher
                .flush_once()
                .await
                .map_err(BillingRuntimeError::Flush)?
            {
                BillingBatchFlushOutcome::Empty => return Ok(report),
                BillingBatchFlushOutcome::Applied { event_count }
                | BillingBatchFlushOutcome::Existing { event_count } => {
                    report.confirm(event_count)?;
                }
            }
        }
    }

    /// 运行批量模式的周期 flush；异常返回后由 supervisor 退避并重建本任务 Future。
    pub(crate) async fn run_periodic_flush(
        &self,
        shutdown: ShutdownController,
    ) -> Result<(), BillingRuntimeError> {
        let mut ticker = interval(self.flush_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => return Ok(()),
                _ = ticker.tick() => {
                    self.batcher
                        .flush_once()
                        .await
                        .map_err(BillingRuntimeError::Flush)?;
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn from_batcher(
        batcher: FileBillingBatcher,
        batch_enabled: bool,
        flush_interval: Duration,
    ) -> Self {
        Self {
            batcher,
            batch_enabled,
            flush_interval,
        }
    }
}

impl fmt::Debug for BillingRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingRuntime")
            .field("batch_enabled", &self.batch_enabled)
            .field("flush_interval", &self.flush_interval)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{
            Arc, Mutex,
            atomic::{AtomicU64, Ordering},
        },
    };

    use af_billing::{
        BillingBatch, BillingBatchApplyOutcome, BillingBatchSinkFuture, UserBillingDelta,
    };
    use af_domain::{BillingReservationId, QuotaDelta, UserId};
    use tokio::sync::Notify;

    use super::*;

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "anyflows-server-billing-{}-{serial}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        batches: Mutex<Vec<BillingBatch>>,
        applied: Notify,
    }

    impl RecordingSink {
        fn batch_count(&self) -> usize {
            self.batches.lock().unwrap().len()
        }
    }

    impl BillingBatchSink for RecordingSink {
        fn apply<'a>(&'a self, batch: &'a BillingBatch) -> BillingBatchSinkFuture<'a> {
            Box::pin(async move {
                self.batches.lock().unwrap().push(batch.clone());
                self.applied.notify_one();
                Ok(BillingBatchApplyOutcome::Applied)
            })
        }
    }

    #[tokio::test]
    async fn realtime_mode_confirms_database_sink_before_record_returns() {
        let directory = TestDirectory::new();
        let sink = Arc::new(RecordingSink::default());
        let batcher = FileBillingBatcher::open(directory.path(), sink.clone()).unwrap();
        let runtime = BillingRuntime::from_batcher(batcher, false, Duration::from_secs(1));

        assert_eq!(
            runtime.record(event(1)).await.unwrap(),
            BillingBatchRecordOutcome::Applied { sequence: 1 }
        );
        assert_eq!(runtime.pending_event_count().unwrap(), 0);
        assert_eq!(sink.batch_count(), 1);
    }

    #[tokio::test]
    async fn batch_mode_periodic_task_flushes_and_obeys_shutdown() {
        let directory = TestDirectory::new();
        let sink = Arc::new(RecordingSink::default());
        let batcher = FileBillingBatcher::open(directory.path(), sink.clone()).unwrap();
        let runtime = BillingRuntime::from_batcher(batcher, true, Duration::from_millis(10));
        runtime.record(event(2)).await.unwrap();
        assert_eq!(runtime.pending_event_count().unwrap(), 1);

        let shutdown = ShutdownController::new();
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let shutdown = shutdown.clone();
            async move { runtime.run_periodic_flush(shutdown).await }
        });
        tokio::time::timeout(Duration::from_secs(1), sink.applied.notified())
            .await
            .unwrap();
        let _ = shutdown.trigger();
        assert_eq!(task.await.unwrap(), Ok(()));
        assert_eq!(runtime.pending_event_count().unwrap(), 0);
        assert_eq!(sink.batch_count(), 1);
    }

    fn event(id: u8) -> BillingBatchEvent {
        let mut event_id = [0_u8; 16];
        event_id[15] = id;
        BillingBatchEvent::new(
            BillingReservationId::new(event_id).unwrap(),
            Some(
                UserBillingDelta::new(
                    UserId::new(i64::from(id) + 1).unwrap(),
                    QuotaDelta::new(-1).unwrap(),
                    QuotaDelta::new(1).unwrap(),
                    1,
                )
                .unwrap(),
            ),
            None,
            None,
        )
        .unwrap()
    }
}
