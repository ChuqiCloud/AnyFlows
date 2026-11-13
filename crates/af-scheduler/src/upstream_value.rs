use af_db::UpstreamRoutingSignal;

use crate::SchedulerCandidate;

const SCALE: u64 = 1_000_000;
const MIN_MULTIPLIER: u64 = SCALE / 2;

/// Compare only explicit upstream values for one model, accounting unit, group and priority.
/// Unknown values, singleton samples and equal samples are neutral, so adding a legacy
/// channel never makes up a price or reads a user's wallet as an upstream balance.
pub(crate) fn value_multiplier(candidate: SchedulerCandidate, tier: &[SchedulerCandidate]) -> u64 {
    let Some(signal) = candidate.upstream_value() else {
        return SCALE;
    };
    let cost = metric_multiplier(candidate, signal, tier, UpstreamRoutingSignal::cost, false);
    let balance = metric_multiplier(
        candidate,
        signal,
        tier,
        UpstreamRoutingSignal::balance,
        true,
    );
    let usage = metric_multiplier(candidate, signal, tier, UpstreamRoutingSignal::usage, false);
    (cost * 4 + balance * 3 + usage * 3) / 10
}

fn metric_multiplier(
    candidate: SchedulerCandidate,
    signal: UpstreamRoutingSignal,
    tier: &[SchedulerCandidate],
    metric: fn(UpstreamRoutingSignal) -> Option<([u8; 32], u64)>,
    higher_is_better: bool,
) -> u64 {
    let Some((key, value)) = metric(signal) else {
        return SCALE;
    };
    let mut min = u64::MAX;
    let mut max = 0;
    let mut count = 0;
    for peer in tier {
        if peer.group_id() != candidate.group_id() || peer.priority() != candidate.priority() {
            continue;
        }
        let Some(peer_signal) = peer.upstream_value() else {
            continue;
        };
        let Some((peer_key, value)) = metric(peer_signal) else {
            continue;
        };
        if peer_key != key {
            continue;
        }
        min = min.min(value);
        max = max.max(value);
        count += 1;
    }
    if count < 2 || min == max {
        return SCALE;
    }
    let distance = if higher_is_better {
        value - min
    } else {
        max - value
    };
    let scaled = u128::from(distance) * u128::from(SCALE) / u128::from(max - min);
    MIN_MULTIPLIER + u64::try_from(scaled).expect("normalized upstream value is bounded by SCALE")
}

#[cfg(test)]
mod tests {
    use af_db::UpstreamRoutingSettings;
    use af_domain::{ChannelId, GroupId};
    use serde_json::json;

    use super::*;

    fn candidate(
        id: i64,
        unit: &str,
        model: &str,
        cost: Option<u64>,
        balance: Option<u64>,
        usage: Option<u64>,
    ) -> SchedulerCandidate {
        let mut entry = json!({});
        if let Some(value) = cost {
            entry["unit_cost"] = json!({"value_micros": value, "unit": unit});
        }
        if let Some(value) = balance {
            entry["available_balance"] = json!({"value_micros": value, "unit": "usd"});
        }
        if let Some(value) = usage {
            entry["recent_usage"] = json!({"requests": value, "window_seconds": 300});
        }
        let settings = UpstreamRoutingSettings::from_channel_settings(
            &json!({"upstream_routing": {"models": {(model): entry}}}),
        )
        .unwrap();
        SchedulerCandidate::new(GroupId::new(1).unwrap(), ChannelId::new(id).unwrap(), 9, 90)
            .with_upstream_value(settings.for_model(model))
    }

    #[test]
    fn explicit_comparable_values_use_cost_balance_and_usage_without_floats() {
        let best = candidate(1, "usd_per_1m_tokens", "model", Some(1), Some(100), Some(0));
        let worst = candidate(2, "usd_per_1m_tokens", "model", Some(9), Some(1), Some(100));
        assert_eq!(value_multiplier(best, &[best, worst]), 1_500_000);
        assert_eq!(value_multiplier(worst, &[best, worst]), 500_000);
    }

    #[test]
    fn missing_equal_singleton_and_incomparable_values_are_neutral() {
        let only_cost = candidate(1, "usd_per_1m_tokens", "model", Some(1), None, None);
        let high_cost = candidate(2, "usd_per_1m_tokens", "model", Some(9), None, None);
        assert_eq!(
            value_multiplier(only_cost, &[only_cost, high_cost]),
            1_200_000
        );
        assert_eq!(
            value_multiplier(high_cost, &[only_cost, high_cost]),
            800_000
        );
        let missing = candidate(3, "usd_per_1m_tokens", "model", None, None, None);
        assert_eq!(
            value_multiplier(missing, &[only_cost, high_cost, missing]),
            SCALE
        );
        assert_eq!(value_multiplier(only_cost, &[only_cost, missing]), SCALE);
        assert_eq!(value_multiplier(only_cost, &[only_cost, only_cost]), SCALE);
        for peer in [
            candidate(4, "cny_per_1m_tokens", "model", Some(9), None, None),
            candidate(5, "usd_per_1m_tokens", "other-model", Some(9), None, None),
            SchedulerCandidate::new(GroupId::new(2).unwrap(), ChannelId::new(6).unwrap(), 9, 90)
                .with_upstream_value(high_cost.upstream_value()),
            SchedulerCandidate::new(GroupId::new(1).unwrap(), ChannelId::new(7).unwrap(), 8, 90)
                .with_upstream_value(high_cost.upstream_value()),
        ] {
            assert_eq!(value_multiplier(only_cost, &[only_cost, peer]), SCALE);
        }
    }

    #[test]
    fn extreme_valid_integers_keep_the_fixed_point_multiplier_bounded() {
        let maximum = 1_000_000_000_000_000;
        let low = candidate(1, "usd_per_1m_tokens", "model", Some(0), Some(0), Some(0));
        let high = candidate(
            2,
            "usd_per_1m_tokens",
            "model",
            Some(maximum),
            Some(maximum),
            Some(maximum),
        );
        assert_eq!(value_multiplier(low, &[low, high]), 1_200_000);
        assert_eq!(value_multiplier(high, &[low, high]), 800_000);
    }

    #[test]
    fn usage_windows_and_balance_currencies_are_compared_independently() {
        let settings = |balance_unit, window_seconds| {
            UpstreamRoutingSettings::from_channel_settings(
                &json!({"upstream_routing": {"models": {"model": {
                    "available_balance": {"value_micros": 100, "unit": balance_unit},
                    "recent_usage": {"requests": 100, "window_seconds": window_seconds}
                }}}}),
            )
            .unwrap()
            .for_model("model")
        };
        let first = candidate(1, "usd_per_1m_tokens", "model", None, Some(10), Some(10));
        let second =
            SchedulerCandidate::new(GroupId::new(1).unwrap(), ChannelId::new(2).unwrap(), 9, 90)
                .with_upstream_value(settings("cny", 3600));
        assert_eq!(value_multiplier(first, &[first, second]), SCALE);
        let second = second.with_upstream_value(settings("usd", 3600));
        assert_eq!(value_multiplier(first, &[first, second]), 850_000);
        assert_eq!(value_multiplier(second, &[first, second]), 1_150_000);
    }
}
