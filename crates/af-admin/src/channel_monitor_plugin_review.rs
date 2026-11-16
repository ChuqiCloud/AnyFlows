//! Read-only plugin analysis of already-collected active channel monitor events.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use af_db::{
    PluginActionProposalCreate, PluginActionProposalRecord, PluginActionProposalRepository,
    PluginActionProposalRepositoryError, PluginActionProposalStatus, PluginArtifactSourceKind,
    PluginDeploymentRecord,
};
use af_domain::PlatformPermission;
use af_plugin_contract::HostActionProposal;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    ActiveChannelMonitorEvent, ActiveChannelMonitorEventKind, ActiveChannelMonitorService,
    ActiveChannelMonitorServiceError, DatabaseAdminPluginLifecycleService, PlatformAuditOutcome,
    PlatformPolicy, SessionPrincipal,
    readonly_plugin_review::{
        MAX_REVIEW_CHECKS, ReadOnlyPluginReviewContract, ReadOnlyPluginReviewError,
        invoke_read_only_review, list_read_only_reviewers, read_only_review_output_schema,
        read_only_review_output_schema_with_action,
    },
};

const REVIEW_CAPABILITY: &str = "channel.monitor.events.review.read";
const ACTION_PROPOSAL_CAPABILITY: &str = "channel.monitor.target.action-propose.read";
const ACTION_PROPOSAL_CAPABILITY_VERSION: u16 = 1;
const ACTION_PROPOSAL_TTL_SECONDS: i64 = 5 * 60;
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
    pub target_config_version: i64,
    pub summary: String,
    pub checks: Vec<ActiveChannelMonitorPluginReviewCheck>,
    pub ui_views: Vec<af_plugin_contract::PluginUiView>,
    pub action_proposal: Option<HostActionProposal>,
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

    fn propose_target_action(
        &self,
        principal: SessionPrincipal,
        target_id: i64,
        plugin_id: String,
        expected_target_version: i64,
        idempotency_key: String,
        request_id: String,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, PluginActionProposalRecord>;

    fn confirm_target_action(
        &self,
        principal: SessionPrincipal,
        proposal_key: String,
        confirmation_key: String,
        request_id: String,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, PluginActionProposalRecord>;
}

/// The host owns event access and sends only the event facts needed for analysis.
pub struct DatabaseActiveChannelMonitorPluginReviewService {
    monitor: Arc<dyn ActiveChannelMonitorService>,
    plugins: Arc<DatabaseAdminPluginLifecycleService>,
    action_proposals: PluginActionProposalRepository,
    slots: Semaphore,
}

impl DatabaseActiveChannelMonitorPluginReviewService {
    #[must_use]
    pub fn new(
        monitor: Arc<dyn ActiveChannelMonitorService>,
        plugins: Arc<DatabaseAdminPluginLifecycleService>,
        action_proposals: PluginActionProposalRepository,
    ) -> Self {
        Self {
            monitor,
            plugins,
            action_proposals,
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
            let contract = review_contract(&input_schema, &output_schema, &public_config, false);
            let mut plugins = list_read_only_reviewers(&self.plugins, &contract, REVIEW_BUDGET)
                .await
                .map_err(map_review_error)?
                .into_iter()
                .map(|plugin| ActiveChannelMonitorPluginReviewer {
                    plugin_id: plugin.plugin_id,
                    display_name: plugin.display_name,
                    plugin_version: plugin.plugin_version,
                })
                .collect::<Vec<_>>();
            let action_output = read_only_review_output_schema_with_action();
            let action_contract =
                review_contract(&input_schema, &action_output, &public_config, true);
            for plugin in list_read_only_reviewers(&self.plugins, &action_contract, REVIEW_BUDGET)
                .await
                .map_err(map_review_error)?
            {
                if !plugins
                    .iter()
                    .any(|existing| existing.plugin_id == plugin.plugin_id)
                {
                    plugins.push(ActiveChannelMonitorPluginReviewer {
                        plugin_id: plugin.plugin_id,
                        display_name: plugin.display_name,
                        plugin_version: plugin.plugin_version,
                    });
                }
            }
            plugins.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
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
                // Bind the recommendation to the target's administrator-controlled version.
                let target = self
                    .monitor
                    .get_target(target_id)
                    .await
                    .map_err(map_monitor_error)?
                    .ok_or(ActiveChannelMonitorPluginReviewError::NotFound)?;
                let events = self
                    .monitor
                    .events(target_id)
                    .await
                    .map_err(map_monitor_error)?;
                if events.len() > MAX_REVIEW_EVENTS {
                    return Err(ActiveChannelMonitorPluginReviewError::Unavailable);
                }
                let input = review_input(&events, target.enabled)?;
                let input_schema = review_input_schema();
                let action_enabled = authorized_action_deployment(&self.plugins, &plugin_id)
                    .await
                    .is_ok();
                let output_schema = if action_enabled {
                    read_only_review_output_schema_with_action()
                } else {
                    read_only_review_output_schema()
                };
                let public_config = json!({});
                let contract = review_contract(
                    &input_schema,
                    &output_schema,
                    &public_config,
                    action_enabled,
                );
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
                let latest_target = self
                    .monitor
                    .get_target(target_id)
                    .await
                    .map_err(map_monitor_error)?
                    .ok_or(ActiveChannelMonitorPluginReviewError::NotFound)?;
                if latest_target.config_version != target.config_version
                    || latest_target.enabled != target.enabled
                {
                    return Err(ActiveChannelMonitorPluginReviewError::Conflict);
                }
                Ok(ActiveChannelMonitorPluginReview {
                    plugin_id: result.reviewer.plugin_id,
                    display_name: result.reviewer.display_name,
                    plugin_version: result.reviewer.plugin_version,
                    event_count: u32::try_from(events.len())
                        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?,
                    target_config_version: target.config_version,
                    summary: result.summary,
                    checks: result.checks.into_iter().map(map_review_check).collect(),
                    ui_views: result
                        .reviewer
                        .ui_views
                        .into_iter()
                        .filter(|view| {
                            view.slot == af_plugin_contract::PluginUiViewSlot::ChannelMonitor
                        })
                        .collect(),
                    action_proposal: result.action_proposal,
                })
            })
            .await
            .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
        })
    }

    fn propose_target_action(
        &self,
        principal: SessionPrincipal,
        target_id: i64,
        plugin_id: String,
        expected_target_version: i64,
        idempotency_key: String,
        request_id: String,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, PluginActionProposalRecord> {
        Box::pin(async move {
            PlatformPolicy::authorize(principal, PlatformPermission::PluginsManage)
                .map_err(|_| ActiveChannelMonitorPluginReviewError::Forbidden)?;
            if target_id <= 0
                || expected_target_version <= 0
                || !valid_request_id(&request_id)
                || !valid_request_id(&idempotency_key)
            {
                return Err(ActiveChannelMonitorPluginReviewError::InvalidInput);
            }
            let review = self
                .review_events(target_id, plugin_id.clone(), request_id.clone())
                .await?;
            if review.target_config_version != expected_target_version {
                return Err(ActiveChannelMonitorPluginReviewError::Conflict);
            }
            let proposal = review
                .action_proposal
                .ok_or(ActiveChannelMonitorPluginReviewError::Conflict)?;
            let input_schema = action_input_schema();
            af_plugin_runtime::validate_plugin_configuration(&input_schema, &proposal.input)
                .map_err(|_| ActiveChannelMonitorPluginReviewError::InvalidInput)?;
            let requested_enabled = proposal.input["enabled"]
                .as_bool()
                .ok_or(ActiveChannelMonitorPluginReviewError::InvalidInput)?;
            let deployment = authorized_action_deployment(&self.plugins, &plugin_id).await?;
            let target = self
                .monitor
                .get_target(target_id)
                .await
                .map_err(map_monitor_error)?
                .ok_or(ActiveChannelMonitorPluginReviewError::NotFound)?;
            if target.config_version != expected_target_version
                || target.enabled == requested_enabled
            {
                return Err(ActiveChannelMonitorPluginReviewError::Conflict);
            }
            let now = action_now()?;
            let create = PluginActionProposalCreate {
                proposal_key: uuid::Uuid::new_v4().simple().to_string(),
                idempotency_key,
                requested_by_user_id: principal.user_id().get(),
                plugin_id,
                artifact_id: deployment.artifact.id,
                artifact_digest: deployment.artifact.digest,
                deployment_generation: deployment.generation,
                capability_version: ACTION_PROPOSAL_CAPABILITY_VERSION,
                action_version: proposal.action_version,
                input_json: serde_json::to_string(&proposal)
                    .map_err(|_| ActiveChannelMonitorPluginReviewError::InvalidInput)?,
                target_id,
                expected_target_version,
                expected_enabled: target.enabled,
                requested_enabled,
                created_at: now,
                expires_at: now.saturating_add(ACTION_PROPOSAL_TTL_SECONDS),
            };
            let (record, created) = self
                .action_proposals
                .create(create)
                .await
                .map_err(map_action_repository_error)?;
            if created {
                let before =
                    json!({"enabled":target.enabled,"configVersion":target.config_version});
                let after = json!({"status":"pending","enabled":requested_enabled});
                if self
                    .plugins
                    .audit_host_action(
                        principal,
                        &request_id,
                        "propose_plugin_action",
                        &record.proposal_key,
                        PlatformAuditOutcome::Succeeded,
                        Some(before),
                        Some(after),
                    )
                    .await
                    .is_err()
                {
                    tracing::warn!(
                        proposal_id = record.id,
                        "plugin action proposal audit could not be recorded"
                    );
                }
            }
            Ok(record)
        })
    }

    fn confirm_target_action(
        &self,
        principal: SessionPrincipal,
        proposal_key: String,
        confirmation_key: String,
        request_id: String,
    ) -> ActiveChannelMonitorPluginReviewFuture<'_, PluginActionProposalRecord> {
        Box::pin(async move {
            PlatformPolicy::authorize(principal, PlatformPermission::PluginsManage)
                .map_err(|_| ActiveChannelMonitorPluginReviewError::Forbidden)?;
            if !valid_action_token(&proposal_key, 36)
                || !valid_request_id(&confirmation_key)
                || !valid_request_id(&request_id)
            {
                return Err(ActiveChannelMonitorPluginReviewError::InvalidInput);
            }
            let before = self
                .action_proposals
                .get_by_key(&proposal_key)
                .await
                .map_err(map_action_repository_error)?;
            let outcome = self
                .action_proposals
                .confirm(
                    &proposal_key,
                    principal.user_id().get(),
                    &confirmation_key,
                    action_now()?,
                )
                .await
                .map_err(map_action_repository_error)?;
            let succeeded = outcome.status == PluginActionProposalStatus::Confirmed;
            let audit_result = self.plugins.audit_host_action(
                principal,
                &request_id,
                "confirm_plugin_action",
                &proposal_key,
                if succeeded { PlatformAuditOutcome::Succeeded } else { PlatformAuditOutcome::Failed },
                Some(json!({"enabled":before.expected_enabled,"configVersion":before.expected_target_version})),
                outcome.outcome.clone(),
            ).await;
            if audit_result.is_err() {
                tracing::warn!(
                    proposal_id = outcome.id,
                    "plugin action confirmation audit could not be recorded"
                );
            }
            Ok(outcome)
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

async fn authorized_action_deployment(
    plugins: &DatabaseAdminPluginLifecycleService,
    plugin_id: &str,
) -> Result<PluginDeploymentRecord, ActiveChannelMonitorPluginReviewError> {
    let deployment = plugins
        .catalog_repository()
        .get_deployment(plugin_id)
        .await
        .map_err(|_| ActiveChannelMonitorPluginReviewError::Unavailable)?
        .ok_or(ActiveChannelMonitorPluginReviewError::NotFound)?;
    let eligible = deployment.enabled
        && deployment.artifact.approval.is_some()
        && deployment.artifact.revocation.is_none()
        && deployment.artifact.source_kind == PluginArtifactSourceKind::LocalProcess
        && deployment.granted_capabilities.iter().any(|grant| {
            grant.id == ACTION_PROPOSAL_CAPABILITY
                && grant.version == ACTION_PROPOSAL_CAPABILITY_VERSION
        })
        && serde_json::from_value::<af_plugin_contract::PluginManifest>(
            deployment.artifact.manifest.clone(),
        )
        .is_ok_and(|manifest| {
            manifest.capabilities.iter().any(|capability| {
                capability.id.as_str() == ACTION_PROPOSAL_CAPABILITY
                    && capability.version == ACTION_PROPOSAL_CAPABILITY_VERSION
                    && capability.access == af_plugin_contract::CapabilityAccess::Read
                    && capability.side_effect == af_plugin_contract::SideEffectClass::None
            })
        });
    if !eligible {
        return Err(ActiveChannelMonitorPluginReviewError::Forbidden);
    }
    Ok(deployment)
}

fn action_input_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["enabled"],
        "properties":{"enabled":{"type":"boolean"}}
    })
}

