use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use af_db::MAX_SUBSCRIPTION_PAGE_SIZE;
use af_domain::{UserSubscriptionId, UserSubscriptionStatus};
use tokio::sync::{Notify, oneshot};
use tokio::time::timeout;

use super::*;

#[test]
fn config_rejects_busy_loop_and_unbounded_values() {
    assert_eq!(
        SubscriptionCycleSupervisorConfig::new(0, Duration::from_secs(1), 1),
        Err(SubscriptionCycleSupervisorConfigError::InvalidBatchSize)
    );
    assert_eq!(
        SubscriptionCycleSupervisorConfig::new(
            MAX_SUBSCRIPTION_PAGE_SIZE + 1,
            Duration::from_secs(1),
            1,
        ),
        Err(SubscriptionCycleSupervisorConfigError::InvalidBatchSize)
    );
    assert_eq!(
        SubscriptionCycleSupervisorConfig::new(1, Duration::ZERO, 1),
        Err(SubscriptionCycleSupervisorConfigError::ZeroInterval)
    );
    assert_eq!(
        SubscriptionCycleSupervisorConfig::new(1, Duration::from_secs(1), 0),
        Err(SubscriptionCycleSupervisorConfigError::InvalidMaxBatchesPerRun)
    );
    assert_eq!(
        SubscriptionCycleSupervisorConfig::new(
            1,
            Duration::from_secs(1),
            MAX_SUBSCRIPTION_CYCLE_BATCHES_PER_RUN + 1,
        ),
        Err(SubscriptionCycleSupervisorConfigError::InvalidMaxBatchesPerRun)
    );
}

#[tokio::test]
async fn run_once_processes_both_phases_and_replays_the_same_unknown_command() {
    let store = FakeStore::default();
    store.push_window_batch(Ok(SubscriptionWindowAdvanceBatch::new(
        vec![window_command(1), window_command(2)],
        Some(reset_cursor(200, 2)),
    )));
    store.push_window_batch(Ok(SubscriptionWindowAdvanceBatch::new(
        vec![window_command(3)],
        None,
    )));
    store.push_expiration_batch(Ok(SubscriptionExpirationBatch::new(
        vec![expiration_command(4), expiration_command(5)],
        None,
    )));
    store.push_window_outcomes([
        Err(SubscriptionRepositoryError::OutcomeUnknown),
        Ok(SubscriptionCycleMutationOutcome::Existing),
        Ok(SubscriptionCycleMutationOutcome::Applied),
        Err(SubscriptionRepositoryError::Conflict),
    ]);
    store.push_expiration_outcomes([
        Ok(SubscriptionCycleMutationOutcome::Applied),
        Ok(SubscriptionCycleMutationOutcome::Skipped),
    ]);
    let supervisor = SubscriptionCycleSupervisor::new(store.clone(), config(2, 3));

    let report = supervisor.run_once_at(300).await;

    let windows = report.window_advances();
    assert_eq!(windows.batches(), 2);
    assert_eq!(windows.loaded(), 3);
    assert_eq!(windows.applied(), 1);
    assert_eq!(windows.existing(), 1);
    assert_eq!(windows.conflicted(), 1);
    assert_eq!(windows.outcome_unknown_replays(), 1);
    assert_eq!(windows.failed(), 0);
    assert!(!windows.scan_failed());
    assert!(!windows.truncated());

    let expirations = report.expirations();
    assert_eq!(expirations.batches(), 1);
    assert_eq!(expirations.loaded(), 2);
    assert_eq!(expirations.applied(), 1);
    assert_eq!(expirations.skipped(), 1);
    assert_eq!(expirations.failed(), 0);

    assert_eq!(
        store.window_loads(),
        vec![(300, None, 2), (300, Some(reset_cursor(200, 2)), 2)]
    );
    assert_eq!(store.expiration_loads(), vec![(300, None, 2)]);
    let addresses = store.window_command_addresses();
    assert_eq!(addresses.len(), 4);
    assert_eq!(addresses[0], addresses[1]);
    assert_ne!(addresses[1], addresses[2]);
}

#[tokio::test]
async fn outcome_unknown_is_replayed_once_then_left_for_the_next_scan() {
    let store = FakeStore::default();
    store.push_window_batch(Ok(SubscriptionWindowAdvanceBatch::new(
        vec![window_command(6)],
        None,
    )));
    store.push_expiration_batch(Ok(SubscriptionExpirationBatch::new(Vec::new(), None)));
    store.push_window_outcomes([
        Err(SubscriptionRepositoryError::OutcomeUnknown),
        Err(SubscriptionRepositoryError::OutcomeUnknown),
        Ok(SubscriptionCycleMutationOutcome::Applied),
    ]);
    let supervisor = SubscriptionCycleSupervisor::new(store.clone(), config(1, 1));

    let report = supervisor.run_once_at(300).await;

    assert_eq!(report.window_advances().outcome_unknown_replays(), 1);
    assert_eq!(report.window_advances().failed(), 1);
    assert_eq!(store.window_command_addresses().len(), 2);
    assert_eq!(store.remaining_window_outcomes(), 1);
}

