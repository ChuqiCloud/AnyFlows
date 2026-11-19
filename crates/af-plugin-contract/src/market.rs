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
    #[serde(default)]
    pub source: Option<PluginMarketSource>,
    pub commit: Option<String>,
    pub engines: Option<PluginMarketEngines>,
    pub capabilities: Vec<String>,
    pub published_at: Option<String>,
}

/// A remotely installable, immutable plugin artifact.
///
/// The archive URL remains in the version record for format-1 compatibility;
/// new publishers should also provide this structured source so future hosts
/// can select a provider-specific adapter without parsing URLs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginMarketSource {
    pub kind: String,
    pub archive_url: String,
    pub sha256: String,
    pub repository: Option<String>,
    pub commit: Option<String>,
    pub path: Option<String>,
}

impl PluginMarketSource {
    pub const GITHUB_RELEASE: &str = "github_release";
    pub const GIT_ARTIFACT: &str = "git_artifact";
    pub const HTTP_ARCHIVE: &str = "http_archive";

    pub fn validate(&self) -> Result<(), PluginMarketError> {
        if !matches!(
            self.kind.as_str(),
            Self::GITHUB_RELEASE | Self::GIT_ARTIFACT | Self::HTTP_ARCHIVE
        ) || !valid_https_url(&self.archive_url)
            || self.archive_url.len() > 4096
            || self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || self
                .repository
                .as_deref()
                .is_some_and(|value| !valid_https_url(value) || value.len() > 2048)
            || self.commit.as_deref().is_some_and(|value| {
                value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            || self.path.as_deref().is_some_and(|value| {
                value.is_empty()
                    || value.len() > 900
                    || value.split('/').any(|part| {
                        part.is_empty()
                            || matches!(part, "." | "..")
                            || !part.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || b"._@+-".contains(&byte)
                            })
                    })
            })
        {
            return Err(PluginMarketError::InvalidVersion);
        }
        if self.kind == Self::GITHUB_RELEASE && self.repository.is_none() {
            return Err(PluginMarketError::InvalidVersion);
        }
        if self.kind == Self::GITHUB_RELEASE
            && (!host_is(&self.archive_url, "github.com")
                || !self
                    .repository
                    .as_deref()
                    .is_some_and(|value| host_is(value, "github.com")))
        {
            return Err(PluginMarketError::InvalidVersion);
        }
        if self.kind == Self::GIT_ARTIFACT && self.commit.is_none() {
            return Err(PluginMarketError::InvalidVersion);
        }
        Ok(())
    }
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
                    || !valid_https_url(&version.archive_url)
                    || version.sha256.len() != 64
                    || !version.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(PluginMarketError::InvalidVersion);
                }
                if let Some(source) = &version.source {
                    source.validate()?;
                    if source.sha256 != version.sha256 {
                        return Err(PluginMarketError::InvalidVersion);
                    }
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

fn valid_https_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty()
        && authority.len() <= 255
        && authority
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
}

fn host_is(value: &str, expected: &str) -> bool {
    value
        .strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .is_some_and(|host| host.eq_ignore_ascii_case(expected))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn source(kind: &str) -> PluginMarketSource {
        PluginMarketSource {
            kind: kind.to_owned(),
            archive_url:
                "https://github.com/example/plugin/releases/download/v1.0.0/plugin.afplugin"
                    .to_owned(),
            sha256: "a".repeat(64),
            repository: Some("https://github.com/example/plugin".to_owned()),
            commit: Some("b".repeat(40)),
            path: Some("dist/plugin.afplugin".to_owned()),
        }
    }

    #[test]
    fn source_requires_immutable_provider_fields() {
        assert!(
            source(PluginMarketSource::GITHUB_RELEASE)
                .validate()
                .is_ok()
        );
        let mut invalid = source(PluginMarketSource::GIT_ARTIFACT);
        invalid.commit = None;
        assert_eq!(invalid.validate(), Err(PluginMarketError::InvalidVersion));
        let mut invalid = source(PluginMarketSource::HTTP_ARCHIVE);
        invalid.archive_url = "http://example.com/plugin.afplugin".to_owned();
        assert_eq!(invalid.validate(), Err(PluginMarketError::InvalidVersion));
        let mut invalid = source(PluginMarketSource::GITHUB_RELEASE);
        invalid.archive_url = "https://downloads.example.com/plugin.afplugin".to_owned();
        assert_eq!(invalid.validate(), Err(PluginMarketError::InvalidVersion));
    }

    #[test]
    fn index_accepts_structured_source_without_relaxing_archive_checks() {
        let index = PluginMarketIndex {
            format: 1,
            title: "Example".to_owned(),
            description: None,
            plugins: vec![PluginMarketEntry {
                id: "org.example.plugin".to_owned(),
                title: "Example".to_owned(),
                description: None,
                icon: None,
                author: None,
                repository: None,
                versions: vec![PluginMarketVersion {
                    version: "1.0.0".to_owned(),
                    archive_url: "https://example.com/plugin.afplugin".to_owned(),
                    sha256: "a".repeat(64),
                    source: Some(source(PluginMarketSource::GITHUB_RELEASE)),
                    commit: Some("b".repeat(40)),
                    engines: None,
                    capabilities: Vec::new(),
                    published_at: None,
                }],
            }],
        };
        assert!(index.validate().is_ok());
    }
}
