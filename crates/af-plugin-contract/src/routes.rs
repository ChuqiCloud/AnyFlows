//! Declarative, host-routed HTTP entry points for plugins.
//!
//! A route is only a public transport binding. The capability declaration still
//! controls the operation and the host performs authentication, schema checks,
//! rate limiting, body limits and execution. Plugins never receive a listener,
//! arbitrary host path or a database connection.

use serde::{Deserialize, Serialize};

use crate::{
    CapabilityAccess, CapabilityId, ContractValidationError, PluginManifest, SideEffectClass,
};

const MAX_PLUGIN_ROUTES: usize = 64;
const MAX_ROUTE_ID_BYTES: usize = 64;
const MAX_ROUTE_PATH_BYTES: usize = 160;
const MAX_ROUTE_TIMEOUT_MS: u32 = 120_000;
const MAX_ROUTE_BODY_BYTES: u32 = 1024 * 1024;

/// HTTP methods exposed through the fixed `/api/plugins/{plugin_id}` prefix.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PluginRouteMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl PluginRouteMethod {
    #[must_use]
    pub const fn requires_write(self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch | Self::Delete)
    }
}

/// Caller class accepted by the host router.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginRouteAudience {
    User,
    Admin,
    /// API-key callers authenticated by the model gateway.
    ModelKey,
}

/// A route declared in `anyflows-plugin.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRoute {
    pub id: String,
    pub schema_version: u16,
    pub method: PluginRouteMethod,
    /// Relative path below `/api/plugins/{plugin_id}`; query strings are not allowed.
    pub path: String,
    pub audience: PluginRouteAudience,
    pub capability_id: CapabilityId,
    pub capability_version: u16,
    /// Maximum request body accepted by the host for this route.
    #[serde(default = "default_route_body_bytes")]
    pub max_body_bytes: u32,
    /// Host deadline for one capability invocation.
    #[serde(default = "default_route_timeout_ms")]
    pub timeout_ms: u32,
}

const fn default_route_body_bytes() -> u32 {
    256 * 1024
}

const fn default_route_timeout_ms() -> u32 {
    30_000
}

impl PluginRoute {
    pub(crate) fn validate(&self, manifest: &PluginManifest) -> bool {
        if !valid_route_id(&self.id)
            || self.schema_version != 1
            || !valid_route_path(&self.path)
            || self.capability_version == 0
            || self.max_body_bytes == 0
            || self.max_body_bytes > MAX_ROUTE_BODY_BYTES
            || self.timeout_ms == 0
            || self.timeout_ms > MAX_ROUTE_TIMEOUT_MS
        {
            return false;
        }
        let Some(capability) = manifest.capabilities.iter().find(|capability| {
            capability.id == self.capability_id && capability.version == self.capability_version
        }) else {
            return false;
        };
        if capability.input_schema.is_none() || capability.output_schema.is_none() {
            return false;
        }
        if self.method.requires_write() {
            capability.access == CapabilityAccess::Write
                && capability.side_effect == SideEffectClass::ExternalWrite
        } else {
            capability.access == CapabilityAccess::Read
        }
    }
}

fn valid_route_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ROUTE_ID_BYTES
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

fn valid_route_path(value: &str) -> bool {
    if value.is_empty()
        || value.len() > MAX_ROUTE_PATH_BYTES
        || !value.starts_with('/')
        || value.contains('?')
        || value.contains('#')
        || value.contains("//")
        || value.contains("..")
        || value.chars().any(char::is_control)
    {
        return false;
    }
    value.split('/').skip(1).all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    })
}

/// Validate all route declarations after deserializing a manifest.
pub(crate) fn validate_routes(manifest: &PluginManifest) -> Result<(), ContractValidationError> {
    if manifest.routes.len() > MAX_PLUGIN_ROUTES {
        return Err(ContractValidationError::InvalidManifest);
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for route in &manifest.routes {
        if !route.validate(manifest)
            || !ids.insert(&route.id)
            || !paths.insert((route.method, &route.path))
        {
            return Err(ContractValidationError::InvalidManifest);
        }
    }
    Ok(())
}
