use super::{
    SubscriptionCycle, SubscriptionIdentifierError, SubscriptionPlanId, SubscriptionPlanStatus,
    SubscriptionStateCodeError, UserSubscriptionId, UserSubscriptionStatus,
};

#[test]
fn subscription_identifiers_round_trip_without_debug_disclosure() {
    let plan_id = SubscriptionPlanId::new([0x12; 16]).unwrap();
    let subscription_id = UserSubscriptionId::new([0x34; 16]).unwrap();

    assert_eq!(
        SubscriptionPlanId::from_persistence_key(&plan_id.persistence_key()),
        Ok(plan_id)
    );
    assert_eq!(
        UserSubscriptionId::from_persistence_key(&subscription_id.persistence_key()),
        Ok(subscription_id)
    );
    assert_eq!(format!("{plan_id:?}"), "SubscriptionPlanId(<redacted>)");
    assert_eq!(
        format!("{subscription_id:?}"),
        "UserSubscriptionId(<redacted>)"
    );
}

#[test]
fn subscription_identifiers_reject_zero_or_noncanonical_keys() {
    assert_eq!(
        SubscriptionPlanId::new([0; 16]),
        Err(SubscriptionIdentifierError::AllZero)
    );
    for invalid in [
        "",
        "1111111111111111111111111111111",
        "1111111111111111111111111111111G",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "00000000000000000000000000000000",
    ] {
        assert_eq!(
            UserSubscriptionId::from_persistence_key(invalid),
            Err(if invalid == "00000000000000000000000000000000" {
                SubscriptionIdentifierError::AllZero
            } else {
                SubscriptionIdentifierError::InvalidEncoding
            })
        );
    }
}

#[test]
fn subscription_state_codes_are_closed() {
    for (status, code) in [
        (SubscriptionPlanStatus::Active, 1),
        (SubscriptionPlanStatus::Disabled, 2),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(SubscriptionPlanStatus::try_from(code), Ok(status));
    }
    assert_eq!(
        SubscriptionPlanStatus::try_from(0),
        Err(SubscriptionStateCodeError::InvalidPlanStatus)
    );

    for (status, code) in [
        (UserSubscriptionStatus::Active, 1),
        (UserSubscriptionStatus::Suspended, 2),
        (UserSubscriptionStatus::Canceled, 3),
        (UserSubscriptionStatus::Expired, 4),
    ] {
        assert_eq!(status.code(), code);
        assert_eq!(UserSubscriptionStatus::try_from(code), Ok(status));
    }
    assert_eq!(
        UserSubscriptionStatus::try_from(5),
        Err(SubscriptionStateCodeError::InvalidSubscriptionStatus)
    );

    for (cycle, code) in [
        (SubscriptionCycle::Daily, 1),
        (SubscriptionCycle::Weekly, 2),
        (SubscriptionCycle::Monthly, 3),
        (SubscriptionCycle::Yearly, 4),
    ] {
        assert_eq!(cycle.code(), code);
        assert_eq!(SubscriptionCycle::try_from(code), Ok(cycle));
    }
    assert_eq!(
        SubscriptionCycle::try_from(0),
        Err(SubscriptionStateCodeError::InvalidCycle)
    );
}
