use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use af_domain::{BillingReservationId, ChannelId, QuotaDelta, TokenId, UserId};

use crate::{
    BillingBatch, BillingBatchApplyOutcome, BillingBatchError, BillingBatchEvent,
    BillingBatchFlushOutcome, BillingBatchRecordOutcome, BillingBatchSink, BillingBatchSinkError,
    BillingBatchSinkFuture, ChannelBillingDelta, FileBillingBatcher, TokenBillingDelta,
    UserBillingDelta,
};

const WAL_HEADER_SIZE: u64 = 48;
const WAL_RECORD_SIZE: u64 = 104;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-billing-wal-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn active_wal(&self) -> PathBuf {
        self.path.join("active.wal")
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Default)]
struct RecordingSink {
    batches: Mutex<Vec<BillingBatch>>,
    outcomes: Mutex<VecDeque<Result<BillingBatchApplyOutcome, BillingBatchSinkError>>>,
}

impl RecordingSink {
    fn with_outcomes(
        outcomes: impl IntoIterator<Item = Result<BillingBatchApplyOutcome, BillingBatchSinkError>>,
    ) -> Self {
        Self {
            batches: Mutex::new(Vec::new()),
            outcomes: Mutex::new(outcomes.into_iter().collect()),
        }
    }

    fn batches(&self) -> Vec<BillingBatch> {
        self.batches.lock().unwrap().clone()
    }
}

impl BillingBatchSink for RecordingSink {
    fn apply<'a>(&'a self, batch: &'a BillingBatch) -> BillingBatchSinkFuture<'a> {
        Box::pin(async move {
            self.batches.lock().unwrap().push(batch.clone());
            self.outcomes
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(BillingBatchApplyOutcome::Applied))
        })
    }
}

#[tokio::test]
async fn records_aggregate_and_flush_exactly_one_segment() {
    let directory = TestDirectory::new();
    let sink = Arc::new(RecordingSink::default());
    let batcher = FileBillingBatcher::open(directory.path(), sink.clone()).unwrap();
    let first = complete_event(1, 10, 10);
    let second = complete_event(2, 5, 5);

    assert_eq!(
        batcher.record(first).await.unwrap(),
        BillingBatchRecordOutcome::Applied { sequence: 1 }
    );
    assert_eq!(
        batcher.record(first).await.unwrap(),
        BillingBatchRecordOutcome::Existing { sequence: 1 }
    );
    assert_eq!(
        batcher.record(second).await.unwrap(),
        BillingBatchRecordOutcome::Applied { sequence: 2 }
    );
    assert_eq!(batcher.pending_event_count().unwrap(), 2);

    assert_eq!(
        batcher.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Applied { event_count: 2 }
    );
    assert_eq!(batcher.pending_event_count().unwrap(), 0);
    assert_eq!(
        batcher.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Empty
    );

    let batches = sink.batches();
    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    assert_eq!((batch.start_sequence(), batch.end_sequence()), (1, 2));
    assert_eq!(batch.event_ids(), &[reservation_id(1), reservation_id(2)]);
    assert_eq!(batch.users().len(), 1);
    assert_eq!(batch.users()[0].user_id(), UserId::new(11).unwrap());
    assert_eq!(batch.users()[0].quota_delta().units(), -15);
    assert_eq!(batch.users()[0].used_quota_delta().units(), 15);
    assert_eq!(batch.users()[0].request_count_delta(), 2);
    assert_eq!(batch.tokens().len(), 1);
    assert_eq!(batch.tokens()[0].remain_quota_delta().units(), -15);
    assert_eq!(batch.tokens()[0].used_quota_delta().units(), 15);
    assert_eq!(batch.channels().len(), 1);
    assert_eq!(batch.channels()[0].used_quota_delta().units(), 15);
}

#[tokio::test]
async fn duplicate_conflict_and_checked_overflow_never_append_a_second_record() {
    let directory = TestDirectory::new();
    let sink = Arc::new(RecordingSink::default());
    let batcher = FileBillingBatcher::open(directory.path(), sink).unwrap();
    let maximum = user_only_event(3, i64::MAX, 0);

    assert_eq!(
        batcher.record(maximum).await.unwrap(),
        BillingBatchRecordOutcome::Applied { sequence: 1 }
    );
    assert_eq!(
        batcher.record(user_only_event(3, 1, 0)).await,
        Err(BillingBatchError::EventConflict)
    );
    assert_eq!(
        batcher.record(user_only_event(4, 1, 0)).await,
        Err(BillingBatchError::AggregateOverflow)
    );
    assert_eq!(batcher.pending_event_count().unwrap(), 1);
    drop(batcher);

    let reopened =
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
    assert_eq!(reopened.pending_event_count().unwrap(), 1);
}

