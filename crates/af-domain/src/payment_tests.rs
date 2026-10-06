use super::{
    TopupIdentifierError, TopupOrderId, TopupOrderStatus, TopupPaymentEventId,
    TopupPaymentEventType, TopupRequestId, TopupStateCodeError,
};

#[test]
fn payment_identifiers_round_trip_without_debug_disclosure() {
    let order = TopupOrderId::new([0x12; 16]).unwrap();
    let request = TopupRequestId::new([0x34; 16]).unwrap();
    let event = TopupPaymentEventId::new([0x56; 16]).unwrap();

    for (key, rendered) in [
        (order.persistence_key(), format!("{order:?}")),
        (request.persistence_key(), format!("{request:?}")),
        (event.persistence_key(), format!("{event:?}")),
    ] {
        assert_eq!(key.len(), 32);
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(&key));
    }
    assert_eq!(
        TopupOrderId::from_persistence_key(&order.persistence_key()).unwrap(),
        order
    );
    assert_eq!(
        TopupRequestId::from_persistence_key(&request.persistence_key()).unwrap(),
        request
    );
    assert_eq!(
        TopupPaymentEventId::from_persistence_key(&event.persistence_key()).unwrap(),
        event
    );
}

#[test]
fn payment_identifiers_reject_zero_and_noncanonical_keys() {
    assert_eq!(
        TopupOrderId::new([0; 16]),
        Err(TopupIdentifierError::AllZero)
    );
    for key in [
        "00000000000000000000000000000000",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "gggggggggggggggggggggggggggggggg",
        "1111111111111111111111111111111",
    ] {
        assert_eq!(
            TopupPaymentEventId::from_persistence_key(key),
            Err(if key.bytes().all(|byte| byte == b'0') {
                TopupIdentifierError::AllZero
            } else {
                TopupIdentifierError::InvalidEncoding
            })
        );
    }
}

#[test]
fn payment_states_have_stable_codes_and_targets() {
    for (status, code, open) in [
        (TopupOrderStatus::Created, 1, true),
        (TopupOrderStatus::Pending, 2, true),
        (TopupOrderStatus::Paid, 3, false),
        (TopupOrderStatus::Failed, 4, false),
        (TopupOrderStatus::Canceled, 5, false),
        (TopupOrderStatus::Expired, 6, false),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(status.is_open(), open);
        assert_eq!(TopupOrderStatus::try_from(code), Ok(status));
    }
    assert_eq!(
        TopupOrderStatus::try_from(0),
        Err(TopupStateCodeError::InvalidOrderStatus)
    );

    for (event, code, target) in [
        (TopupPaymentEventType::Succeeded, 1, TopupOrderStatus::Paid),
        (TopupPaymentEventType::Failed, 2, TopupOrderStatus::Failed),
        (TopupPaymentEventType::Expired, 3, TopupOrderStatus::Expired),
    ] {
        assert_eq!(event.code(), code);
        assert_eq!(event.target_status(), target);
        assert_eq!(TopupPaymentEventType::try_from(code), Ok(event));
    }
    assert_eq!(
        TopupPaymentEventType::try_from(0),
        Err(TopupStateCodeError::InvalidEventType)
    );
}
