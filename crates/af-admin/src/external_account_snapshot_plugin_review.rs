//! User-selected, read-only analysis of locally stored external balance snapshots.

use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use af_plugin_contract::PluginId;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::Semaphore;

use crate::{
    DatabaseAdminPluginLifecycleService, ExternalAccountBalanceSnapshot,
    ExternalAccountBalanceStatus, ExternalAccountError, SessionPrincipal,
    UserExternalAccountService,
    readonly_plugin_review::{
        ReadOnlyPluginReviewCheck, ReadOnlyPluginReviewContract, ReadOnlyPluginReviewError,
        ReadOnlyPluginReviewStatus, invoke_read_only_review, list_read_only_reviewers,
        read_only_review_output_schema,
    },
};

const REVIEW_CAPABILITY: &str = "account.balance.snapshot.review.read";
const REVIEW_BUDGET: Duration = Duration::from_secs(5);
const MAX_SNAPSHOTS: u32 = 20;
const MAX_REVIEW_INPUT_BYTES: u32 = 16 * 1024;

pub type ExternalAccountSnapshotPluginReviewFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ExternalAccountSnapshotPluginReviewError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ExternalAccountSnapshotPluginReviewError {
    #[error("request is invalid")]
    InvalidInput,
    #[error("linked account or plugin was not found")]
    NotFound,
    #[error("no stored balance snapshots are available")]
    NoSnapshots,
    #[error("plugin is not enabled for this analysis")]
    Forbidden,
    #[error("account or plugin changed during analysis")]
    Conflict,
    #[error("snapshot analysis is unavailable")]
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccountSnapshotPluginReviewer {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccountSnapshotPluginReviewerList {
    pub plugins: Vec<ExternalAccountSnapshotPluginReviewer>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExternalAccountSnapshotPluginReviewStatus {
    Pass,
    Info,
    Warning,
    Fail,
}

impl From<ReadOnlyPluginReviewStatus> for ExternalAccountSnapshotPluginReviewStatus {
    fn from(value: ReadOnlyPluginReviewStatus) -> Self {
        match value {
            ReadOnlyPluginReviewStatus::Pass => Self::Pass,
            ReadOnlyPluginReviewStatus::Info => Self::Info,
            ReadOnlyPluginReviewStatus::Warning => Self::Warning,
            ReadOnlyPluginReviewStatus::Fail => Self::Fail,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccountSnapshotPluginReviewCheck {
    pub code: String,
    pub title: String,
    pub status: ExternalAccountSnapshotPluginReviewStatus,
    pub message: String,
    pub recommendation: Option<String>,
}

impl From<ReadOnlyPluginReviewCheck> for ExternalAccountSnapshotPluginReviewCheck {
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAccountSnapshotPluginReview {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub snapshot_count: u32,
    pub first_observed_at: i64,
    pub last_observed_at: i64,
    pub summary: String,
    pub checks: Vec<ExternalAccountSnapshotPluginReviewCheck>,
}

/// A session-scoped service. Account ownership is enforced by the existing account service for
/// every snapshot read; this layer never refreshes a connector or receives its stored secret.
pub trait ExternalAccountSnapshotPluginReviewService: Send + Sync {
    fn reviewers(
        &self,
        principal: SessionPrincipal,
    ) -> ExternalAccountSnapshotPluginReviewFuture<'_, ExternalAccountSnapshotPluginReviewerList>;

    fn review_snapshots(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> ExternalAccountSnapshotPluginReviewFuture<'_, ExternalAccountSnapshotPluginReview>;
}

pub struct DatabaseExternalAccountSnapshotPluginReviewService {
    accounts: Arc<dyn UserExternalAccountService>,
    plugins: Arc<DatabaseAdminPluginLifecycleService>,
    slots: Semaphore,
}

impl DatabaseExternalAccountSnapshotPluginReviewService {
    #[must_use]
    pub fn new(
        accounts: Arc<dyn UserExternalAccountService>,
        plugins: Arc<DatabaseAdminPluginLifecycleService>,
    ) -> Self {
        Self {
            accounts,
            plugins,
            slots: Semaphore::new(4),
        }
    }
}

impl ExternalAccountSnapshotPluginReviewService
    for DatabaseExternalAccountSnapshotPluginReviewService
{
    fn reviewers(
        &self,
        _principal: SessionPrincipal,
    ) -> ExternalAccountSnapshotPluginReviewFuture<'_, ExternalAccountSnapshotPluginReviewerList>
    {
        Box::pin(async move {
            let input_schema = snapshot_input_schema();
            let output_schema = read_only_review_output_schema();
            let public_config = json!({});
            let contract = review_contract(&input_schema, &output_schema, &public_config);
            let plugins = list_read_only_reviewers(&self.plugins, &contract, REVIEW_BUDGET)
                .await
                .map_err(map_review_error)?
                .into_iter()
                .map(|plugin| ExternalAccountSnapshotPluginReviewer {
                    plugin_id: plugin.plugin_id,
                    display_name: plugin.display_name,
                    plugin_version: plugin.plugin_version,
                })
                .collect();
            Ok(ExternalAccountSnapshotPluginReviewerList { plugins })
        })
    }

    fn review_snapshots(
        &self,
        principal: SessionPrincipal,
        connection_id: i64,
        plugin_id: String,
        request_id: String,
    ) -> ExternalAccountSnapshotPluginReviewFuture<'_, ExternalAccountSnapshotPluginReview> {
        Box::pin(async move {
            if connection_id <= 0 || !valid_request_id(&request_id) {
                return Err(ExternalAccountSnapshotPluginReviewError::InvalidInput);
            }
            PluginId::new(plugin_id.clone())
                .map_err(|_| ExternalAccountSnapshotPluginReviewError::InvalidInput)?;
            let _permit = self
                .slots
                .try_acquire()
                .map_err(|_| ExternalAccountSnapshotPluginReviewError::Unavailable)?;

            tokio::time::timeout(REVIEW_BUDGET, async {
                let history = self
                    .accounts
                    .balance_history(principal, connection_id, MAX_SNAPSHOTS)
                    .await
                    .map_err(map_account_error)?;
                if history.connection_id != connection_id {
                    return Err(ExternalAccountSnapshotPluginReviewError::NotFound);
                }
                if history.items.is_empty() {
                    return Err(ExternalAccountSnapshotPluginReviewError::NoSnapshots);
                }

                let mut snapshots = history.items;
                snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.observed_at));
                let first_observed_at = snapshots
                    .iter()
                    .map(|snapshot| snapshot.observed_at)
                    .min()
                    .ok_or(ExternalAccountSnapshotPluginReviewError::NoSnapshots)?;
                let last_observed_at = snapshots
                    .iter()
                    .map(|snapshot| snapshot.observed_at)
                    .max()
                    .ok_or(ExternalAccountSnapshotPluginReviewError::NoSnapshots)?;
                let snapshot_count = u32::try_from(snapshots.len())
                    .map_err(|_| ExternalAccountSnapshotPluginReviewError::Unavailable)?;
                let input = json!({
                    // Deliberately omits the AnyFlows connection id, owner, provider subject,
                    // provider URL, credentials, and any stable external account identifier.
                    "snapshots": snapshots.iter().map(snapshot_projection).collect::<Vec<_>>(),
                });
                let input_schema = snapshot_input_schema();
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

                // Recheck ownership after plugin work so an unlink during the call prevents a
                // response from escaping the request. This is a local snapshot read only.
                let latest = self
                    .accounts
                    .balance_history(principal, connection_id, 1)
                    .await
                    .map_err(map_account_error)?;
                if latest.connection_id != connection_id {
                    return Err(ExternalAccountSnapshotPluginReviewError::NotFound);
                }

                Ok(ExternalAccountSnapshotPluginReview {
                    plugin_id: output.reviewer.plugin_id,
                    display_name: output.reviewer.display_name,
                    plugin_version: output.reviewer.plugin_version,
                    snapshot_count,
                    first_observed_at,
                    last_observed_at,
                    summary: output.summary,
                    checks: output.checks.into_iter().map(Into::into).collect(),
                })
            })
            .await
            .map_err(|_| ExternalAccountSnapshotPluginReviewError::Unavailable)?
        })
    }
}

fn snapshot_projection(snapshot: &ExternalAccountBalanceSnapshot) -> Value {
    let status = match snapshot.status {
        ExternalAccountBalanceStatus::Available => "available",
        ExternalAccountBalanceStatus::Unavailable => "unavailable",
        ExternalAccountBalanceStatus::AuthenticationRequired => "authentication_required",
    };
    json!({
        "amount": snapshot.amount.as_deref(),
        "unit": snapshot.unit.as_deref(),
        "status": status,
        "observedAt": snapshot.observed_at,
    })
}

fn snapshot_input_schema() -> Value {
    json!({
        "type":"object",
        "additionalProperties":false,
        "required":["snapshots"],
        "properties":{
            "snapshots":{
                "type":"array","minItems":1,"maxItems":MAX_SNAPSHOTS,
                "items":{
                    "type":"object","additionalProperties":false,
                    "required":["amount","unit","status","observedAt"],
                    "properties":{
                        "amount":{"type":["string","null"],"maxLength":128},
                        "unit":{"type":["string","null"],"maxLength":64},
                        "status":{"type":"string","enum":["available","unavailable","authentication_required"]},
                        "observedAt":{"type":"integer","minimum":1}
                    }
                }
            }
        }
    })
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
        max_checks: 64,
        budget: REVIEW_BUDGET,
        action_proposal_capability: None,
    }
}

fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn map_review_error(error: ReadOnlyPluginReviewError) -> ExternalAccountSnapshotPluginReviewError {
    match error {
        ReadOnlyPluginReviewError::InvalidInput => {
            ExternalAccountSnapshotPluginReviewError::InvalidInput
        }
        ReadOnlyPluginReviewError::NotFound => ExternalAccountSnapshotPluginReviewError::NotFound,
        ReadOnlyPluginReviewError::Forbidden => ExternalAccountSnapshotPluginReviewError::Forbidden,
        ReadOnlyPluginReviewError::Conflict => ExternalAccountSnapshotPluginReviewError::Conflict,
        ReadOnlyPluginReviewError::Unavailable => {
            ExternalAccountSnapshotPluginReviewError::Unavailable
        }
    }
}

fn map_account_error(error: ExternalAccountError) -> ExternalAccountSnapshotPluginReviewError {
    match error {
        ExternalAccountError::InvalidInput => {
            ExternalAccountSnapshotPluginReviewError::InvalidInput
        }
        ExternalAccountError::NotFound => ExternalAccountSnapshotPluginReviewError::NotFound,
        ExternalAccountError::Unsupported => ExternalAccountSnapshotPluginReviewError::Forbidden,
        ExternalAccountError::Conflict => ExternalAccountSnapshotPluginReviewError::Conflict,
        ExternalAccountError::AuthenticationRequired
        | ExternalAccountError::Unavailable
        | ExternalAccountError::RateLimited
        | ExternalAccountError::LimitReached
        | ExternalAccountError::Internal => ExternalAccountSnapshotPluginReviewError::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_contains_only_balance_values_and_time() {
        let snapshot = ExternalAccountBalanceSnapshot {
            amount: Some("12.3400".to_owned()),
            unit: Some("USD".to_owned()),
            status: ExternalAccountBalanceStatus::Available,
            observed_at: 1_700_000_000,
        };
        let projected = snapshot_projection(&snapshot);
        assert_eq!(
            projected,
            json!({
                "amount":"12.3400",
                "unit":"USD",
                "status":"available",
                "observedAt":1_700_000_000_i64,
            })
        );
        assert!(projected.get("connectionId").is_none());
        assert!(projected.get("externalSubject").is_none());
        assert!(projected.get("baseUrl").is_none());
        assert!(projected.get("credential").is_none());
    }

    #[test]
    fn request_ids_are_bounded_opaque_tokens() {
        assert!(valid_request_id("snapshot-review-123"));
        assert!(!valid_request_id(" contains space"));
        assert!(!valid_request_id(&"x".repeat(65)));
        assert!(!valid_request_id("token/secret"));
    }
}
