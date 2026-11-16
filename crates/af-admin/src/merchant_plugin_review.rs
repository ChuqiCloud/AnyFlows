//! Read-only merchant service review through an explicitly granted plugin capability.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use af_domain::{MerchantOwnedService, MerchantPoolError};
use af_plugin_contract::PluginId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    DatabaseAdminPluginLifecycleService, MerchantPoolService, SessionPrincipal,
    readonly_plugin_review::{
        MAX_REVIEW_CHECKS, ReadOnlyPluginReviewCheck, ReadOnlyPluginReviewContract,
        ReadOnlyPluginReviewError, ReadOnlyPluginReviewStatus, invoke_read_only_review,
        list_read_only_reviewers, read_only_review_output_schema,
    },
};

const REVIEW_CAPABILITY: &str = "merchant.service.review.read";
const REVIEW_BUDGET: Duration = Duration::from_secs(5);
const MAX_REVIEW_INPUT_BYTES: u32 = 64 * 1024;

/// A plugin that is enabled and granted the fixed merchant review capability.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantPluginReviewer {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
}

/// Safe list response; it contains no deployment configuration or artifact metadata.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantPluginReviewerSummary {
    pub plugins: Vec<MerchantPluginReviewer>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MerchantPluginReviewStatus {
    Pass,
    Info,
    Warning,
    Fail,
}

