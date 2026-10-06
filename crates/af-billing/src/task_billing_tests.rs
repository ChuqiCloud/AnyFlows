use std::{
    collections::VecDeque,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};

use crate::{
    TaskBillingError, TaskBillingLifecycle, TaskBillingPlan, TaskBillingReleaseSignalOutcome,
    TaskBillingReleaseSignalPort, TaskBillingReserveError, TaskBillingReserveFuture,
    TaskBillingReservePort, TaskBillingSettlementError, TaskBillingSettlementFuture,
    TaskBillingSettlementPort, TaskBillingSettlementRequest, TaskBillingState,
};

#[derive(Default)]
struct RecordingReservePort {
    requests: Mutex<Vec<(BillingReservationId, GatewayPrincipal, Quota)>>,
}

impl TaskBillingReservePort for RecordingReservePort {
    fn reserve<'a>(
        &'a self,
        reservation_id: BillingReservationId,
        principal: GatewayPrincipal,
        upper_bound: Quota,
    ) -> TaskBillingReserveFuture<'a> {
        Box::pin(async move {
            self.requests
                .lock()
                .unwrap()
                .push((reservation_id, principal, upper_bound));
            Ok(())
        })
    }
}

struct RecordingSettlementPort {
    outcomes: Mutex<VecDeque<Result<(), TaskBillingSettlementError>>>,
    requests: Mutex<Vec<TaskBillingSettlementRequest>>,
}

impl RecordingSettlementPort {
    fn new(outcomes: impl IntoIterator<Item = Result<(), TaskBillingSettlementError>>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<TaskBillingSettlementRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl TaskBillingSettlementPort for RecordingSettlementPort {
    fn settle<'a>(
        &'a self,
        request: TaskBillingSettlementRequest,
    ) -> TaskBillingSettlementFuture<'a> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            self.outcomes.lock().unwrap().pop_front().unwrap_or(Ok(()))
        })
    }
}

struct RecordingReleasePort {
    outcomes: Mutex<VecDeque<TaskBillingReleaseSignalOutcome>>,
    received: Mutex<Vec<BillingReservationId>>,
    attempts: AtomicUsize,
}

impl RecordingReleasePort {
    fn new(outcomes: impl IntoIterator<Item = TaskBillingReleaseSignalOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            received: Mutex::new(Vec::new()),
            attempts: AtomicUsize::new(0),
        }
    }

    fn received(&self) -> Vec<BillingReservationId> {
        self.received.lock().unwrap().clone()
    }
}

impl TaskBillingReleaseSignalPort for RecordingReleasePort {
    fn try_signal_release(
        &self,
        reservation_id: BillingReservationId,
    ) -> TaskBillingReleaseSignalOutcome {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        let outcome = self
            .outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(TaskBillingReleaseSignalOutcome::Accepted);
        if outcome == TaskBillingReleaseSignalOutcome::Accepted {
            self.received.lock().unwrap().push(reservation_id);
        }
        outcome
    }
}

#[tokio::test]
async fn plan_rejects_zero_and_replays_the_same_reservation_parameters() {
    let id = reservation_id(1);
    assert!(matches!(
        TaskBillingPlan::new(id, principal(), quota(0)),
        Err(TaskBillingError::ZeroUpperBound)
    ));

    let plan = TaskBillingPlan::new(id, principal(), quota(100)).unwrap();
    let reserve = RecordingReservePort::default();
    plan.reserve(&reserve).await.unwrap();
    plan.reserve(&reserve).await.unwrap();
    let requests = reserve.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);
}