#[tokio::test]
async fn each_phase_has_an_independent_batch_budget() {
    let store = FakeStore::default();
    for database_id in 1..=3 {
        store.push_window_batch(Ok(SubscriptionWindowAdvanceBatch::new(
            vec![window_command(database_id as u8)],
            Some(reset_cursor(200, database_id)),
        )));
        store.push_expiration_batch(Ok(SubscriptionExpirationBatch::new(
            vec![expiration_command((database_id + 10) as u8)],
            Some(expiration_cursor(200, database_id)),
        )));
    }
    let supervisor = SubscriptionCycleSupervisor::new(store.clone(), config(1, 2));

    let report = supervisor.run_once_at(300).await;

    assert_eq!(report.window_advances().batches(), 2);
    assert_eq!(report.window_advances().loaded(), 2);
    assert_eq!(report.window_advances().applied(), 2);
    assert!(report.window_advances().truncated());
    assert_eq!(report.expirations().batches(), 2);
    assert_eq!(report.expirations().loaded(), 2);
    assert_eq!(report.expirations().applied(), 2);
    assert!(report.expirations().truncated());
    assert_eq!(store.remaining_window_batches(), 1);
    assert_eq!(store.remaining_expiration_batches(), 1);
}

#[tokio::test]
async fn one_phase_load_failure_does_not_block_the_other_phase() {
    let store = FakeStore::default();
    store.push_window_batch(Err(SubscriptionRepositoryError::Query));
    store.push_expiration_batch(Ok(SubscriptionExpirationBatch::new(
        vec![expiration_command(20)],
        None,
    )));
    let supervisor = SubscriptionCycleSupervisor::new(store, config(1, 1));

    let report = supervisor.run_once_at(300).await;

    assert!(report.window_advances().scan_failed());
    assert_eq!(report.window_advances().batches(), 0);
    assert_eq!(report.expirations().applied(), 1);
    assert!(!report.expirations().scan_failed());
}

#[tokio::test]
async fn invalid_cursor_contract_stops_only_the_broken_phase() {
    let store = FakeStore::default();
    store.push_window_batch(Ok(SubscriptionWindowAdvanceBatch::new(
        Vec::new(),
        Some(reset_cursor(200, 1)),
    )));
    store.push_expiration_batch(Ok(SubscriptionExpirationBatch::new(
        vec![expiration_command(21)],
        None,
    )));
    let supervisor = SubscriptionCycleSupervisor::new(store.clone(), config(1, 2));

    let report = supervisor.run_once_at(300).await;

    assert!(report.window_advances().scan_failed());
    assert_eq!(report.window_advances().loaded(), 0);
    assert!(store.window_command_addresses().is_empty());
    assert_eq!(report.expirations().applied(), 1);
}

