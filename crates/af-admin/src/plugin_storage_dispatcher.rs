use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};

use af_db::{PluginStorageRepository, PluginStorageRepositoryError};
use af_plugin_contract::{
    PluginStorageEntry, PluginStorageOperation, PluginStorageRequest, PluginStorageResponse,
    PluginStorageResult, PluginStorageValue,
};
use async_trait::async_trait;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

const PLUGIN_STORAGE_PURGE_INTERVAL_SECONDS: i64 = 60;

/// Adapter between the Sidecar storage callback and the shared host repository.
/// The host chooses user scope from the authenticated route, not from plugin data.
pub struct DatabasePluginStorageDispatcher {
    repository: Arc<PluginStorageRepository>,
    last_purge_at: AtomicI64,
    purge_lock: Mutex<()>,
}

impl DatabasePluginStorageDispatcher {
    #[must_use]
    pub fn new(repository: Arc<PluginStorageRepository>) -> Self {
        Self {
            repository,
            last_purge_at: AtomicI64::new(0),
            purge_lock: Mutex::new(()),
        }
    }

    pub async fn cleanup_expired_storage(&self) -> Result<u64, PluginStorageRepositoryError> {
        let now = time::OffsetDateTime::now_utc().unix_timestamp().max(1);
        self.repository.purge_expired(now, 100).await
    }
}

#[async_trait]
impl af_plugin_runtime::PluginStorageDispatcher for DatabasePluginStorageDispatcher {
    async fn dispatch(
        &self,
        request: PluginStorageRequest,
        owner_user_id: Option<i64>,
    ) -> PluginStorageResponse {
        let now = time::OffsetDateTime::now_utc().unix_timestamp().max(1);
        let plugin_namespace = storage_namespace(request.plugin_id.as_str(), owner_user_id);
        let plugin_id = plugin_namespace.as_str();
        let result = match request.operation {
            PluginStorageOperation::Get { key } => self
                .repository
                .get(plugin_id, &key.key, now)
                .await
                .map(|record| {
                    PluginStorageResult::Value(record.map(|record| {
                        PluginStorageValue {
                            value: record.value,
                            expires_at_unix_ms: record
                                .expires_at
                                .and_then(|value| u64::try_from(value).ok())
                                .map(|value| value.saturating_mul(1_000)),
                        }
                    }))
                }),
            PluginStorageOperation::Put { key, value } => self
                .repository
                .put(
                    plugin_id,
                    &key.key,
                    value.value,
                    expires_at_unix_seconds(value.expires_at_unix_ms),
                    now,
                )
                .await
                .map(|()| PluginStorageResult::Value(None)),
            PluginStorageOperation::Delete { key } => self
                .repository
                .delete(plugin_id, &key.key)
                .await
                .map(|_| PluginStorageResult::Value(None)),
            PluginStorageOperation::List { prefix, limit } => self
                .repository
                .list(plugin_id, prefix.as_deref(), u64::from(limit), now)
                .await
                .map(|records| {
                    PluginStorageResult::Entries(
                        records
                            .into_iter()
                            .map(|record| PluginStorageEntry {
                                key: record.key,
                                value: PluginStorageValue {
                                    value: record.value,
                                    expires_at_unix_ms: record
                                        .expires_at
                                        .and_then(|value| u64::try_from(value).ok())
                                        .map(|value| value.saturating_mul(1_000)),
                                },
                            })
                            .collect(),
                    )
                }),
        };
        let result = result.unwrap_or_else(|error| PluginStorageResult::Error {
            code: storage_error_code(error).to_owned(),
        });
        PluginStorageResponse {
            protocol_version: request.protocol_version,
            plugin_id: request.plugin_id,
            plugin_version: request.plugin_version,
            artifact_sha256: request.artifact_sha256,
            deployment_generation: request.deployment_generation,
            request_id: request.request_id,
            storage_request_id: request.storage_request_id,
            result,
        }
    }

    async fn purge_expired(&self) {
        let now = time::OffsetDateTime::now_utc().unix_timestamp().max(1);
        if now.saturating_sub(self.last_purge_at.load(Ordering::Relaxed))
            < PLUGIN_STORAGE_PURGE_INTERVAL_SECONDS
        {
            return;
        }
        let _guard = self.purge_lock.lock().await;
        let now = time::OffsetDateTime::now_utc().unix_timestamp().max(1);
        if now.saturating_sub(self.last_purge_at.load(Ordering::Relaxed))
            < PLUGIN_STORAGE_PURGE_INTERVAL_SECONDS
        {
            return;
        }
        self.last_purge_at.store(now, Ordering::Relaxed);
        if let Err(error) = self.cleanup_expired_storage().await {
            tracing::warn!(?error, "expired plugin storage cleanup failed");
        }
    }
}

fn storage_namespace(plugin_id: &str, owner_user_id: Option<i64>) -> String {
    let Some(owner_user_id) = owner_user_id else {
        return plugin_id.to_owned();
    };
    let digest = Sha256::digest(
        format!("anyflows:plugin-storage:user:v1:{plugin_id}:{owner_user_id}").as_bytes(),
    );
    format!("{digest:x}")
}

fn expires_at_unix_seconds(expires_at_unix_ms: Option<u64>) -> Option<i64> {
    expires_at_unix_ms.map(|value| {
        // The largest millisecond timestamp still fits in i64 after conversion to seconds.
        value.div_ceil(1_000) as i64
    })
}

fn storage_error_code(error: PluginStorageRepositoryError) -> &'static str {
    match error {
        PluginStorageRepositoryError::InvalidInput => "invalid_key",
        PluginStorageRepositoryError::Query => "storage_unavailable",
        PluginStorageRepositoryError::Timeout => "storage_unavailable",
    }
}

#[cfg(test)]
mod tests {
    use super::{expires_at_unix_seconds, storage_namespace};

    #[test]
    fn millisecond_expiration_rounds_up_to_a_live_second() {
        assert_eq!(expires_at_unix_seconds(None), None);
        assert_eq!(expires_at_unix_seconds(Some(10_000)), Some(10));
        assert_eq!(expires_at_unix_seconds(Some(10_001)), Some(11));
        assert_eq!(
            expires_at_unix_seconds(Some(u64::MAX)),
            Some(u64::MAX.div_ceil(1_000) as i64)
        );
    }

    #[test]
    fn user_storage_namespaces_are_stable_and_do_not_overlap_plugin_storage() {
        let shared = storage_namespace("org.example.profile", None);
        let first_user = storage_namespace("org.example.profile", Some(17));
        let same_user = storage_namespace("org.example.profile", Some(17));
        let second_user = storage_namespace("org.example.profile", Some(18));

        assert_eq!(first_user, same_user);
        assert_ne!(first_user, second_user);
        assert_ne!(first_user, shared);
    }
}
