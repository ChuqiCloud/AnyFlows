use std::{
    collections::VecDeque,
    error::Error,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use af_domain::{BillingReservationId, Quota};

use crate::{
    BillingSession, BillingSessionError, BillingSessionState, RefundSignalOutcome, RefundSignalPort,
};

#[derive(Default)]
struct RecordingRefundPort {
    outcomes: Mutex<VecDeque<RefundSignalOutcome>>,
    received: Mutex<Vec<BillingReservationId>>,
    attempts: AtomicUsize,
}

impl RecordingRefundPort {
    fn with_outcomes(outcomes: impl IntoIterator<Item = RefundSignalOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
            ..Self::default()
        }
    }

    fn received(&self) -> Vec<BillingReservationId> {
        self.received
            .lock()
            .expect("测试退款记录锁不能损坏")
            .clone()
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Acquire)
    }
}

impl RefundSignalPort for RecordingRefundPort {
    fn try_signal_refund(&self, reservation_id: BillingReservationId) -> RefundSignalOutcome {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        let outcome = self
            .outcomes
            .lock()
            .expect("测试退款结果锁不能损坏")
            .pop_front()
            .unwrap_or(RefundSignalOutcome::Accepted);
        if outcome == RefundSignalOutcome::Accepted {
            self.received
                .lock()
                .expect("测试退款记录锁不能损坏")
                .push(reservation_id);
        }
        outcome
    }
}

struct PanickingRefundPort {
    attempts: AtomicUsize,
}

impl RefundSignalPort for PanickingRefundPort {
    fn try_signal_refund(&self, _reservation_id: BillingReservationId) -> RefundSignalOutcome {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        panic!("测试退款端口异常");
    }
}

#[test]
fn reserved_drop_signals_the_exact_reservation_once() {
    let port = Arc::new(RecordingRefundPort::default());
    let id = reservation_id(1);

    {
        let session = BillingSession::from_reserved(id, port.clone());
        assert_eq!(session.reservation_id(), id);
        assert_eq!(session.state(), BillingSessionState::Reserved);
    }

    assert_eq!(port.received(), vec![id]);
    assert_eq!(port.attempts(), 1);
}

#[test]
fn question_mark_early_return_still_signals_once() {
    let port = Arc::new(RecordingRefundPort::default());
    let id = reservation_id(2);

    assert_eq!(early_return_with_question_mark(id, port.clone()), Err(()));
    assert_eq!(port.received(), vec![id]);
    assert_eq!(port.attempts(), 1);
}

#[test]
fn panic_unwind_still_signals_once() {
    let port = Arc::new(RecordingRefundPort::default());
    let id = reservation_id(3);

    let unwind = catch_unwind(AssertUnwindSafe({
        let port = Arc::clone(&port);
        move || {
            let _session = BillingSession::from_reserved(id, port);
            panic!("测试业务 panic");
        }
    }));

    assert!(unwind.is_err());
    assert_eq!(port.received(), vec![id]);
    assert_eq!(port.attempts(), 1);
}

#[test]
fn settlement_is_frozen_before_io_and_never_refunds_on_unwind() {
    let port = Arc::new(RecordingRefundPort::default());
    let id = reservation_id(4);
    let actual = quota(37);

    let unwind = catch_unwind(AssertUnwindSafe({
        let port = Arc::clone(&port);
        move || {
            let mut session = BillingSession::from_reserved(id, port);
            let request = session.begin_settlement(actual).unwrap();
            assert_eq!(request.reservation_id(), id);
            assert_eq!(request.actual_quota(), actual);
            assert_eq!(session.pending_settlement(), Some(request));
            panic!("模拟结算 future 被取消");
        }
    }));

    assert!(unwind.is_err());
    assert!(port.received().is_empty());
    assert_eq!(port.attempts(), 0);
}

#[test]
fn settlement_replay_requires_the_first_actual_quota() {
    let port = Arc::new(RecordingRefundPort::default());
    let mut session = BillingSession::from_reserved(reservation_id(5), port.clone());
    let zero = quota(0);

    let request = session.begin_settlement(zero).unwrap();
    assert_eq!(session.begin_settlement(zero), Ok(request));
    assert_eq!(
        session.begin_settlement(quota(1)),
        Err(BillingSessionError::SettlementQuotaConflict)
    );
    assert_eq!(session.pending_settlement(), Some(request));
    assert_eq!(session.state(), BillingSessionState::SettlementPending);
    drop(session);

    assert!(port.received().is_empty());
    assert_eq!(port.attempts(), 0);
}

#[test]
fn settled_transition_is_monotonic_and_idempotent() {
    let port = Arc::new(RecordingRefundPort::default());
    let mut reserved = BillingSession::from_reserved(reservation_id(6), port.clone());

    assert_eq!(
        reserved.mark_settled(),
        Err(BillingSessionError::InvalidTransition)
    );
    assert_eq!(reserved.state(), BillingSessionState::Reserved);
    let _request = reserved.begin_settlement(quota(12)).unwrap();
    assert_eq!(reserved.mark_settled(), Ok(()));
    assert_eq!(reserved.mark_settled(), Ok(()));
    assert_eq!(reserved.state(), BillingSessionState::Settled);
    assert_eq!(
        reserved.request_refund(),
        Err(BillingSessionError::InvalidTransition)
    );
    drop(reserved);

    assert!(port.received().is_empty());
    assert_eq!(port.attempts(), 0);
}

