//! Shared execution boundary for host-owned, read-only plugin reviews.

use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use af_db::{
    PluginArtifactSourceKind, PluginCatalogError, PluginCatalogRepository, PluginDeploymentQuery,
    PluginDeploymentRecord,
};
use af_plugin_contract::{
    CapabilityAccess, HostActionProposal, InvocationMetadata, PluginId, PluginManifest,
    PluginProxyError, PluginProxyErrorCode, PluginProxyRequest, PluginProxyResponse,
    PluginProxyResult, SideEffectClass,
};
use af_plugin_runtime::{CapabilityExecutionContext, PluginProxyDispatcher};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AdminPluginLifecycleError, DatabaseAdminPluginLifecycleService};

const MAX_REVIEWER_DEPLOYMENTS: u32 = 100;
const MAX_REVIEWER_SCAN_PAGES: usize = 11;
const MAX_REVIEW_TEXT_BYTES: usize = 2_048;
pub(crate) const MAX_REVIEW_CHECKS: usize = 64;
const LIVE_EXTERNAL_ACCOUNT_REVIEW_CAPABILITY: &str = "account.balance.snapshot.live-review.read";
const EXTERNAL_ACCOUNT_PROXY_CAPABILITIES: [&str; 2] = [
    "external.account.profile.read",
    "external.account.balance.read",
];

/// Rechecks durable deployment and capability state before and after every
/// host callback. A deployment revoked during the upstream request cannot
/// receive the resulting account projection.
struct DeploymentGuardedProxyDispatcher {
    repository: PluginCatalogRepository,
    inner: Arc<dyn PluginProxyDispatcher>,
    expected_plugin_id: String,
    expected_artifact_id: i64,
    expected_digest: String,
    expected_generation: i64,
    expected_request_id: String,
    expected_config: Value,
    expected_manifest: PluginManifest,
    parent_capability_id: String,
    parent_capability_version: u16,
}

#[async_trait::async_trait]
impl PluginProxyDispatcher for DeploymentGuardedProxyDispatcher {
    async fn dispatch(&self, request: PluginProxyRequest) -> PluginProxyResponse {
        if !self.request_matches_expected_deployment(&request)
            || !self.deployment_is_currently_authorized(&request).await
        {
            return proxy_denied(request);
        }

        let response = self.inner.dispatch(request.clone()).await;
        if !response.matches_request(&request)
            || !self.deployment_is_currently_authorized(&request).await
        {
            return proxy_denied(request);
        }
        response
    }
}

impl DeploymentGuardedProxyDispatcher {
    fn request_matches_expected_deployment(&self, request: &PluginProxyRequest) -> bool {
        request.validate().is_ok()
            && request.plugin_id.as_str() == self.expected_plugin_id
            && request.artifact_sha256 == self.expected_digest
            && i64::try_from(request.deployment_generation).ok() == Some(self.expected_generation)
            && request.request_id == self.expected_request_id
            && EXTERNAL_ACCOUNT_PROXY_CAPABILITIES.contains(&request.capability_id.as_str())
            && request.capability_version == 1
    }

    async fn deployment_is_currently_authorized(&self, request: &PluginProxyRequest) -> bool {
        let Ok(Some(current)) = self
            .repository
            .get_deployment(&self.expected_plugin_id)
            .await
        else {
            return false;
        };
        current.plugin_id == self.expected_plugin_id
            && current.generation == self.expected_generation
            && current.artifact.id == self.expected_artifact_id
            && current.artifact.digest == self.expected_digest
            && current.artifact.approval.is_some()
            && current.artifact.revocation.is_none()
            && current.artifact.source_kind == PluginArtifactSourceKind::LocalProcess
            && current.enabled
            && current.config == self.expected_config
            && serde_json::from_value::<PluginManifest>(current.artifact.manifest)
                .is_ok_and(|manifest| manifest == self.expected_manifest)
            && has_exact_grant(
                &current.granted_capabilities,
                &self.parent_capability_id,
                self.parent_capability_version,
            )
            && has_exact_grant(
                &current.granted_capabilities,
                request.capability_id.as_str(),
                request.capability_version,
            )
            && self
                .expected_manifest
                .capabilities
                .iter()
                .any(|capability| {
                    capability.id == request.capability_id
                        && capability.version == request.capability_version
                        && capability.access == CapabilityAccess::Read
                        && capability.side_effect == SideEffectClass::ExternalRead
                })
    }
}

