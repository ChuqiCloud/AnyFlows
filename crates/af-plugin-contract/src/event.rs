use std::{collections::BTreeMap, sync::OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, ContractValidationError, SideEffectClass,
};

/// Versioned host event that a plugin may consume.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginEventSubscription {
    pub event_kind: String,
    pub capability_id: CapabilityId,
    pub capability_version: u16,
    /// Equality filters over fields explicitly marked filterable in the event catalog.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub filters: BTreeMap<String, Vec<String>>,
}

impl PluginEventSubscription {
    pub fn validate(
        &self,
        capabilities: &[CapabilityDeclaration],
    ) -> Result<(), ContractValidationError> {
        if self.capability_version == 0 {
            return Err(ContractValidationError::InvalidManifest);
        }
        let Some(event) = plugin_event_definition(&self.event_kind) else {
            return Err(ContractValidationError::InvalidManifest);
        };
        if self.capability_id.as_str() != event.capability_id
            || self.capability_version != event.capability_version
        {
            return Err(ContractValidationError::InvalidManifest);
        }
        let Some(capability) = capabilities.iter().find(|capability| {
            capability.id == self.capability_id && capability.version == self.capability_version
        }) else {
            return Err(ContractValidationError::InvalidManifest);
        };
        if capability.access != CapabilityAccess::Read
            || capability.side_effect != SideEffectClass::None
            || capability.input_schema.as_ref() != Some(&event.payload_schema)
        {
            return Err(ContractValidationError::InvalidManifest);
        }
        for (field, values) in &self.filters {
            let Some(definition) = event
                .filter_fields
                .iter()
                .find(|definition| definition.name == *field)
            else {
                return Err(ContractValidationError::InvalidManifest);
            };
            if values.is_empty()
                || values.len() > definition.allowed_values.len()
                || values
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != values.len()
                || values
                    .iter()
                    .any(|value| !definition.allowed_values.contains(value))
            {
                return Err(ContractValidationError::InvalidManifest);
            }
        }
        Ok(())
    }

    /// Returns whether this subscription accepts one already host-projected event.
    #[must_use]
    pub fn matches(&self, payload: &Value) -> bool {
        self.filters.iter().all(|(field, accepted)| {
            payload
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|value| accepted.iter().any(|candidate| candidate == value))
        })
    }
}

/// Host-owned event descriptor exposed to plugin tooling and administration.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEventDefinition {
    pub event_kind: String,
    pub capability_id: String,
    pub capability_version: u16,
    pub payload_schema: Value,
    pub filter_fields: Vec<PluginEventFilterField>,
}

/// One field that a manifest may use to filter deliveries.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginEventFilterField {
    pub name: String,
    pub allowed_values: Vec<String>,
}

