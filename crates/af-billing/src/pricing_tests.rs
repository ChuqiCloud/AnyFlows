use af_domain::{Quota, QuotaError};
use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
use rust_decimal::Decimal;

use crate::{
    BillingMode, PricingContext, PricingError, PricingRatio, PricingRatios, PricingResolver,
    RatioPricingResolver, TokenPrices,
    quota_math::{QuotaClampKind, QuotaMathOperation},
};

fn count(tokens: i64) -> TokenCount {
    TokenCount::new(tokens).unwrap()
}

fn usage(
    input_tokens: i64,
    output_tokens: i64,
    cache_read: i64,
    cache_creation_5m: i64,
    cache_creation_1h: i64,
    semantics: UsageSemantics,
) -> Usage {
    Usage::new(
        count(input_tokens),
        count(output_tokens),
        UsageDetails::new(
            count(cache_read),
            count(cache_creation_5m),
            count(cache_creation_1h),
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        semantics,
    )
    .unwrap()
}

fn prices(values: [Decimal; 5]) -> TokenPrices {
    TokenPrices::new(values[0], values[1], values[2], values[3], values[4]).unwrap()
}

fn ratio(micros: i64) -> PricingRatio {
    PricingRatio::new(micros).unwrap()
}

fn unit_ratios() -> PricingRatios {
    PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE)
}

#[test]
fn ratio_mode_prices_five_disjoint_token_buckets_and_three_ratios() {
    let resolver = RatioPricingResolver::metered(
        prices([
            Decimal::ONE,
            Decimal::from(2),
            Decimal::from(3),
            Decimal::from(4),
            Decimal::from(5),
        ]),
        PricingRatios::new(ratio(2_000_000), ratio(1_500_000), ratio(500_000)),
    );
    let inclusive = usage(
        13_000_000,
        2_000_000,
        3_000_000,
        4_000_000,
        5_000_000,
        UsageSemantics::Inclusive,
    );
    let separated = usage(
        1_000_000,
        2_000_000,
        3_000_000,
        4_000_000,
        5_000_000,
        UsageSemantics::CacheSeparated,
    );

    let inclusive_price = resolver.resolve(&PricingContext::new(&inclusive)).unwrap();
    let separated_price = resolver.resolve(&PricingContext::new(&separated)).unwrap();

    assert_eq!(inclusive_price, separated_price);
    assert_eq!(inclusive_price.quota(), Quota::new(41_250_000).unwrap());
    assert_eq!(inclusive_price.billing_mode(), BillingMode::PerToken);
    assert!(!inclusive_price.is_free());

    let breakdown = inclusive_price.breakdown();
    assert_eq!(breakdown.input_usd(), Decimal::new(15, 1));
    assert_eq!(breakdown.output_usd(), Decimal::from(6));
    assert_eq!(breakdown.cache_read_usd(), Decimal::new(135, 1));
    assert_eq!(breakdown.cache_creation_5m_usd(), Decimal::from(24));
    assert_eq!(breakdown.cache_creation_1h_usd(), Decimal::new(375, 1));
    assert_eq!(breakdown.total_usd(), Decimal::new(825, 1));
}

#[test]
fn catalog_prices_reuse_checked_three_ratio_formula_without_quota_rounding() {
    let resolver = RatioPricingResolver::metered(
        prices([
            Decimal::ONE,
            Decimal::from(2),
            Decimal::from(3),
            Decimal::from(4),
            Decimal::from(5),
        ]),
        PricingRatios::new(ratio(2_000_000), ratio(1_500_000), ratio(500_000)),
    );
    let effective = resolver.effective_token_prices().unwrap().unwrap();
    assert_eq!(
        effective.into_values(),
        [
            Decimal::new(15, 1),
            Decimal::from(3),
            Decimal::new(45, 1),
            Decimal::from(6),
            Decimal::new(75, 1),
        ]
    );

    let zero_ratio = RatioPricingResolver::metered(
        prices([Decimal::ONE; 5]),
        PricingRatios::new(PricingRatio::ZERO, PricingRatio::ONE, PricingRatio::ONE),
    );
    assert_eq!(
        zero_ratio
            .effective_token_prices()
            .unwrap()
            .unwrap()
            .into_values(),
        [Decimal::ZERO; 5]
    );
    assert!(
        RatioPricingResolver::free()
            .effective_token_prices()
            .unwrap()
            .is_none()
    );
}

#[test]
fn ratio_mode_rounds_only_after_summing_all_components() {
    let resolver = RatioPricingResolver::metered(
        prices([
            Decimal::ONE,
            Decimal::ONE,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ]),
        unit_ratios(),
    );
    let measured_usage = usage(1, 1, 0, 0, 0, UsageSemantics::Inclusive);

    let price = resolver
        .resolve(&PricingContext::new(&measured_usage))
        .unwrap();

    // 两个分项各自等于 0.5 quota；整笔只舍入一次，正确结果是 1 而不是 2。
    assert_eq!(price.breakdown().input_usd(), Decimal::new(1, 6));
    assert_eq!(price.breakdown().output_usd(), Decimal::new(1, 6));
    assert_eq!(price.breakdown().total_usd(), Decimal::new(2, 6));
    assert_eq!(price.quota(), Quota::new(1).unwrap());
}

