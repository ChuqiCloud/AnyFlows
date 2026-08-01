use super::{
    RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId, RedemptionCodeStatus,
    RedemptionIdentifierError, RedemptionStateCodeError,
};

#[test]
fn redemption_identifiers_round_trip_without_debug_disclosure() {
    let batch = RedemptionBatchId::new([0x24; 16]).unwrap();
    let code = RedemptionCodeId::new([0x42; 16]).unwrap();

    for (key, rendered) in [
        (batch.persistence_key(), format!("{batch:?}")),
        (code.persistence_key(), format!("{code:?}")),
    ] {
        assert_eq!(key.len(), 32);
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(&key));
    }
    assert_eq!(
        RedemptionBatchId::from_persistence_key(&batch.persistence_key()).unwrap(),
        batch
    );
    assert_eq!(
        RedemptionCodeId::from_persistence_key(&code.persistence_key()).unwrap(),
        code
    );
}

#[test]
fn redemption_identifiers_and_states_reject_unknown_values() {
    assert_eq!(
        RedemptionCodeId::new([0; 16]),
        Err(RedemptionIdentifierError::AllZero)
    );
    assert_eq!(
        RedemptionBatchId::from_persistence_key("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
        Err(RedemptionIdentifierError::InvalidEncoding)
    );

    for (status, code, active) in [
        (RedemptionBatchStatus::Active, 1, true),
        (RedemptionBatchStatus::Disabled, 2, false),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(status.is_active(), active);
        assert_eq!(RedemptionBatchStatus::try_from(code), Ok(status));
    }
    assert_eq!(
        RedemptionBatchStatus::try_from(0),
        Err(RedemptionStateCodeError::InvalidBatchStatus)
    );

    for (status, code) in [
        (RedemptionCodeStatus::Available, 1),
        (RedemptionCodeStatus::Redeemed, 2),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(RedemptionCodeStatus::try_from(code), Ok(status));
    }
    assert_eq!(
        RedemptionCodeStatus::try_from(0),
        Err(RedemptionStateCodeError::InvalidCodeStatus)
    );
}