fn has_exact_grant(grants: &[af_db::PluginCapabilityGrant], id: &str, version: u16) -> bool {
    grants
        .iter()
        .any(|grant| grant.id == id && grant.version == version)
}

fn proxy_denied(request: PluginProxyRequest) -> PluginProxyResponse {
    PluginProxyResponse {
        protocol_version: request.protocol_version,
        plugin_id: request.plugin_id,
        plugin_version: request.plugin_version,
        artifact_sha256: request.artifact_sha256,
        deployment_generation: request.deployment_generation,
        request_id: request.request_id,
        capability_id: request.capability_id,
        capability_version: request.capability_version,
        proxy_request_id: request.proxy_request_id,
        result: PluginProxyResult::Error(PluginProxyError {
            code: PluginProxyErrorCode::Forbidden,
        }),
    }
}

/// Error classification shared by module-specific review service adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ReadOnlyPluginReviewError {
    #[error("plugin review input is invalid")]
    InvalidInput,
    #[error("plugin review target or deployment was not found")]
    NotFound,
    #[error("plugin is not available for this review capability")]
    Forbidden,
    #[error("plugin deployment changed during review")]
    Conflict,
    #[error("plugin review is unavailable")]
    Unavailable,
}

/// Fixed contract supplied by the host module; this does not register a capability.
pub(crate) struct ReadOnlyPluginReviewContract<'a> {
    pub capability_id: &'a str,
    pub capability_version: u16,
    pub input_schema: &'a Value,
    pub output_schema: &'a Value,
    pub public_config: &'a Value,
    pub max_input_bytes: u32,
    pub max_output_bytes: u32,
    pub max_checks: usize,
    pub budget: Duration,
    pub action_proposal_capability: Option<&'a str>,
}

