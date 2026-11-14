//! Host-owned, read-only model discovery from installed catalog plugins.
//!
//! These records are informational. They cannot create routable platform models,
//! price cards, credentials, or billing authority.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use af_db::{PluginDeploymentQuery, PluginDeploymentRecord};
use af_plugin_contract::{InvocationMetadata, PluginId};
use af_plugin_runtime::CapabilityExecutionContext;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Semaphore;

use crate::{AdminPluginLifecycleError, DatabaseAdminPluginLifecycleService, SessionPrincipal};

const CATALOG_READ: &str = "catalog.models.read";
const CATALOG_SEARCH: &str = "catalog.models.search";
const MAX_PAGE_SIZE: u32 = 50;
const MAX_CATALOG_BYTES: u32 = 256 * 1024;
const CALL_BUDGET: Duration = Duration::from_secs(5);
const USER_REFILL_INTERVAL: Duration = Duration::from_millis(500);
const USER_BURST: u32 = 8;
const USER_BUCKET_CAPACITY: Duration = USER_REFILL_INTERVAL.saturating_mul(USER_BURST);
const MAX_TRACKED_USERS: usize = 4_096;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogSource {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub generation: i64,
    pub supports_search: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogSourcePage {
    pub sources: Vec<PluginCatalogSource>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCatalogModel {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub context_window: Option<u32>,
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCatalogModelPage {
    pub source: PluginCatalogSource,
    pub models: Vec<PluginCatalogModel>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PluginCatalogExtensionQuery {
    search: Option<String>,
    cursor: Option<String>,
    limit: u32,
}

impl PluginCatalogExtensionQuery {
    pub fn new(
        search: Option<String>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<Self, AdminPluginLifecycleError> {
        let search = search.filter(|value| !value.is_empty());
        if limit == 0
            || limit > MAX_PAGE_SIZE
            || search
                .as_deref()
                .is_some_and(|value| !valid_text(value, 128))
            || cursor
                .as_deref()
                .is_some_and(|value| !valid_text(value, 2_048))
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self {
            search,
            cursor,
            limit,
        })
    }
}

impl DatabaseAdminPluginLifecycleService {
    /// List only host-enabled, approved public-catalog sources. The caller must
    /// be an authenticated user; no plugin config or storage locator is returned.
    pub async fn catalog_sources(
        &self,
        _principal: SessionPrincipal,
        cursor: Option<String>,
    ) -> Result<PluginCatalogSourcePage, AdminPluginLifecycleError> {
        let query = PluginDeploymentQuery::new(cursor, 100)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let deployments = self
            .catalog_repository()
            .list_deployments(query)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let next_cursor = (deployments.len() == 100)
            .then(|| deployments.last().map(|item| item.plugin_id.clone()))
            .flatten();
        let sources = deployments
            .iter()
            .filter(|deployment| catalog_capability(deployment, false).is_some())
            .map(source)
            .collect();
        Ok(PluginCatalogSourcePage {
            sources,
            next_cursor,
        })
    }

    /// Invoke a fixed model-discovery capability. Plugin output is separately
    /// decoded into this narrow public projection after the runtime's Schema
    /// validation, so a plugin cannot publish prices or request routing rules.
    pub async fn catalog_models(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        query: PluginCatalogExtensionQuery,
        request_id: &str,
    ) -> Result<PluginCatalogModelPage, AdminPluginLifecycleError> {
        PluginId::new(plugin_id).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let _permit = self
            .catalog_admission
            .slots
            .try_acquire()
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        tokio::time::timeout(CALL_BUDGET, async {
            let repository = self.catalog_repository();
            let deployment = repository
                .get_deployment(plugin_id)
                .await
                .map_err(|_| AdminPluginLifecycleError::Internal)?
                .ok_or(AdminPluginLifecycleError::NotFound)?;
            let capability = catalog_capability(&deployment, query.search.is_some())
                .ok_or(AdminPluginLifecycleError::Forbidden)?;
            let mut query = query;
            if let Some(cursor) = &query.cursor {
                query.cursor = Some(decode_cursor(cursor, &deployment, query.search.as_deref())?);
            }
            self.catalog_admission
                .admit_user(principal.user_id().get())
                .await?;
            let declared_page_size = deployment
                .artifact
                .manifest
                .pointer("/limits/maxPageSize")
                .and_then(serde_json::Value::as_u64)
                .and_then(|limit| u32::try_from(limit).ok())
                .filter(|limit| *limit > 0)
                .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
            query.limit = query.limit.min(declared_page_size);
            let runtime = self
                .capability_runtime()
                .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| AdminPluginLifecycleError::Internal)?;
            let deadline_unix_ms = u64::try_from(now.as_millis())
                .ok()
                .and_then(|now| now.checked_add(5_000))
                .ok_or(AdminPluginLifecycleError::Internal)?;
            let context = CapabilityExecutionContext::new(
                format!("public-catalog:{}", deployment.artifact.digest),
                InvocationMetadata {
                    request_id: request_id.to_owned(),
                    deadline_unix_ms,
                },
                json!({}),
                MAX_CATALOG_BYTES,
            )
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            let response = runtime
                .invoke_capability(
                    repository,
                    plugin_id,
                    deployment.generation,
                    &context,
                    capability,
                    1,
                    json!({"query": query.search, "cursor": query.cursor, "limit": query.limit}),
                )
                .await?;
            let parsed = decode_catalog(response.output, &query)?;
            // A concurrent disable, revoke, upgrade, or grant change must take
            // effect before this response is allowed to leave the host.
            let latest = repository
                .get_deployment(plugin_id)
                .await
                .map_err(|_| AdminPluginLifecycleError::Internal)?
                .ok_or(AdminPluginLifecycleError::NotFound)?;
            if latest.generation != deployment.generation
                || catalog_capability(&latest, query.search.is_some()).is_none()
            {
                return Err(AdminPluginLifecycleError::Conflict);
            }
            Ok(PluginCatalogModelPage {
                source: source(&deployment),
                models: parsed.models,
                next_cursor: parsed
                    .next_cursor
                    .map(|cursor| encode_cursor(&deployment, query.search, cursor))
                    .transpose()?,
            })
        })
        .await
        .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?
    }
}

fn catalog_capability(deployment: &PluginDeploymentRecord, search: bool) -> Option<&'static str> {
    if !deployment.enabled
        || deployment.artifact.approval.is_none()
        || deployment.artifact.revocation.is_some()
        || deployment.artifact.source_kind != af_db::PluginArtifactSourceKind::LocalProcess
    {
        return None;
    }
    let has = |id| {
        deployment
            .granted_capabilities
            .iter()
            .any(|grant| grant.id == id && grant.version == 1)
    };
    if search && has(CATALOG_SEARCH) {
        Some(CATALOG_SEARCH)
    } else if has(CATALOG_READ) {
        Some(CATALOG_READ)
    } else if has(CATALOG_SEARCH) {
        Some(CATALOG_SEARCH)
    } else {
        None
    }
}

fn source(deployment: &PluginDeploymentRecord) -> PluginCatalogSource {
    let display_name = deployment
        .artifact
        .manifest
        .get("displayName")
        .and_then(serde_json::Value::as_str)
        .filter(|name| valid_text(name, 128))
        .unwrap_or(&deployment.plugin_id)
        .to_owned();
    PluginCatalogSource {
        plugin_id: deployment.plugin_id.clone(),
        display_name,
        plugin_version: deployment.artifact.plugin_version.clone(),
        generation: deployment.generation,
        supports_search: deployment
            .granted_capabilities
            .iter()
            .any(|grant| grant.id == CATALOG_SEARCH && grant.version == 1),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogOutput {
    models: Vec<PluginCatalogModel>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CatalogCursor {
    version: u8,
    plugin_id: String,
    generation: i64,
    digest: String,
    search: Option<String>,
    cursor: String,
}

fn decode_cursor(
    cursor: &str,
    deployment: &PluginDeploymentRecord,
    search: Option<&str>,
) -> Result<String, AdminPluginLifecycleError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
    let cursor: CatalogCursor =
        serde_json::from_slice(&bytes).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
    if cursor.version != 1 || !valid_text(&cursor.cursor, 512) {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    if cursor.plugin_id != deployment.plugin_id
        || cursor.generation != deployment.generation
        || cursor.digest != deployment.artifact.digest
        || cursor.search.as_deref() != search
    {
        return Err(AdminPluginLifecycleError::Conflict);
    }
    Ok(cursor.cursor)
}

fn encode_cursor(
    deployment: &PluginDeploymentRecord,
    search: Option<String>,
    cursor: String,
) -> Result<String, AdminPluginLifecycleError> {
    let cursor = CatalogCursor {
        version: 1,
        plugin_id: deployment.plugin_id.clone(),
        generation: deployment.generation,
        digest: deployment.artifact.digest.clone(),
        search,
        cursor,
    };
    let bytes = serde_json::to_vec(&cursor).map_err(|_| AdminPluginLifecycleError::Internal)?;
    let encoded = URL_SAFE_NO_PAD.encode(bytes);
    if encoded.len() > 2_048 {
        return Err(AdminPluginLifecycleError::RuntimeUnavailable);
    }
    Ok(encoded)
}

fn decode_catalog(
    output: serde_json::Value,
    query: &PluginCatalogExtensionQuery,
) -> Result<CatalogOutput, AdminPluginLifecycleError> {
    let invalid = || AdminPluginLifecycleError::RuntimeUnavailable;
    if serde_json::to_vec(&output).map_err(|_| invalid())?.len() > MAX_CATALOG_BYTES as usize {
        return Err(invalid());
    }
    let parsed: CatalogOutput = serde_json::from_value(output).map_err(|_| invalid())?;
    let mut ids = BTreeSet::new();
    if parsed.models.len() > query.limit as usize
        || parsed.next_cursor.as_deref().is_some_and(|cursor| {
            !valid_text(cursor, 512) || query.cursor.as_deref() == Some(cursor)
        })
        || parsed.models.iter().any(|model| {
            !valid_text(&model.id, 256)
                || !ids.insert(model.id.as_str())
                || !valid_text(&model.display_name, 128)
                || !valid_text(&model.provider, 64)
                || model
                    .description
                    .as_deref()
                    .is_some_and(|value| !valid_text(value, 4_096))
                || model
                    .context_window
                    .is_some_and(|value| value == 0 || value > i32::MAX as u32)
                || !valid_modalities(&model.input_modalities)
                || !valid_modalities(&model.output_modalities)
                || model.tags.len() > 32
                || model.tags.iter().any(|tag| !valid_text(tag, 64))
                || model.tags.iter().collect::<BTreeSet<_>>().len() != model.tags.len()
        })
    {
        return Err(invalid());
    }
    Ok(parsed)
}

fn valid_modalities(values: &[String]) -> bool {
    values.len() <= 4
        && values
            .iter()
            .all(|value| matches!(value.as_str(), "text" | "image" | "audio" | "video"))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn valid_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(crate) struct PluginCatalogAdmission {
    slots: Semaphore,
    recent: Mutex<BTreeMap<i64, UserCatalogBucket>>,
}

struct UserCatalogBucket {
    available: Duration,
    updated_at: Instant,
}

impl Default for PluginCatalogAdmission {
    fn default() -> Self {
        Self {
            slots: Semaphore::new(8),
            recent: Mutex::new(BTreeMap::new()),
        }
    }
}

impl PluginCatalogAdmission {
    async fn admit_user(&self, user_id: i64) -> Result<(), AdminPluginLifecycleError> {
        loop {
            let wait = self.user_wait_at(user_id, Instant::now())?;
            if wait.is_zero() {
                return Ok(());
            }
            // A paginated refresh may request consecutive pages immediately.
            // Allow a small burst, then pace further calls within CALL_BUDGET.
            // The caller holds one of the eight global slots while waiting.
            tokio::time::sleep(wait).await;
        }
    }

    fn user_wait_at(
        &self,
        user_id: i64,
        now: Instant,
    ) -> Result<Duration, AdminPluginLifecycleError> {
        let mut recent = self
            .recent
            .lock()
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        if recent.len() >= MAX_TRACKED_USERS && !recent.contains_key(&user_id) {
            // Only fully refilled idle buckets can be forgotten. Evicting an
            // exhausted user would let account churn reset its rate limit.
            recent.retain(|_, bucket| {
                bucket
                    .available
                    .saturating_add(now.saturating_duration_since(bucket.updated_at))
                    < USER_BUCKET_CAPACITY
            });
            if recent.len() >= MAX_TRACKED_USERS {
                return Err(AdminPluginLifecycleError::RuntimeUnavailable);
            }
        }
        let bucket = recent.entry(user_id).or_insert(UserCatalogBucket {
            available: USER_BUCKET_CAPACITY,
            updated_at: now,
        });
        bucket.available = bucket
            .available
            .saturating_add(now.saturating_duration_since(bucket.updated_at))
            .min(USER_BUCKET_CAPACITY);
        bucket.updated_at = now;
        if bucket.available >= USER_REFILL_INTERVAL {
            bucket.available -= USER_REFILL_INTERVAL;
            Ok(Duration::ZERO)
        } else {
            Ok(USER_REFILL_INTERVAL - bucket.available)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_admission_is_bounded_per_host_service_not_process_global_user_ids() {
        let first = PluginCatalogAdmission::default();
        let second = PluginCatalogAdmission::default();
        let now = Instant::now();
        for _ in 0..USER_BURST {
            assert_eq!(first.user_wait_at(1, now).unwrap(), Duration::ZERO);
        }
        assert_eq!(first.user_wait_at(1, now).unwrap(), USER_REFILL_INTERVAL);
        assert_eq!(first.user_wait_at(2, now).unwrap(), Duration::ZERO);
        assert_eq!(second.user_wait_at(1, now).unwrap(), Duration::ZERO);
        assert_eq!(first.slots.available_permits(), 8);
    }

    #[test]
    fn pagination_bursts_are_paced_with_exact_partial_refill_and_bounded_idle_credit() {
        let admission = PluginCatalogAdmission::default();
        let now = Instant::now();
        for _ in 0..USER_BURST {
            assert_eq!(admission.user_wait_at(1, now).unwrap(), Duration::ZERO);
        }
        assert_eq!(
            admission.user_wait_at(1, now).unwrap(),
            USER_REFILL_INTERVAL
        );
        assert_eq!(
            admission
                .user_wait_at(1, now + Duration::from_millis(200))
                .unwrap(),
            Duration::from_millis(300)
        );
        assert_eq!(
            admission
                .user_wait_at(1, now + USER_REFILL_INTERVAL)
                .unwrap(),
            Duration::ZERO
        );
        assert_eq!(
            admission
                .user_wait_at(1, now + USER_REFILL_INTERVAL)
                .unwrap(),
            USER_REFILL_INTERVAL
        );
        let later = now + Duration::from_secs(60);
        for _ in 0..USER_BURST {
            assert_eq!(admission.user_wait_at(1, later).unwrap(), Duration::ZERO);
        }
        assert_eq!(
            admission.user_wait_at(1, later).unwrap(),
            USER_REFILL_INTERVAL
        );
    }

    #[test]
    fn tracked_users_are_bounded_and_only_fully_refilled_buckets_are_evicted() {
        let admission = PluginCatalogAdmission::default();
        let now = Instant::now();
        for user_id in 0..i64::try_from(MAX_TRACKED_USERS).unwrap() {
            assert_eq!(
                admission.user_wait_at(user_id, now).unwrap(),
                Duration::ZERO
            );
        }
        let newcomer = i64::try_from(MAX_TRACKED_USERS).unwrap();
        assert!(matches!(
            admission.user_wait_at(newcomer, now),
            Err(AdminPluginLifecycleError::RuntimeUnavailable)
        ));
        for _ in 1..USER_BURST {
            assert_eq!(admission.user_wait_at(0, now).unwrap(), Duration::ZERO);
        }
        assert_eq!(
            admission.user_wait_at(0, now).unwrap(),
            USER_REFILL_INTERVAL
        );
        let partially_refilled = now + USER_REFILL_INTERVAL;
        assert_eq!(
            admission
                .user_wait_at(newcomer, partially_refilled)
                .unwrap(),
            Duration::ZERO
        );
        assert_eq!(admission.recent.lock().unwrap().len(), 2);
        // The exhausted user's bucket survived pruning and has only one token.
        assert_eq!(
            admission.user_wait_at(0, partially_refilled).unwrap(),
            Duration::ZERO
        );
        assert_eq!(
            admission.user_wait_at(0, partially_refilled).unwrap(),
            USER_REFILL_INTERVAL
        );
    }

    fn deployment() -> PluginDeploymentRecord {
        let user = af_domain::UserId::new(1).unwrap();
        PluginDeploymentRecord {
            plugin_id: "org.example.catalog".to_owned(),
            generation: 1,
            artifact: af_db::PluginArtifactRecord {
                id: 1,
                plugin_id: "org.example.catalog".to_owned(),
                plugin_version: "1.0.0".to_owned(),
                digest: "ab".repeat(32),
                manifest: json!({}),
                source_kind: af_db::PluginArtifactSourceKind::LocalProcess,
                source_reference: "fixture".to_owned(),
                signer_key_id: String::new(),
                signature: String::new(),
                created_at: 1,
                approval: Some(af_db::PluginArtifactApproval {
                    artifact_id: 1,
                    approved_by: user,
                    approved_at: 1,
                    request_id: "fixture-approve".to_owned(),
                }),
                revocation: None,
            },
            enabled: true,
            granted_capabilities: vec![af_db::PluginCapabilityGrant {
                id: CATALOG_READ.to_owned(),
                version: 1,
            }],
            config: json!({}),
            updated_by: user,
            updated_at: 1,
            request_id: "fixture-deploy".to_owned(),
        }
    }

    fn model() -> serde_json::Value {
        json!({"id":"model-1","displayName":"Model One","provider":"example","inputModalities":["text"],"outputModalities":["text"]})
    }

    #[test]
    fn catalog_projection_rejects_billing_fields_and_duplicate_ids() {
        let query = PluginCatalogExtensionQuery::new(None, None, 10).unwrap();
        let mut billable = model();
        billable["price"] = json!("0.01");
        assert!(decode_catalog(json!({"models":[billable],"nextCursor":null}), &query).is_err());
        assert!(
            decode_catalog(
                json!({"models":[model(),model()],"nextCursor":null}),
                &query
            )
            .is_err()
        );
        assert!(decode_catalog(json!({"models":[model()],"nextCursor":null}), &query).is_ok());
    }

    #[test]
    fn plugin_output_cannot_exceed_requested_page_or_repeat_cursor() {
        let query = PluginCatalogExtensionQuery::new(None, Some("next".to_owned()), 1).unwrap();
        assert!(
            decode_catalog(
                json!({"models":[model(),model()],"nextCursor":null}),
                &query
            )
            .is_err()
        );
        assert!(decode_catalog(json!({"models":[model()],"nextCursor":"next"}), &query).is_err());
        assert!(PluginCatalogExtensionQuery::new(None, Some("bad\nvalue".to_owned()), 1).is_err());
        assert!(PluginCatalogExtensionQuery::new(None, None, 51).is_err());
    }

    #[test]
    fn cursor_is_bound_to_source_generation_and_search() {
        let mut deployment = deployment();
        let cursor = encode_cursor(
            &deployment,
            Some("query".to_owned()),
            "offset-24".to_owned(),
        )
        .unwrap();
        assert_eq!(
            decode_cursor(&cursor, &deployment, Some("query")).unwrap(),
            "offset-24"
        );
        assert!(matches!(
            decode_cursor(&cursor, &deployment, Some("different")),
            Err(AdminPluginLifecycleError::Conflict)
        ));
        deployment.generation += 1;
        assert!(matches!(
            decode_cursor(&cursor, &deployment, Some("query")),
            Err(AdminPluginLifecycleError::Conflict)
        ));
        assert!(decode_cursor("not-a-cursor", &deployment, None).is_err());
    }
}
