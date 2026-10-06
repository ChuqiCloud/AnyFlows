use std::error::Error as _;

use super::{WalletEventId, WalletEventIdError};

#[test]
fn wallet_event_id_round_trips_without_log_disclosure() {
    let event_id = WalletEventId::new([0xab; 16]).unwrap();
    let key = event_id.persistence_key();

    assert_eq!(key, "abababababababababababababababab");
    assert_eq!(WalletEventId::from_persistence_key(&key), Ok(event_id));
    assert_eq!(format!("{event_id:?}"), "WalletEventId(<redacted>)");
    assert!(!event_id.is_system_opening());
}

#[test]
fn wallet_event_id_rejects_invalid_and_reserved_shapes_are_detectable() {
    assert_eq!(
        WalletEventId::new([0; 16]),
        Err(WalletEventIdError::AllZero)
    );
    for invalid in [
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "gggggggggggggggggggggggggggggggg",
    ] {
        assert_eq!(
            WalletEventId::from_persistence_key(invalid),
            Err(WalletEventIdError::InvalidEncoding)
        );
    }

    let opening = WalletEventId::from_persistence_key("0000000000000001000000000000002a")
        .expect("系统 opening 标识必须满足持久化格式");
    assert!(opening.is_system_opening());
    assert!(WalletEventIdError::AllZero.source().is_none());
}
