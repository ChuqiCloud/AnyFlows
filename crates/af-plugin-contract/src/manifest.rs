use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{
    CapabilityAccess, CapabilityDeclaration, CapabilityId, ContractValidationError,
    ContractVersionRange, PluginEventSubscription, PluginVersion, SideEffectClass,
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
    /// Optional subscriptions to host-owned, versioned events.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub event_subscriptions: Vec<PluginEventSubscription>,
    pub authentication_modes: Vec<String>,
    pub limits: ConnectorLimits,
    pub config_schema: Value,
    /// Optional static, host-rendered UI cards. They contain no markup, routes, links, or actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_contributions: Vec<PluginUiContribution>,
    /// Optional versioned static views rendered only with host-owned components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_views: Vec<PluginUiView>,
    /// Optional administrator pages rendered by the host and bound to a declared read-only capability.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_pages: Vec<PluginUiPage>,
}

/// A host-routed administrator page with a fixed input for one read-only capability.
/// Plugins can choose a title and a capability binding, but never a URL or executable UI code.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginUiPage {
    pub id: String,
    pub schema_version: u16,
    pub title: PluginUiText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<PluginUiText>,
    pub capability_id: CapabilityId,
    pub capability_version: u16,
    /// Optional JSON input sent through the host's normal capability schema validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    /// Optional host-confirmed actions. Actions never execute from the manifest alone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<PluginUiAction>,
}

/// A host-confirmed action entry bound to an explicitly idempotent external-write capability.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginUiAction {
    pub id: String,
    pub schema_version: u16,
    pub title: PluginUiText,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<PluginUiText>,
    pub capability_id: CapabilityId,
    pub capability_version: u16,
    /// Optional initial JSON input; the host validates it against the capability schema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
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

/// A versioned, static view that can be placed only in a host-registered slot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginUiView {
    pub id: String,
    pub schema_version: u16,
    pub slot: PluginUiViewSlot,
    pub blocks: Vec<PluginUiBlock>,
}

/// Fixed module locations. A view cannot register a route or choose a URL.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginUiViewSlot {
    PluginManagement,
    ChannelMonitor,
    ChannelModelMapping,
    MerchantService,
    ExternalAccount,
}

/// Finite set of host-rendered presentation blocks. All values are static text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginUiBlock {
    Text {
        text: PluginUiText,
    },
    Badge {
        label: PluginUiText,
        tone: PluginUiTone,
    },
    Metric {
        label: PluginUiText,
        value: PluginUiText,
    },
    Table {
        columns: Vec<PluginUiText>,
        rows: Vec<Vec<PluginUiText>>,
    },
    List {
        items: Vec<PluginUiText>,
    },
    Notice {
        title: PluginUiText,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<PluginUiText>,
        tone: PluginUiTone,
    },
    Progress {
        label: PluginUiText,
        percent: u8,
    },
    InfoCard {
        title: PluginUiText,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<PluginUiText>,
    },
}

/// Host-controlled badge colors; plugins cannot supply classes or styles.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginUiTone {
    Neutral,
    Success,
    Warning,
    Danger,
}

const MAX_PLUGIN_UI_VIEWS: usize = 8;
const MAX_PLUGIN_UI_PAGES: usize = 32;
const MAX_PLUGIN_UI_BLOCKS_PER_VIEW: usize = 24;
const MAX_PLUGIN_UI_TABLE_COLUMNS: usize = 8;
const MAX_PLUGIN_UI_TABLE_ROWS: usize = 16;
const MAX_PLUGIN_UI_LIST_ITEMS: usize = 16;
const MAX_PLUGIN_UI_VIEW_BYTES: usize = 8 * 1024;
const MAX_PLUGIN_UI_PAGE_BYTES: usize = 8 * 1024;
const MAX_PLUGIN_UI_ACTIONS_PER_PAGE: usize = 16;
const MAX_PLUGIN_UI_ACTION_BYTES: usize = 8 * 1024;
const MAX_PLUGIN_UI_TOTAL_BYTES: usize = 32 * 1024;

