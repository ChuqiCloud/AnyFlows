use std::thread;

use af_domain::Quota;
use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
use rust_decimal::Decimal;

use crate::{
    BillingExpression, BillingExpressionDefinition, BillingExpressionError, BillingExpressionUsage,
    BillingExpressionUsageSemantics, ExpressionVariable, ExpressionVersion,
    MAX_BILLING_EXPRESSION_BYTES, PricingRatio, PricingRatios,
};

fn count(tokens: i64) -> TokenCount {
    TokenCount::new(tokens).unwrap()
}

fn usage(
    input: i64,
    output: i64,
    cache_read: i64,
    cache_creation_5m: i64,
    cache_creation_1h: i64,
    semantics: UsageSemantics,
) -> Usage {
    Usage::new(
        count(input),
        count(output),
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

fn unit_ratios() -> PricingRatios {
    PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE)
}

#[test]
fn expression_uses_decimal_math_and_only_subtracts_referenced_inclusive_details() {
    let expression = BillingExpression::compile(
        r#"tier("base", p * 2.5 + c * 10 + cr * 0.5)"#,
        unit_ratios(),
    )
    .unwrap();
    let result = expression
        .evaluate(&usage(1_000, 500, 200, 100, 50, UsageSemantics::Inclusive))
        .unwrap();

    assert_eq!(result.total_usd(), Decimal::new(71, 4));
    assert_eq!(result.base_usd(), Decimal::new(71, 4));
    assert_eq!(result.quota(), Quota::new(3_550).unwrap());
    assert_eq!(result.matched_tier(), "base");
    let variables = result.variables();
    assert_eq!(variables.input_tokens(), 800);
    assert_eq!(variables.output_tokens(), 500);
    assert_eq!(variables.cache_read_tokens(), 200);
    assert_eq!(variables.cache_creation_5m_tokens(), 100);
    assert_eq!(variables.cache_creation_1h_tokens(), 50);
    assert_eq!(variables.context_length_tokens(), 1_000);
    assert!(expression.variables().uses(ExpressionVariable::Input));
    assert!(expression.variables().uses(ExpressionVariable::Output));
    assert!(expression.variables().uses(ExpressionVariable::CacheRead));
    assert!(
        !expression
            .variables()
            .uses(ExpressionVariable::CacheCreation5m)
    );
}

#[test]
fn cache_separated_context_length_includes_cache_without_reducing_input() {
    let expression = BillingExpression::compile(
        r#"if len >= 200 { tier("long", p) } else { tier("short", p) }"#,
        unit_ratios(),
    )
    .unwrap();
    let result = expression
        .evaluate(&usage(100, 0, 50, 20, 30, UsageSemantics::CacheSeparated))
        .unwrap();

    assert_eq!(result.matched_tier(), "long");
    assert_eq!(result.total_usd(), Decimal::new(1, 4));
    assert_eq!(result.quota(), Quota::new(50).unwrap());
    assert_eq!(result.variables().input_tokens(), 100);
    assert_eq!(result.variables().context_length_tokens(), 200);
}

#[test]
fn prepared_usage_keeps_protocol_construction_inside_billing_boundary() {
    let prepared = BillingExpressionUsage::new(
        100,
        20,
        50,
        20,
        30,
        BillingExpressionUsageSemantics::CacheSeparated,
    )
    .unwrap();
    assert_eq!(prepared.context_length_tokens(), 200);
    let expression = BillingExpression::compile(r#"tier("base", len)"#, unit_ratios()).unwrap();
    let result = expression.evaluate_prepared_usage(&prepared).unwrap();
    assert_eq!(result.variables().input_tokens(), 100);
    assert_eq!(result.variables().context_length_tokens(), 200);

    assert!(
        BillingExpressionUsage::new(10, 0, 11, 0, 0, BillingExpressionUsageSemantics::Inclusive,)
            .is_err()
    );
}

#[test]
fn preview_result_keeps_base_and_ratio_adjusted_costs_from_one_execution() {
    let expression = BillingExpression::compile(
        r#"tier("base", p * 2)"#,
        PricingRatios::new(
            PricingRatio::new(1_500_000).unwrap(),
            PricingRatio::new(800_000).unwrap(),
            PricingRatio::new(2_000_000).unwrap(),
        ),
    )
    .unwrap();
    let result = expression
        .evaluate(&usage(1_000, 0, 0, 0, 0, UsageSemantics::Inclusive))
        .unwrap();

    assert_eq!(result.base_usd(), Decimal::new(2, 3));
    assert_eq!(result.total_usd(), Decimal::new(48, 4));
    assert_eq!(result.quota(), Quota::new(2_400).unwrap());
}

#[test]
fn explicit_and_implicit_v1_are_compatible() {
    let implicit = BillingExpression::compile(r#"tier("base", p)"#, unit_ratios()).unwrap();
    let explicit = BillingExpression::compile(r#"v1:tier("base", p)"#, unit_ratios()).unwrap();
    let usage = usage(10, 0, 0, 0, 0, UsageSemantics::Inclusive);

    assert_eq!(implicit.version(), ExpressionVersion::V1);
    assert_eq!(explicit.version(), ExpressionVersion::V1);
    assert_eq!(
        implicit.evaluate(&usage).unwrap(),
        explicit.evaluate(&usage).unwrap()
    );
    assert_eq!(
        BillingExpression::compile(r#"v2:tier("base", p)"#, unit_ratios()).unwrap_err(),
        BillingExpressionError::UnsupportedVersion
    );
}

#[test]
fn versioned_definition_validates_once_and_compiles_with_frozen_ratios() {
    let definition = BillingExpressionDefinition::new("v1:tier(\"base\", p)".to_owned()).unwrap();
    let result = definition
        .compile(unit_ratios())
        .unwrap()
        .evaluate(&usage(1_000, 0, 0, 0, 0, UsageSemantics::Inclusive))
        .unwrap();

    assert_eq!(definition.version(), ExpressionVersion::V1);
    assert_eq!(result.quota(), Quota::new(500).unwrap());
    assert!(!format!("{definition:?}").contains("tier"));
}

#[test]
fn integer_tier_value_is_promoted_to_decimal() {
    let expression = BillingExpression::compile(r#"tier("free", 0)"#, unit_ratios()).unwrap();
    let result = expression
        .evaluate(&usage(10, 0, 0, 0, 0, UsageSemantics::Inclusive))
        .unwrap();

    assert_eq!(result.total_usd(), Decimal::ZERO);
    assert_eq!(result.quota(), Quota::ZERO);
    assert_eq!(result.matched_tier(), "free");
}

#[test]
fn compile_rejects_unknown_variables_functions_and_missing_tier() {
    assert_eq!(
        BillingExpression::compile(r#"tier("base", unknown)"#, unit_ratios()).unwrap_err(),
        BillingExpressionError::Compile
    );
    assert_eq!(
        BillingExpression::compile(r#"tier("base", abs(p))"#, unit_ratios()).unwrap_err(),
        BillingExpressionError::UnsupportedFeature
    );
    assert_eq!(
        BillingExpression::compile("p * 2", unit_ratios()).unwrap_err(),
        BillingExpressionError::MissingTier
    );
}

#[test]
fn compile_rejects_empty_and_oversized_sources_without_echoing_them() {
    assert_eq!(
        BillingExpression::compile("  ", unit_ratios()).unwrap_err(),
        BillingExpressionError::Empty
    );
    let source = "x".repeat(MAX_BILLING_EXPRESSION_BYTES + 1);
    let error = BillingExpression::compile(&source, unit_ratios()).unwrap_err();
    assert_eq!(error, BillingExpressionError::SourceTooLong);
    assert!(!error.to_string().contains(&source));
}

#[test]
fn sandbox_rejects_stateful_or_unbounded_constructs() {
    for source in [
        r#"{ let price = p; tier("base", price) }"#,
        r#"{ loop { } tier("base", p) }"#,
        r#"eval("tier(\"base\", p)")"#,
    ] {
        assert!(BillingExpression::compile(source, unit_ratios()).is_err());
    }
}

#[test]
fn evaluation_rejects_negative_cost_and_multiple_tiers() {
    let negative = BillingExpression::compile(r#"tier("base", -p)"#, unit_ratios()).unwrap();
    assert_eq!(
        negative
            .evaluate(&usage(1, 0, 0, 0, 0, UsageSemantics::Inclusive))
            .unwrap_err(),
        BillingExpressionError::NegativeResult
    );

    let multiple =
        BillingExpression::compile(r#"tier("first", p) + tier("second", c)"#, unit_ratios())
            .unwrap();
    assert_eq!(
        multiple
            .evaluate(&usage(1, 1, 0, 0, 0, UsageSemantics::Inclusive))
            .unwrap_err(),
        BillingExpressionError::Evaluation
    );

    let invalid_name = BillingExpression::compile(r#"tier("", p)"#, unit_ratios()).unwrap();
    assert_eq!(
        invalid_name
            .evaluate(&usage(1, 0, 0, 0, 0, UsageSemantics::Inclusive))
            .unwrap_err(),
        BillingExpressionError::Evaluation
    );
}

#[test]
fn errors_and_debug_output_do_not_include_expression_source() {
    let secret = "do_not_log_this_expression";
    let error = BillingExpression::compile(&format!(r#"tier("base", {secret})"#), unit_ratios())
        .unwrap_err();
    assert!(!format!("{error:?} {error}").contains(secret));

    let expression =
        BillingExpression::compile(r#"tier("hidden-source", p)"#, unit_ratios()).unwrap();
    assert!(!format!("{expression:?}").contains("hidden-source"));
}

#[test]
fn concurrent_evaluations_keep_tier_capture_isolated() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BillingExpression>();

    let expression = BillingExpression::compile(
        r#"if len >= 100 { tier("large", p) } else { tier("small", p) }"#,
        unit_ratios(),
    )
    .unwrap();
    let large = expression.clone();
    let small = expression;
    let large = thread::spawn(move || {
        large
            .evaluate(&usage(100, 0, 0, 0, 0, UsageSemantics::Inclusive))
            .unwrap()
            .matched_tier()
            .to_owned()
    });
    let small = thread::spawn(move || {
        small
            .evaluate(&usage(10, 0, 0, 0, 0, UsageSemantics::Inclusive))
            .unwrap()
            .matched_tier()
            .to_owned()
    });

    assert_eq!(large.join().unwrap(), "large");
    assert_eq!(small.join().unwrap(), "small");
}
