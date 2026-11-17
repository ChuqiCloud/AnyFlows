//! Read-only plugin analysis of already-collected active channel monitor events.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    ActiveChannelMonitorEvent, ActiveChannelMonitorEventKind, ActiveChannelMonitorService,
    ActiveChannelMonitorServiceError, DatabaseAdminPluginLifecycleService,
    readonly_plugin_review::{
        MAX_REVIEW_CHECKS, ReadOnlyPluginReviewContract, ReadOnlyPluginReviewError,
        invoke_read_only_review, list_read_only_reviewers, read_only_review_output_schema,
    },
};

const REVIEW_CAPABILITY: &str = "channel.monitor.events.review.read";
const REVIEW_BUDGET: Duration = Duration::from_secs(5);
const MAX_REVIEW_INPUT_BYTES: u32 = 64 * 1024;
const MAX_REVIEW_OUTPUT_BYTES: u32 = 16 * 1024;
const MAX_REVIEW_EVENTS: usize = 100;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveChannelMonitorPluginReviewStatus {
    Pass,
    Info,
    Warning,
    Fail,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveChannelMonitorPluginReviewCheck {
    pub code: String,
    pub title: String,
    pub status: ActiveChannelMonitorPluginReviewStatus,
    pub message: String,
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
            let input_schema = review_input_schema();
            let output_schema = read_only_review_output_schema();
            let public_config = json!({});
            let contract = review_contract(&input_schema, &output_schema, &public_config);
            let plugins = list_read_only_reviewers(&self.plugins, &contract, REVIEW_BUDGET)
                .await
                .map_err(map_review_error)?
                .into_iter()
                .map(|plugin| ActiveChannelMonitorPluginReviewer {
                    plugin_id: plugin.plugin_id,
                    display_name: plugin.display_name,
                    plugin_version: plugin.plugin_version,
                })
                .collect();
            Ok(ActiveChannelMonitorPluginReviewerList { plugins })
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
                let input_schema = review_input_schema();
                let output_schema = read_only_review_output_schema();
                let public_config = json!({});
                let contract = review_contract(&input_schema, &output_schema, &public_config);
                let result = invoke_read_only_review(
                    &self.plugins,
                    &contract,
                    &plugin_id,
                    &request_id,
                    input,
                )
                .await
                .map_err(map_review_error)?;

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
                Ok(ActiveChannelMonitorPluginReview {
                    plugin_id: result.reviewer.plugin_id,
                    display_name: result.reviewer.display_name,
                    plugin_version: result.reviewer.plugin_version,
                    event_count: u32::try_from(events.len())
                        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?,
                    summary: result.summary,
                    checks: result.checks.into_iter().map(map_review_check).collect(),
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

fn map_review_error(error: ReadOnlyPluginReviewError) -> ActiveChannelMonitorPluginReviewError {
    match error {
        ReadOnlyPluginReviewError::Forbidden => ActiveChannelMonitorPluginReviewError::Forbidden,
        ReadOnlyPluginReviewError::InvalidInput => {
            ActiveChannelMonitorPluginReviewError::InvalidInput
        }
        ReadOnlyPluginReviewError::NotFound => ActiveChannelMonitorPluginReviewError::NotFound,
        ReadOnlyPluginReviewError::Conflict => ActiveChannelMonitorPluginReviewError::Conflict,
        ReadOnlyPluginReviewError::Unavailable => {
            ActiveChannelMonitorPluginReviewError::Unavailable
        }
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
    }
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

fn map_review_check(
    check: crate::readonly_plugin_review::ReadOnlyPluginReviewCheck,
) -> ActiveChannelMonitorPluginReviewCheck {
    ActiveChannelMonitorPluginReviewCheck {
        code: check.code,
        title: check.title,
        status: match check.status {
            crate::readonly_plugin_review::ReadOnlyPluginReviewStatus::Pass => {
                ActiveChannelMonitorPluginReviewStatus::Pass
            }
            crate::readonly_plugin_review::ReadOnlyPluginReviewStatus::Info => {
                ActiveChannelMonitorPluginReviewStatus::Info
            }
            crate::readonly_plugin_review::ReadOnlyPluginReviewStatus::Warning => {
                ActiveChannelMonitorPluginReviewStatus::Warning
            }
            crate::readonly_plugin_review::ReadOnlyPluginReviewStatus::Fail => {
                ActiveChannelMonitorPluginReviewStatus::Fail
            }
        },
        message: check.message,
        recommendation: check.recommendation,
    }
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
}