impl From<ReadOnlyPluginReviewStatus> for MerchantPluginReviewStatus {
    fn from(value: ReadOnlyPluginReviewStatus) -> Self {
        match value {
            ReadOnlyPluginReviewStatus::Pass => Self::Pass,
            ReadOnlyPluginReviewStatus::Info => Self::Info,
            ReadOnlyPluginReviewStatus::Warning => Self::Warning,
            ReadOnlyPluginReviewStatus::Fail => Self::Fail,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantPluginReviewCheck {
    pub code: String,
    pub title: String,
    pub status: MerchantPluginReviewStatus,
    pub message: String,
    #[serde(default)]
    pub recommendation: Option<String>,
}

impl From<ReadOnlyPluginReviewCheck> for MerchantPluginReviewCheck {
    fn from(value: ReadOnlyPluginReviewCheck) -> Self {
        Self {
            code: value.code,
            title: value.title,
            status: value.status.into(),
            message: value.message,
            recommendation: value.recommendation,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MerchantPluginReview {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub service_revision: i64,
    pub summary: String,
    pub checks: Vec<MerchantPluginReviewCheck>,
}

/// Session-authenticated merchant plugin review operations.
pub type MerchantPluginReviewFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, MerchantPoolError>> + Send + 'a>>;

pub trait MerchantPluginReviewService: Send + Sync {
    fn reviewers(
        &self,
        principal: SessionPrincipal,
    ) -> MerchantPluginReviewFuture<'_, MerchantPluginReviewerSummary>;

    fn review_service(
        &self,
        principal: SessionPrincipal,
        service_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> MerchantPluginReviewFuture<'_, MerchantPluginReview>;
}

/// The host resolves ownership and projects a deliberately small service view before invoking a plugin.
pub struct DatabaseMerchantPluginReviewService {
    merchants: Arc<dyn MerchantPoolService>,
    plugins: Arc<DatabaseAdminPluginLifecycleService>,
    slots: Semaphore,
}

impl DatabaseMerchantPluginReviewService {
    #[must_use]
    pub fn new(
        merchants: Arc<dyn MerchantPoolService>,
        plugins: Arc<DatabaseAdminPluginLifecycleService>,
    ) -> Self {
        Self {
            merchants,
            plugins,
            slots: Semaphore::new(8),
        }
    }
}

impl MerchantPluginReviewService for DatabaseMerchantPluginReviewService {
    fn reviewers(
        &self,
        _principal: SessionPrincipal,
    ) -> MerchantPluginReviewFuture<'_, MerchantPluginReviewerSummary> {
        Box::pin(async move {
            let input_schema = review_input_schema();
            let output_schema = read_only_review_output_schema();
            let public_config = json!({});
            let contract = review_contract(&input_schema, &output_schema, &public_config);
            let plugins = list_read_only_reviewers(&self.plugins, &contract, REVIEW_BUDGET)
                .await
                .map_err(map_review_error)?
                .into_iter()
                .map(|plugin| MerchantPluginReviewer {
                    plugin_id: plugin.plugin_id,
                    display_name: plugin.display_name,
                    plugin_version: plugin.plugin_version,
                })
                .collect();
            Ok(MerchantPluginReviewerSummary { plugins })
        })
    }

    fn review_service(
        &self,
        principal: SessionPrincipal,
        service_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> MerchantPluginReviewFuture<'_, MerchantPluginReview> {
        Box::pin(async move {
            if service_id <= 0 || request_id.is_empty() || request_id.len() > 64 {
                return Err(MerchantPoolError::InvalidInput);
            }
            PluginId::new(plugin_id.clone()).map_err(|_| MerchantPoolError::InvalidInput)?;
            let _permit = self
                .slots
                .try_acquire()
                .map_err(|_| MerchantPoolError::Unavailable)?;

            tokio::time::timeout(REVIEW_BUDGET, async {
                let service = self.merchants.service(principal, service_id).await?;
                let revision = review_revision(&service).ok_or(MerchantPoolError::NotFound)?;
                let input = review_input(revision).map_err(|_| MerchantPoolError::InvalidInput)?;
                let input_schema = review_input_schema();
                let output_schema = read_only_review_output_schema();
                let public_config = json!({});
                let contract = review_contract(&input_schema, &output_schema, &public_config);
                let output = invoke_read_only_review(
                    &self.plugins,
                    &contract,
                    &plugin_id,
                    &request_id,
                    input,
                )
                .await
                .map_err(map_review_error)?;

                // A concurrent service edit, ownership change, or plugin permission change
                // invalidates the advice before it is returned to the caller.
                let latest_service = self.merchants.service(principal, service_id).await?;
                let latest_revision =
                    review_revision(&latest_service).ok_or(MerchantPoolError::NotFound)?;
                if latest_revision.revision != revision.revision {
                    return Err(MerchantPoolError::Conflict);
                }
                Ok(MerchantPluginReview {
                    plugin_id: output.reviewer.plugin_id,
                    display_name: output.reviewer.display_name,
                    plugin_version: output.reviewer.plugin_version,
                    service_revision: revision.revision,
                    summary: output.summary,
                    checks: output.checks.into_iter().map(Into::into).collect(),
                })
            })
            .await
            .map_err(|_| MerchantPoolError::Unavailable)?
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
        max_output_bytes: 16 * 1024,
        max_checks: MAX_REVIEW_CHECKS,
        budget: REVIEW_BUDGET,
        action_proposal_capability: None,
        action_proposal_capability: None,
    }
}

fn review_input_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["service"],
        "properties":{"service":{
            "type":"object",
            "additionalProperties":false,
            "required":["name","description","protocol","models"],
            "properties":{
                "name":{"type":"string","minLength":1,"maxLength":256},
                "description":{"type":"string","maxLength":4096},
                "protocol":{"type":"string","minLength":1,"maxLength":64},
                "models":{
                    "type":"array","maxItems":128,
                    "items":{
                        "type":"object","additionalProperties":false,
                        "required":["model","context_window","max_output_tokens","supports_streaming","supports_tools","supports_vision"],
                        "properties":{
                            "model":{"type":"string","minLength":1,"maxLength":256},
                            "context_window":{"type":["integer","null"],"minimum":1},
                            "max_output_tokens":{"type":["integer","null"],"minimum":1},
                            "supports_streaming":{"type":"boolean"},
                            "supports_tools":{"type":"boolean"},
                            "supports_vision":{"type":"boolean"}
                        }
                    }
                }
            }
        }}
    })
}

fn review_revision(service: &MerchantOwnedService) -> Option<&af_domain::MerchantRevision> {
    service.draft.as_ref().or(service.published.as_ref())
}

fn review_input(revision: &af_domain::MerchantRevision) -> Result<serde_json::Value, ()> {
    let models = revision
        .models
        .iter()
        .map(|model| {
            json!({
                "model": model.model,
                "context_window": model.context_window,
                "max_output_tokens": model.max_output_tokens,
                "supports_streaming": model.supports_streaming,
                "supports_tools": model.supports_tools,
                "supports_vision": model.supports_vision,
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "service": {
            "name": revision.name,
            "description": revision.description,
            "protocol": revision.protocol,
            "models": models,
        }
    });
    if serde_json::to_vec(&value).map_err(|_| ())?.len() > MAX_REVIEW_INPUT_BYTES as usize {
        return Err(());
    }
    Ok(value)
}

fn map_review_error(error: ReadOnlyPluginReviewError) -> MerchantPoolError {
    match error {
        ReadOnlyPluginReviewError::Forbidden => MerchantPoolError::Forbidden,
        ReadOnlyPluginReviewError::InvalidInput => MerchantPoolError::InvalidInput,
        ReadOnlyPluginReviewError::NotFound => MerchantPoolError::NotFound,
        ReadOnlyPluginReviewError::Conflict => MerchantPoolError::Conflict,
        ReadOnlyPluginReviewError::Unavailable => MerchantPoolError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::{MerchantModelWrite, MerchantRevision};

    fn revision() -> MerchantRevision {
        MerchantRevision {
            id: 9,
            revision: 3,
            status: "draft".to_owned(),
            name: "Public name".to_owned(),
            description: "Public description".to_owned(),
            base_url: "https://private-upstream.example".to_owned(),
            protocol: "openai".to_owned(),
            default_ratio_micros: 1_000_000,
            models: vec![MerchantModelWrite {
                model: "public-model".to_owned(),
                upstream_model: "private-upstream-model".to_owned(),
                ratio_micros: Some(7),
                context_window: Some(8_192),
                max_output_tokens: Some(2_048),
                supports_streaming: true,
                supports_tools: false,
                supports_vision: true,
            }],
            concurrency: Some(5),
            timeout_secs: 60,
            has_api_key: true,
            review_reason: Some("private review note".to_owned()),
            created_at: 1,
            updated_at: 2,
        }
    }

    #[test]
    fn plugin_input_contains_only_public_service_description_and_model_capabilities() {
        let value = review_input(&revision()).unwrap();
        let text = value.to_string();
        assert!(text.contains("Public name"));
        assert!(text.contains("Public description"));
        assert!(text.contains("public-model"));
        assert!(text.contains("context_window"));
        assert!(value["service"].get("base_url").is_none());
        assert!(value["service"].get("default_ratio_micros").is_none());
        assert!(
            value["service"]["models"][0]
                .get("upstream_model")
                .is_none()
        );
        assert!(value["service"]["models"][0].get("ratio_micros").is_none());
        assert!(!text.contains("private-upstream"));
        assert!(!text.contains("private review"));
        assert!(!text.contains("https://"));
        assert!(!text.contains("api_key"));
        assert!(!text.contains("ratio_micros"));
        assert!(!text.contains("merchant_id"));
    }
}
