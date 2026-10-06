use std::{collections::HashSet, error::Error as _};

use crate::{Quota, QuotaDelta, QuotaError};

#[test]
fn quota_accepts_only_non_negative_i64_values() {
    assert_eq!(Quota::ZERO, Quota::new(0).unwrap());
    assert!(Quota::ZERO.is_zero());

    for units in [0, 1, i64::MAX] {
        let quota = Quota::new(units).unwrap();
        assert_eq!(quota.units(), units);
        assert_eq!(Quota::try_from(units), Ok(quota));
        assert_eq!(quota.is_zero(), units == 0);
        assert_eq!(format!("{quota:?}"), "Quota(<redacted>)");
    }

    for units in [i64::MIN, -3_131_313_131_313_131, -1] {
        let error = Quota::new(units).unwrap_err();
        let rendered = format!("{error:?}\n{error}");
        assert_eq!(error, QuotaError::Negative);
        assert_eq!(Quota::try_from(units), Err(QuotaError::Negative));
        assert!(!rendered.contains(&units.to_string()));
    }
}

#[test]
fn quota_checked_arithmetic_never_wraps_or_becomes_negative() {
    let two = Quota::new(2).unwrap();
    let three = Quota::new(3).unwrap();
    let five = Quota::new(5).unwrap();
    let maximum = Quota::new(i64::MAX).unwrap();

    assert_eq!(maximum.checked_add(Quota::ZERO), Ok(maximum));
    assert_eq!(two.checked_add(three), Ok(five));
    assert_eq!(five.checked_sub(three), Ok(two));
    assert_eq!(
        Quota::new(1).unwrap().checked_sub(Quota::new(1).unwrap()),
        Ok(Quota::ZERO)
    );
    assert_eq!(maximum.checked_sub(maximum), Ok(Quota::ZERO));
    assert_eq!(
        maximum.checked_add(Quota::new(1).unwrap()),
        Err(QuotaError::Overflow)
    );
    assert_eq!(
        Quota::ZERO.checked_sub(Quota::new(1).unwrap()),
        Err(QuotaError::Underflow)
    );

    assert_eq!(two.checked_apply(QuotaDelta::new(3).unwrap()), Ok(five));
    assert_eq!(five.checked_apply(QuotaDelta::new(-3).unwrap()), Ok(two));
    assert_eq!(two.checked_apply(QuotaDelta::ZERO), Ok(two));
    assert_eq!(
        maximum.checked_apply(QuotaDelta::new(1).unwrap()),
        Err(QuotaError::Overflow)
    );

    // Copy 值对象在失败后保持原值，调用方不会观察到部分更新。
    assert_eq!(maximum.units(), i64::MAX);
    assert_eq!(Quota::ZERO.units(), 0);
}

#[test]
fn quota_delta_preserves_signed_settlement_differences() {
    let zero = Quota::ZERO;
    let five = Quota::new(5).unwrap();
    let seven = Quota::new(7).unwrap();
    let maximum = Quota::new(i64::MAX).unwrap();

    for (actual, baseline, expected) in [
        (seven, five, 2),
        (five, seven, -2),
        (five, five, 0),
        (maximum, zero, i64::MAX),
        (zero, maximum, -i64::MAX),
    ] {
        let delta = actual.delta_from(baseline);
        assert_eq!(delta.units(), expected);
        assert_eq!(delta.is_zero(), expected == 0);
        assert_eq!(delta.is_positive(), expected > 0);
        assert_eq!(delta.is_negative(), expected < 0);
        assert_eq!(format!("{delta:?}"), "QuotaDelta(<redacted>)");
        assert_eq!(baseline.checked_apply(delta), Ok(actual));
    }

    for units in [-i64::MAX, 0, i64::MAX] {
        let delta = QuotaDelta::new(units).unwrap();
        assert_eq!(delta.units(), units);
        assert_eq!(format!("{delta:?}"), "QuotaDelta(<redacted>)");
    }

    let error = QuotaDelta::new(i64::MIN).unwrap_err();
    let rendered = format!("{error:?}\n{error}");
    assert_eq!(error, QuotaError::InvalidDelta);
    assert!(!rendered.contains(&i64::MIN.to_string()));
}

#[test]
fn quota_types_and_errors_keep_their_static_contracts() {
    fn assert_value<T: Copy + Eq + Ord + std::hash::Hash + Send + Sync + 'static>() {}
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}

    assert_value::<Quota>();
    assert_value::<QuotaDelta>();
    assert_error::<QuotaError>();

    let mut values = HashSet::new();
    assert!(values.insert(Quota::ZERO));
    assert!(!values.insert(Quota::ZERO));

    for (error, display) in [
        (QuotaError::Negative, "额度不能为负数"),
        (QuotaError::Overflow, "额度运算溢出"),
        (QuotaError::Underflow, "额度扣减结果不能为负数"),
        (QuotaError::InvalidDelta, "额度调整量超出有效范围"),
    ] {
        assert_eq!(error.to_string(), display);
        assert!(error.source().is_none());
    }
}
