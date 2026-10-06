use super::*;

fn id(byte: u8) -> RefundRequestId {
    RefundRequestId::new([byte; 16]).expect("non-zero refund id")
}

fn key(byte: u8) -> RefundRequestKey {
    RefundRequestKey::new([byte; 16]).expect("non-zero idempotency key")
}

#[test]
fn refund_ids_round_trip_without_exposing_debug_contents() {
    let value = id(0xabu8);
    let encoded = value.persistence_key();
    assert_eq!(encoded, "ab".repeat(16));
    assert_eq!(RefundRequestId::from_persistence_key(&encoded), Ok(value));
    assert_eq!(format!("{value:?}"), "RefundRequestId(<redacted>)");
}

#[test]
fn refund_state_machine_is_closed_and_replay_safe() {
    assert!(RefundRequestStatus::Requested.can_transition_to(RefundRequestStatus::Submitted));
    assert!(RefundRequestStatus::Requested.can_transition_to(RefundRequestStatus::Canceled));
    assert!(RefundRequestStatus::Submitted.can_transition_to(RefundRequestStatus::Succeeded));
    assert!(RefundRequestStatus::Submitted.can_transition_to(RefundRequestStatus::Failed));
    assert!(RefundRequestStatus::Failed.can_transition_to(RefundRequestStatus::Submitted));
    assert!(!RefundRequestStatus::Succeeded.can_transition_to(RefundRequestStatus::Submitted));
    assert!(!RefundRequestStatus::Canceled.can_transition_to(RefundRequestStatus::Requested));
}

#[test]
fn refund_create_rejects_partial_overflow_and_invalid_order_facts() {
    let user = UserId::new(7).expect("user");
    let valid = RefundRequestCreate::new(
        id(1),
        key(2),
        user,
        RefundOrderKind::Topup,
        "01".repeat(16),
        "stripe".to_owned(),
        "pi_original".to_owned(),
        "USD".to_owned(),
        100,
        50,
        1,
    );
    assert!(valid.is_ok());
    assert!(
        RefundRequestCreate::new(
            id(1),
            key(2),
            user,
            RefundOrderKind::Topup,
            "01".repeat(16),
            "stripe".to_owned(),
            "pi_original".to_owned(),
            "USD".to_owned(),
            100,
            101,
            1,
        )
        .is_err()
    );
    assert!(
        RefundRequestCreate::new(
            id(1),
            key(2),
            user,
            RefundOrderKind::Subscription,
            "AA".repeat(16),
            "stripe".to_owned(),
            "pi_original".to_owned(),
            "USD".to_owned(),
            100,
            50,
            1,
        )
        .is_err()
    );
}

#[test]
fn persisted_history_without_payment_reference_is_readable_but_not_replayable() {
    let record = RefundRequestRecord::from_persistence(
        1,
        id(3),
        key(4),
        UserId::new(7).expect("user"),
        RefundOrderKind::Topup,
        "01".repeat(16),
        "stripe".to_owned(),
        None,
        "USD".to_owned(),
        100,
        50,
        None,
        RefundRequestStatus::Requested,
        RefundApprovalStatus::Pending,
        None,
        None,
        1,
        1,
        1,
    )
    .expect("legacy nullable payment reference remains readable");
    assert_eq!(record.payment_reference(), None);

    let write = RefundRequestCreate::new(
        id(3),
        key(4),
        UserId::new(7).expect("user"),
        RefundOrderKind::Topup,
        "01".repeat(16),
        "stripe".to_owned(),
        "pi_original".to_owned(),
        "USD".to_owned(),
        100,
        50,
        1,
    )
    .expect("new refund request");
    assert!(!record.matches_create(&write));
}
