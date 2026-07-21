use std::error::Error as _;

use crate::{BillingReservationId, BillingReservationIdError};

#[test]
fn reservation_id_round_trips_through_canonical_persistence_key() {
    let bytes = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32,
        0x10,
    ];
    let reservation_id = BillingReservationId::new(bytes).unwrap();
    let key = reservation_id.persistence_key();

    assert_eq!(key, "0123456789abcdeffedcba9876543210");
    assert_eq!(
        BillingReservationId::from_persistence_key(&key),
        Ok(reservation_id)
    );
    assert_eq!(reservation_id.bytes(), bytes);
    assert_eq!(
        format!("{reservation_id:?}"),
        "BillingReservationId(<redacted>)"
    );
}

#[test]
fn reservation_id_rejects_zero_and_noncanonical_keys_without_echoing_them() {
    assert_eq!(
        BillingReservationId::new([0_u8; 16]),
        Err(BillingReservationIdError::AllZero)
    );

    for invalid in [
        "",
        "0123456789abcdeffedcba987654321",
        "0123456789abcdeffedcba98765432100",
        "0123456789ABCDEFFEDCBA9876543210",
        "0123456789abcdeffedcba987654321g",
    ] {
        let error = BillingReservationId::from_persistence_key(invalid).unwrap_err();
        let rendered = format!("{error:?}\n{error}");
        assert_eq!(error, BillingReservationIdError::InvalidEncoding);
        if !invalid.is_empty() {
            assert!(!rendered.contains(invalid));
        }
    }

    assert_eq!(
        BillingReservationId::from_persistence_key("00000000000000000000000000000000"),
        Err(BillingReservationIdError::AllZero)
    );
}

#[test]
fn reservation_id_errors_are_static_and_redacted() {
    fn assert_value<T: Copy + Eq + Ord + std::hash::Hash + Send + Sync + 'static>() {}
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}

    assert_value::<BillingReservationId>();
    assert_error::<BillingReservationIdError>();
    assert!(BillingReservationIdError::AllZero.source().is_none());
    assert!(
        BillingReservationIdError::InvalidEncoding
            .source()
            .is_none()
    );
}
