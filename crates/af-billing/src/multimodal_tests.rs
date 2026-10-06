use af_domain::Quota;
use af_protocol::{ImageCount, ImageDimensions};

use super::{
    BillingDurationSeconds, BillingFactor, BillingImageCount, BillingPixelCount, BillingResolution,
    MAX_BILLING_DURATION_SECONDS, MAX_BILLING_IMAGE_COUNT, MAX_BILLING_RESOLUTION_EDGE,
    MAX_BILLING_RESOLUTION_PIXELS, MAX_MULTIMODAL_DIMENSIONS, MultimodalBillingDimensions,
    MultimodalBillingError, MultimodalDimensionError,
};

#[test]
fn dimensions_reject_negative_values_and_keep_fixed_upper_bounds() {
    assert_eq!(
        BillingImageCount::new(-1),
        Err(MultimodalDimensionError::Negative)
    );
    assert_eq!(
        BillingImageCount::new(MAX_BILLING_IMAGE_COUNT + 1),
        Err(MultimodalDimensionError::OutOfRange)
    );
    assert_eq!(BillingImageCount::new(0).unwrap().count(), 0);
    assert_eq!(
        BillingImageCount::from_u64(u64::MAX),
        Err(MultimodalDimensionError::OutOfRange)
    );

    assert_eq!(
        BillingDurationSeconds::new(-1),
        Err(MultimodalDimensionError::Negative)
    );
    assert_eq!(
        BillingDurationSeconds::new(MAX_BILLING_DURATION_SECONDS + 1),
        Err(MultimodalDimensionError::OutOfRange)
    );
    assert_eq!(BillingDurationSeconds::new(0).unwrap().seconds(), 0);
    assert_eq!(
        BillingDurationSeconds::from_u64(u64::MAX),
        Err(MultimodalDimensionError::OutOfRange)
    );

    assert_eq!(
        BillingPixelCount::new(-1),
        Err(MultimodalDimensionError::Negative)
    );
    assert_eq!(
        BillingPixelCount::new(MAX_BILLING_RESOLUTION_PIXELS + 1),
        Err(MultimodalDimensionError::OutOfRange)
    );
    assert_eq!(
        BillingPixelCount::from_u64(u64::MAX),
        Err(MultimodalDimensionError::OutOfRange)
    );
}

#[test]
fn resolution_uses_checked_pixel_calculation_and_rejects_zero_edges() {
    let resolution = BillingResolution::new(1_024, 1_024).unwrap();
    assert_eq!(resolution.pixels().pixels(), 1_048_576);
    assert_eq!(resolution.to_string(), "1024x1024");
    assert_eq!(
        BillingResolution::new(0, 1_024),
        Err(MultimodalDimensionError::Zero)
    );
    assert_eq!(
        BillingResolution::new(-1, 1_024),
        Err(MultimodalDimensionError::Negative)
    );
    assert_eq!(
        BillingResolution::new(MAX_BILLING_RESOLUTION_EDGE + 1, 1),
        Err(MultimodalDimensionError::OutOfRange)
    );
}

#[test]
fn protocol_image_values_are_reused_at_the_billing_boundary() {
    let count = ImageCount::new(3).unwrap();
    let dimensions = ImageDimensions::new(1_024, 1_024).unwrap();
    assert_eq!(BillingImageCount::from_protocol(count).unwrap().count(), 3);
    assert_eq!(
        BillingResolution::from_protocol(dimensions)
            .unwrap()
            .pixels()
            .pixels(),
        1_048_576
    );
}

#[test]
fn factor_combination_is_checked_and_has_a_dimension_cap() {
    let maximum = BillingFactor::new(i64::MAX).unwrap();
    assert_eq!(
        maximum.checked_mul(BillingFactor::new(2).unwrap()),
        Err(MultimodalBillingError::FactorOverflow)
    );
    let factors = [BillingFactor::new(2).unwrap(); MAX_MULTIMODAL_DIMENSIONS + 1];
    assert_eq!(
        BillingFactor::checked_product(&factors),
        Err(MultimodalBillingError::TooManyDimensions)
    );
    assert_eq!(
        BillingFactor::checked_product(&[]).unwrap().get(),
        BillingFactor::ONE.get()
    );
}

#[test]
fn dimension_set_multiplies_only_explicit_values_with_checked_math() {
    let resolution = BillingResolution::new(1_024, 1_024).unwrap();
    let dimensions = MultimodalBillingDimensions::with_resolution(
        Some(BillingImageCount::new(2).unwrap()),
        Some(BillingDurationSeconds::new(3).unwrap()),
        Some(resolution),
    );
    assert_eq!(dimensions.checked_factor().unwrap().get(), 6_291_456);
    assert_eq!(
        MultimodalBillingDimensions::empty()
            .checked_factor()
            .unwrap()
            .get(),
        1
    );
}

#[test]
fn applying_factor_reuses_quota_math_overflow_boundary() {
    let base = Quota::new(2).unwrap();
    let result = BillingFactor::new(i64::MAX)
        .unwrap()
        .apply_to_quota(base)
        .unwrap_err();
    assert!(matches!(
        result,
        MultimodalBillingError::Quota(crate::quota_math::QuotaMathError::Clamp(_))
    ));

    assert_eq!(
        BillingFactor::new(0).unwrap().apply_to_quota(base),
        Ok(Quota::ZERO)
    );
}
