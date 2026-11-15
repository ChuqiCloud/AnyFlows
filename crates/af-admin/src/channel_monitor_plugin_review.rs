//! Read-only plugin analysis of already-collected active channel monitor events.

use std::{
    collections::BTreeSet,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_db::{PluginDeploymentQuery, PluginDeploymentRecord};
use af_plugin_contract::{
    CapabilityAccess, InvocationMetadata, PluginId, PluginManifest, SideEffectClass,
};
use af_plugin_runtime::CapabilityExecutionContext;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    ActiveChannelMonitorEvent, ActiveChannelMonitorEventKind, ActiveChannelMonitorService,
    ActiveChannelMonitorServiceError, AdminPluginLifecycleError,
    DatabaseAdminPluginLifecycleService,
};

const REVIEW_CAPABILITY: &str = "channel.monitor.events.review.read";
const REVIEW_BUDGET: Duration = Duration::from_secs(5);
const MAX_REVIEW_INPUT_BYTES: u32 = 64 * 1024;
const MAX_REVIEW_OUTPUT_BYTES: u32 = 16 * 1024;
const MAX_REVIEW_CHECKS: usize = 64;
const MAX_REVIEWER_DEPLOYMENTS: u32 = 100;
const MAX_REVIEWER_SCAN_PAGES: usize = 11;
const MAX_REVIEW_EVENTS: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveChannelMonitorPluginReviewStatus {
    Pass,
    Info,
    Warning,
    Fail,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveChannelMonitorPluginReviewCheck {
    pub code: String,
    pub title: String,
    pub status: ActiveChannelMonitorPluginReviewStatus,
    pub message: String,
    #[serde(default)]
    pub recommendation: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveChannelMonitorPluginReviewer {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveChannelMonitorPluginReviewerList {
    pub plugins: Vec<ActiveChannelMonitorPluginReviewer>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveChannelMonitorPluginReview {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub event_count: u32,
    pub summary: String,
    pub checks: Vec<ActiveChannelMonitorPluginReviewCheck>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ActiveChannelMonitorPluginReviewError {
    #[error("active channel monitor plugin review request is invalid")]
    InvalidInput,
    #[error("active channel monitor target was not found")]
    NotFound,
    #[error("plugin is not available for channel monitor event review")]
    Forbidden,
    #[error("monitor events or plugin deployment changed during review")]
    Conflict,
    #[error("active channel monitor plugin review is unavailable")]
    Unavailable,
}

pub type ActiveChannelMonitorPluginReviewFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ActiveChannelMonitorPluginReviewError>> + Send + 'a>>;

pub trait ActiveChannelMonitorPluginReviewService: Send + Sync {
    fn reviewers(
        &self,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, ActiveChannelMonitorPluginReviewerList>;

    fn review_events(
        &self,
        target_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, ActiveChannelMonitorPluginReview>;
}

/// The host owns event access and sends only the event facts needed for analysis.
pub struct DatabaseActiveChannelMonitorPluginReviewService {
    monitor: Arc<dyn ActiveChannelMonitorService>,
    plugins: Arc<DatabaseAdminPluginLifecycleService>,
    slots: Semaphore,
}

impl DatabaseActiveChannelMonitorPluginReviewService {
    #[must_use]
    pub fn new(
        monitor: Arc<dyn ActiveChannelMonitorService>,
        plugins: Arc<DatabaseAdminPluginLifecycleService>,
    ) -> Self {
        Self {
            monitor,
            plugins,
            slots: Semaphore::new(8),
        }
    }
}

impl ActiveChannelMonitorPluginReviewService for DatabaseActiveChannelMonitorPluginReviewService {
    fn reviewers(
        &self,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, ActiveChannelMonitorPluginReviewerList> {
        Box::pin(async move {
            if self.plugins.capability_runtime().is_none() {
                return Ok(ActiveChannelMonitorPluginReviewerList {
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
                                .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?,
                        )
                        .await
                        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?;
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
                        return Err(ActiveChannelMonitorPluginReviewError::Unavailable);
                    }
                }
                Ok(ActiveChannelMonitorPluginReviewerList { plugins })
            })
            .await
            .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
        })
    }

    fn review_events(
        &self,
        target_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, ActiveChannelMonitorPluginReview> {
        Box::pin(async move {
            if target_id <= 0 || request_id.is_empty() || request_id.len() > 64 {
                return Err(ActiveChannelMonitorPluginReviewError::InvalidInput);
            }
            PluginId::new(plugin_id.clone())
                .map_err(|_| ActiveChannelMonitorPluginReviewError::InvalidInput)?;
            let _permit = self
                .slots
                .try_acquire()
                .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?;

            tokio::time::timeout(REVIEW_BUDGET, async {
                // Reading the existing event history also validates that the target still exists.
                let events = self
                    .monitor
                    .events(target_id)
                    .await
                    .map_err(map_monitor_error)?;
                if events.len() > MAX_REVIEW_EVENTS {
                    return Err(ActiveChannelMonitorPluginReviewError::Unavailable);
                }
                let input = review_input(&events)?;
                let input_bytes = serde_json::to_vec(&input)
                    .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?;
                if input_bytes.len() > MAX_REVIEW_INPUT_BYTES as usize {
                    return Err(ActiveChannelMonitorPluginReviewError::InvalidInput);
                }

                let repository = self.plugins.catalog_repository();
                let deployment = repository
                    .get_deployment(&plugin_id)
                    .await
                    .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
                    .ok_or(ActiveChannelMonitorPluginReviewError::NotFound)?;
                if review_capability(&deployment).is_none() {
                    return Err(ActiveChannelMonitorPluginReviewError::Forbidden);
                }
                let runtime = self
                    .plugins
                    .capability_runtime()
                    .ok_or(ActiveChannelMonitorPluginReviewError::Unavailable)?;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?;
                let deadline_unix_ms = u64::try_from(now.as_millis())
                    .ok()
                    .and_then(|now| now.checked_add(u64::try_from(REVIEW_BUDGET.as_millis()).ok()?))
                    .ok_or(ActiveChannelMonitorPluginReviewError::Unavailable)?;
                let context = CapabilityExecutionContext::new(
                    format!("channel-monitor-review:{}", deployment.artifact.digest),
                    InvocationMetadata {
                        request_id: request_id.clone(),
                        deadline_unix_ms,
                    },
                    json!({}),
                    MAX_REVIEW_INPUT_BYTES,
                )
                .map_err(|_| ActiveChannelMonitorPluginReviewError::InvalidInput)?;
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

                // New events or a target deletion make the analysis stale; never return advice
                // that no longer describes the current event history.
                let latest_events = self
                    .monitor
                    .events(target_id)
                    .await
                    .map_err(map_monitor_error)?;
                if latest_events != events {
                    return Err(ActiveChannelMonitorPluginReviewError::Conflict);
                }
                let latest_deployment = repository
                    .get_deployment(&plugin_id)
                    .await
                    .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
                    .ok_or(ActiveChannelMonitorPluginReviewError::NotFound)?;
                if latest_deployment.generation != deployment.generation
                    || review_capability(&latest_deployment).is_none()
                {
                    return Err(ActiveChannelMonitorPluginReviewError::Conflict);
                }

                let source = reviewer(&deployment);
                Ok(ActiveChannelMonitorPluginReview {
                    plugin_id: source.plugin_id,
                    display_name: source.display_name,
                    plugin_version: source.plugin_version,
                    event_count: u32::try_from(events.len())
                        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?,
                    summary: output.summary,
                    checks: output.checks,
                })
            })
            .await
            .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
        })
    }
}

fn map_monitor_error(
    error: ActiveChannelMonitorServiceError,
) -> ActiveChannelMonitorPluginReviewError {
    match error {
        ActiveChannelMonitorServiceError::InvalidInput => {
            ActiveChannelMonitorPluginReviewError::InvalidInput
        }
        ActiveChannelMonitorServiceError::NotFound => {
            ActiveChannelMonitorPluginReviewError::NotFound
        }
        ActiveChannelMonitorServiceError::Unavailable => {
            ActiveChannelMonitorPluginReviewError::Unavailable
        }
    }
}

fn map_plugin_error(error: AdminPluginLifecycleError) -> ActiveChannelMonitorPluginReviewError {
    match error {
        AdminPluginLifecycleError::Forbidden => ActiveChannelMonitorPluginReviewError::Forbidden,
        AdminPluginLifecycleError::InvalidInput => {
            ActiveChannelMonitorPluginReviewError::InvalidInput
        }
        AdminPluginLifecycleError::NotFound => ActiveChannelMonitorPluginReviewError::NotFound,
        AdminPluginLifecycleError::Conflict => ActiveChannelMonitorPluginReviewError::Conflict,
        AdminPluginLifecycleError::CapabilityReviewRequired
        | AdminPluginLifecycleError::PayloadTooLarge
        | AdminPluginLifecycleError::RuntimeUnavailable
        | AdminPluginLifecycleError::Internal => ActiveChannelMonitorPluginReviewError::Unavailable,
    }
}

fn review_capability(deployment: &PluginDeploymentRecord) -> Option<&'static str> {
    if !deployment.enabled
        || deployment.artifact.approval.is_none()
        || deployment.artifact.revocation.is_some()
        || deployment.artifact.source_kind != af_db::PluginArtifactSourceKind::LocalProcess
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
        "required":["events"],
        "properties":{"events":{
            "type":"array","maxItems":MAX_REVIEW_EVENTS,
            "items":{
                "type":"object","additionalProperties":false,
                "required":["kind","latency_ms","occurred_epoch_seconds"],
                "properties":{
                    "kind":{"type":"string","enum":["down","recovered"]},
                    "latency_ms":{"type":["integer","null"],"minimum":0},
                    "occurred_epoch_seconds":{"type":"integer","minimum":0}
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

fn reviewer(deployment: &PluginDeploymentRecord) -> ActiveChannelMonitorPluginReviewer {
    let display_name = deployment
        .artifact
        .manifest
        .get("displayName")
        .and_then(serde_json::Value::as_str)
        .filter(|value| valid_text(value, 128))
        .unwrap_or(&deployment.plugin_id)
        .to_owned();
    ActiveChannelMonitorPluginReviewer {
        plugin_id: deployment.plugin_id.clone(),
        display_name,
        plugin_version: deployment.artifact.plugin_version.clone(),
    }
}

fn review_input(
    events: &[ActiveChannelMonitorEvent],
) -> Result<Value, ActiveChannelMonitorPluginReviewError> {
    let projected = events
        .iter()
        .map(|event| {
            json!({
                "kind": match event.kind {
                    ActiveChannelMonitorEventKind::Down => "down",
                    ActiveChannelMonitorEventKind::Recovered => "recovered",
                },
                "latency_ms": event.latency_ms,
                "occurred_epoch_seconds": event.occurred_epoch_seconds,
            })
        })
        .collect::<Vec<_>>();
    let input = json!({"events": projected});
    if serde_json::to_vec(&input)
        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
        .len()
        > MAX_REVIEW_INPUT_BYTES as usize
    {
        return Err(ActiveChannelMonitorPluginReviewError::InvalidInput);
    }
    Ok(input)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewOutput {
    summary: String,
    checks: Vec<ActiveChannelMonitorPluginReviewCheck>,
}

fn decode_review(output: Value) -> Result<ReviewOutput, ActiveChannelMonitorPluginReviewError> {
    if serde_json::to_vec(&output)
        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
        .len()
        > MAX_REVIEW_OUTPUT_BYTES as usize
    {
        return Err(ActiveChannelMonitorPluginReviewError::Unavailable);
    }
    let parsed: ReviewOutput = serde_json::from_value(output)
        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?;
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
        return Err(ActiveChannelMonitorPluginReviewError::Unavailable);
    }
    Ok(parsed)
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_projection_omits_all_target_and_channel_identifiers() {
        let event = ActiveChannelMonitorEvent {
            id: 101,
            target_id: 202,
            kind: ActiveChannelMonitorEventKind::Down,
            latency_ms: None,
            occurred_epoch_seconds: 1_700_000_000,
        };
        let value = review_input(&[event]).unwrap();
        let text = value.to_string();
        assert!(text.contains("down"));
        assert!(!text.contains("101"));
        assert!(!text.contains("202"));
        assert!(!text.contains("target_id"));
        assert!(!text.contains("channel_id"));
        assert!(!text.contains("credential_id"));
        assert!(!text.contains("model"));
    }

    #[test]
    fn monitor_review_output_rejects_duplicate_codes_and_unknown_fields() {
        assert!(decode_review(json!({
            "summary":"ok",
            "checks":[
                {"code":"same","title":"one","status":"pass","message":"ok","recommendation":null},
                {"code":"same","title":"two","status":"info","message":"ok","recommendation":null}
            ]
        })).is_err());
        assert!(decode_review(json!({"summary":"ok","checks":[],"extra":"no"})).is_err());
    }
}