#[tokio::test]
async fn success_settles_final_fact_once_and_replays_idempotently() {
    let settlement = Arc::new(RecordingSettlementPort::new([Ok(())]));
    let release = Arc::new(RecordingReleasePort::new([]));
    let mut lifecycle = lifecycle(2, 100, release.clone(), settlement.clone());
    lifecycle.accept_submission(quota(80)).unwrap();

    let completion = lifecycle.complete_success(Some(quota(70))).await.unwrap();
    assert_eq!(completion.actual_quota(), quota(70));
    assert!(!completion.used_submission_fallback());
    assert_eq!(lifecycle.state(), TaskBillingState::Settled);
    assert!(lifecycle.pending_settlement().is_none());

    assert_eq!(
        lifecycle.complete_success(Some(quota(70))).await.unwrap(),
        completion
    );
    assert_eq!(settlement.requests().len(), 1);
    assert_eq!(release.attempts.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn missing_final_fact_uses_the_frozen_submission_fallback() {
    let settlement = Arc::new(RecordingSettlementPort::new([Ok(())]));
    let release = Arc::new(RecordingReleasePort::new([]));
    let mut lifecycle = lifecycle(3, 100, release, settlement.clone());
    lifecycle.accept_submission(quota(61)).unwrap();

    let completion = lifecycle.complete_success(None).await.unwrap();
    assert_eq!(completion.actual_quota(), quota(61));
    assert!(completion.used_submission_fallback());
    assert_eq!(settlement.requests()[0].actual_quota(), quota(61));
}

#[tokio::test]
async fn upper_bound_violations_fail_before_persistent_settlement() {
    let settlement = Arc::new(RecordingSettlementPort::new([]));
    let release = Arc::new(RecordingReleasePort::new([]));
    let mut lifecycle = lifecycle(4, 100, release, settlement.clone());
    assert_eq!(
        lifecycle.accept_submission(quota(101)),
        Err(TaskBillingError::FallbackExceedsUpperBound)
    );
    lifecycle.accept_submission(quota(80)).unwrap();
    assert_eq!(
        lifecycle.complete_success(Some(quota(101))).await,
        Err(TaskBillingError::ActualExceedsUpperBound)
    );
    assert_eq!(lifecycle.state(), TaskBillingState::Submitted);
    assert!(settlement.requests().is_empty());
}

#[tokio::test]
async fn outcome_unknown_keeps_exact_settlement_for_same_parameter_replay() {
    let settlement = Arc::new(RecordingSettlementPort::new([
        Err(TaskBillingSettlementError::OutcomeUnknown),
        Ok(()),
    ]));
    let release = Arc::new(RecordingReleasePort::new([]));
    let mut lifecycle = lifecycle(5, 100, release, settlement.clone());
    lifecycle.accept_submission(quota(80)).unwrap();

    assert_eq!(
        lifecycle.complete_success(Some(quota(75))).await,
        Err(TaskBillingError::Settlement(
            TaskBillingSettlementError::OutcomeUnknown
        ))
    );
    assert_eq!(lifecycle.state(), TaskBillingState::SettlementPending);
    let pending = lifecycle.pending_settlement().unwrap();
    assert_eq!(pending.actual_quota(), quota(75));
    assert_eq!(
        lifecycle.complete_success(Some(quota(76))).await,
        Err(TaskBillingError::SettlementConflict)
    );

    let completion = lifecycle.complete_success(Some(quota(75))).await.unwrap();
    assert_eq!(completion.actual_quota(), quota(75));
    assert_eq!(settlement.requests(), vec![pending, pending]);
}

#[test]
fn submission_failure_and_drop_release_exactly_once() {
    let release = Arc::new(RecordingReleasePort::new([]));
    let settlement = Arc::new(RecordingSettlementPort::new([]));
    let id = reservation_id(6);
    let mut lifecycle = TaskBillingPlan::new(id, principal(), quota(100))
        .unwrap()
        .start_reserved(release.clone(), settlement);
    lifecycle.complete_failure().unwrap();
    lifecycle.complete_failure().unwrap();
    drop(lifecycle);

    assert_eq!(release.received(), vec![id]);
    assert_eq!(release.attempts.load(Ordering::Acquire), 1);
}

#[test]
fn pre_submission_early_drop_releases_but_accepted_task_requires_explicit_terminal() {
    let pre_submit = Arc::new(RecordingReleasePort::new([]));
    drop(lifecycle(
        7,
        100,
        pre_submit.clone(),
        Arc::new(RecordingSettlementPort::new([])),
    ));
    assert_eq!(pre_submit.attempts.load(Ordering::Acquire), 1);

    let submitted = Arc::new(RecordingReleasePort::new([]));
    let mut accepted = lifecycle(
        8,
        100,
        submitted.clone(),
        Arc::new(RecordingSettlementPort::new([])),
    );
    accepted.accept_submission(quota(50)).unwrap();
    drop(accepted);
    assert_eq!(submitted.attempts.load(Ordering::Acquire), 0);
}

#[test]
fn rejected_release_signal_keeps_reserved_drop_fallback_armed() {
    let release = Arc::new(RecordingReleasePort::new([
        TaskBillingReleaseSignalOutcome::Saturated,
        TaskBillingReleaseSignalOutcome::Accepted,
    ]));
    let id = reservation_id(9);
    let mut lifecycle = TaskBillingPlan::new(id, principal(), quota(100))
        .unwrap()
        .start_reserved(release.clone(), Arc::new(RecordingSettlementPort::new([])));
    assert_eq!(
        lifecycle.complete_failure(),
        Err(TaskBillingError::ReleaseSignalSaturated)
    );
    assert_eq!(lifecycle.state(), TaskBillingState::Reserved);
    drop(lifecycle);
    assert_eq!(release.received(), vec![id]);
    assert_eq!(release.attempts.load(Ordering::Acquire), 2);
}

#[test]
fn debug_output_hides_identifiers_principals_and_quotas() {
    let id = reservation_id(0xab);
    let key = id.persistence_key();
    let plan = TaskBillingPlan::new(id, principal(), quota(314_159)).unwrap();
    let plan_debug = format!("{plan:?}");
    assert!(!plan_debug.contains(&key));
    assert!(!plan_debug.contains("314159"));

    let mut lifecycle = plan.start_reserved(
        Arc::new(RecordingReleasePort::new([])),
        Arc::new(RecordingSettlementPort::new([])),
    );
    lifecycle.accept_submission(quota(271_828)).unwrap();
    let debug = format!("{lifecycle:?}");
    assert!(debug.contains("Submitted"));
    assert!(!debug.contains(&key));
    assert!(!debug.contains("271828"));
}

#[test]
fn release_port_panic_never_escapes_drop() {
    struct PanickingReleasePort;

    impl TaskBillingReleaseSignalPort for PanickingReleasePort {
        fn try_signal_release(
            &self,
            _reservation_id: BillingReservationId,
        ) -> TaskBillingReleaseSignalOutcome {
            panic!("测试任务释放端口异常");
        }
    }

    let unwind = catch_unwind(AssertUnwindSafe(|| {
        drop(
            TaskBillingPlan::new(reservation_id(10), principal(), quota(100))
                .unwrap()
                .start_reserved(
                    Arc::new(PanickingReleasePort),
                    Arc::new(RecordingSettlementPort::new([])),
                ),
        );
    }));
    assert!(unwind.is_ok());
}

fn lifecycle(
    marker: u8,
    upper_bound: i64,
    release: Arc<RecordingReleasePort>,
    settlement: Arc<RecordingSettlementPort>,
) -> TaskBillingLifecycle {
    TaskBillingPlan::new(reservation_id(marker), principal(), quota(upper_bound))
        .unwrap()
        .start_reserved(release, settlement)
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).unwrap()
}

fn principal() -> GatewayPrincipal {
    GatewayPrincipal::new(
        TokenId::new(1).unwrap(),
        UserId::new(2).unwrap(),
        GroupId::new(3).unwrap(),
    )
}

fn quota(units: i64) -> Quota {
    Quota::new(units).unwrap()
}

#[allow(dead_code)]
fn _assert_error_is_send_sync(error: TaskBillingReserveError) {
    fn assert_send_sync<T: Send + Sync>(_: T) {}
    assert_send_sync(error);
}