#[test]
fn explicit_refund_is_idempotent_and_disarms_drop() {
    let port = Arc::new(RecordingRefundPort::default());
    let id = reservation_id(7);
    let mut session = BillingSession::from_reserved(id, port.clone());

    assert_eq!(session.request_refund(), Ok(()));
    assert_eq!(session.request_refund(), Ok(()));
    assert_eq!(session.state(), BillingSessionState::RefundQueued);
    assert!(session.pending_settlement().is_none());
    assert_eq!(
        session.begin_settlement(quota(2)),
        Err(BillingSessionError::InvalidTransition)
    );
    drop(session);

    assert_eq!(port.received(), vec![id]);
    assert_eq!(port.attempts(), 1);
}

#[test]
fn pending_session_rejects_refund_without_rearming_drop() {
    let port = Arc::new(RecordingRefundPort::default());
    let mut session = BillingSession::from_reserved(reservation_id(8), port.clone());
    let _request = session.begin_settlement(quota(5)).unwrap();

    assert_eq!(
        session.request_refund(),
        Err(BillingSessionError::InvalidTransition)
    );
    assert_eq!(session.state(), BillingSessionState::SettlementPending);
    drop(session);

    assert!(port.received().is_empty());
    assert_eq!(port.attempts(), 0);
}

#[test]
fn rejected_explicit_signal_keeps_drop_fallback_armed() {
    let port = Arc::new(RecordingRefundPort::with_outcomes([
        RefundSignalOutcome::Saturated,
        RefundSignalOutcome::Accepted,
    ]));
    let id = reservation_id(9);
    let mut session = BillingSession::from_reserved(id, port.clone());

    assert_eq!(
        session.request_refund(),
        Err(BillingSessionError::RefundSignalSaturated)
    );
    assert_eq!(session.state(), BillingSessionState::Reserved);
    drop(session);

    assert_eq!(port.received(), vec![id]);
    assert_eq!(port.attempts(), 2);
}

#[test]
fn closed_or_panicking_port_never_panics_from_drop() {
    let closed = Arc::new(RecordingRefundPort::with_outcomes([
        RefundSignalOutcome::Closed,
    ]));
    let closed_drop = catch_unwind(AssertUnwindSafe({
        let closed = Arc::clone(&closed);
        move || drop(BillingSession::from_reserved(reservation_id(10), closed))
    }));
    assert!(closed_drop.is_ok());
    assert!(closed.received().is_empty());
    assert_eq!(closed.attempts(), 1);

    let panicking = Arc::new(PanickingRefundPort {
        attempts: AtomicUsize::new(0),
    });
    let business_unwind = catch_unwind(AssertUnwindSafe({
        let panicking = Arc::clone(&panicking);
        move || {
            let _session = BillingSession::from_reserved(reservation_id(11), panicking);
            panic!("测试原始业务 panic");
        }
    }));
    assert!(business_unwind.is_err());
    assert_eq!(panicking.attempts.load(Ordering::Acquire), 1);
}

#[test]
fn explicit_panicking_signal_is_reported_and_keeps_drop_fallback_armed() {
    let port = Arc::new(PanickingRefundPort {
        attempts: AtomicUsize::new(0),
    });
    let mut session = BillingSession::from_reserved(reservation_id(13), port.clone());

    assert_eq!(
        session.request_refund(),
        Err(BillingSessionError::RefundSignalPanicked)
    );
    assert_eq!(session.state(), BillingSessionState::Reserved);
    drop(session);

    assert_eq!(port.attempts.load(Ordering::Acquire), 2);
}

#[test]
fn signal_errors_and_debug_output_are_redacted() {
    let port = Arc::new(RecordingRefundPort::with_outcomes([
        RefundSignalOutcome::Closed,
    ]));
    let id = reservation_id(0xab);
    let key = id.persistence_key();
    let actual = quota(314_159);
    let mut session = BillingSession::from_reserved(id, port);

    assert_eq!(
        session.request_refund(),
        Err(BillingSessionError::RefundSignalClosed)
    );
    let request = session.begin_settlement(actual).unwrap();
    let rendered = format!("{session:?} {request:?}");
    assert!(rendered.contains("SettlementPending"));
    assert!(!rendered.contains(&key));
    assert!(!rendered.contains("314159"));
    assert!(!rendered.contains("RecordingRefundPort"));

    for error in [
        BillingSessionError::InvalidTransition,
        BillingSessionError::SettlementQuotaConflict,
        BillingSessionError::RefundSignalSaturated,
        BillingSessionError::RefundSignalClosed,
        BillingSessionError::RefundSignalPanicked,
    ] {
        let diagnostic = format!("{error:?} {error}");
        assert!(!diagnostic.contains(&key));
        assert!(!diagnostic.contains("314159"));
        assert!(error.source().is_none());
    }
}

#[test]
fn session_can_move_across_threads_without_duplicate_signal() {
    assert_send_sync_static::<BillingSession>();

    let port = Arc::new(RecordingRefundPort::default());
    let id = reservation_id(12);
    let session = BillingSession::from_reserved(id, port.clone());
    thread::spawn(move || drop(session))
        .join()
        .expect("跨线程释放计费会话不能 panic");

    assert_eq!(port.received(), vec![id]);
    assert_eq!(port.attempts(), 1);
}

fn early_return_with_question_mark(
    id: BillingReservationId,
    port: Arc<dyn RefundSignalPort>,
) -> Result<(), ()> {
    let _session = BillingSession::from_reserved(id, port);
    Err(())?;
    Ok(())
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).expect("测试预留标识必须非零")
}

fn quota(units: i64) -> Quota {
    Quota::new(units).expect("测试额度必须非负")
}

fn assert_send_sync_static<T: Send + Sync + 'static>() {}
