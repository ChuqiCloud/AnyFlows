//! Read-only plugin analysis of a channel's public model names and mappings.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use af_domain::ChannelId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    AdminChannelReadError, AdminChannelReader, DatabaseAdminPluginLifecycleService,
    SessionPrincipal,
    readonly_plugin_review::{
        MAX_REVIEW_CHECKS, ReadOnlyPluginReviewCheck, ReadOnlyPluginReviewContract,
        ReadOnlyPluginReviewError, ReadOnlyPluginReviewStatus, invoke_read_only_review,
        list_read_only_reviewers, read_only_review_output_schema,
    },
};

const REVIEW_CAPABILITY: &str = "channel.model_mapping.review.read";
const REVIEW_BUDGET: Duration = Duration::from_secs(5);
const MAX_REVIEW_INPUT_BYTES: u32 = 512 * 1024;
const MAX_REVIEW_OUTPUT_BYTES: u32 = 16 * 1024;
const MAX_REVIEW_MODELS: usize = 512;
const MAX_MODEL_NAME_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelModelMappingPluginReviewStatus {
    Pass,
    Info,
    Warning,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelModelMappingPluginReviewCheck {
    pub code: String,
    pub title: String,
    pub status: ChannelModelMappingPluginReviewStatus,
    pub message: String,
    pub recommendation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChannelModelMappingPluginReviewer {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChannelModelMappingPluginReviewerList {
    pub plugins: Vec<ChannelModelMappingPluginReviewer>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChannelModelMappingPluginReview {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub model_count: u32,
    pub mapping_count: u32,
    pub summary: String,
    pub checks: Vec<ChannelModelMappingPluginReviewCheck>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ChannelModelMappingPluginReviewError {
    #[error("channel model mapping plugin review request is invalid")]
    InvalidInput,
    #[error("channel or plugin deployment was not found")]
    NotFound,
    #[error("plugin is not available for channel model mapping review")]
    Forbidden,
    #[error("channel model mappings or plugin deployment changed during review")]
    Conflict,
    #[error("channel model mapping plugin review is unavailable")]
    Unavailable,
}

pub type ChannelModelMappingPluginReviewFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ChannelModelMappingPluginReviewError>> + Send + 'a>>;

pub trait ChannelModelMappingPluginReviewService: Send + Sync {
    fn reviewers(
        &self,
        principal: SessionPrincipal,
    ) -> ChannelModelMappingPluginReviewFuture<'_, ChannelModelMappingPluginReviewerList>;

    fn review_mappings(
        &self,
        principal: SessionPrincipal,
        channel_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> ChannelModelMappingPluginReviewFuture<'_, ChannelModelMappingPluginReview>;
}

/// The host projects only public model names and exact model mappings to the plugin.
pub struct DatabaseChannelModelMappingPluginReviewService {
    channels: Arc<dyn AdminChannelReader>,
    plugins: Arc<DatabaseAdminPluginLifecycleService>,
    slots: Semaphore,
}

impl DatabaseChannelModelMappingPluginReviewService {
    #[must_use]
    pub fn new(
        channels: Arc<dyn AdminChannelReader>,
        plugins: Arc<DatabaseAdminPluginLifecycleService>,
    ) -> Self {
        Self {
            channels,
            plugins,
            slots: Semaphore::new(8),
        }
    }
}

impl ChannelModelMappingPluginReviewService for DatabaseChannelModelMappingPluginReviewService {
    fn reviewers(
        &self,
        _principal: SessionPrincipal,
    ) -> ChannelModelMappingPluginReviewFuture<'_, ChannelModelMappingPluginReviewerList> {
        Box::pin(async move {
            let input_schema = review_input_schema();
            let output_schema = read_only_review_output_schema();
            let public_config = json!({});
            let contract = review_contract(&input_schema, &output_schema, &public_config);
            let reviewers = list_read_only_reviewers(&self.plugins, &contract, REVIEW_BUDGET)
                .await
                .map_err(map_review_error)?
                .into_iter()
                .map(|reviewer| ChannelModelMappingPluginReviewer {
                    plugin_id: reviewer.plugin_id,
                    display_name: reviewer.display_name,
                    plugin_version: reviewer.plugin_version,
                })
                .collect();
            Ok(ChannelModelMappingPluginReviewerList { plugins: reviewers })
        })
    }

    fn review_mappings(
        &self,
        principal: SessionPrincipal,
        channel_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> ChannelModelMappingPluginReviewFuture<'_, ChannelModelMappingPluginReview> {
        Box::pin(async move {
            if channel_id <= 0 || request_id.is_empty() || request_id.len() > 64 {
                return Err(ChannelModelMappingPluginReviewError::InvalidInput);
            }
            let channel_id = ChannelId::new(channel_id)
                .map_err(|_| ChannelModelMappingPluginReviewError::InvalidInput)?;
            let _permit = self
                .slots
                .try_acquire()
                .map_err(|_| ChannelModelMappingPluginReviewError::Unavailable)?;

            tokio::time::timeout(REVIEW_BUDGET, async {
                let channel = self
                    .channels
                    .get_channel(principal, channel_id)
                    .await
                    .map_err(map_channel_error)?;
                let input = review_input(channel.models(), channel.model_mapping())?;
                let model_count = input["models"]
                    .as_array()
                    .and_then(|items| u32::try_from(items.len()).ok())
                    .ok_or(ChannelModelMappingPluginReviewError::Unavailable)?;
                let mapping_count = input["mappings"]
                    .as_array()
                    .and_then(|items| u32::try_from(items.len()).ok())
                    .ok_or(ChannelModelMappingPluginReviewError::Unavailable)?;

                let input_schema = review_input_schema();
                let output_schema = read_only_review_output_schema();
                let public_config = json!({});
                let contract = review_contract(&input_schema, &output_schema, &public_config);
                let result = invoke_read_only_review(
                    &self.plugins,
                    &contract,
                    &plugin_id,
                    &request_id,
                    input.clone(),
                )
                .await
                .map_err(map_review_error)?;

                // Advice is valid only while the exact public projection remains unchanged.
                let latest = self
                    .channels
                    .get_channel(principal, channel_id)
                    .await
                    .map_err(map_channel_error)?;
                if review_input(latest.models(), latest.model_mapping())? != input {
                    return Err(ChannelModelMappingPluginReviewError::Conflict);
                }

                Ok(ChannelModelMappingPluginReview {
                    plugin_id: result.reviewer.plugin_id,
                    display_name: result.reviewer.display_name,
                    plugin_version: result.reviewer.plugin_version,
                    model_count,
                    mapping_count,
                    summary: result.summary,
                    checks: result.checks.into_iter().map(Into::into).collect(),
                })
            })
            .await
            .map_err(|_| ChannelModelMappingPluginReviewError::Unavailable)?
        })
    }
}

fn review_contract<'a>(
    input_schema: &'a Value,
    output_schema: &'a Value,
    public_config: &'a Value,
) -> ReadOnlyPluginReviewContract<'a> {
    ReadOnlyPluginReviewContract {
        capability_id: REVIEW_CAPABILITY,
        capability_version: 1,
        input_schema,
        output_schema,
        public_config,
        max_input_bytes: MAX_REVIEW_INPUT_BYTES,
        max_output_bytes: MAX_REVIEW_OUTPUT_BYTES,
        max_checks: MAX_REVIEW_CHECKS,
        budget: REVIEW_BUDGET,
        action_proposal_capability: None,
        action_proposal_capability: None,
    }
}

fn review_input(
    models: &[String],
    mappings: &Value,
) -> Result<Value, ChannelModelMappingPluginReviewError> {
    if models.len() > MAX_REVIEW_MODELS || models.iter().any(|model| !valid_model_name(model)) {
        return Err(ChannelModelMappingPluginReviewError::Unavailable);
    }
    let mapping_object = mappings
        .as_object()
        .ok_or(ChannelModelMappingPluginReviewError::Unavailable)?;
    if mapping_object.len() > MAX_REVIEW_MODELS {
        return Err(ChannelModelMappingPluginReviewError::Unavailable);
    }
    let projected_mappings = mapping_object
        .iter()
        .map(|(model, upstream_model)| {
            let upstream_model = upstream_model
                .as_str()
                .ok_or(ChannelModelMappingPluginReviewError::Unavailable)?;
            if !valid_model_name(model) || !valid_model_name(upstream_model) {
                return Err(ChannelModelMappingPluginReviewError::Unavailable);
            }
            Ok(json!({"model": model, "upstreamModel": upstream_model}))
        })
        .collect::<Result<Vec<_>, ChannelModelMappingPluginReviewError>>()?;
    let input = json!({
        "models": models,
        "mappings": projected_mappings,
    });
    if serde_json::to_vec(&input)
        .map_err(|_| ChannelModelMappingPluginReviewError::Unavailable)?
        .len()
        > MAX_REVIEW_INPUT_BYTES as usize
    {
        return Err(ChannelModelMappingPluginReviewError::Unavailable);
    }
    Ok(input)
}

fn valid_model_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODEL_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn review_input_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["models","mappings"],
        "properties":{
            "models":{
                "type":"array","maxItems":MAX_REVIEW_MODELS,
                "items":{"type":"string","minLength":1,"maxLength":MAX_MODEL_NAME_BYTES}
            },
            "mappings":{
                "type":"array","maxItems":MAX_REVIEW_MODELS,
                "items":{
                    "type":"object","additionalProperties":false,
                    "required":["model","upstreamModel"],
                    "properties":{
                        "model":{"type":"string","minLength":1,"maxLength":MAX_MODEL_NAME_BYTES},
                        "upstreamModel":{"type":"string","minLength":1,"maxLength":MAX_MODEL_NAME_BYTES}
                    }
                }
            }
        }
    })
}

