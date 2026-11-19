use std::sync::Arc;

use af_db::{PluginStorageRepository, PluginStorageRepositoryError};
use af_plugin_contract::{
    PluginStorageEntry, PluginStorageOperation, PluginStorageRequest, PluginStorageResponse,
    PluginStorageResult, PluginStorageValue,
};
use async_trait::async_trait;

/// Adapter between the Sidecar storage callback and the shared host repository.
/// The request's plugin ID is the only namespace selector; callers cannot
/// choose another plugin or access a business table.
pub struct DatabasePluginStorageDispatcher {
    repository: Arc<PluginStorageRepository>,
}

impl DatabasePluginStorageDispatcher {
    #[must_use]
    pub fn new(repository: Arc<PluginStorageRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl af_plugin_runtime::PluginStorageDispatcher for DatabasePluginStorageDispatcher {
    async fn dispatch(&self, request: PluginStorageRequest) -> PluginStorageResponse {
        let now = time::OffsetDateTime::now_utc().unix_timestamp().max(1);
        let plugin_id = request.plugin_id.as_str();
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
                    value
                        .expires_at_unix_ms
                        .and_then(|value| i64::try_from(value / 1_000).ok()),
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
}

fn storage_error_code(error: PluginStorageRepositoryError) -> &'static str {
    match error {
        PluginStorageRepositoryError::InvalidInput => "invalid_key",
        PluginStorageRepositoryError::Query => "storage_unavailable",
        PluginStorageRepositoryError::Timeout => "storage_unavailable",
    }
}
