//! Shared execution boundary for host-owned, read-only plugin reviews.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use af_db::{
    PluginArtifactSourceKind, PluginCatalogError, PluginDeploymentQuery, PluginDeploymentRecord,
};
use af_plugin_contract::{
    CapabilityAccess, InvocationMetadata, PluginId, PluginManifest, SideEffectClass,
};
use af_plugin_runtime::CapabilityExecutionContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AdminPluginLifecycleError, DatabaseAdminPluginLifecycleService};

const MAX_REVIEWER_DEPLOYMENTS: u32 = 100;
const MAX_REVIEWER_SCAN_PAGES: usize = 11;
const MAX_REVIEW_TEXT_BYTES: usize = 2_048;
pub(crate) const MAX_REVIEW_CHECKS: usize = 64;

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ReadOnlyPluginReviewOutput {
    pub summary: String,
    pub checks: Vec<ReadOnlyPluginReviewCheck>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadOnlyPluginReviewer {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadOnlyPluginReviewResult {
    pub reviewer: ReadOnlyPluginReviewer,
    pub summary: String,
    pub checks: Vec<ReadOnlyPluginReviewCheck>,
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
    if request_id.is_empty() || request_id.len() > 64 || !valid_contract(contract) {
        return Err(ReadOnlyPluginReviewError::InvalidInput);
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
            .invoke_capability(
                repository,
                plugin_id,
                deployment.generation,
                &context,
                contract.capability_id,
                contract.capability_version,
                input,
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
            summary: output.summary,
            checks: output.checks,
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
}

fn valid_contract(contract: &ReadOnlyPluginReviewContract<'_>) -> bool {
    !contract.capability_id.is_empty()
        && contract.capability_id.len() <= 128
        && contract.capability_version > 0
        && contract.input_schema.is_object()
        && contract.output_schema.is_object()
        && contract.max_input_bytes > 0
        && contract.max_output_bytes > 0
        && contract
            .public_config
            .as_object()
            .is_some_and(|object| object.is_empty())
}

fn reviewer(deployment: &PluginDeploymentRecord) -> ReadOnlyPluginReviewer {
    let display_name = deployment
        .artifact
        .manifest
        .get("displayName")
        .and_then(Value::as_str)
        .filter(|value| valid_text(value, 128))
        .unwrap_or(&deployment.plugin_id)
        .to_owned();
    ReadOnlyPluginReviewer {
        plugin_id: deployment.plugin_id.clone(),
        display_name,
        plugin_version: deployment.artifact.plugin_version.clone(),
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
    Ok(parsed)
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
