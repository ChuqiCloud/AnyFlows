use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{CapabilityDeclaration, ContractValidationError, ContractVersionRange, PluginVersion};

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
    /// Optional static, host-rendered UI cards. They contain no markup, routes, links, or actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_contributions: Vec<PluginUiContribution>,
}

/// A bounded declarative card rendered with host-owned components and translations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginUiContribution {
    pub id: String,
    pub slot: PluginUiSlot,
    pub component: PluginUiComponent,
    pub title: PluginUiText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<PluginUiText>,
}

/// Fixed UI locations supported by the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginUiSlot {
    PluginManagement,
}

/// Fixed host-owned presentation components.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginUiComponent {
    InfoCard,
}

/// Plain localized text. No markup, URL, or executable content is interpreted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginUiText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub en: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zh: Option<String>,
}

impl PluginUiText {
    fn validate(&self) -> bool {
        [&self.en, &self.zh]
            .into_iter()
            .any(|value| value.as_deref().is_some_and(|text| valid_ui_text(text)))
            && [&self.en, &self.zh]
                .into_iter()
                .all(|value| value.as_deref().is_none_or(valid_ui_text))
    }
}

fn valid_ui_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    !text.trim().is_empty()
        && text.len() <= 512
        && !text.chars().any(char::is_control)
        && !text.contains('<')
        && !text.contains('>')
        && ![
            "http://",
            "https://",
            "javascript:",
            "data:",
            "mailto:",
            "www.",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        self.plugin_version.validate()?;
        self.contract_range.validate()?;
        if !self.config_schema.is_object()
            || serde_json::to_vec(&self.config_schema)
                .map_err(|_| ContractValidationError::InvalidManifest)?
                .len()
                > 64 * 1024
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
        self.limits.validate()?;
        let mut ids = BTreeSet::new();
        for capability in &self.capabilities {
            capability.validate()?;
            if !ids.insert(&capability.id) {
                return Err(ContractValidationError::DuplicateCapability);
            }
        }
        if self.ui_contributions.len() > 16 {
            return Err(ContractValidationError::InvalidManifest);
        }
        let mut contribution_ids = BTreeSet::new();
        if self.ui_contributions.iter().any(|contribution| {
            contribution.id.is_empty()
                || contribution.id.len() > 64
                || !contribution
                    .id
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                || !contribution
                    .id
                    .as_bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_alphanumeric())
                || !contribution.id.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || byte == b'-'
                        || byte == b'_'
                })
                || !contribution_ids.insert(&contribution.id)
                || !contribution.title.validate()
                || contribution
                    .description
                    .as_ref()
                    .is_some_and(|description| !description.validate())
        }) {
            return Err(ContractValidationError::InvalidManifest);
        }
        Ok(())
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
            ui_contributions: Vec::new(),
        }
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
        assert_eq!(
            manifest(serde_json::json!(true)).validate(),
            Err(ContractValidationError::InvalidManifest)
        );
    }

    #[test]
    fn manifest_accepts_only_bounded_static_plugin_management_cards() {
        let mut value = manifest(serde_json::json!({ "type": "object" }));
        value.ui_contributions.push(PluginUiContribution {
            id: "service-summary".to_owned(),
            slot: PluginUiSlot::PluginManagement,
            component: PluginUiComponent::InfoCard,
            title: PluginUiText {
                en: Some("Service summary".to_owned()),
                zh: Some("服务摘要".to_owned()),
            },
            description: Some(PluginUiText {
                en: Some("Static information rendered by AnyFlows.".to_owned()),
                zh: None,
            }),
        });
        assert!(value.validate().is_ok());

        let mut duplicate = value.clone();
        duplicate
            .ui_contributions
            .push(duplicate.ui_contributions[0].clone());
        assert_eq!(
            duplicate.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut unsafe_text = value;
        unsafe_text.ui_contributions[0].title.en = Some("<script>alert(1)</script>".to_owned());
        assert_eq!(
            unsafe_text.validate(),
            Err(ContractValidationError::InvalidManifest)
        );
    }
}
