use std::{fmt, sync::Arc};

use thiserror::Error;

use crate::{
    MAX_MODEL_NAME_BYTES, MAX_TOKEN_MODEL_ALLOWLIST_COUNT, MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES,
};

/// Explicit model grants for a service identity. An empty grant denies all models.
#[derive(Clone, Eq, PartialEq)]
pub struct ServiceModelPolicy(Arc<[String]>);

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("Invalid service model allowlist")]
pub struct ServiceModelPolicyError;

impl ServiceModelPolicy {
    pub fn new(mut models: Vec<String>) -> Result<Self, ServiceModelPolicyError> {
        if models.len() > MAX_TOKEN_MODEL_ALLOWLIST_COUNT {
            return Err(ServiceModelPolicyError);
        }
        let mut bytes = 0_usize;
        for model in &models {
            if model.is_empty()
                || model.len() > MAX_MODEL_NAME_BYTES
                || model.trim() != model
                || model.chars().any(char::is_control)
            {
                return Err(ServiceModelPolicyError);
            }
            bytes = bytes
                .checked_add(model.len())
                .filter(|total| *total <= MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES)
                .ok_or(ServiceModelPolicyError)?;
        }
        models.sort_unstable();
        models.dedup();
        Ok(Self(models.into()))
    }

    #[must_use]
    pub fn allows(&self, model: &str) -> bool {
        self.0
            .binary_search_by(|item| item.as_str().cmp(model))
            .is_ok()
    }

    /// Each subordinate scope may narrow a grant, never widen it.
    #[must_use]
    pub fn intersect(&self, restriction: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter(|model| restriction.allows(model))
                .cloned()
                .collect::<Vec<_>>()
                .into(),
        )
    }

    #[must_use]
    pub fn models(&self) -> &[String] {
        &self.0
    }
}

impl fmt::Debug for ServiceModelPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ServiceModelPolicy(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_requires_explicit_exact_names_and_empty_means_deny() {
        let policy = ServiceModelPolicy::new(vec![
            "gpt-5".into(),
            "deepseek-flash".into(),
            "gpt-5".into(),
        ])
        .unwrap();
        assert_eq!(policy.models(), &["deepseek-flash", "gpt-5"]);
        assert!(policy.allows("gpt-5"));
        assert!(!policy.allows("GPT-5"));
        assert!(!policy.allows("gpt-5-high"));
        assert!(!ServiceModelPolicy::new(vec![]).unwrap().allows("gpt-5"));
        assert_eq!(format!("{policy:?}"), "ServiceModelPolicy(<redacted>)");
    }

    #[test]
    fn hierarchy_intersection_cannot_expand_platform_grants() {
        let platform = ServiceModelPolicy::new(vec!["a".into(), "b".into()]).unwrap();
        let child = ServiceModelPolicy::new(vec!["b".into(), "c".into()]).unwrap();
        assert_eq!(platform.intersect(&child).models(), &["b"]);
        assert!(
            platform
                .intersect(&ServiceModelPolicy::new(vec![]).unwrap())
                .models()
                .is_empty()
        );
    }

    #[test]
    fn malformed_or_oversized_grants_are_rejected_before_deduplication() {
        for models in [
            vec!["".into()],
            vec![" a".into()],
            vec!["a\n".into()],
            vec!["a".repeat(MAX_MODEL_NAME_BYTES + 1)],
            vec!["a".into(); MAX_TOKEN_MODEL_ALLOWLIST_COUNT + 1],
            vec!["a".repeat(MAX_MODEL_NAME_BYTES); 129],
        ] {
            assert_eq!(
                ServiceModelPolicy::new(models).unwrap_err(),
                ServiceModelPolicyError
            );
        }
    }
}