impl PluginUiBlock {
    fn validate(&self) -> bool {
        match self {
            Self::Text { text } => text.validate(),
            Self::Badge { label, .. } => label.validate(),
            Self::Metric { label, value } => label.validate() && value.validate(),
            Self::Table { columns, rows } => {
                !columns.is_empty()
                    && columns.len() <= MAX_PLUGIN_UI_TABLE_COLUMNS
                    && rows.len() <= MAX_PLUGIN_UI_TABLE_ROWS
                    && columns.iter().all(PluginUiText::validate)
                    && rows.iter().all(|row| {
                        row.len() == columns.len() && row.iter().all(PluginUiText::validate)
                    })
            }
            Self::List { items } => {
                !items.is_empty()
                    && items.len() <= MAX_PLUGIN_UI_LIST_ITEMS
                    && items.iter().all(PluginUiText::validate)
            }
            Self::Notice {
                title, description, ..
            } => title.validate() && description.as_ref().is_none_or(PluginUiText::validate),
            Self::Progress { label, percent } => label.validate() && *percent <= 100,
            Self::InfoCard { title, description } => {
                title.validate() && description.as_ref().is_none_or(PluginUiText::validate)
            }
        }
    }
}

impl PluginUiText {
    fn validate(&self) -> bool {
        [&self.en, &self.zh]
            .into_iter()
            .any(|value| value.as_deref().is_some_and(valid_ui_text))
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
        let mut event_kinds = BTreeSet::new();
        if self.event_subscriptions.len() > 32
            || self.event_subscriptions.iter().any(|subscription| {
                !event_kinds.insert(&subscription.event_kind)
                    || subscription.validate(&self.capabilities).is_err()
            })
        {
            return Err(ContractValidationError::InvalidManifest);
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
        if self.ui_views.len() > MAX_PLUGIN_UI_VIEWS {
            return Err(ContractValidationError::InvalidManifest);
        }
        let mut total_view_bytes = 0usize;
        for view in &self.ui_views {
            if !valid_ui_id(&view.id)
                || view.schema_version != 1
                || view.blocks.is_empty()
                || view.blocks.len() > MAX_PLUGIN_UI_BLOCKS_PER_VIEW
                || !view.blocks.iter().all(PluginUiBlock::validate)
            {
                return Err(ContractValidationError::InvalidManifest);
            }
            let view_bytes = serde_json::to_vec(view)
                .map_err(|_| ContractValidationError::InvalidManifest)?
                .len();
            if view_bytes > MAX_PLUGIN_UI_VIEW_BYTES {
                return Err(ContractValidationError::InvalidManifest);
            }
            total_view_bytes = total_view_bytes
                .checked_add(view_bytes)
                .filter(|size| *size <= MAX_PLUGIN_UI_TOTAL_BYTES)
                .ok_or(ContractValidationError::InvalidManifest)?;
            if !contribution_ids.insert(&view.id) {
                return Err(ContractValidationError::InvalidManifest);
            }
        }
        if self.ui_pages.len() > MAX_PLUGIN_UI_PAGES {
            return Err(ContractValidationError::InvalidManifest);
        }
        let mut total_ui_bytes = total_view_bytes;
        for page in &self.ui_pages {
            let capability_available = self.capabilities.iter().any(|capability| {
                capability.id == page.capability_id
                    && capability.version == page.capability_version
                    && capability.access == CapabilityAccess::Read
                    && capability.side_effect == SideEffectClass::None
                    && capability.input_schema.is_some()
                    && capability.output_schema.is_some()
            });
            if !capability_available {
                return Err(ContractValidationError::InvalidManifest);
            }
            if !valid_ui_id(&page.id)
                || page.schema_version != 1
                || page.capability_version == 0
                || !contribution_ids.insert(&page.id)
                || !page.title.validate()
                || page
                    .description
                    .as_ref()
                    .is_some_and(|description| !description.validate())
            {
                return Err(ContractValidationError::InvalidManifest);
            }
            if page.actions.len() > MAX_PLUGIN_UI_ACTIONS_PER_PAGE {
                return Err(ContractValidationError::InvalidManifest);
            }
            let mut action_ids = BTreeSet::new();
            for action in &page.actions {
                let capability_available = self.capabilities.iter().any(|capability| {
                    capability.id == action.capability_id
                        && capability.version == action.capability_version
                        && capability.access == CapabilityAccess::Write
                        && capability.side_effect == SideEffectClass::ExternalWrite
                        && capability
                            .write_semantics
                            .is_some_and(|semantics| semantics.native_idempotency_key)
                        && capability.input_schema.is_some()
                        && capability.output_schema.is_some()
                });
                if !capability_available
                    || !valid_ui_id(&action.id)
                    || action.schema_version != 1
                    || action.capability_version == 0
                    || !action_ids.insert(&action.id)
                    || !contribution_ids.insert(&action.id)
                    || !action.title.validate()
                    || action
                        .description
                        .as_ref()
                        .is_some_and(|description| !description.validate())
                {
                    return Err(ContractValidationError::InvalidManifest);
                }
                let action_bytes = serde_json::to_vec(action)
                    .map_err(|_| ContractValidationError::InvalidManifest)?
                    .len();
                if action_bytes > MAX_PLUGIN_UI_ACTION_BYTES {
                    return Err(ContractValidationError::InvalidManifest);
                }
                if let Some(input) = &action.input
                    && serde_json::to_vec(input)
                        .map_err(|_| ContractValidationError::InvalidManifest)?
                        .len()
                        > MAX_PLUGIN_UI_ACTION_BYTES
                {
                    return Err(ContractValidationError::InvalidManifest);
                }
            }
            let page_bytes = serde_json::to_vec(page)
                .map_err(|_| ContractValidationError::InvalidManifest)?
                .len();
            if page_bytes > MAX_PLUGIN_UI_PAGE_BYTES {
                return Err(ContractValidationError::InvalidManifest);
            }
            total_ui_bytes = total_ui_bytes
                .checked_add(page_bytes)
                .filter(|size| *size <= MAX_PLUGIN_UI_TOTAL_BYTES)
                .ok_or(ContractValidationError::InvalidManifest)?;
            if let Some(input) = &page.input
                && serde_json::to_vec(input)
                    .map_err(|_| ContractValidationError::InvalidManifest)?
                    .len()
                    > MAX_PLUGIN_UI_PAGE_BYTES
            {
                return Err(ContractValidationError::InvalidManifest);
            }
        }
        Ok(())
    }
}

fn valid_ui_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
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
            event_subscriptions: Vec::new(),
            authentication_modes: vec!["bearer_token".to_owned()],
            limits: ConnectorLimits {
                max_concurrency: 1,
                request_timeout_ms: 1_000,
                max_response_bytes: 1_024,
                max_page_size: 1,
            },
            config_schema,
            ui_contributions: Vec::new(),
            ui_views: Vec::new(),
            ui_pages: Vec::new(),
        }
    }

    fn ui_page(id: &str, capability_id: &str, version: u16) -> PluginUiPage {
        PluginUiPage {
            id: id.to_owned(),
            schema_version: 1,
            title: PluginUiText {
                en: Some("Lookup details".to_owned()),
                zh: Some("查询详情".to_owned()),
            },
            description: None,
            capability_id: CapabilityId::new(capability_id).unwrap(),
            capability_version: version,
            input: Some(serde_json::json!({"query": "ready"})),
            actions: Vec::new(),
        }
    }

    fn add_custom_write_capability(value: &mut PluginManifest) {
        value.capabilities.push(CapabilityDeclaration {
            id: CapabilityId::new("vendor.customer.refresh").unwrap(),
            version: 1,
            access: CapabilityAccess::Write,
            side_effect: SideEffectClass::ExternalWrite,
            write_semantics: Some(crate::ExternalWriteSemantics {
                native_idempotency_key: true,
                result_lookup: crate::ResultLookupSupport::ByRequestId,
                reconciliation: true,
            }),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {"target": {"type": "string"}}
            })),
            output_schema: Some(serde_json::json!({"type": "object"})),
        });
    }

    fn add_custom_read_capability(value: &mut PluginManifest) {
        value.capabilities.push(CapabilityDeclaration {
            id: CapabilityId::new("vendor.customer.lookup").unwrap(),
            version: 1,
            access: CapabilityAccess::Read,
            side_effect: SideEffectClass::None,
            write_semantics: None,
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {"query": {"type": "string"}},
                "required": ["query"],
                "additionalProperties": false
            })),
            output_schema: Some(serde_json::json!({"type": "object"})),
        });
    }

    #[test]
    fn manifest_supports_bounded_administrator_pages_bound_to_declared_read_only_capabilities() {
        let mut value = manifest(serde_json::json!({ "type": "object" }));
        add_custom_read_capability(&mut value);
        value
            .ui_pages
            .push(ui_page("customer-lookup", "vendor.customer.lookup", 1));
        assert!(value.validate().is_ok());

        let mut old_wire =
            serde_json::to_value(manifest(serde_json::json!({ "type": "object" }))).unwrap();
        old_wire.as_object_mut().unwrap().remove("uiPages");
        let old: PluginManifest = serde_json::from_value(old_wire).unwrap();
        assert!(old.ui_pages.is_empty());
        assert!(old.validate().is_ok());

        let mut missing_capability = value.clone();
        missing_capability.ui_pages[0].capability_id =
            CapabilityId::new("vendor.customer.missing").unwrap();
        assert_eq!(
            missing_capability.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut wrong_version = value.clone();
        wrong_version.ui_pages[0].capability_version = 2;
        assert_eq!(
            wrong_version.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut unsupported_page_schema = value.clone();
        unsupported_page_schema.ui_pages[0].schema_version = 2;
        assert_eq!(
            unsupported_page_schema.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut write_capability = value.clone();
        write_capability.capabilities[1].access = CapabilityAccess::Write;
        assert_eq!(
            write_capability.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut side_effectful_capability = value.clone();
        side_effectful_capability.capabilities[1].side_effect = SideEffectClass::ExternalRead;
        assert_eq!(
            side_effectful_capability.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut untyped_capability = value.clone();
        untyped_capability.capabilities[1].input_schema = None;
        untyped_capability.capabilities[1].output_schema = None;
        assert_eq!(
            untyped_capability.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut duplicate_id = value.clone();
        duplicate_id.ui_contributions.push(PluginUiContribution {
            id: "customer-lookup".to_owned(),
            slot: PluginUiSlot::PluginManagement,
            component: PluginUiComponent::InfoCard,
            title: PluginUiText {
                en: Some("Lookup".to_owned()),
                zh: None,
            },
            description: None,
        });
        assert_eq!(
            duplicate_id.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut invalid_text = value.clone();
        invalid_text.ui_pages[0].title.en = Some("https://example.invalid".to_owned());
        assert_eq!(
            invalid_text.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut oversized_input = value;
        oversized_input.ui_pages[0].input =
            Some(Value::String("x".repeat(MAX_PLUGIN_UI_PAGE_BYTES)));
        assert_eq!(
            oversized_input.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut too_many_pages = manifest(serde_json::json!({ "type": "object" }));
        add_custom_read_capability(&mut too_many_pages);
        too_many_pages.ui_pages = (0..=MAX_PLUGIN_UI_PAGES)
            .map(|index| ui_page(&format!("page-{index}"), "vendor.customer.lookup", 1))
            .collect();
        assert_eq!(
            too_many_pages.validate(),
            Err(ContractValidationError::InvalidManifest)
        );
    }

    #[test]
    fn manifest_supports_explicitly_idempotent_write_actions_on_pages() {
        let mut value = manifest(serde_json::json!({ "type": "object" }));
        add_custom_read_capability(&mut value);
        add_custom_write_capability(&mut value);
        let mut page = ui_page("customer-lookup", "vendor.customer.lookup", 1);
        page.actions.push(PluginUiAction {
            id: "refresh-customer".to_owned(),
            schema_version: 1,
            title: PluginUiText {
                en: Some("Refresh customer".to_owned()),
                zh: Some("刷新客户".to_owned()),
            },
            description: None,
            capability_id: CapabilityId::new("vendor.customer.refresh").unwrap(),
            capability_version: 1,
            input: Some(serde_json::json!({"target": "customer-1"})),
        });
        value.ui_pages.push(page);
        assert!(value.validate().is_ok());

        let mut non_idempotent = value.clone();
        non_idempotent.capabilities[2]
            .write_semantics
            .as_mut()
            .unwrap()
            .native_idempotency_key = false;
        assert_eq!(
            non_idempotent.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut duplicate = value.clone();
        duplicate.ui_pages[0]
            .actions
            .push(duplicate.ui_pages[0].actions[0].clone());
        assert_eq!(
            duplicate.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut wrong_access = value.clone();
        wrong_access.capabilities[2].access = CapabilityAccess::Read;
        assert_eq!(
            wrong_access.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut too_many_actions = value;
        too_many_actions.ui_pages[0].actions = (0..=MAX_PLUGIN_UI_ACTIONS_PER_PAGE)
            .map(|index| PluginUiAction {
                id: format!("action-{index}"),
                schema_version: 1,
                title: PluginUiText {
                    en: Some("Action".to_owned()),
                    zh: None,
                },
                description: None,
                capability_id: CapabilityId::new("vendor.customer.refresh").unwrap(),
                capability_version: 1,
                input: None,
            })
            .collect();
        assert_eq!(
            too_many_actions.validate(),
            Err(ContractValidationError::InvalidManifest)
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

    #[test]
    fn manifest_accepts_bounded_host_rendered_views_and_old_manifests() {
        let mut value = manifest(serde_json::json!({ "type": "object" }));
        value.ui_views.push(PluginUiView {
            id: "account-summary".to_owned(),
            schema_version: 1,
            slot: PluginUiViewSlot::ExternalAccount,
            blocks: vec![
                PluginUiBlock::Text {
                    text: PluginUiText {
                        en: Some("Snapshot report".to_owned()),
                        zh: Some("快照报告".to_owned()),
                    },
                },
                PluginUiBlock::Badge {
                    label: PluginUiText {
                        en: Some("Current".to_owned()),
                        zh: None,
                    },
                    tone: PluginUiTone::Success,
                },
                PluginUiBlock::Metric {
                    label: PluginUiText {
                        en: Some("Balance".to_owned()),
                        zh: None,
                    },
                    value: PluginUiText {
                        en: Some("12.50 USD".to_owned()),
                        zh: None,
                    },
                },
                PluginUiBlock::Table {
                    columns: vec![PluginUiText {
                        en: Some("Observed".to_owned()),
                        zh: None,
                    }],
                    rows: vec![vec![PluginUiText {
                        en: Some("Today".to_owned()),
                        zh: Some("今天".to_owned()),
                    }]],
                },
                PluginUiBlock::List {
                    items: vec![PluginUiText {
                        en: Some("First item".to_owned()),
                        zh: Some("第一项".to_owned()),
                    }],
                },
                PluginUiBlock::Notice {
                    title: PluginUiText {
                        en: Some("Notice".to_owned()),
                        zh: Some("提示".to_owned()),
                    },
                    description: None,
                    tone: PluginUiTone::Warning,
                },
                PluginUiBlock::Progress {
                    label: PluginUiText {
                        en: Some("Progress".to_owned()),
                        zh: Some("进度".to_owned()),
                    },
                    percent: 75,
                },
                PluginUiBlock::InfoCard {
                    title: PluginUiText {
                        en: Some("Account".to_owned()),
                        zh: None,
                    },
                    description: None,
                },
            ],
        });
        assert!(value.validate().is_ok());

        let mut unsupported_version = value.clone();
        unsupported_version.ui_views[0].schema_version = 2;
        assert_eq!(
            unsupported_version.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut unsafe_text = value.clone();
        unsafe_text.ui_views[0].blocks[0] = PluginUiBlock::Text {
            text: PluginUiText {
                en: Some("<img src=x>".to_owned()),
                zh: None,
            },
        };
        assert_eq!(
            unsafe_text.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut too_many_blocks = value.clone();
        too_many_blocks.ui_views[0].blocks.resize(
            MAX_PLUGIN_UI_BLOCKS_PER_VIEW + 1,
            PluginUiBlock::Text {
                text: PluginUiText {
                    en: Some("bounded".to_owned()),
                    zh: None,
                },
            },
        );
        assert_eq!(
            too_many_blocks.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut mismatched_table = value.clone();
        mismatched_table.ui_views[0].blocks[3] = PluginUiBlock::Table {
            columns: vec![PluginUiText {
                en: Some("A".to_owned()),
                zh: None,
            }],
            rows: vec![vec![], vec![]],
        };
        assert_eq!(
            mismatched_table.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut empty_list = value.clone();
        let list = empty_list.ui_views[0]
            .blocks
            .iter_mut()
            .find(|block| matches!(block, PluginUiBlock::List { .. }))
            .expect("list block exists");
        *list = PluginUiBlock::List { items: Vec::new() };
        assert_eq!(
            empty_list.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut invalid_progress = value.clone();
        let progress = invalid_progress.ui_views[0]
            .blocks
            .iter_mut()
            .find(|block| matches!(block, PluginUiBlock::Progress { .. }))
            .expect("progress block exists");
        if let PluginUiBlock::Progress { percent, .. } = progress {
            *percent = 101;
        }
        assert_eq!(
            invalid_progress.validate(),
            Err(ContractValidationError::InvalidManifest)
        );

        let mut old_manifest = serde_json::to_value(manifest(serde_json::json!({
            "type": "object"
        })))
        .unwrap();
        old_manifest.as_object_mut().unwrap().remove("uiViews");
        assert_eq!(
            serde_json::from_value::<PluginManifest>(old_manifest)
                .unwrap()
                .ui_views,
            Vec::new()
        );

        let mut unknown_block = serde_json::to_value(value).unwrap();
        unknown_block["uiViews"][0]["blocks"][0] = serde_json::json!({
            "type": "custom_html",
            "html": "<script>bad()</script>"
        });
        assert!(serde_json::from_value::<PluginManifest>(unknown_block).is_err());
    }
}