/// The shared summary/check response schema used by host-owned read-only reviews.
pub(crate) fn read_only_review_output_schema() -> Value {
    serde_json::json!({
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

pub(crate) fn read_only_review_output_schema_with_action() -> Value {
    let mut schema = read_only_review_output_schema();
    schema["properties"]["actionProposal"] = serde_json::json!({
        "type":"object","additionalProperties":false,
        "required":["actionId","actionVersion","input"],
        "properties":{
            "actionId":{"const":"active-channel-monitor.target.set-enabled"},
            "actionVersion":{"const":1},
            "input":{"type":"object","additionalProperties":false,"required":["enabled"],"properties":{"enabled":{"type":"boolean"}}}
        }
    });
    schema
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReadOnlyPluginReviewStatus {
    Pass,
    Info,
    Warning,
    Fail,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadOnlyPluginReviewCheck {
    pub code: String,
    pub title: String,
    pub status: ReadOnlyPluginReviewStatus,
    pub message: String,
    pub recommendation: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct ReadOnlyPluginReviewOutput {
    pub summary: String,
    pub checks: Vec<ReadOnlyPluginReviewCheck>,
    #[serde(rename = "actionProposal", default)]
    pub action_proposal: Option<HostActionProposal>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadOnlyPluginReviewer {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub supports_external_account_reads: bool,
    pub ui_views: Vec<af_plugin_contract::PluginUiView>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReadOnlyPluginReviewResult {
    pub reviewer: ReadOnlyPluginReviewer,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub summary: String,
    pub checks: Vec<ReadOnlyPluginReviewCheck>,
    pub action_proposal: Option<HostActionProposal>,
}

/// Enumerate only deployments that satisfy the module's complete fixed contract.
pub(crate) async fn list_read_only_reviewers(
    plugins: &DatabaseAdminPluginLifecycleService,
    contract: &ReadOnlyPluginReviewContract<'_>,
    budget: Duration,
) -> Result<Vec<ReadOnlyPluginReviewer>, ReadOnlyPluginReviewError> {
    if !valid_contract(contract) {
        return Err(ReadOnlyPluginReviewError::InvalidInput);
    }
    if plugins.capability_runtime().is_none() {
        return Ok(Vec::new());
    }

    tokio::time::timeout(budget, async {
        let repository = plugins.catalog_repository();
        let mut cursor = None;
        let mut reviewers = Vec::new();
        for page_index in 0..MAX_REVIEWER_SCAN_PAGES {
            let deployments = repository
                .list_deployments(
                    PluginDeploymentQuery::new(cursor.clone(), MAX_REVIEWER_DEPLOYMENTS)
                        .map_err(map_catalog_error)?,
                )
                .await
                .map_err(map_catalog_error)?;
            let page_is_full = deployments.len() == MAX_REVIEWER_DEPLOYMENTS as usize;
            if let Some(last) = deployments.last() {
                cursor = Some(last.plugin_id.clone());
            }
            reviewers.extend(
                deployments
                    .iter()
                    .filter(|deployment| satisfies_contract(deployment, contract))
                    .map(reviewer),
            );
            if reviewers.len() >= MAX_REVIEWER_DEPLOYMENTS as usize {
                reviewers.truncate(MAX_REVIEWER_DEPLOYMENTS as usize);
                break;
            }
            if !page_is_full {
                break;
            }
            if page_index + 1 == MAX_REVIEWER_SCAN_PAGES {
                return Err(ReadOnlyPluginReviewError::Unavailable);
            }
        }
        Ok(reviewers)
    })
    .await
    .map_err(|_| ReadOnlyPluginReviewError::Unavailable)?
}

/// Invoke one explicitly selected deployment, validate its output, then recheck that
/// the same deployment generation still has the exact capability grant.
pub(crate) async fn invoke_read_only_review(
    plugins: &DatabaseAdminPluginLifecycleService,
    contract: &ReadOnlyPluginReviewContract<'_>,
    plugin_id: &str,
    request_id: &str,
    input: Value,
) -> Result<ReadOnlyPluginReviewResult, ReadOnlyPluginReviewError> {
    invoke_read_only_review_with_proxy(plugins, contract, plugin_id, request_id, input, None).await
}

/// Invoke a fixed module capability, optionally with a per-request host-managed
/// external read dispatcher. Ordinary review callers do not receive one.
pub(crate) async fn invoke_read_only_review_with_proxy(
    plugins: &DatabaseAdminPluginLifecycleService,
    contract: &ReadOnlyPluginReviewContract<'_>,
    plugin_id: &str,
    request_id: &str,
    input: Value,
    proxy_dispatcher: Option<Arc<dyn PluginProxyDispatcher>>,
) -> Result<ReadOnlyPluginReviewResult, ReadOnlyPluginReviewError> {
    if request_id.is_empty() || request_id.len() > 64 || !valid_contract(contract) {
        return Err(ReadOnlyPluginReviewError::InvalidInput);
    }
    if proxy_dispatcher.is_some()
        && contract.capability_id != LIVE_EXTERNAL_ACCOUNT_REVIEW_CAPABILITY
    {
        return Err(ReadOnlyPluginReviewError::Forbidden);
    }
    PluginId::new(plugin_id.to_owned()).map_err(|_| ReadOnlyPluginReviewError::InvalidInput)?;
    let input_bytes =
        serde_json::to_vec(&input).map_err(|_| ReadOnlyPluginReviewError::Unavailable)?;
    if input_bytes.len() > contract.max_input_bytes as usize {
        return Err(ReadOnlyPluginReviewError::InvalidInput);
    }

    tokio::time::timeout(contract.budget, async {
        let repository = plugins.catalog_repository();
        let deployment = repository
            .get_deployment(plugin_id)
            .await
            .map_err(map_catalog_error)?
            .ok_or(ReadOnlyPluginReviewError::NotFound)?;
        if !satisfies_contract(&deployment, contract) {
            return Err(ReadOnlyPluginReviewError::Forbidden);
        }
        let runtime = plugins
            .capability_runtime()
            .ok_or(ReadOnlyPluginReviewError::Unavailable)?;
        let proxy_dispatcher = if let Some(inner) = proxy_dispatcher {
            let manifest = serde_json::from_value::<PluginManifest>(
                deployment.artifact.manifest.clone(),
            )
            .map_err(|_| ReadOnlyPluginReviewError::Forbidden)?;
            Some(Arc::new(DeploymentGuardedProxyDispatcher {
                repository: repository.clone(),
                inner,
                expected_plugin_id: deployment.plugin_id.clone(),
                expected_artifact_id: deployment.artifact.id,
                expected_digest: deployment.artifact.digest.clone(),
                expected_generation: deployment.generation,
                expected_request_id: request_id.to_owned(),
                expected_config: deployment.config.clone(),
                expected_manifest: manifest,
                parent_capability_id: contract.capability_id.to_owned(),
                parent_capability_version: contract.capability_version,
            }) as Arc<dyn PluginProxyDispatcher>)
        } else {
            None
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ReadOnlyPluginReviewError::Unavailable)?;
        let deadline_unix_ms = u64::try_from(now.as_millis())
            .ok()
            .and_then(|now| now.checked_add(u64::try_from(contract.budget.as_millis()).ok()?))
            .ok_or(ReadOnlyPluginReviewError::Unavailable)?;
        let context = CapabilityExecutionContext::new(
            format!("readonly-review:{}", deployment.artifact.digest),
            InvocationMetadata {
                request_id: request_id.to_owned(),
                deadline_unix_ms,
            },
            contract.public_config.clone(),
            contract.max_input_bytes.max(contract.max_output_bytes),
        )
        .map_err(|_| ReadOnlyPluginReviewError::InvalidInput)?;
        let started_at = Instant::now();
        let response = runtime
            .invoke_capability_with_proxy(
                repository,
                plugin_id,
                deployment.generation,
                &context,
                contract.capability_id,
                contract.capability_version,
                input,
                proxy_dispatcher,
            )
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(
                    target: "anyflows_plugin",
                    plugin_id,
                    capability_id = contract.capability_id,
                    capability_version = contract.capability_version,
                    deployment_generation = deployment.generation,
                    request_id,
                    outcome = "failed",
                    failure_kind = ?error,
                    elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "plugin read-only capability invocation failed"
                );
                return Err(map_lifecycle_error(error));
            }
        };
        let output = match decode_output(response.output, contract) {
            Ok(output) => output,
            Err(error) => {
                tracing::warn!(
                    target: "anyflows_plugin",
                    plugin_id,
                    capability_id = contract.capability_id,
                    capability_version = contract.capability_version,
                    deployment_generation = deployment.generation,
                    request_id,
                    outcome = "invalid_output",
                    elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
                    "plugin read-only capability returned invalid output"
                );
                return Err(error);
            }
        };
        if output.action_proposal.is_some()
            && !contract.action_proposal_capability.is_some_and(|capability_id| {
                satisfies_action_proposal_grant(&deployment, capability_id)
            })
        {
            return Err(ReadOnlyPluginReviewError::Forbidden);
        }
        tracing::info!(
            target: "anyflows_plugin",
            plugin_id,
            capability_id = contract.capability_id,
            capability_version = contract.capability_version,
            deployment_generation = deployment.generation,
            request_id,
            outcome = "success",
            elapsed_ms = u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            "plugin read-only capability invocation completed"
        );

        let latest = repository
            .get_deployment(plugin_id)
            .await
            .map_err(map_catalog_error)?
            .ok_or(ReadOnlyPluginReviewError::NotFound)?;
        if latest.generation != deployment.generation || !satisfies_contract(&latest, contract) {
            return Err(ReadOnlyPluginReviewError::Conflict);
        }

        Ok(ReadOnlyPluginReviewResult {
            reviewer: reviewer(&deployment),
            artifact_id: deployment.artifact.id,
            artifact_digest: deployment.artifact.digest.clone(),
            deployment_generation: deployment.generation,
            summary: output.summary,
            checks: output.checks,
            action_proposal: output.action_proposal,
        })
    })
    .await
    .map_err(|_| ReadOnlyPluginReviewError::Unavailable)?
}

fn satisfies_contract(
    deployment: &PluginDeploymentRecord,
    contract: &ReadOnlyPluginReviewContract<'_>,
) -> bool {
    if !deployment.enabled
        || deployment.artifact.approval.is_none()
        || deployment.artifact.revocation.is_some()
        || deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess
        || &deployment.config != contract.public_config
        || !deployment.granted_capabilities.iter().any(|grant| {
            grant.id == contract.capability_id && grant.version == contract.capability_version
        })
    {
        return false;
    }
    let Ok(manifest) =
        serde_json::from_value::<PluginManifest>(deployment.artifact.manifest.clone())
    else {
        return false;
    };
    manifest.limits.max_response_bytes >= contract.max_input_bytes.max(contract.max_output_bytes)
        && manifest.capabilities.iter().any(|capability| {
            capability.id.as_str() == contract.capability_id
                && capability.version == contract.capability_version
                && capability.access == CapabilityAccess::Read
                && capability.side_effect == SideEffectClass::None
                && capability.input_schema.as_ref() == Some(contract.input_schema)
                && capability.output_schema.as_ref() == Some(contract.output_schema)
        })
        && contract
            .action_proposal_capability
            .is_none_or(|action_capability| {
                deployment
                    .granted_capabilities
                    .iter()
                    .any(|grant| grant.id == action_capability && grant.version == 1)
                    && manifest.capabilities.iter().any(|capability| {
                        capability.id.as_str() == action_capability
                            && capability.version == 1
                            && capability.access == CapabilityAccess::Read
                            && capability.side_effect == SideEffectClass::None
                    })
            })
}

fn valid_contract(contract: &ReadOnlyPluginReviewContract<'_>) -> bool {
    !contract.capability_id.is_empty()
        && contract.capability_id.len() <= 128
        && contract.capability_version > 0
        && contract.input_schema.is_object()
        && contract.output_schema.is_object()
        && contract
            .action_proposal_capability
            .is_none_or(|capability| !capability.is_empty() && capability.len() <= 128)
        && contract.max_input_bytes > 0
        && contract.max_output_bytes > 0
        && contract
            .public_config
            .as_object()
            .is_some_and(|object| object.is_empty())
}

fn reviewer(deployment: &PluginDeploymentRecord) -> ReadOnlyPluginReviewer {
    let manifest =
        serde_json::from_value::<PluginManifest>(deployment.artifact.manifest.clone()).ok();
    let display_name = manifest
        .as_ref()
        .map(|manifest| manifest.display_name.as_str())
        .filter(|value| valid_text(value, 128))
        .unwrap_or(&deployment.plugin_id)
        .to_owned();
    let declares_live_review = manifest.as_ref().is_some_and(|manifest| {
        manifest.capabilities.iter().any(|capability| {
            capability.id.as_str() == LIVE_EXTERNAL_ACCOUNT_REVIEW_CAPABILITY
                && capability.version == 1
                && capability.access == CapabilityAccess::Read
                && capability.side_effect == SideEffectClass::None
        })
    });
    let proxy_grant = EXTERNAL_ACCOUNT_PROXY_CAPABILITIES
        .iter()
        .any(|capability_id| {
            deployment.granted_capabilities.iter().any(|grant| {
                grant.id == *capability_id
                    && grant.version == 1
                    && manifest.as_ref().is_some_and(|manifest| {
                        manifest.capabilities.iter().any(|capability| {
                            capability.id.as_str() == *capability_id
                                && capability.version == 1
                                && capability.access == CapabilityAccess::Read
                                && capability.side_effect == SideEffectClass::ExternalRead
                        })
                    })
            })
        });
    ReadOnlyPluginReviewer {
        plugin_id: deployment.plugin_id.clone(),
        display_name,
        plugin_version: deployment.artifact.plugin_version.clone(),
        supports_external_account_reads: declares_live_review && proxy_grant,
        ui_views: manifest.map_or_else(Vec::new, |manifest| manifest.ui_views),
    }
}

fn decode_output(
    output: Value,
    contract: &ReadOnlyPluginReviewContract<'_>,
) -> Result<ReadOnlyPluginReviewOutput, ReadOnlyPluginReviewError> {
    if serde_json::to_vec(&output)
        .map_err(|_| ReadOnlyPluginReviewError::Unavailable)?
        .len()
        > contract.max_output_bytes as usize
    {
        return Err(ReadOnlyPluginReviewError::Unavailable);
    }
    let parsed: ReadOnlyPluginReviewOutput =
        serde_json::from_value(output).map_err(|_| ReadOnlyPluginReviewError::Unavailable)?;
    let mut codes = std::collections::BTreeSet::new();
    if !valid_text(&parsed.summary, MAX_REVIEW_TEXT_BYTES)
        || parsed.checks.len() > contract.max_checks
        || parsed.checks.iter().any(|check| {
            !valid_text(&check.code, 64)
                || !codes.insert(check.code.as_str())
                || !valid_text(&check.title, 128)
                || !valid_text(&check.message, MAX_REVIEW_TEXT_BYTES)
                || check
                    .recommendation
                    .as_deref()
                    .is_some_and(|value| !valid_text(value, MAX_REVIEW_TEXT_BYTES))
        })
    {
        return Err(ReadOnlyPluginReviewError::Unavailable);
    }
    if parsed.action_proposal.as_ref().is_some_and(|proposal| {
        contract.action_proposal_capability.is_none() || !valid_action_proposal(proposal)
    }) {
        return Err(ReadOnlyPluginReviewError::Unavailable);
    }
    Ok(parsed)
}

fn valid_action_proposal(proposal: &HostActionProposal) -> bool {
    proposal.action_id.as_str() == "active-channel-monitor.target.set-enabled"
        && proposal.action_version == 1
        && proposal.input.as_object().is_some_and(|input| {
            input.len() == 1 && input.get("enabled").and_then(Value::as_bool).is_some()
        })
}

fn satisfies_action_proposal_grant(
    deployment: &PluginDeploymentRecord,
    capability_id: &str,
) -> bool {
    deployment
        .granted_capabilities
        .iter()
        .any(|grant| grant.id == capability_id && grant.version == 1)
        && serde_json::from_value::<PluginManifest>(deployment.artifact.manifest.clone()).is_ok_and(
            |manifest| {
                manifest.capabilities.iter().any(|capability| {
                    capability.id.as_str() == capability_id
                        && capability.version == 1
                        && capability.access == CapabilityAccess::Read
                        && capability.side_effect == SideEffectClass::None
                })
            },
        )
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn map_catalog_error(error: PluginCatalogError) -> ReadOnlyPluginReviewError {
    match error {
        PluginCatalogError::InvalidInput => ReadOnlyPluginReviewError::InvalidInput,
        PluginCatalogError::NotFound => ReadOnlyPluginReviewError::NotFound,
        PluginCatalogError::Conflict => ReadOnlyPluginReviewError::Conflict,
        PluginCatalogError::Unavailable
        | PluginCatalogError::Timeout
        | PluginCatalogError::Invariant => ReadOnlyPluginReviewError::Unavailable,
    }
}

fn map_lifecycle_error(error: AdminPluginLifecycleError) -> ReadOnlyPluginReviewError {
    match error {
        AdminPluginLifecycleError::Forbidden => ReadOnlyPluginReviewError::Forbidden,
        AdminPluginLifecycleError::InvalidInput => ReadOnlyPluginReviewError::InvalidInput,
        AdminPluginLifecycleError::NotFound => ReadOnlyPluginReviewError::NotFound,
        AdminPluginLifecycleError::Conflict => ReadOnlyPluginReviewError::Conflict,
        AdminPluginLifecycleError::CapabilityReviewRequired
        | AdminPluginLifecycleError::PayloadTooLarge
        | AdminPluginLifecycleError::RuntimeUnavailable
        | AdminPluginLifecycleError::Internal => ReadOnlyPluginReviewError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn contract<'a>(
        input_schema: &'a Value,
        output_schema: &'a Value,
        public_config: &'a Value,
    ) -> ReadOnlyPluginReviewContract<'a> {
        ReadOnlyPluginReviewContract {
            capability_id: "example.review.read",
            capability_version: 1,
            input_schema,
            output_schema,
            public_config,
            max_input_bytes: 64 * 1024,
            max_output_bytes: 16 * 1024,
            max_checks: 64,
            budget: Duration::from_secs(5),
            action_proposal_capability: None,
        }
    }

    #[test]
    fn review_output_rejects_duplicate_codes_and_unknown_fields() {
        let input_schema = json!({});
        let output_schema = json!({});
        let public_config = json!({});
        let contract = contract(&input_schema, &output_schema, &public_config);
        assert!(decode_output(
            json!({
                "summary":"ok",
                "checks":[
                    {"code":"same","title":"one","status":"pass","message":"ok","recommendation":null},
                    {"code":"same","title":"two","status":"info","message":"ok","recommendation":null}
                ]
            }),
            &contract,
        )
        .is_err());
        assert!(
            decode_output(json!({"summary":"ok","checks":[],"extra":true}), &contract).is_err()
        );
    }

    #[test]
    fn review_output_rejects_unbounded_or_control_text() {
        let input_schema = json!({});
        let output_schema = json!({});
        let public_config = json!({});
        let contract = contract(&input_schema, &output_schema, &public_config);
        assert!(decode_output(json!({"summary":"bad\nsummary","checks":[]}), &contract,).is_err());
        assert!(decode_output(
            json!({"summary":"ok","checks":[{"code":"x","title":"title","status":"warning","message":"ok","recommendation":" "}]}),
            &contract,
        )
        .is_err());
    }
}
