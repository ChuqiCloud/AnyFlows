//! Read-only merchant service review through an explicitly granted plugin capability.

use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_db::{PluginDeploymentQuery, PluginDeploymentRecord};
use af_domain::{MerchantOwnedService, MerchantPoolError};
use af_plugin_contract::{
    CapabilityAccess, InvocationMetadata, PluginId, PluginManifest, SideEffectClass,
};
use af_plugin_runtime::CapabilityExecutionContext;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    AdminPluginLifecycleError, DatabaseAdminPluginLifecycleService, MerchantPoolService,
    SessionPrincipal,
};

const REVIEW_CAPABILITY: &str = "merchant.service.review.read";
const REVIEW_BUDGET: Duration = Duration::from_secs(5);
const MAX_REVIEW_INPUT_BYTES: u32 = 64 * 1024;
const MAX_REVIEW_OUTPUT_BYTES: u32 = 16 * 1024;
const MAX_REVIEW_CHECKS: usize = 64;
const MAX_REVIEWER_DEPLOYMENTS: u32 = 100;
const MAX_REVIEWER_SCAN_PAGES: usize = 11;

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
            if self.plugins.capability_runtime().is_none() {
                return Ok(MerchantPluginReviewerSummary {
                    plugins: Vec::new(),
                });
            }
            tokio::time::timeout(REVIEW_BUDGET, async {
                let repository = self.plugins.catalog_repository();
                let mut cursor = None;
                let mut plugins = Vec::new();
                for page_index in 0..MAX_REVIEWER_SCAN_PAGES {
                    let deployments = repository
                        .list_deployments(
                            PluginDeploymentQuery::new(cursor.clone(), MAX_REVIEWER_DEPLOYMENTS)
                                .map_err(|_| MerchantPoolError::Unavailable)?,
                        )
                        .await
                        .map_err(|_| MerchantPoolError::Unavailable)?;
                    let page_is_full = deployments.len() == MAX_REVIEWER_DEPLOYMENTS as usize;
                    if let Some(last) = deployments.last() {
                        cursor = Some(last.plugin_id.clone());
                    }
                    plugins.extend(
                        deployments
                            .iter()
                            .filter(|deployment| review_capability(deployment).is_some())
                            .map(reviewer),
                    );
                    if plugins.len() >= MAX_REVIEWER_DEPLOYMENTS as usize {
                        plugins.truncate(MAX_REVIEWER_DEPLOYMENTS as usize);
                        break;
                    }
                    if !page_is_full {
                        break;
                    }
                    if page_index + 1 == MAX_REVIEWER_SCAN_PAGES {
                        return Err(MerchantPoolError::Unavailable);
                    }
                }
                Ok(MerchantPluginReviewerSummary { plugins })
            })
            .await
            .map_err(|_| MerchantPoolError::Unavailable)?
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
                let input_bytes =
                    serde_json::to_vec(&input).map_err(|_| MerchantPoolError::Unavailable)?;
                if input_bytes.len() > MAX_REVIEW_INPUT_BYTES as usize {
                    return Err(MerchantPoolError::InvalidInput);
                }

                let repository = self.plugins.catalog_repository();
                let deployment = repository
                    .get_deployment(&plugin_id)
                    .await
                    .map_err(|_| MerchantPoolError::Unavailable)?
                    .ok_or(MerchantPoolError::NotFound)?;
                if review_capability(&deployment).is_none() {
                    return Err(MerchantPoolError::Forbidden);
                }
                let runtime = self
                    .plugins
                    .capability_runtime()
                    .ok_or(MerchantPoolError::Unavailable)?;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| MerchantPoolError::Unavailable)?;
                let deadline_unix_ms = u64::try_from(now.as_millis())
                    .ok()
                    .and_then(|now| now.checked_add(u64::try_from(REVIEW_BUDGET.as_millis()).ok()?))
                    .ok_or(MerchantPoolError::Unavailable)?;
                let context = CapabilityExecutionContext::new(
                    format!("merchant-service-review:{}", deployment.artifact.digest),
                    InvocationMetadata {
                        request_id: request_id.clone(),
                        deadline_unix_ms,
                    },
                    json!({}),
                    MAX_REVIEW_INPUT_BYTES,
                )
                .map_err(|_| MerchantPoolError::InvalidInput)?;
                let response = runtime
                    .invoke_capability(
                        repository,
                        &plugin_id,
                        deployment.generation,
                        &context,
                        REVIEW_CAPABILITY,
                        1,
                        input,
                    )
                    .await
                    .map_err(map_plugin_error)?;
                let output = decode_review(response.output)?;

                // A concurrent service edit, ownership change, or plugin permission change
                // invalidates the advice before it is returned to the caller.
                let latest_service = self.merchants.service(principal, service_id).await?;
                let latest_revision =
                    review_revision(&latest_service).ok_or(MerchantPoolError::NotFound)?;
                if latest_revision.revision != revision.revision {
                    return Err(MerchantPoolError::Conflict);
                }
                let latest_deployment = repository
                    .get_deployment(&plugin_id)
                    .await
                    .map_err(|_| MerchantPoolError::Unavailable)?
                    .ok_or(MerchantPoolError::NotFound)?;
                if latest_deployment.generation != deployment.generation
                    || review_capability(&latest_deployment).is_none()
                {
                    return Err(MerchantPoolError::Conflict);
                }

                let source = reviewer(&deployment);
                Ok(MerchantPluginReview {
                    plugin_id: source.plugin_id,
                    display_name: source.display_name,
                    plugin_version: source.plugin_version,
                    service_revision: revision.revision,
                    summary: output.summary,
                    checks: output.checks,
                })
            })
            .await
            .map_err(|_| MerchantPoolError::Unavailable)?
        })
    }
}

