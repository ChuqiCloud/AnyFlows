use std::{
    collections::VecDeque,
    future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use tokio::sync::{Barrier, Notify, oneshot};

use super::*;
use crate::UpstreamOAuthProvider;

#[test]
fn config_rejects_unbounded_or_busy_loop_values() {
    let second = Duration::from_secs(1);
    for (batch, concurrency, interval, lead, expected) in [
        (
            0,
            1,
            second,
            second,
            OAuthRefreshSupervisorConfigError::InvalidBatchSize,
        ),
        (
            MAX_OAUTH_REFRESH_CANDIDATES + 1,
            1,
            second,
            second,
            OAuthRefreshSupervisorConfigError::InvalidBatchSize,
        ),
        (
            1,
            0,
            second,
            second,
            OAuthRefreshSupervisorConfigError::InvalidConcurrency,
        ),
        (
            1,
            2,
            second,
            second,
            OAuthRefreshSupervisorConfigError::InvalidConcurrency,
        ),
        (
            1,
            1,
            Duration::ZERO,
            second,
            OAuthRefreshSupervisorConfigError::ZeroInterval,
        ),
        (
            1,
            1,
            second,
            Duration::ZERO,
            OAuthRefreshSupervisorConfigError::ZeroRefreshBeforeExpiry,
        ),
    ] {
        assert_eq!(
            OAuthRefreshSupervisorConfig::new(batch, concurrency, interval, lead),
            Err(expected)
        );
    }
}

#[tokio::test]
async fn run_once_bounds_concurrency_and_summarizes_closed_outcomes() {
    let source = FakeSource::new(
        vec![Ok(OAuthRefreshBackfillPage {
            scanned: 3,
            projected: 1,
            incomplete: 1,
            conflicted: 1,
            last_credential_id: Some(credential_id(3)),
        })],
        vec![Ok((0..6).collect())],
    );
    let runner = FakeRunner::new(vec![
        Ok(OAuthRefreshCoordinatorOutcome::Stored),
        Ok(OAuthRefreshCoordinatorOutcome::Stale),
        Ok(OAuthRefreshCoordinatorOutcome::TargetNotFound),
        Ok(OAuthRefreshCoordinatorOutcome::ProviderMismatch),
        Ok(OAuthRefreshCoordinatorOutcome::LeaseHeld),
        Err(OAuthRefreshCoordinatorError::ProviderProfileNotConfigured {
            provider: UpstreamOAuthProvider::Codex,
        }),
    ]);
    let mut supervisor = OAuthRefreshSupervisorCore::new(
        source.clone(),
        runner.clone(),
        config(6, 2, Duration::from_secs(60)),
    );

    let report = supervisor
        .run_once_at(UNIX_EPOCH + Duration::from_secs(100))
        .await
        .unwrap();

    assert_eq!(report.backfill_scanned(), 3);
    assert_eq!(report.backfill_projected(), 1);
    assert_eq!(report.backfill_incomplete(), 1);
    assert_eq!(report.backfill_conflicted(), 1);
    assert_eq!(report.loaded(), 6);
    assert_eq!(report.stored(), 1);
    assert_eq!(report.stale(), 1);
    assert_eq!(report.target_not_found(), 1);
    assert_eq!(report.provider_mismatch(), 1);
    assert_eq!(report.lease_held(), 1);
    assert_eq!(report.failed(), 1);
    assert_eq!(runner.max_active(), 2);
    assert_eq!(source.due_calls(), vec![(110, 6)]);
}

#[tokio::test]
async fn backfill_advances_by_cursor_and_stops_after_the_tail() {
    let source = FakeSource::new(
        vec![
            Ok(OAuthRefreshBackfillPage {
                scanned: 2,
                last_credential_id: Some(credential_id(2)),
                ..OAuthRefreshBackfillPage::default()
            }),
            Ok(OAuthRefreshBackfillPage::default()),
        ],
        vec![Ok(Vec::new()), Ok(Vec::new()), Ok(Vec::new())],
    );
    let mut supervisor = OAuthRefreshSupervisorCore::new(
        source.clone(),
        FakeRunner::new(Vec::new()),
        config(2, 1, Duration::from_secs(60)),
    );

    for _ in 0..3 {
        supervisor
            .run_once_at(UNIX_EPOCH + Duration::from_secs(100))
            .await
            .unwrap();
    }

    assert_eq!(
        source.backfill_calls(),
        vec![(None, 2), (Some(credential_id(2)), 2)]
    );
}

#[tokio::test(start_paused = true)]
async fn periodic_runner_starts_immediately_and_retries_after_load_failure() {
    let source = FakeSource::new(
        vec![Ok(OAuthRefreshBackfillPage::default())],
        vec![
            Err(OAuthRefreshPersistenceError::RepositoryUnavailable),
            Ok(Vec::new()),
        ],
    );
    let supervisor = OAuthRefreshSupervisorCore::new(
        source.clone(),
        FakeRunner::new(Vec::new()),
        config(1, 1, Duration::from_millis(250)),
    );
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut supervisor = supervisor;
        supervisor
            .run_periodic_until(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    tokio::time::timeout(Duration::from_millis(100), source.wait_for_due_calls(1))
        .await
        .expect("首轮必须在启动后立即执行");
    tokio::time::advance(Duration::from_millis(250)).await;
    tokio::time::timeout(Duration::from_secs(1), source.wait_for_due_calls(2))
        .await
        .expect("候选加载失败后必须进入下一周期");
    shutdown_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn shutdown_cancels_inflight_refresh_future() {
    let source = FakeSource::new(
        vec![Ok(OAuthRefreshBackfillPage::default())],
        vec![Ok(vec![1])],
    );
    let runner = BlockingRunner::default();
    let supervisor = OAuthRefreshSupervisorCore::new(
        source,
        runner.clone(),
        config(1, 1, Duration::from_secs(60)),
    );
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut supervisor = supervisor;
        supervisor
            .run_periodic_until(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    runner.started.notified().await;
    shutdown_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(runner.dropped.load(Ordering::Acquire), 1);
}

fn config(
    batch_size: usize,
    concurrency: usize,
    interval: Duration,
) -> OAuthRefreshSupervisorConfig {
    OAuthRefreshSupervisorConfig::new(batch_size, concurrency, interval, Duration::from_secs(10))
        .unwrap()
}

fn credential_id(value: i64) -> CredentialId {
    CredentialId::new(value).unwrap()
}

#[derive(Clone)]
struct FakeSource {
    state: Arc<Mutex<FakeSourceState>>,
    due_changed: Arc<Notify>,
}

struct FakeSourceState {
    backfills: VecDeque<Result<OAuthRefreshBackfillPage, OAuthRefreshPersistenceError>>,
    candidates: VecDeque<Result<Vec<usize>, OAuthRefreshPersistenceError>>,
    backfill_calls: Vec<(Option<CredentialId>, usize)>,
    due_calls: Vec<(i64, usize)>,
}

impl FakeSource {
    fn new(
        backfills: Vec<Result<OAuthRefreshBackfillPage, OAuthRefreshPersistenceError>>,
        candidates: Vec<Result<Vec<usize>, OAuthRefreshPersistenceError>>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeSourceState {
                backfills: backfills.into(),
                candidates: candidates.into(),
                backfill_calls: Vec::new(),
                due_calls: Vec::new(),
            })),
            due_changed: Arc::new(Notify::new()),
        }
    }

    fn backfill_calls(&self) -> Vec<(Option<CredentialId>, usize)> {
        self.state.lock().unwrap().backfill_calls.clone()
    }

    fn due_calls(&self) -> Vec<(i64, usize)> {
        self.state.lock().unwrap().due_calls.clone()
    }

    async fn wait_for_due_calls(&self, expected: usize) {
        loop {
            let notified = self.due_changed.notified();
            if self.state.lock().unwrap().due_calls.len() >= expected {
                return;
            }
            notified.await;
        }
    }
}

impl OAuthRefreshCandidateSource for FakeSource {
    type Candidate = usize;

    async fn backfill_missing_expiration_projections(
        &self,
        after_credential_id: Option<CredentialId>,
        limit: usize,
    ) -> Result<OAuthRefreshBackfillPage, OAuthRefreshPersistenceError> {
        let mut state = self.state.lock().unwrap();
        state.backfill_calls.push((after_credential_id, limit));
        state
            .backfills
            .pop_front()
            .unwrap_or(Ok(OAuthRefreshBackfillPage::default()))
    }

    async fn due_candidates(
        &self,
        refresh_before_epoch_seconds: i64,
        limit: usize,
    ) -> Result<Vec<Self::Candidate>, OAuthRefreshPersistenceError> {
        let result = {
            let mut state = self.state.lock().unwrap();
            state.due_calls.push((refresh_before_epoch_seconds, limit));
            state.candidates.pop_front().unwrap_or(Ok(Vec::new()))
        };
        self.due_changed.notify_one();
        result
    }
}

#[derive(Clone)]
struct FakeRunner {
    results: Arc<Vec<Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError>>>,
    active: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
    barrier: Arc<Barrier>,
}

impl FakeRunner {
    fn new(
        results: Vec<Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError>>,
    ) -> Self {
        Self {
            results: Arc::new(results),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            // 汇总测试使用并发 2 且六个候选；空候选测试不会等待屏障。
            barrier: Arc::new(Barrier::new(2)),
        }
    }

    fn max_active(&self) -> usize {
        self.max_active.load(Ordering::Acquire)
    }
}

impl OAuthRefreshCandidateRunner<usize> for FakeRunner {
    async fn refresh(
        &self,
        candidate: usize,
    ) -> Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError> {
        let result = self.results[candidate];
        let current = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.max_active.fetch_max(current, Ordering::AcqRel);
        self.barrier.wait().await;
        self.active.fetch_sub(1, Ordering::AcqRel);
        result
    }
}

#[derive(Clone, Default)]
struct BlockingRunner {
    started: Arc<Notify>,
    dropped: Arc<AtomicUsize>,
}

impl OAuthRefreshCandidateRunner<usize> for BlockingRunner {
    async fn refresh(
        &self,
        _candidate: usize,
    ) -> Result<OAuthRefreshCoordinatorOutcome, OAuthRefreshCoordinatorError> {
        let guard = DropCounter(Arc::clone(&self.dropped));
        let _guard = guard;
        self.started.notify_one();
        future::pending().await
    }
}

struct DropCounter(Arc<AtomicUsize>);

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}