/// Return the built-in event catalog. Producers define event creation and safe projections;
/// this catalog defines the public wire shape and accepted subscription filters.
#[must_use]
pub fn plugin_event_catalog() -> &'static [PluginEventDefinition] {
    static CATALOG: OnceLock<Vec<PluginEventDefinition>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        vec![
            PluginEventDefinition {
                event_kind: "channel.health.changed".to_owned(),
                capability_id: "events.channel-health.changed.consume".to_owned(),
                capability_version: 1,
                payload_schema: json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","targetId","channelId","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "targetId":{"type":"integer","minimum":1},
                        "channelId":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["down","recovered"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                filter_fields: vec![PluginEventFilterField {
                    name: "status".to_owned(),
                    allowed_values: vec!["down".to_owned(), "recovered".to_owned()],
                }],
            },
            PluginEventDefinition {
                event_kind: "merchant.service.changed".to_owned(),
                capability_id: "events.merchant-service.changed.consume".to_owned(),
                capability_version: 1,
                payload_schema: json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","serviceId","revision","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "serviceId":{"type":"integer","minimum":1},
                        "revision":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["active","paused"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                filter_fields: vec![PluginEventFilterField {
                    name: "status".to_owned(),
                    allowed_values: vec!["active".to_owned(), "paused".to_owned()],
                }],
            },
            event_definition(
                "model.catalog.changed",
                "events.model-catalog.changed.consume",
                json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","modelId","revision","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "modelId":{"type":"string","minLength":1,"maxLength":128},
                        "revision":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["added","updated","removed"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                &["status"],
                &["added", "updated", "removed"],
            ),
            event_definition(
                "channel.config.changed",
                "events.channel-config.changed.consume",
                json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","channelId","revision","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "channelId":{"type":"integer","minimum":1},
                        "revision":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["created","updated","disabled","deleted"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                &["status"],
                &["created", "updated", "disabled", "deleted"],
            ),
            event_definition(
                "channel.credential.changed",
                "events.channel-credential.changed.consume",
                json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","channelId","revision","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "channelId":{"type":"integer","minimum":1},
                        "revision":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["configured","rotated","invalid","revoked"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                &["status"],
                &["configured", "rotated", "invalid", "revoked"],
            ),
            event_definition(
                "model.pricing.changed",
                "events.model-pricing.changed.consume",
                json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","modelId","revision","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "modelId":{"type":"string","minLength":1,"maxLength":128},
                        "revision":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["published","updated","retired"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                &["status"],
                &["published", "updated", "retired"],
            ),
            event_definition(
                "external.account.balance.changed",
                "events.external-account-balance.changed.consume",
                json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","accountRef","revision","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "accountRef":{"type":"string","minLength":1,"maxLength":128},
                        "revision":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["observed","increased","decreased","unavailable"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                &["status"],
                &["observed", "increased", "decreased", "unavailable"],
            ),
            event_definition(
                "plugin.deployment.changed",
                "events.plugin-deployment.changed.consume",
                json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","pluginId","generation","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "pluginId":{"type":"string","minLength":1,"maxLength":128},
                        "generation":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["installed","ready","disabled","failed"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                &["status"],
                &["installed", "ready", "disabled", "failed"],
            ),
            event_definition(
                "plugin.task.changed",
                "events.plugin-task.changed.consume",
                json!({
                    "type":"object", "additionalProperties":false,
                    "required":["eventKey","taskRef","revision","status","observedAt"],
                    "properties":{
                        "eventKey":{"type":"string","minLength":1,"maxLength":128},
                        "taskRef":{"type":"string","minLength":1,"maxLength":128},
                        "revision":{"type":"integer","minimum":1},
                        "status":{"type":"string","enum":["submitted","queued","running","succeeded","failed","cancelled"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }),
                &["status"],
                &["submitted", "queued", "running", "succeeded", "failed", "cancelled"],
            ),
        ]
    })
}

fn event_definition(
    event_kind: &str,
    capability_id: &str,
    payload_schema: Value,
    filter_names: &[&str],
    allowed_values: &[&str],
) -> PluginEventDefinition {
    PluginEventDefinition {
        event_kind: event_kind.to_owned(),
        capability_id: capability_id.to_owned(),
        capability_version: 1,
        payload_schema,
        filter_fields: filter_names
            .iter()
            .map(|name| PluginEventFilterField {
                name: (*name).to_owned(),
                allowed_values: allowed_values
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect(),
            })
            .collect(),
    }
}

/// Find a registered event by its stable wire name.
#[must_use]
pub fn plugin_event_definition(event_kind: &str) -> Option<&'static PluginEventDefinition> {
    plugin_event_catalog()
        .iter()
        .find(|definition| definition.event_kind == event_kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(id: &str) -> CapabilityDeclaration {
        let event_kind = match id {
            "events.channel-health.changed.consume" => "channel.health.changed",
            "events.merchant-service.changed.consume" => "merchant.service.changed",
            _ => unreachable!("test capability must be a registered event capability"),
        };
        CapabilityDeclaration {
            id: CapabilityId::new(id).unwrap(),
            version: 1,
            access: CapabilityAccess::Read,
            side_effect: SideEffectClass::None,
            write_semantics: None,
            input_schema: Some(
                plugin_event_definition(event_kind)
                    .unwrap()
                    .payload_schema
                    .clone(),
            ),
            output_schema: Some(json!({"type":"object"})),
        }
    }

    #[test]
    fn event_catalog_exposes_schemas_and_matching_filters() {
        let catalog = plugin_event_catalog();
        assert!(catalog.len() >= 9);
        let subscription = PluginEventSubscription {
            event_kind: "channel.health.changed".to_owned(),
            capability_id: CapabilityId::new("events.channel-health.changed.consume").unwrap(),
            capability_version: 1,
            filters: BTreeMap::from([("status".to_owned(), vec!["down".to_owned()])]),
        };
        subscription
            .validate(&[capability("events.channel-health.changed.consume")])
            .unwrap();
        assert!(subscription.matches(&json!({"status":"down"})));
        assert!(!subscription.matches(&json!({"status":"recovered"})));
        assert!(catalog[0].payload_schema["properties"]["eventKey"].is_object());

        let mut incompatible = capability("events.channel-health.changed.consume");
        incompatible.input_schema = Some(json!({"type":"object"}));
        assert!(subscription.validate(&[incompatible]).is_err());
    }

    #[test]
    fn subscriptions_reject_unknown_events_capabilities_and_filter_values() {
        let good_capability = capability("events.channel-health.changed.consume");
        let mut subscription = PluginEventSubscription {
            event_kind: "channel.health.changed".to_owned(),
            capability_id: CapabilityId::new("events.channel-health.changed.consume").unwrap(),
            capability_version: 1,
            filters: BTreeMap::new(),
        };
        subscription.event_kind = "channel.private.data".to_owned();
        assert!(
            subscription
                .validate(std::slice::from_ref(&good_capability))
                .is_err()
        );
        subscription.event_kind = "channel.health.changed".to_owned();
        subscription.capability_id =
            CapabilityId::new("events.merchant-service.changed.consume").unwrap();
        assert!(
            subscription
                .validate(std::slice::from_ref(&good_capability))
                .is_err()
        );
        subscription.capability_id =
            CapabilityId::new("events.channel-health.changed.consume").unwrap();
        subscription
            .filters
            .insert("status".to_owned(), vec!["secret".to_owned()]);
        assert!(subscription.validate(&[good_capability]).is_err());
    }
}
