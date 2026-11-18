//! Namespaced plugin state contract.
//!
//! The host owns persistence and transactions. A plugin can only address its
//! own namespace and stores bounded JSON values; SQL, cross-plugin reads and
//! host business tables are never exposed.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ContractValidationError;

pub const MAX_PLUGIN_STORAGE_KEY_BYTES: usize = 128;
pub const MAX_PLUGIN_STORAGE_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_PLUGIN_STORAGE_LIST_LIMIT: u16 = 100;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginStorageKey {
    pub key: String,
}

impl PluginStorageKey {
    pub fn validate(&self) -> Result<(), ContractValidationError> {
        if self.key.is_empty()
            || self.key.len() > MAX_PLUGIN_STORAGE_KEY_BYTES
            || self.key.trim() != self.key
            || self.key.chars().any(char::is_control)
            || !self.key.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/')
            })
            || self.key.starts_with('/')
            || self.key.contains("//")
            || self.key.contains("..")
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginStorageValue {
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix_ms: Option<u64>,
}

impl PluginStorageValue {
    pub fn validate(&self, now_unix_ms: u64) -> Result<(), ContractValidationError> {
        if serde_json::to_vec(&self.value)
            .map_err(|_| ContractValidationError::InvalidExternalValue)?
            .len()
            > MAX_PLUGIN_STORAGE_VALUE_BYTES
            || self
                .expires_at_unix_ms
                .is_some_and(|expires| expires <= now_unix_ms)
        {
            return Err(ContractValidationError::InvalidExternalValue);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum PluginStorageOperation {
    Get {
        key: PluginStorageKey,
    },
    Put {
        key: PluginStorageKey,
        value: PluginStorageValue,
    },
    Delete {
        key: PluginStorageKey,
    },
    List {
        prefix: Option<String>,
        limit: u16,
    },
}

impl PluginStorageOperation {
    pub fn validate(&self, now_unix_ms: u64) -> Result<(), ContractValidationError> {
        match self {
            Self::Get { key } | Self::Delete { key } => key.validate(),
            Self::Put { key, value } => {
                key.validate()?;
                value.validate(now_unix_ms)
            }
            Self::List { prefix, limit } => {
                if *limit == 0 || *limit > MAX_PLUGIN_STORAGE_LIST_LIMIT {
                    return Err(ContractValidationError::InvalidExternalValue);
                }
                if let Some(prefix) = prefix {
                    PluginStorageKey {
                        key: prefix.clone(),
                    }
                    .validate()?;
                }
                Ok(())
            }
        }
    }
}