#[tokio::test]
async fn startup_replays_active_wal_and_preserves_writer_identity() {
    let directory = TestDirectory::new();
    let first_sink = Arc::new(RecordingSink::default());
    let first = FileBillingBatcher::open(directory.path(), first_sink).unwrap();
    let writer_id = first.writer_id().unwrap();
    first.record(complete_event(5, 7, 7)).await.unwrap();
    drop(first);

    let recovery_sink = Arc::new(RecordingSink::default());
    let recovered = FileBillingBatcher::open(directory.path(), recovery_sink.clone()).unwrap();
    assert_eq!(recovered.writer_id().unwrap(), writer_id);
    assert_eq!(recovered.pending_event_count().unwrap(), 1);
    assert_eq!(
        recovered.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Applied { event_count: 1 }
    );
    assert_eq!(recovery_sink.batches()[0].writer_id(), writer_id);
}

#[tokio::test]
async fn confirmed_flush_persists_the_next_sequence_across_restart() {
    let directory = TestDirectory::new();
    let first =
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
    first.record(complete_event(11, 1, 1)).await.unwrap();
    first.flush_once().await.unwrap();
    drop(first);

    let recovered =
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
    assert_eq!(
        recovered.record(complete_event(12, 1, 1)).await.unwrap(),
        BillingBatchRecordOutcome::Applied { sequence: 2 }
    );
}

#[tokio::test]
async fn sink_failure_keeps_oldest_segment_and_new_events_wait_in_active_wal() {
    let directory = TestDirectory::new();
    let sink = Arc::new(RecordingSink::with_outcomes([
        Err(BillingBatchSinkError::Query),
        Ok(BillingBatchApplyOutcome::Applied),
        Ok(BillingBatchApplyOutcome::Applied),
    ]));
    let batcher = FileBillingBatcher::open(directory.path(), sink.clone()).unwrap();
    batcher.record(complete_event(6, 3, 3)).await.unwrap();

    assert_eq!(
        batcher.flush_once().await,
        Err(BillingBatchError::Sink(BillingBatchSinkError::Query))
    );
    batcher.record(complete_event(7, 4, 4)).await.unwrap();
    assert_eq!(batcher.pending_event_count().unwrap(), 2);
    assert_eq!(
        batcher.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Applied { event_count: 1 }
    );
    assert_eq!(batcher.pending_event_count().unwrap(), 1);
    assert_eq!(
        batcher.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Applied { event_count: 1 }
    );

    let ranges = sink
        .batches()
        .into_iter()
        .map(|batch| (batch.start_sequence(), batch.end_sequence()))
        .collect::<Vec<_>>();
    assert_eq!(ranges, [(1, 1), (1, 1), (2, 2)]);
}

#[tokio::test]
async fn existing_sink_result_confirms_a_replayed_sealed_segment() {
    let directory = TestDirectory::new();
    let failing_sink = Arc::new(RecordingSink::with_outcomes([Err(
        BillingBatchSinkError::OutcomeUnknown,
    )]));
    let first = FileBillingBatcher::open(directory.path(), failing_sink).unwrap();
    first.record(complete_event(8, 9, 9)).await.unwrap();
    assert_eq!(
        first.flush_once().await,
        Err(BillingBatchError::Sink(
            BillingBatchSinkError::OutcomeUnknown
        ))
    );
    drop(first);

    let replay_sink = Arc::new(RecordingSink::with_outcomes([Ok(
        BillingBatchApplyOutcome::Existing,
    )]));
    let recovered = FileBillingBatcher::open(directory.path(), replay_sink).unwrap();
    assert_eq!(
        recovered.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Existing { event_count: 1 }
    );
    assert_eq!(recovered.pending_event_count().unwrap(), 0);
}

#[tokio::test]
async fn restart_preserves_sealed_before_active_order() {
    let directory = TestDirectory::new();
    let first = FileBillingBatcher::open(
        directory.path(),
        Arc::new(RecordingSink::with_outcomes([Err(
            BillingBatchSinkError::Query,
        )])),
    )
    .unwrap();
    first.record(complete_event(13, 1, 1)).await.unwrap();
    assert_eq!(
        first.flush_once().await,
        Err(BillingBatchError::Sink(BillingBatchSinkError::Query))
    );
    first.record(complete_event(14, 1, 1)).await.unwrap();
    drop(first);

    let sink = Arc::new(RecordingSink::default());
    let recovered = FileBillingBatcher::open(directory.path(), sink.clone()).unwrap();
    assert_eq!(
        recovered.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Applied { event_count: 1 }
    );
    assert_eq!(
        recovered.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Applied { event_count: 1 }
    );
    let ranges = sink
        .batches()
        .into_iter()
        .map(|batch| (batch.start_sequence(), batch.end_sequence()))
        .collect::<Vec<_>>();
    assert_eq!(ranges, [(1, 1), (2, 2)]);
}