fn action_now() -> Result<i64, ActiveChannelMonitorPluginReviewError> {
    time::OffsetDateTime::now_utc()
        .unix_timestamp()
        .checked_add(0)
        .filter(|value| *value > 0)
        .ok_or(ActiveChannelMonitorPluginReviewError::Unavailable)
}

fn valid_request_id(value: &str) -> bool {
    valid_action_token(value, 128)
}

fn valid_action_token(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn map_action_repository_error(
    error: PluginActionProposalRepositoryError,
) -> ActiveChannelMonitorPluginReviewError {
    match error {
        PluginActionProposalRepositoryError::Invariant => {
            ActiveChannelMonitorPluginReviewError::InvalidInput
        }
        PluginActionProposalRepositoryError::NotFound => {
            ActiveChannelMonitorPluginReviewError::NotFound
        }
        PluginActionProposalRepositoryError::Conflict => {
            ActiveChannelMonitorPluginReviewError::Conflict
        }
        PluginActionProposalRepositoryError::Query
        | PluginActionProposalRepositoryError::Timeout => {
            ActiveChannelMonitorPluginReviewError::Unavailable
        }
    }
}

fn review_contract<'a>(
    input_schema: &'a Value,
    output_schema: &'a Value,
    public_config: &'a Value,
    action_proposals: bool,
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
        action_proposal_capability: action_proposals.then_some(ACTION_PROPOSAL_CAPABILITY),
    }
}

fn review_input_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["events","targetEnabled"],
        "properties":{
            "targetEnabled":{"type":"boolean"},
            "events":{
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
            }
        }
    })
}

fn review_input(
    events: &[ActiveChannelMonitorEvent],
    target_enabled: bool,
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
    let input = json!({"events": projected, "targetEnabled": target_enabled});
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
        let value = review_input(&[event], true).unwrap();
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
