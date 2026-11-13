use std::{collections::BTreeMap, fmt};

use af_domain::MAX_MODEL_NAME_BYTES;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_MODELS: usize = 64;
const MAX_SIGNAL_VALUE: u64 = 1_000_000_000_000_000;

/// Explicit administrator-maintained upstream observations, independent of billing.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct UpstreamRoutingSettings {
    models: BTreeMap<String, ModelValueSettings>,
}

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct MonetaryValue {
    value_micros: u64,
    unit: String,
}

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct RecentUsage {
    requests: u64,
    window_seconds: u64,
}

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ModelValueSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unit_cost: Option<MonetaryValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    available_balance: Option<MonetaryValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recent_usage: Option<RecentUsage>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SettingsWire {
    models: BTreeMap<String, ModelValueSettings>,
}

/// One validated model-scoped upstream observation. Unit keys cannot be forged by callers.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct UpstreamRoutingSignal {
    cost: Option<([u8; 32], u64)>,
    balance: Option<([u8; 32], u64)>,
    usage: Option<([u8; 32], u64)>,
}

#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
#[error("invalid upstream routing settings")]
pub struct UpstreamRoutingSettingsError;

impl UpstreamRoutingSettings {
    /// Read optional channel settings; observations are supplied by the administrator.
    pub fn from_channel_settings(settings: &Value) -> Result<Self, UpstreamRoutingSettingsError> {
        let object = settings.as_object().ok_or(UpstreamRoutingSettingsError)?;
        object
            .get("upstream_routing")
            .map_or(Ok(Self::default()), Self::parse)
    }

    pub(crate) fn parse(value: &Value) -> Result<Self, UpstreamRoutingSettingsError> {
        if serde_json::to_vec(value)
            .map_err(|_| UpstreamRoutingSettingsError)?
            .len()
            > 64 * 1024
        {
            return Err(UpstreamRoutingSettingsError);
        }
        let wire: SettingsWire =
            serde_json::from_value(value.clone()).map_err(|_| UpstreamRoutingSettingsError)?;
        if wire.models.len() > MAX_MODELS
            || wire.models.iter().any(|(model, signal)| {
                model.is_empty()
                    || model.len() > MAX_MODEL_NAME_BYTES
                    || model.trim() != model
                    || model.chars().any(char::is_control)
                    || [&signal.unit_cost, &signal.available_balance]
                        .into_iter()
                        .flatten()
                        .any(|value| {
                            value.value_micros > MAX_SIGNAL_VALUE
                                || value.unit.is_empty()
                                || value.unit.len() > 64
                                || !value.unit.bytes().all(|byte| {
                                    byte.is_ascii_lowercase()
                                        || byte.is_ascii_digit()
                                        || byte == b'_'
                                })
                        })
                    || signal.recent_usage.as_ref().is_some_and(|usage| {
                        usage.requests > MAX_SIGNAL_VALUE
                            || !(1..=604_800).contains(&usage.window_seconds)
                    })
            })
        {
            return Err(UpstreamRoutingSettingsError);
        }
        Ok(Self {
            models: wire.models,
        })
    }

    /// Resolve the canonical model, before channel-specific upstream model mapping.
    #[must_use]
    pub fn for_model(&self, model: &str) -> Option<UpstreamRoutingSignal> {
        let signal = self.models.get(model)?;
        let key = |kind: &[u8], unit: &[u8]| {
            let mut hash = Sha256::new();
            hash.update(model.as_bytes());
            hash.update([0]);
            hash.update(kind);
            hash.update([0]);
            hash.update(unit);
            <[u8; 32]>::from(hash.finalize())
        };
        Some(UpstreamRoutingSignal {
            cost: signal
                .unit_cost
                .as_ref()
                .map(|value| (key(b"cost", value.unit.as_bytes()), value.value_micros)),
            balance: signal
                .available_balance
                .as_ref()
                .map(|value| (key(b"balance", value.unit.as_bytes()), value.value_micros)),
            usage: signal.recent_usage.as_ref().map(|value| {
                (
                    key(b"usage", &value.window_seconds.to_be_bytes()),
                    value.requests,
                )
            }),
        })
    }

    pub(crate) fn to_projection_json(&self) -> Option<Value> {
        (!self.models.is_empty()).then(|| serde_json::json!({ "models": self.models }))
    }
}

impl UpstreamRoutingSignal {
    #[must_use]
    pub const fn cost(self) -> Option<([u8; 32], u64)> {
        self.cost
    }
    #[must_use]
    pub const fn balance(self) -> Option<([u8; 32], u64)> {
        self.balance
    }
    #[must_use]
    pub const fn usage(self) -> Option<([u8; 32], u64)> {
        self.usage
    }
    #[must_use]
    pub fn unit_cost_micros(self) -> Option<u64> {
        self.cost.map(|(_, value)| value)
    }
    #[must_use]
    pub fn available_balance_micros(self) -> Option<u64> {
        self.balance.map(|(_, value)| value)
    }
    #[must_use]
    pub fn recent_requests(self) -> Option<u64> {
        self.usage.map(|(_, value)| value)
    }
}

impl fmt::Debug for UpstreamRoutingSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpstreamRoutingSettings")
            .field("model_count", &self.models.len())
            .finish()
    }
}

impl fmt::Debug for UpstreamRoutingSignal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UpstreamRoutingSignal(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_are_optional_and_projection_preserves_model_unit_scope() {
        assert_eq!(
            UpstreamRoutingSettings::from_channel_settings(&json!({})).unwrap(),
            UpstreamRoutingSettings::default()
        );
        let settings = UpstreamRoutingSettings::from_channel_settings(
            &json!({"upstream_routing": {"models": {
                "model-a": {"unit_cost": {"unit": "usd_per_1m_tokens", "value_micros": 0}},
                "model-b": {"unit_cost": {"unit": "usd_per_1m_tokens", "value_micros": 0}}
            }}}),
        )
        .unwrap();
        assert_ne!(
            settings.for_model("model-a").unwrap().cost(),
            settings.for_model("model-b").unwrap().cost()
        );
        assert!(settings.for_model("unknown").is_none());
        assert_eq!(
            UpstreamRoutingSettings::parse(&settings.to_projection_json().unwrap()).unwrap(),
            settings
        );
    }

    #[test]
    fn settings_reject_unknown_fields_floats_unbounded_values_and_invalid_windows() {
        for entry in [
            json!({"unit_cost": {"unit": "USD", "value_micros": 1}}),
            json!({"unit_cost": {"unit": "usd", "value_micros": 1.5}}),
            json!({"unit_cost": {"unit": "usd", "value_micros": -1}}),
            json!({"unit_cost": {"unit": "usd", "value_micros": MAX_SIGNAL_VALUE + 1}}),
            json!({"user_balance": 100}),
            json!({"recent_usage": {"requests": 1, "window_seconds": 0}}),
            json!({"recent_usage": {"requests": 1, "window_seconds": 604_801}}),
        ] {
            assert!(UpstreamRoutingSettings::parse(&json!({"models": {"model": entry}})).is_err());
        }
    }
}
