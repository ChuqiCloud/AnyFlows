use af_domain::Quota;
use af_protocol::{VideoDuration, VideoResolution};

use crate::{PricingRatio, PricingRatios, XaiVideoPricingError, XaiVideoPricingSnapshot};

#[test]
fn official_rate_card_and_group_ratios_are_applied_once() {
    let snapshot = XaiVideoPricingSnapshot::new(
        Some(VideoResolution::P720),
        PricingRatios::new(
            PricingRatio::new(1_500_000).unwrap(),
            PricingRatio::new(800_000).unwrap(),
            PricingRatio::new(1_250_000).unwrap(),
        ),
    );
    let rate = snapshot.rate_microusd("grok-imagine-video-1.5").unwrap();
    let quota = snapshot
        .actual_quota(rate, VideoDuration::new(8).unwrap())
        .unwrap();

    // $0.14/s * 8s * 1.5 * 0.8 * 1.25 * 500000 quota/USD。
    assert_eq!(quota, Quota::new(840_000).unwrap());
}

#[test]
fn missing_values_use_xai_defaults_but_upper_bound_always_covers_fifteen_seconds() {
    let snapshot = XaiVideoPricingSnapshot::new(
        None,
        PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE),
    );
    let rate = snapshot
        .rate_microusd("grok-imagine-video-1.5-preview")
        .unwrap();

    assert_eq!(snapshot.resolution(), VideoResolution::P480);
    assert_eq!(
        snapshot.fallback_quota(rate, None).unwrap(),
        Quota::new(320_000).unwrap()
    );
    assert_eq!(
        snapshot
            .upper_bound(["grok-imagine-video", "grok-imagine-video-1.5"])
            .unwrap(),
        Quota::new(600_000).unwrap()
    );
}

#[test]
fn unsupported_models_resolutions_and_corrupt_rates_fail_closed() {
    let ratios = PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE);
    let full_hd = XaiVideoPricingSnapshot::new(Some(VideoResolution::P1080), ratios);
    assert_eq!(
        full_hd.rate_microusd("grok-imagine-video"),
        Err(XaiVideoPricingError::UnsupportedResolution)
    );
    assert_eq!(
        full_hd.rate_microusd("private-video-model"),
        Err(XaiVideoPricingError::UnsupportedModel)
    );
    assert_eq!(
        full_hd.actual_quota(70_000, VideoDuration::new(8).unwrap()),
        Err(XaiVideoPricingError::InvalidSnapshot)
    );
}

#[test]
fn zero_effective_ratio_still_reserves_the_minimum_positive_quota() {
    let snapshot = XaiVideoPricingSnapshot::new(
        Some(VideoResolution::P480),
        PricingRatios::new(PricingRatio::ZERO, PricingRatio::ONE, PricingRatio::ONE),
    );
    assert_eq!(
        snapshot.upper_bound(["grok-imagine-video"]).unwrap(),
        Quota::new(1).unwrap()
    );
}