#[tokio::test]
async fn startup_truncates_only_an_incomplete_active_tail() {
    let directory = TestDirectory::new();
    let batcher =
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
    batcher.record(complete_event(9, 2, 2)).await.unwrap();
    drop(batcher);

    let mut file = OpenOptions::new()
        .append(true)
        .open(directory.active_wal())
        .unwrap();
    file.write_all(&[1, 2, 3, 4, 5]).unwrap();
    file.sync_all().unwrap();
    drop(file);

    let recovered =
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
    assert_eq!(recovered.pending_event_count().unwrap(), 1);
    assert_eq!(
        fs::metadata(directory.active_wal()).unwrap().len(),
        WAL_HEADER_SIZE + WAL_RECORD_SIZE
    );
}

#[tokio::test]
async fn startup_rejects_a_complete_record_with_bad_checksum() {
    let directory = TestDirectory::new();
    let batcher =
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
    batcher.record(complete_event(10, 2, 2)).await.unwrap();
    drop(batcher);

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.active_wal())
        .unwrap();
    file.seek(SeekFrom::Start(WAL_HEADER_SIZE + 20)).unwrap();
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte).unwrap();
    byte[0] ^= 0xff;
    file.seek(SeekFrom::Start(WAL_HEADER_SIZE + 20)).unwrap();
    file.write_all(&byte).unwrap();
    file.sync_all().unwrap();
    drop(file);

    assert_eq!(
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap_err(),
        BillingBatchError::CorruptWal
    );
}

#[test]
fn wal_directory_is_exclusive_but_reopens_after_owner_drop() {
    let directory = TestDirectory::new();
    let first =
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
    assert_eq!(
        FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap_err(),
        BillingBatchError::WalLocked
    );
    drop(first);
    FileBillingBatcher::open(directory.path(), Arc::new(RecordingSink::default())).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_records_receive_a_contiguous_sequence_and_checked_total() {
    let directory = TestDirectory::new();
    let sink = Arc::new(RecordingSink::default());
    let batcher = FileBillingBatcher::open(directory.path(), sink.clone()).unwrap();
    let mut tasks = Vec::new();
    for marker in 20..36 {
        let batcher = batcher.clone();
        tasks.push(tokio::spawn(async move {
            batcher.record(user_only_event(marker, 1, 1)).await
        }));
    }
    let mut sequences = Vec::new();
    for task in tasks {
        let outcome = task.await.unwrap().unwrap();
        let BillingBatchRecordOutcome::Applied { sequence } = outcome else {
            panic!("唯一并发事件必须首次写入");
        };
        sequences.push(sequence);
    }
    sequences.sort_unstable();
    assert_eq!(sequences, (1_u64..=16).collect::<Vec<_>>());
    assert_eq!(
        batcher.flush_once().await.unwrap(),
        BillingBatchFlushOutcome::Applied { event_count: 16 }
    );
    let batches = sink.batches();
    let user = batches[0].users()[0];
    assert_eq!(user.used_quota_delta().units(), 16);
    assert_eq!(user.request_count_delta(), 16);
}

#[tokio::test]
async fn public_debug_output_never_exposes_identifiers_or_deltas() {
    let directory = TestDirectory::new();
    let sink = Arc::new(RecordingSink::default());
    let batcher = FileBillingBatcher::open(directory.path(), sink.clone()).unwrap();
    let event = complete_event(71, 987_654, 123_456);
    batcher.record(event).await.unwrap();
    batcher.flush_once().await.unwrap();
    let batch = sink.batches().pop().unwrap();
    let rendered = format!(
        "{event:?} {:?} {:?} {:?} {batch:?} {batcher:?} {:?}",
        event.user().unwrap(),
        event.token().unwrap(),
        event.channel().unwrap(),
        batch.writer_id()
    );
    for secret in ["987654", "123456", "717171", "1111", "2222", "3333"] {
        assert!(!rendered.contains(secret));
    }
}

fn complete_event(marker: u8, quota: i64, used: i64) -> BillingBatchEvent {
    BillingBatchEvent::new(
        reservation_id(marker),
        Some(
            UserBillingDelta::new(UserId::new(11).unwrap(), delta(-quota), delta(used), 1).unwrap(),
        ),
        Some(
            TokenBillingDelta::new(TokenId::new(22).unwrap(), delta(-quota), delta(used)).unwrap(),
        ),
        Some(ChannelBillingDelta::new(ChannelId::new(33).unwrap(), delta(used)).unwrap()),
    )
    .unwrap()
}

fn user_only_event(marker: u8, used: i64, requests: u64) -> BillingBatchEvent {
    BillingBatchEvent::new(
        reservation_id(marker),
        Some(
            UserBillingDelta::new(
                UserId::new(11).unwrap(),
                QuotaDelta::ZERO,
                delta(used),
                requests,
            )
            .unwrap(),
        ),
        None,
        None,
    )
    .unwrap()
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).unwrap()
}

fn delta(units: i64) -> QuotaDelta {
    QuotaDelta::new(units).unwrap()
}