#[test]
fn ratio_mode_preserves_tiny_prices_before_large_ratios_are_applied() {
    let resolver = RatioPricingResolver::metered(
        prices([
            Decimal::new(1, 28),
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ]),
        PricingRatios::new(ratio(i64::MAX), ratio(i64::MAX), ratio(i64::MAX)),
    );
    let measured_usage = usage(1, 0, 0, 0, 0, UsageSemantics::Inclusive);

    let price = resolver
        .resolve(&PricingContext::new(&measured_usage))
        .unwrap();

    // 先除以一百万会把极小单价截成零，必须保留到大倍率应用之后再做除法。
    assert_eq!(price.quota(), Quota::new(39_231_885_846).unwrap());
    assert!(price.breakdown().input_usd() > Decimal::ZERO);
}

#[test]
fn free_mode_is_explicit_and_distinct_from_zero_metered_cost() {
    let measured_usage = usage(7, 3, 0, 0, 0, UsageSemantics::Inclusive);
    let context = PricingContext::new(&measured_usage);
    let free = RatioPricingResolver::free().resolve(&context).unwrap();
    let zero_metered = RatioPricingResolver::metered(prices([Decimal::ZERO; 5]), unit_ratios())
        .resolve(&context)
        .unwrap();

    assert_eq!(free.quota(), Quota::ZERO);
    assert_eq!(free.billing_mode(), BillingMode::Free);
    assert!(free.is_free());
    assert_eq!(free.breakdown().total_usd(), Decimal::ZERO);

    assert_eq!(zero_metered.quota(), Quota::ZERO);
    assert_eq!(zero_metered.billing_mode(), BillingMode::PerToken);
    assert!(!zero_metered.is_free());
    assert_eq!(zero_metered.breakdown().total_usd(), Decimal::ZERO);
}

#[test]
fn pricing_configuration_rejects_every_negative_price_and_ratio() {
    for index in 0..5 {
        let mut values = [Decimal::ONE; 5];
        values[index] = Decimal::new(-1, 28);
        assert_eq!(
            TokenPrices::new(values[0], values[1], values[2], values[3], values[4]),
            Err(PricingError::InvalidPrice)
        );
    }

    for micros in [i64::MIN, -1] {
        assert_eq!(PricingRatio::new(micros), Err(PricingError::InvalidRatio));
    }
    assert_eq!(PricingRatio::new(0), Ok(PricingRatio::ZERO));
    assert_eq!(PricingRatio::new(1_000_000), Ok(PricingRatio::ONE));
}

#[test]
fn ratio_mode_reports_decimal_multiplier_and_final_quota_overflow() {
    let maximum_usage = usage(i64::MAX, 0, 0, 0, 0, UsageSemantics::Inclusive);
    let decimal_overflow = RatioPricingResolver::metered(
        prices([
            Decimal::MAX,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ]),
        unit_ratios(),
    )
    .resolve(&PricingContext::new(&maximum_usage))
    .unwrap_err();
    assert_pricing_clamp(decimal_overflow);

    let multiplier_overflow = RatioPricingResolver::metered(
        prices([
            Decimal::ONE,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ]),
        PricingRatios::new(ratio(i64::MAX), ratio(i64::MAX), ratio(i64::MAX)),
    )
    .resolve(&PricingContext::new(&usage(
        1,
        0,
        0,
        0,
        0,
        UsageSemantics::Inclusive,
    )))
    .unwrap_err();
    assert_pricing_clamp(multiplier_overflow);

    let quota_overflow = RatioPricingResolver::metered(
        prices([
            Decimal::from(20_000_000_000_000_i64),
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ]),
        unit_ratios(),
    )
    .resolve(&PricingContext::new(&usage(
        1_000_000,
        0,
        0,
        0,
        0,
        UsageSemantics::Inclusive,
    )))
    .unwrap_err();
    assert_pricing_clamp(quota_overflow);
}

#[test]
fn pricing_contract_is_object_safe_thread_safe_and_debug_redacted() {
    fn assert_send_sync_static<T: Send + Sync + 'static>() {}

    assert_send_sync_static::<RatioPricingResolver>();

    let configured_prices = prices([
        Decimal::from(987_654_321_i64),
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
        Decimal::ZERO,
    ]);
    let configured_ratios =
        PricingRatios::new(ratio(123_456_789), PricingRatio::ONE, PricingRatio::ONE);
    let resolver = RatioPricingResolver::metered(configured_prices, configured_ratios);
    let measured_usage = usage(777_777, 0, 0, 0, 0, UsageSemantics::Inclusive);
    let context = PricingContext::new(&measured_usage);
    let object: &dyn PricingResolver = &resolver;
    let price = object.resolve(&context).unwrap();

    let rendered = format!(
        "{configured_prices:?}\n{configured_ratios:?}\n{resolver:?}\n{context:?}\n{price:?}\n{:?}",
        price.breakdown()
    );
    for secret in ["987654321", "123456789", "777777"] {
        assert!(!rendered.contains(secret));
    }

    let error = PricingRatio::new(-987_654_321).unwrap_err();
    let rendered_error = format!("{error:?}\n{error}");
    assert!(!rendered_error.contains("987654321"));
}

fn assert_pricing_clamp(error: PricingError) {
    let PricingError::Math(error) = error else {
        panic!("定价溢出必须通过额度数学错误返回");
    };
    assert_eq!(error.quota_error(), QuotaError::Overflow);
    let clamp = error.clamp().expect("定价溢出必须携带结构化审计标记");
    assert_eq!(clamp.operation(), QuotaMathOperation::PricingResolution);
    assert_eq!(clamp.operation().as_str(), "pricing_resolution");
    assert_eq!(clamp.kind(), QuotaClampKind::Overflow);

    let rendered = format!("{error:?}\n{error}");
    assert!(!rendered.contains(&i64::MAX.to_string()));
}
