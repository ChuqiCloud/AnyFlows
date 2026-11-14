//! Administrative runtime observations and explicit uninstall/cache collection.

use af_db::{
    PluginArtifactCleanupRequest, PluginDeploymentWrite, PluginDistributionError,
    PluginRuntimeStatus,
};
use serde::Serialize;

use crate::plugin_lifecycle::current_timestamp;
use crate::{
    AdminPluginLifecycleError as Error, DatabaseAdminPluginLifecycleService, SessionPrincipal,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginRuntimeOverview {
    pub local_node_id: Option<String>,
    pub isolation: String,
    pub nodes: Vec<AdminPluginRuntimeNode>,
    pub states: Vec<AdminPluginRuntimeState>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginRuntimeNode {
    pub node_id: String,
    pub host_label: String,
    pub last_seen_at: i64,
    pub live: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginRuntimeState {
    pub node_id: String,
    pub plugin_id: String,
    pub generation: i64,
    pub digest: String,
    pub status: String,
    pub isolation: String,
    pub error_code: Option<String>,
    pub updated_at: i64,
    pub node_last_seen_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginUninstallResult {
    pub plugin_id: String,
    pub generation: i64,
    pub pending_nodes: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifactCleanup {
    pub artifact_id: i64,
    pub archive_removed: bool,
    pub local_files_removed: bool,
}

impl DatabaseAdminPluginLifecycleService {
    pub async fn runtime_overview(
        &self,
        principal: SessionPrincipal,
    ) -> Result<AdminPluginRuntimeOverview, Error> {
        self.authorize(principal).await?;
        let repository = self.catalog_repository().distribution_repository();
        let runtime = self.capability_runtime();
        if let Some(runtime) = &runtime {
            runtime.heartbeat(self.catalog_repository()).await?;
        }
        let now = current_timestamp()?;
        let nodes = repository
            .list_nodes(now)
            .await
            .map_err(map_distribution_error)?;
        let states = repository
            .list_states(None, now)
            .await
            .map_err(map_distribution_error)?;
        Ok(AdminPluginRuntimeOverview {
            local_node_id: runtime.as_ref().map(|runtime| runtime.node_id().to_owned()),
            isolation: runtime.map_or_else(
                || "unavailable".to_owned(),
                |runtime| runtime.isolation_policy().as_str().to_owned(),
            ),
            nodes: nodes
                .into_iter()
                .map(|node| AdminPluginRuntimeNode {
                    node_id: node.node_id,
                    host_label: node.host_label,
                    last_seen_at: node.last_seen_at,
                    live: node.live,
                })
                .collect(),
            states: states
                .into_iter()
                .map(|state| AdminPluginRuntimeState {
                    node_id: state.node_id,
                    plugin_id: state.plugin_id,
                    generation: state.generation,
                    digest: state.digest,
                    status: state.status.as_str().to_owned(),
                    isolation: state.isolation,
                    error_code: state.error_code.map(|error| error.as_str().to_owned()),
                    updated_at: state.updated_at,
                    node_last_seen_at: state.node_last_seen_at,
                })
                .collect(),
        })
    }

    pub async fn uninstall(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        expected_generation: i64,
        request_id: &str,
    ) -> Result<AdminPluginUninstallResult, Error> {
        const ROUTE: &str = "/admin/plugins/{pluginId}/uninstall";
        self.authorize_mutation(
            principal,
            request_id,
            ROUTE,
            "uninstall",
            "plugin_deployment",
            Some(plugin_id),
        )
        .await?;
        let result = async {
            let current = self
                .catalog_repository()
                .get_deployment(plugin_id)
                .await
                .map_err(crate::plugin_lifecycle::map_catalog_error)?
                .ok_or(Error::NotFound)?;
            let disabled = self
                .catalog_repository()
                .uninstall(PluginDeploymentWrite {
                    plugin_id: plugin_id.to_owned(),
                    artifact_digest: current.artifact.digest.clone(),
                    expected_generation: Some(expected_generation),
                    enabled: false,
                    granted_capabilities: current.granted_capabilities,
                    config: current.config,
                    updated_by: principal.user_id(),
                    updated_at: current_timestamp()?,
                    request_id: request_id.to_owned(),
                })
                .await
                .map_err(crate::plugin_lifecycle::map_catalog_error)?;
            let generation = disabled.generation;
            let distribution = self.catalog_repository().distribution_repository();
            if let Some(runtime) = self.capability_runtime() {
                runtime.heartbeat(self.catalog_repository()).await?;
                runtime
                    .stop_through_generation(plugin_id, generation)
                    .await?;
                runtime.reconcile_cleanup(self.catalog_repository()).await?;
            }
            let now = current_timestamp()?;
            let nodes = distribution
                .live_nodes(now)
                .await
                .map_err(map_distribution_error)?;
            let states = distribution
                .list_states(Some(plugin_id), now)
                .await
                .map_err(map_distribution_error)?;
            let pending = nodes
                .iter()
                .filter(|node| {
                    !states.iter().any(|state| {
                        state.node_id == node.node_id
                            && state.generation == generation
                            && state.status == PluginRuntimeStatus::Stopped
                    })
                })
                .count();
            Ok(AdminPluginUninstallResult {
                plugin_id: plugin_id.to_owned(),
                generation,
                pending_nodes: u32::try_from(pending).map_err(|_| Error::Internal)?,
            })
        }
        .await;
        self.finish_mutation(
            principal,
            request_id,
            ROUTE,
            "uninstall",
            "plugin_deployment",
            Some(plugin_id.to_owned()),
            None,
            None,
            result,
        )
        .await
    }

    pub async fn cleanup_artifact(
        &self,
        principal: SessionPrincipal,
        artifact_id: i64,
        request_id: &str,
    ) -> Result<AdminPluginArtifactCleanup, Error> {
        const ROUTE: &str = "/admin/plugins/artifacts/{artifactId}/cleanup";
        let resource = artifact_id.to_string();
        self.authorize_mutation(
            principal,
            request_id,
            ROUTE,
            "cleanup",
            "plugin_artifact",
            Some(&resource),
        )
        .await?;
        let result = async {
            if artifact_id <= 0 {
                return Err(Error::InvalidInput);
            }
            let repository = self.catalog_repository();
            let artifact = repository
                .get_artifact(artifact_id)
                .await
                .map_err(|_| Error::Internal)?
                .ok_or(Error::NotFound)?;
            if artifact.revocation.is_none() {
                return Err(Error::Conflict);
            }
            let distribution = repository.distribution_repository();
            distribution
                .record_cleanup_request(PluginArtifactCleanupRequest {
                    artifact_id,
                    requested_by: principal.user_id(),
                    requested_at: current_timestamp()?,
                    request_id: request_id.to_owned(),
                })
                .await
                .map_err(map_distribution_error)?;
            let archive_removed = distribution
                .remove_package(artifact_id)
                .await
                .map_err(map_distribution_error)?;
            let local_files_removed = match self.capability_runtime() {
                Some(runtime) => runtime.cleanup_materialized(repository, &artifact).await?,
                None => false,
            };
            Ok(AdminPluginArtifactCleanup {
                artifact_id,
                archive_removed,
                local_files_removed,
            })
        }
        .await;
        self.finish_mutation(
            principal,
            request_id,
            ROUTE,
            "cleanup",
            "plugin_artifact",
            Some(resource),
            None,
            None,
            result,
        )
        .await
    }
}

pub(crate) const fn map_distribution_error(error: PluginDistributionError) -> Error {
    match error {
        PluginDistributionError::InvalidInput => Error::InvalidInput,
        PluginDistributionError::NotFound => Error::NotFound,
        PluginDistributionError::Conflict => Error::Conflict,
        PluginDistributionError::Invariant
        | PluginDistributionError::Unavailable
        | PluginDistributionError::LimitReached => Error::Internal,
    }
}