fn map_channel_error(error: AdminChannelReadError) -> ChannelModelMappingPluginReviewError {
    match error {
        AdminChannelReadError::InvalidPagination => {
            ChannelModelMappingPluginReviewError::InvalidInput
        }
        AdminChannelReadError::Forbidden => ChannelModelMappingPluginReviewError::Forbidden,
        AdminChannelReadError::ChannelNotFound | AdminChannelReadError::CredentialNotFound => {
            ChannelModelMappingPluginReviewError::NotFound
        }
        AdminChannelReadError::Internal => ChannelModelMappingPluginReviewError::Unavailable,
    }
}

fn map_review_error(error: ReadOnlyPluginReviewError) -> ChannelModelMappingPluginReviewError {
    match error {
        ReadOnlyPluginReviewError::InvalidInput => {
            ChannelModelMappingPluginReviewError::InvalidInput
        }
        ReadOnlyPluginReviewError::NotFound => ChannelModelMappingPluginReviewError::NotFound,
        ReadOnlyPluginReviewError::Forbidden => ChannelModelMappingPluginReviewError::Forbidden,
        ReadOnlyPluginReviewError::Conflict => ChannelModelMappingPluginReviewError::Conflict,
        ReadOnlyPluginReviewError::Unavailable => ChannelModelMappingPluginReviewError::Unavailable,
    }
}

