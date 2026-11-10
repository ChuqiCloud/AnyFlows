use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Exact, case-sensitive restrictions on final upstream model identifiers.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamModelPolicyMode {
    #[default]
    Inherit,
    Allowlist,
    Denylist,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamModelPolicy {
    pub mode: UpstreamModelPolicyMode,
    pub models: Vec<String>,
    pub disabled_models: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("upstream model policy is invalid")]
pub struct UpstreamModelPolicyError;

impl UpstreamModelPolicy {
    pub fn validate(&self) -> Result<(), UpstreamModelPolicyError> {
        let mut bytes = 0_usize;
        for list in [&self.models, &self.disabled_models] {
            if list.len() > 1000 {
                return Err(UpstreamModelPolicyError);
            }
            for name in list {
                bytes = bytes.saturating_add(name.len());
                if name.is_empty()
                    || name.len() > 255
                    || name.trim() != name
                    || name.chars().any(char::is_control)
                    || bytes > 256 * 1024
                {
                    return Err(UpstreamModelPolicyError);
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn allows(&self, upstream_model: &str) -> bool {
        if self
            .disabled_models
            .iter()
            .any(|model| model == upstream_model)
        {
            return false;
        }
        match self.mode {
            UpstreamModelPolicyMode::Inherit => true,
            UpstreamModelPolicyMode::Allowlist => {
                self.models.iter().any(|model| model == upstream_model)
            }
            UpstreamModelPolicyMode::Denylist => {
                !self.models.iter().any(|model| model == upstream_model)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_empty_and_explicit_denials_are_distinct() {
        assert!(UpstreamModelPolicy::default().allows("vendor/Model"));
        let mut policy = UpstreamModelPolicy {
            mode: UpstreamModelPolicyMode::Allowlist,
            ..Default::default()
        };
        assert!(!policy.allows("vendor/Model"));
        policy.models.push("vendor/Model".to_owned());
        assert!(policy.allows("vendor/Model"));
        assert!(!policy.allows("vendor/model"));
        policy.disabled_models.push("vendor/Model".to_owned());
        assert!(!policy.allows("vendor/Model"));
        policy.mode = UpstreamModelPolicyMode::Denylist;
        assert!(policy.allows("other"));
        assert!(!policy.allows("vendor/Model"));
    }
}
