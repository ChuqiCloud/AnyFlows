use af_domain::{Quota, QuotaError};
use rust_decimal::Decimal;

use crate::quota_math::{
    QUOTA_PER_USD, QuotaClampKind, QuotaMathError, QuotaMathOperation, checked_mul_quota,
    quota_from_tokens, quota_from_usd,
};

#[test]
fn usd_conversion_uses_fixed_units_and_explicit_rounding() {
    assert_eq!(quota_from_usd(Decimal::ZERO), Ok(Quota::ZERO));
    assert_eq!(
        quota_from_usd(Decimal::ONE),
        Ok(Quota::new(QUOTA_PER_USD).unwrap())
    );
    assert_eq!(
        quota_from_usd(Decimal::new(2, 6)),
        Ok(Quota::new(1).unwrap())
    );

    // 0.5 quota 按中点远离零进位，略低于中点时保持为零。
    assert_eq!(
        quota_from_usd(Decimal::new(1, 6)),
        Ok(Quota::new(1).unwrap())
    );
    assert_eq!(quota_from_usd(Decimal::new(999_999, 12)), Ok(Quota::ZERO));
}

#[test]
fn token_conversion_uses_usd_per_million_tokens() {
    assert_eq!(
        quota_from_tokens(1_000_000, Decimal::new(2, 0)),
        Ok(Quota::new(1_000_000).unwrap())
    );
    assert_eq!(
        quota_from_tokens(1, Decimal::ONE),
        Ok(Quota::new(1).unwrap())
    );
    assert_eq!(
        quota_from_tokens(1, Decimal::new(999_999, 6)),
        Ok(Quota::ZERO)
    );
    assert_eq!(
        quota_from_tokens(
            1,
            Decimal::from_str_exact("0.9999999999999999999999999998").unwrap(),
        ),
        Ok(Quota::ZERO)
    );
    assert_eq!(
        quota_from_tokens(i64::MAX, Decimal::new(2, 0)),
        Ok(Quota::new(i64::MAX).unwrap())
    );
}

#[test]
fn conversion_rejects_every_negative_input() {
    assert_eq!(
        quota_from_usd(Decimal::new(-1, 28)),
        Err(QuotaMathError::Quota(QuotaError::Negative))
    );
    assert_eq!(
        quota_from_tokens(-1, Decimal::ONE),
        Err(QuotaMathError::Quota(QuotaError::Negative))
    );
    assert_eq!(
        quota_from_tokens(1, Decimal::new(-1, 28)),
        Err(QuotaMathError::Quota(QuotaError::Negative))
    );
    assert_eq!(
        checked_mul_quota(Quota::new(1).unwrap(), -1),
        Err(QuotaMathError::Quota(QuotaError::Negative))
    );
}

#[test]
fn conversion_reports_decimal_and_integer_overflow() {
    assert_clamp(
        quota_from_usd(Decimal::from(i64::MAX)).unwrap_err(),
        QuotaMathOperation::UsdConversion,
    );
    assert_clamp(
        quota_from_usd(Decimal::MAX).unwrap_err(),
        QuotaMathOperation::UsdConversion,
    );
    assert_clamp(
        quota_from_tokens(i64::MAX, Decimal::new(2_000_001, 6)).unwrap_err(),
        QuotaMathOperation::TokenConversion,
    );
    assert_clamp(
        quota_from_tokens(i64::MAX, Decimal::MAX).unwrap_err(),
        QuotaMathOperation::TokenConversion,
    );
}

#[test]
fn quota_multiplier_preserves_boundaries_without_wrapping() {
    let maximum = Quota::new(i64::MAX).unwrap();
    let three = Quota::new(3).unwrap();

    assert_eq!(checked_mul_quota(Quota::ZERO, i64::MAX), Ok(Quota::ZERO));
    assert_eq!(checked_mul_quota(three, 0), Ok(Quota::ZERO));
    assert_eq!(checked_mul_quota(three, 2), Ok(Quota::new(6).unwrap()));
    assert_eq!(checked_mul_quota(maximum, 1), Ok(maximum));
    assert_clamp(
        checked_mul_quota(maximum, 2).unwrap_err(),
        QuotaMathOperation::Multiplication,
    );
}

fn assert_clamp(error: QuotaMathError, operation: QuotaMathOperation) {
    assert_eq!(error.quota_error(), QuotaError::Overflow);
    let clamp = error.clamp().expect("越界必须携带结构化审计标记");
    assert_eq!(clamp.operation(), operation);
    assert_eq!(clamp.operation().as_str(), operation.as_str());
    assert_eq!(clamp.kind(), QuotaClampKind::Overflow);
    assert_eq!(clamp.kind().as_str(), "overflow");

    let rendered = format!("{error:?}\n{error}");
    assert!(!rendered.contains(&i64::MAX.to_string()));
}
