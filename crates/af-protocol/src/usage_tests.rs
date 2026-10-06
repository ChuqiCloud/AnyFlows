use std::error::Error as _;

use crate::{TokenCount, Usage, UsageDetails, UsageError, UsageSemantics, UsageSource};

fn count(tokens: i64) -> TokenCount {
    TokenCount::new(tokens).unwrap()
}

fn details(
    cache_read: i64,
    cache_creation_5m: i64,
    cache_creation_1h: i64,
    reasoning: i64,
    audio_input: i64,
    audio_output: i64,
) -> UsageDetails {
    UsageDetails::new(
        count(cache_read),
        count(cache_creation_5m),
        count(cache_creation_1h),
        count(reasoning),
        count(audio_input),
        count(audio_output),
    )
}

#[test]
fn token_count_accepts_zero_and_i64_max_but_rejects_negative_values() {
    assert_eq!(TokenCount::new(0), Ok(TokenCount::ZERO));
    assert_eq!(TokenCount::try_from(0), Ok(TokenCount::ZERO));
    assert_eq!(TokenCount::ZERO.get(), 0);

    let maximum = TokenCount::new(i64::MAX).unwrap();
    assert_eq!(maximum.get(), i64::MAX);
    assert_eq!(TokenCount::try_from(i64::MAX), Ok(maximum));

    for tokens in [i64::MIN, -1] {
        let error = TokenCount::new(tokens).unwrap_err();
        let rendered = format!("{error:?}\n{error}");
        assert_eq!(error, UsageError::NegativeTokenCount);
        assert_eq!(TokenCount::try_from(tokens), Err(error));
        assert!(!rendered.contains(&tokens.to_string()));
    }
}

#[test]
fn usage_applies_inclusive_and_cache_separated_semantics() {
    let usage_details = details(10, 5, 3, 7, 2, 3);

    let inclusive = Usage::new(
        count(20),
        count(10),
        usage_details,
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    assert_eq!(inclusive.checked_input_tokens(), Ok(count(20)));
    assert_eq!(inclusive.checked_total_tokens(), Ok(count(30)));

    let separated = Usage::new(
        count(20),
        count(10),
        usage_details,
        UsageSource::Estimated,
        UsageSemantics::CacheSeparated,
    )
    .unwrap();
    assert_eq!(separated.checked_input_tokens(), Ok(count(38)));
    assert_eq!(separated.checked_total_tokens(), Ok(count(48)));

    assert_eq!(separated.input_tokens(), count(20));
    assert_eq!(separated.output_tokens(), count(10));
    assert_eq!(separated.details(), &usage_details);
    assert_eq!(separated.details().cache_read(), count(10));
    assert_eq!(separated.details().cache_creation_5m(), count(5));
    assert_eq!(separated.details().cache_creation_1h(), count(3));
    assert_eq!(separated.details().reasoning(), count(7));
    assert_eq!(separated.details().audio_input(), count(2));
    assert_eq!(separated.details().audio_output(), count(3));
    assert_eq!(separated.source(), UsageSource::Estimated);
    assert_eq!(separated.semantics(), UsageSemantics::CacheSeparated);
}

#[test]
fn usage_rejects_input_and_output_details_outside_their_totals() {
    assert_eq!(
        Usage::new(
            count(9),
            count(10),
            details(3, 2, 1, 0, 4, 0),
            UsageSource::Upstream,
            UsageSemantics::Inclusive,
        ),
        Err(UsageError::InputDetailsExceedTotal)
    );

    assert_eq!(
        Usage::new(
            count(3),
            count(10),
            details(100, 100, 100, 0, 4, 0),
            UsageSource::Upstream,
            UsageSemantics::CacheSeparated,
        ),
        Err(UsageError::InputDetailsExceedTotal)
    );

    for semantics in [UsageSemantics::Inclusive, UsageSemantics::CacheSeparated] {
        assert_eq!(
            Usage::new(
                count(10),
                count(9),
                details(0, 0, 0, 6, 0, 4),
                UsageSource::Upstream,
                semantics,
            ),
            Err(UsageError::OutputDetailsExceedTotal)
        );
    }
}

#[test]
fn usage_reports_every_accumulation_overflow() {
    let separated_input_overflow = Usage::new(
        count(i64::MAX),
        TokenCount::ZERO,
        details(1, 0, 0, 0, 0, 0),
        UsageSource::Upstream,
        UsageSemantics::CacheSeparated,
    )
    .unwrap();
    assert_eq!(
        separated_input_overflow.checked_input_tokens(),
        Err(UsageError::Overflow)
    );
    assert_eq!(
        separated_input_overflow.checked_total_tokens(),
        Err(UsageError::Overflow)
    );

    let inclusive_total_overflow = Usage::new(
        count(i64::MAX),
        count(1),
        details(0, 0, 0, 0, 0, 0),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    assert_eq!(
        inclusive_total_overflow.checked_total_tokens(),
        Err(UsageError::Overflow)
    );

    assert_eq!(
        Usage::new(
            count(i64::MAX),
            TokenCount::ZERO,
            details(i64::MAX, 1, 0, 0, 0, 0),
            UsageSource::Upstream,
            UsageSemantics::Inclusive,
        ),
        Err(UsageError::Overflow)
    );
    assert_eq!(
        Usage::new(
            TokenCount::ZERO,
            count(i64::MAX),
            details(0, 0, 0, i64::MAX, 0, 1),
            UsageSource::Upstream,
            UsageSemantics::CacheSeparated,
        ),
        Err(UsageError::Overflow)
    );
}

#[test]
fn absent_usage_is_distinct_from_an_explicit_zero_usage() {
    let absent: Option<Usage> = None;
    let zero_usage = Usage::new(
        TokenCount::ZERO,
        TokenCount::ZERO,
        details(0, 0, 0, 0, 0, 0),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    let zero = Some(zero_usage);

    assert_ne!(absent, zero);
    assert_eq!(zero_usage.checked_total_tokens(), Ok(TokenCount::ZERO));
}

#[test]
fn usage_debug_contains_only_structured_usage_fields() {
    let external_content = "never-log-model-or-prompt-content";
    let usage = Usage::new(
        count(12),
        count(5),
        details(1, 0, 0, 2, 0, 1),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    let rendered = format!("{usage:?}");

    assert!(rendered.starts_with("Usage {"));
    assert!(rendered.contains("input_tokens"));
    assert!(rendered.contains("UsageDetails"));
    assert!(!rendered.contains(external_content));
}

#[test]
fn usage_errors_have_fixed_chinese_diagnostics_without_sources() {
    for (error, display) in [
        (UsageError::NegativeTokenCount, "令牌数不能为负数"),
        (
            UsageError::InputDetailsExceedTotal,
            "输入令牌明细不能超过输入总量",
        ),
        (
            UsageError::OutputDetailsExceedTotal,
            "输出令牌明细不能超过输出总量",
        ),
        (UsageError::Overflow, "令牌数累加溢出"),
    ] {
        assert_eq!(error.to_string(), display);
        assert!(error.source().is_none());
    }
}
