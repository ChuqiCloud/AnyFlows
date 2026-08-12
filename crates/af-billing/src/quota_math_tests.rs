use af_domain::{Quota, QuotaError};
use rust_decimal::Decimal;

use crate::quota_math::{
    QUOTA_PER_USD, QuotaClampKind, QuotaFactorError, QuotaMathError, QuotaMathOperation,
    checked_mul_quota, checked_mul_quota_with_limit, quota_from_cny_minor, quota_from_tokens,
    quota_from_usd, quota_from_usd_with_ratios,
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
fn usd_ratio_conversion_preserves_the_dollar_scale() {
    assert_eq!(
        quota_from_usd_with_ratios(Decimal::ONE, [1_500_000_i64, 800_000_i64, 1_250_000_i64],),
        Ok(Quota::new(750_000).unwrap())
    );
}

#[test]
fn cny_minor_conversion_uses_checked_integer_rounding() {
    assert_eq!(quota_from_cny_minor(0, 500_000), Ok(Quota::ZERO));
    assert_eq!(
        quota_from_cny_minor(100, 500_000),
        Ok(Quota::new(500_000).unwrap())
    );
    assert_eq!(quota_from_cny_minor(1, 50), Ok(Quota::new(1).unwrap()));
    assert_eq!(quota_from_cny_minor(1, 49), Ok(Quota::ZERO));
    assert_eq!(
        quota_from_cny_minor(1234, 500_000),
        Ok(Quota::new(6_170_000).unwrap())
    );
}

#[test]
fn cny_minor_conversion_rejects_negative_rate_and_overflow() {
    assert_eq!(
        quota_from_cny_minor(100, -1),
        Err(QuotaMathError::Quota(QuotaError::Negative))
    );
    assert_clamp(
        quota_from_cny_minor(u64::MAX, i64::MAX).unwrap_err(),
        QuotaMathOperation::CnyMinorConversion,
    );
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

#[test]
fn user_controlled_factor_is_rejected_before_quota_multiplication() {
    let base = Quota::new(i64::MAX).unwrap();

    assert_eq!(
        checked_mul_quota_with_limit(base, 11, 10),
        Err(QuotaFactorError::FactorOutOfRange)
    );
    assert_eq!(
        checked_mul_quota_with_limit(base, -1, 10),
        Err(QuotaFactorError::FactorOutOfRange)
    );
    assert_eq!(
        checked_mul_quota_with_limit(base, 1, -1),
        Err(QuotaFactorError::InvalidLimit)
    );
    assert_eq!(
        checked_mul_quota_with_limit(Quota::new(7).unwrap(), 10, 10),
        Ok(Quota::new(70).unwrap())
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