#[tokio::test]
async fn periodic_runner_cancels_an_inflight_scan_on_shutdown() {
    let started = Arc::new(Notify::new());
    let supervisor = SubscriptionCycleSupervisor::new(
        BlockingStore {
            started: Arc::clone(&started),
        },
        SubscriptionCycleSupervisorConfig::new(1, Duration::from_secs(3_600), 1).unwrap(),
    );
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        supervisor
            .run_periodic_until(async move {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    started.notified().await;
    shutdown_tx.send(()).unwrap();
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
}

fn config(batch_size: usize, max_batches_per_run: usize) -> SubscriptionCycleSupervisorConfig {
    SubscriptionCycleSupervisorConfig::new(
        batch_size,
        Duration::from_millis(1),
        max_batches_per_run,
    )
    .unwrap()
}

fn window_command(seed: u8) -> UserSubscriptionWindowAdvance {
    UserSubscriptionWindowAdvance::new(subscription_id(seed), 1, 100, 200, 300).unwrap()
}

fn expiration_command(seed: u8) -> UserSubscriptionLifecycleTransition {
    UserSubscriptionLifecycleTransition::new(
        subscription_id(seed),
        1,
        UserSubscriptionStatus::Canceled,
        UserSubscriptionStatus::Expired,
        100,
        200,
        300,
    )
    .unwrap()
}

fn subscription_id(seed: u8) -> UserSubscriptionId {
    UserSubscriptionId::new([seed; 16]).unwrap()
}

fn reset_cursor(window_ends_at: u64, database_id: i64) -> SubscriptionResetDueCursor {
    SubscriptionResetDueCursor::new(window_ends_at, database_id).unwrap()
}

fn expiration_cursor(window_ends_at: u64, database_id: i64) -> SubscriptionExpirationDueCursor {
    SubscriptionExpirationDueCursor::new(window_ends_at, database_id).unwrap()
}

type MutationResult = Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError>;

#[derive(Default)]
struct FakeState {
    window_batches: VecDeque<Result<SubscriptionWindowAdvanceBatch, SubscriptionRepositoryError>>,
    expiration_batches: VecDeque<Result<SubscriptionExpirationBatch, SubscriptionRepositoryError>>,
    window_outcomes: VecDeque<MutationResult>,
    expiration_outcomes: VecDeque<MutationResult>,
    window_loads: Vec<(u64, Option<SubscriptionResetDueCursor>, usize)>,
    expiration_loads: Vec<(u64, Option<SubscriptionExpirationDueCursor>, usize)>,
    window_command_addresses: Vec<usize>,
}

#[derive(Clone, Default)]
struct FakeStore {
    state: Arc<Mutex<FakeState>>,
}

impl FakeStore {
    fn push_window_batch(
        &self,
        batch: Result<SubscriptionWindowAdvanceBatch, SubscriptionRepositoryError>,
    ) {
        self.state.lock().unwrap().window_batches.push_back(batch);
    }

    fn push_expiration_batch(
        &self,
        batch: Result<SubscriptionExpirationBatch, SubscriptionRepositoryError>,
    ) {
        self.state
            .lock()
            .unwrap()
            .expiration_batches
            .push_back(batch);
    }

    fn push_window_outcomes(&self, outcomes: impl IntoIterator<Item = MutationResult>) {
        self.state.lock().unwrap().window_outcomes.extend(outcomes);
    }

    fn push_expiration_outcomes(&self, outcomes: impl IntoIterator<Item = MutationResult>) {
        self.state
            .lock()
            .unwrap()
            .expiration_outcomes
            .extend(outcomes);
    }

    fn window_loads(&self) -> Vec<(u64, Option<SubscriptionResetDueCursor>, usize)> {
        self.state.lock().unwrap().window_loads.clone()
    }

    fn expiration_loads(&self) -> Vec<(u64, Option<SubscriptionExpirationDueCursor>, usize)> {
        self.state.lock().unwrap().expiration_loads.clone()
    }

    fn window_command_addresses(&self) -> Vec<usize> {
        self.state.lock().unwrap().window_command_addresses.clone()
    }

    fn remaining_window_batches(&self) -> usize {
        self.state.lock().unwrap().window_batches.len()
    }

    fn remaining_expiration_batches(&self) -> usize {
        self.state.lock().unwrap().expiration_batches.len()
    }

    fn remaining_window_outcomes(&self) -> usize {
        self.state.lock().unwrap().window_outcomes.len()
    }
}

impl SubscriptionCycleStore for FakeStore {
    async fn load_window_advances(
        &self,
        now: u64,
        after: Option<SubscriptionResetDueCursor>,
        limit: usize,
    ) -> Result<SubscriptionWindowAdvanceBatch, SubscriptionRepositoryError> {
        {
            let mut state = self.state.lock().unwrap();
            state.window_loads.push((now, after, limit));
            state
                .window_batches
                .pop_front()
                .unwrap_or_else(|| Ok(SubscriptionWindowAdvanceBatch::new(Vec::new(), None)))
        }
    }

    async fn advance_window(
        &self,
        command: &UserSubscriptionWindowAdvance,
    ) -> Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError> {
        {
            let mut state = self.state.lock().unwrap();
            state
                .window_command_addresses
                .push(std::ptr::from_ref(command).addr());
            state
                .window_outcomes
                .pop_front()
                .unwrap_or(Ok(SubscriptionCycleMutationOutcome::Applied))
        }
    }

    async fn load_expirations(
        &self,
        now: u64,
        after: Option<SubscriptionExpirationDueCursor>,
        limit: usize,
    ) -> Result<SubscriptionExpirationBatch, SubscriptionRepositoryError> {
        {
            let mut state = self.state.lock().unwrap();
            state.expiration_loads.push((now, after, limit));
            state
                .expiration_batches
                .pop_front()
                .unwrap_or_else(|| Ok(SubscriptionExpirationBatch::new(Vec::new(), None)))
        }
    }

    async fn expire_subscription(
        &self,
        _command: &UserSubscriptionLifecycleTransition,
    ) -> Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError> {
        self.state
            .lock()
            .unwrap()
            .expiration_outcomes
            .pop_front()
            .unwrap_or(Ok(SubscriptionCycleMutationOutcome::Applied))
    }
}

struct BlockingStore {
    started: Arc<Notify>,
}

impl SubscriptionCycleStore for BlockingStore {
    async fn load_window_advances(
        &self,
        _now: u64,
        _after: Option<SubscriptionResetDueCursor>,
        _limit: usize,
    ) -> Result<SubscriptionWindowAdvanceBatch, SubscriptionRepositoryError> {
        let started = Arc::clone(&self.started);
        started.notify_one();
        std::future::pending().await
    }

    async fn advance_window(
        &self,
        _command: &UserSubscriptionWindowAdvance,
    ) -> Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError> {
        Ok(SubscriptionCycleMutationOutcome::Applied)
    }

    async fn load_expirations(
        &self,
        _now: u64,
        _after: Option<SubscriptionExpirationDueCursor>,
        _limit: usize,
    ) -> Result<SubscriptionExpirationBatch, SubscriptionRepositoryError> {
        Ok(SubscriptionExpirationBatch::new(Vec::new(), None))
    }

    async fn expire_subscription(
        &self,
        _command: &UserSubscriptionLifecycleTransition,
    ) -> Result<SubscriptionCycleMutationOutcome, SubscriptionRepositoryError> {
        Ok(SubscriptionCycleMutationOutcome::Applied)
    }
}
