use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{
    CapabilityDeclaration, ContractValidationError, ContractVersionRange, PluginVersion,
    schema::compile_json_schema,
};

/// Extensible namespaced plugin ID; never a product platform enum.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct PluginId(String);

impl PluginId {
    pub fn new(value: impl Into<String>) -> Result<Self, ContractValidationError> {
        let value = value.into();
        let segments = value.split('.').collect::<Vec<_>>();
        let valid = value.len() <= 128
            && segments.len() >= 2
            && segments.iter().all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
                    && segment.as_bytes()[0].is_ascii_alphanumeric()
                    && segment.as_bytes()[segment.len() - 1].is_ascii_alphanumeric()
            });
        if !valid {
            return Err(ContractValidationError::InvalidIdentifier);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for PluginId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for PluginId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Versioned plugin descriptor consumed by the host before activation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub plugin_id: PluginId,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    pub plugin_version: PluginVersion,
    pub contract_range: ContractVersionRange,
    pub capabilities: Vec<CapabilityDeclaration>,
    pub authentication_modes: Vec<String>,
    pub limits: ConnectorLimits,
    pub config_schema: Value,
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        self.plugin_version.validate()?;
        self.contract_range.validate()?;
        if !self.config_schema.is_object()
            || self.capabilities.is_empty()
            || self.capabilities.len() > 128
            || self.display_name.len() > 128
            || self.display_name.chars().any(char::is_control)
            || self.authentication_modes.is_empty()
            || self.authentication_modes.len() > 16
            || self
                .authentication_modes
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.authentication_modes.len()
            || self.authentication_modes.iter().any(|mode| {
                mode.is_empty()
                    || mode.len() > 128
                    || !mode.split('.').all(|segment| {
                        !segment.is_empty()
                            && segment.bytes().all(|byte| {
                                byte.is_ascii_lowercase()
                                    || byte.is_ascii_digit()
                                    || byte == b'-'
                                    || byte == b'_'
                            })
                    })
            })
        {
            return Err(ContractValidationError::InvalidManifest);
        }
        compile_json_schema(&self.config_schema)
            .map_err(|_| ContractValidationError::InvalidJsonSchema)?;
        self.limits.validate()?;
        let mut ids = BTreeSet::new();
        for capability in &self.capabilities {
            capability.validate()?;
            if !ids.insert(&capability.id) {
                return Err(ContractValidationError::DuplicateCapability);
            }
        }
        Ok(())
    }

    /// Validate a public connector configuration against this manifest's schema.
    /// Validation errors intentionally contain no instance values.
    pub fn validate_config(&self, config: &Value) -> Result<(), ContractValidationError> {
        crate::schema::validate_json_schema(&self.config_schema, config)
    }
}

/// Resource ceilings advertised before the host enables a connector.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorLimits {
    pub max_concurrency: u16,
    pub request_timeout_ms: u32,
    pub max_response_bytes: u32,
    pub max_page_size: u16,
}

impl ConnectorLimits {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.max_concurrency == 0
            || self.max_concurrency > 1024
            || self.request_timeout_ms == 0
            || self.request_timeout_ms > 300_000
            || self.max_response_bytes == 0
            || self.max_response_bytes > 16 * 1024 * 1024
            || self.max_page_size == 0
            || self.max_page_size > 10_000
        {
            return Err(ContractValidationError::InvalidManifest);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CapabilityAccess, CapabilityDeclaration, CapabilityId, ContractVersion, SideEffectClass,
    };

    fn manifest(config_schema: Value) -> PluginManifest {
        PluginManifest {
            plugin_id: PluginId::new("org.example.schema-test").unwrap(),
            display_name: "Schema test".to_owned(),
            plugin_version: PluginVersion::new("1.0.0").unwrap(),
            contract_range: ContractVersionRange::new(
                ContractVersion::new(1, 0),
                ContractVersion::new(1, 0),
            )
            .unwrap(),
            capabilities: vec![CapabilityDeclaration {
                id: CapabilityId::new("external.account.profile.read").unwrap(),
                version: 1,
                access: CapabilityAccess::Read,
                side_effect: SideEffectClass::ExternalRead,
                write_semantics: None,
                input_schema: None,
                output_schema: None,
            }],
            authentication_modes: vec!["bearer_token".to_owned()],
            limits: ConnectorLimits {
                max_concurrency: 1,
                request_timeout_ms: 1_000,
                max_response_bytes: 1_024,
                max_page_size: 1,
            },
            config_schema,
        }
    }

    #[test]
    fn manifest_compiles_config_schema_and_validates_config_instances() {
        let manifest = manifest(serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "baseUrl": { "$ref": "#/$defs/url" }
            },
            "required": ["baseUrl"],
            "additionalProperties": false,
            "$defs": {
                "url": { "type": "string", "format": "uri", "minLength": 1 }
            }
        }));

        assert_eq!(manifest.validate(), Ok(()));
        assert_eq!(
            manifest.validate_config(&serde_json::json!({ "baseUrl": "https://example.test" })),
            Ok(())
        );
        assert_eq!(
            manifest.validate_config(
                &serde_json::json!({ "baseUrl": 123, "secret": "private-canary" })
            ),
            Err(ContractValidationError::InvalidConfiguration)
        );
    }

    #[test]
    fn manifest_rejects_invalid_or_externally_referenced_config_schemas() {
        assert_eq!(
            manifest(serde_json::json!({ "type": "invalid-type" })).validate(),
            Err(ContractValidationError::InvalidJsonSchema)
        );
        assert_eq!(
            manifest(serde_json::json!({
                "type": "object",
                "properties": {
                    "baseUrl": { "$dynamicRef": "https://example.invalid/schema" }
                }
            }))
            .validate(),
            Err(ContractValidationError::InvalidJsonSchema)
        );
    }

    #[test]
    fn manifest_matches_wire_schema_rules_for_display_name_and_auth_modes() {
        let mut empty_display_name = manifest(serde_json::json!({ "type": "object" }));
        empty_display_name.display_name.clear();
        assert_eq!(empty_display_name.validate(), Ok(()));
        let mut duplicate_auth_modes = manifest(serde_json::json!({ "type": "object" }));
        duplicate_auth_modes
            .authentication_modes
            .push("bearer_token".to_owned());
        assert_eq!(
            duplicate_auth_modes.validate(),
            Err(ContractValidationError::InvalidManifest)
        );
    }
}
