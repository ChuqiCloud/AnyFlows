use serde::{Deserialize, Serialize};

/// Static, cacheable market index format. A market is a discovery source;
/// installation still goes through the host artifact verifier.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginMarketIndex {
    pub format: u16,
    pub title: String,
    pub description: Option<String>,
    pub plugins: Vec<PluginMarketEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginMarketEntry {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub author: Option<String>,
    pub repository: Option<String>,
    pub versions: Vec<PluginMarketVersion>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginMarketVersion {
    pub version: String,
    pub archive_url: String,
    pub sha256: String,
    pub commit: Option<String>,
    pub engines: Option<PluginMarketEngines>,
    pub capabilities: Vec<String>,
    pub published_at: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginMarketEngines {
    pub anyflows: Option<String>,
}

impl PluginMarketIndex {
    pub fn validate(&self) -> Result<(), PluginMarketError> {
        if self.format != 1 || self.plugins.len() > 10_000 {
            return Err(PluginMarketError::InvalidIndex);
        }
        for plugin in &self.plugins {
            if plugin.id.is_empty() || plugin.id.len() > 128 || plugin.versions.len() > 256 {
                return Err(PluginMarketError::InvalidEntry);
            }
            for version in &plugin.versions {
                if version.version.is_empty()
                    || version.archive_url.len() > 4096
                    || !version.archive_url.starts_with("https://")
                    || version.sha256.len() != 64
                    || !version.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(PluginMarketError::InvalidVersion);
                }
                if version.commit.as_ref().is_some_and(|commit| {
                    commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
                }) {
                    return Err(PluginMarketError::InvalidVersion);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PluginMarketError {
    #[error("plugin market index is invalid")]
    InvalidIndex,
    #[error("plugin market entry is invalid")]
    InvalidEntry,
    #[error("plugin market version is invalid")]
    InvalidVersion,
}