fn review_capability(deployment: &PluginDeploymentRecord) -> Option<&'static str> {
    if !deployment.enabled
        || deployment.artifact.approval.is_none()
        || deployment.artifact.revocation.is_some()
        || deployment.artifact.source_kind != af_db::PluginArtifactSourceKind::LocalProcess
        // This first business integration passes no plugin config; all input is the
        // explicit service projection below, never operator-supplied configuration.
        || deployment.config != json!({})
        || !deployment
            .granted_capabilities
            .iter()
            .any(|grant| grant.id == REVIEW_CAPABILITY && grant.version == 1)
    {
        return None;
    }
    let manifest: PluginManifest =
        serde_json::from_value(deployment.artifact.manifest.clone()).ok()?;
    if manifest.limits.max_response_bytes < MAX_REVIEW_INPUT_BYTES {
        return None;
    }
    manifest
        .capabilities
        .iter()
        .any(|capability| {
            capability.id.as_str() == REVIEW_CAPABILITY
                && capability.version == 1
                && capability.access == CapabilityAccess::Read
                && capability.side_effect == SideEffectClass::None
                && capability.input_schema.as_ref() == Some(&review_input_schema())
                && capability.output_schema.as_ref() == Some(&review_output_schema())
        })
        .then_some(REVIEW_CAPABILITY)
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

fn review_output_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["summary","checks"],
        "properties":{
            "summary":{"type":"string","minLength":1,"maxLength":2048},
            "checks":{
                "type":"array","maxItems":MAX_REVIEW_CHECKS,
                "items":{
                    "type":"object","additionalProperties":false,
                    "required":["code","title","status","message","recommendation"],
                    "properties":{
                        "code":{"type":"string","minLength":1,"maxLength":64},
                        "title":{"type":"string","minLength":1,"maxLength":128},
                        "status":{"type":"string","enum":["pass","info","warning","fail"]},
                        "message":{"type":"string","minLength":1,"maxLength":2048},
                        "recommendation":{"type":["string","null"],"minLength":1,"maxLength":2048}
                    }
                }
            }
        }
    })
}

fn reviewer(deployment: &PluginDeploymentRecord) -> MerchantPluginReviewer {
    let display_name = deployment
        .artifact
        .manifest
        .get("displayName")
        .and_then(serde_json::Value::as_str)
        .filter(|value| valid_text(value, 128))
        .unwrap_or(&deployment.plugin_id)
        .to_owned();
    MerchantPluginReviewer {
        plugin_id: deployment.plugin_id.clone(),
        display_name,
        plugin_version: deployment.artifact.plugin_version.clone(),
    }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewOutput {
    summary: String,
    checks: Vec<MerchantPluginReviewCheck>,
}

fn decode_review(output: serde_json::Value) -> Result<ReviewOutput, MerchantPoolError> {
    let encoded = serde_json::to_vec(&output).map_err(|_| MerchantPoolError::Unavailable)?;
    if encoded.len() > MAX_REVIEW_OUTPUT_BYTES as usize {
        return Err(MerchantPoolError::Unavailable);
    }
    let parsed: ReviewOutput =
        serde_json::from_value(output).map_err(|_| MerchantPoolError::Unavailable)?;
    let mut codes = BTreeSet::new();
    if !valid_text(&parsed.summary, 2_048)
        || parsed.checks.len() > MAX_REVIEW_CHECKS
        || parsed.checks.iter().any(|check| {
            !valid_text(&check.code, 64)
                || !codes.insert(check.code.as_str())
                || !valid_text(&check.title, 128)
                || !valid_text(&check.message, 2_048)
                || check
                    .recommendation
                    .as_deref()
                    .is_some_and(|value| !valid_text(value, 2_048))
        })
    {
        return Err(MerchantPoolError::Unavailable);
    }
    Ok(parsed)
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn map_plugin_error(error: AdminPluginLifecycleError) -> MerchantPoolError {
    match error {
        AdminPluginLifecycleError::Forbidden => MerchantPoolError::Forbidden,
        AdminPluginLifecycleError::InvalidInput => MerchantPoolError::InvalidInput,
        AdminPluginLifecycleError::NotFound => MerchantPoolError::NotFound,
        AdminPluginLifecycleError::Conflict => MerchantPoolError::Conflict,
        AdminPluginLifecycleError::CapabilityReviewRequired
        | AdminPluginLifecycleError::PayloadTooLarge
        | AdminPluginLifecycleError::RuntimeUnavailable
        | AdminPluginLifecycleError::Internal => MerchantPoolError::Unavailable,
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

    #[test]
    fn review_output_rejects_duplicate_codes_oversize_and_unknown_fields() {
        assert!(
            decode_review(json!({
                "summary":"ok",
                "checks":[
                    {"code":"same","title":"one","status":"pass","message":"ok"},
                    {"code":"same","title":"two","status":"info","message":"ok"}
                ]
            }))
            .is_err()
        );
        assert!(decode_review(json!({"summary":"ok","checks":[],"extra":"no"})).is_err());
    }
}