impl From<ReadOnlyPluginReviewCheck> for ChannelModelMappingPluginReviewCheck {
    fn from(value: ReadOnlyPluginReviewCheck) -> Self {
        Self {
            code: value.code,
            title: value.title,
            status: match value.status {
                ReadOnlyPluginReviewStatus::Pass => ChannelModelMappingPluginReviewStatus::Pass,
                ReadOnlyPluginReviewStatus::Info => ChannelModelMappingPluginReviewStatus::Info,
                ReadOnlyPluginReviewStatus::Warning => {
                    ChannelModelMappingPluginReviewStatus::Warning
                }
                ReadOnlyPluginReviewStatus::Fail => ChannelModelMappingPluginReviewStatus::Fail,
            },
            message: value.message,
            recommendation: value.recommendation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_projection_contains_only_models_and_exact_mappings() {
        let models = vec!["public-model".to_owned()];
        let mappings = json!({"public-model":"upstream-model"});
        let input = review_input(&models, &mappings).unwrap();
        assert_eq!(
            input,
            json!({
                "models":["public-model"],
                "mappings":[{"model":"public-model","upstreamModel":"upstream-model"}]
            })
        );
        let keys = input
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(keys.len(), 2);
        let text = input.to_string();
        for forbidden in [
            "channel_id",
            "base_url",
            "credential",
            "api_key",
            "param_override",
        ] {
            assert!(!text.contains(forbidden));
        }
    }

    #[test]
    fn plugin_projection_rejects_unbounded_or_invalid_mapping_data() {
        let too_long_name = ["x".repeat(MAX_MODEL_NAME_BYTES + 1)];
        assert!(review_input(&too_long_name, &json!({})).is_err());
        assert!(review_input(&[], &json!({"source":"bad\nmodel"})).is_err());
        assert!(review_input(&[], &json!({"source": 42})).is_err());
        let too_many_models = vec!["model".to_owned(); MAX_REVIEW_MODELS + 1];
        assert!(review_input(&too_many_models, &json!({})).is_err());
    }
}
