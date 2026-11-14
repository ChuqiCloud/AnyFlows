//! Host-side reconciliation between durable plugin deployments and sidecar processes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    future::Future,
    path::{Component, Path, PathBuf},
    sync::{Arc, Weak},
};

use af_config::PluginTrustedSigner;
use af_db::{
    MAX_PLUGIN_DEPLOYMENT_HISTORY, PluginArtifactRecord, PluginArtifactSourceKind,
    PluginCatalogRepository, PluginDeploymentQuery, PluginDeploymentRecord,
};
use af_plugin_contract::{CapabilityId, CapabilityResult, PluginId, PluginManifest, PluginVersion};
use af_plugin_runtime::{
    CapabilityExecutionContext, PluginArtifactDescriptor, SidecarError, SidecarIsolationPolicy,
    SidecarLimits, SidecarSupervisor, TrustedPluginKey, VerifiedPluginArtifact,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;

use crate::AdminPluginLifecycleError;

/// Runtime owner for administrator-installed local-process plugins.
///
/// The artifact root and publisher keys come from host configuration. Nothing
/// in the database can add a signer or choose an absolute executable path.
pub struct AdminPluginSidecarRuntime {
    pub(crate) artifact_root: PathBuf,
    pub(crate) trusted_signers: BTreeMap<String, TrustedPluginKey>,
    pub(crate) supervisor: Arc<SidecarSupervisor>,
    pub(crate) official_signers: BTreeSet<String>,
    pub(crate) node_id: String,
    pub(crate) host_label: String,
    pub(crate) isolation_policy: SidecarIsolationPolicy,
    pub(crate) cache_operations: tokio::sync::Mutex<()>,
    pub(crate) cleaned_uninstalls: tokio::sync::Mutex<BTreeMap<String, i64>>,
    pub(crate) collected_artifacts: tokio::sync::Mutex<BTreeSet<i64>>,
    application_slots: tokio::sync::Mutex<BTreeMap<String, Weak<tokio::sync::Mutex<()>>>>,
    installation_slots: Arc<tokio::sync::Semaphore>,
}

impl AdminPluginSidecarRuntime {
    /// Create the runtime from already validated application configuration.
    pub fn new(
        artifact_directory: &Path,
        signers: &[PluginTrustedSigner],
    ) -> Result<Self, AdminPluginLifecycleError> {
        if artifact_directory.is_absolute()
            || artifact_directory
                .components()
                .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let mut trusted_signers = BTreeMap::new();
        for signer in signers {
            let bytes = BASE64
                .decode(signer.public_key_base64())
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            let public_key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            let key = TrustedPluginKey::new(signer.key_id().to_owned(), public_key)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            if trusted_signers
                .insert(signer.key_id().to_owned(), key)
                .is_some()
            {
                return Err(AdminPluginLifecycleError::InvalidInput);
            }
        }
        Ok(Self {
            artifact_root: artifact_directory.to_path_buf(),
            trusted_signers,
            supervisor: Arc::new(SidecarSupervisor::default()),
            installation_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            official_signers: BTreeSet::new(),
            node_id: uuid::Uuid::new_v4().to_string(),
            host_label: crate::plugin_distribution_runtime::runtime_host_label(),
            isolation_policy: SidecarIsolationPolicy::Auto,
            cache_operations: tokio::sync::Mutex::new(()),
            cleaned_uninstalls: tokio::sync::Mutex::new(BTreeMap::new()),
            collected_artifacts: tokio::sync::Mutex::new(BTreeSet::new()),
            application_slots: tokio::sync::Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn installation_root(&self) -> Result<PathBuf, AdminPluginLifecycleError> {
        let working_directory = std::env::current_dir()
            .and_then(|directory| directory.canonicalize())
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let mut directory = working_directory.clone();
        for component in self.artifact_root.components() {
            if matches!(component, Component::CurDir) {
                continue;
            }
            directory.push(component.as_os_str());
            match std::fs::symlink_metadata(&directory) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
                Ok(_) => return Err(AdminPluginLifecycleError::InvalidInput),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match std::fs::create_dir(&directory) {
                        Ok(()) => {
                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                std::fs::set_permissions(
                                    &directory,
                                    std::fs::Permissions::from_mode(0o700),
                                )
                                .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                            let metadata = std::fs::symlink_metadata(&directory)
                                .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
                            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                                return Err(AdminPluginLifecycleError::InvalidInput);
                            }
                        }
                        Err(_) => return Err(AdminPluginLifecycleError::RuntimeUnavailable),
                    }
                }
                Err(_) => return Err(AdminPluginLifecycleError::RuntimeUnavailable),
            }
        }
        let directory = directory
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        if !directory.starts_with(&working_directory) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(directory)
    }

    pub(crate) fn acquire_installation_slot(
        &self,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, AdminPluginLifecycleError> {
        Arc::clone(&self.installation_slots)
            .try_acquire_owned()
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)
    }

    pub(crate) fn recognizes_publisher(&self, key_id: &str) -> bool {
        self.trusted_signers.contains_key(key_id)
    }

    /// Verify an administrator-selected package under the host-owned trust root.
    #[allow(clippy::too_many_arguments)]
    pub async fn verify_candidate(
        &self,
        plugin_id: String,
        plugin_version: String,
        manifest: serde_json::Value,
        package_path: String,
        executable_path: String,
        sha256: String,
        signer_key_id: String,
        signature_base64: String,
    ) -> Result<VerifiedPluginArtifact, AdminPluginLifecycleError> {
        let package_relative = safe_posix_relative_path(&package_path)?;
        let executable_relative = safe_posix_relative_path(&executable_path)?;
        let plugin_id =
            PluginId::new(plugin_id).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let plugin_version = PluginVersion::new(plugin_version)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let manifest: PluginManifest = serde_json::from_value(manifest)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        manifest
            .validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if manifest.plugin_id != plugin_id || manifest.plugin_version != plugin_version {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let key = if signer_key_id.is_empty() && signature_base64.is_empty() {
            None
        } else {
            Some(
                self.trusted_signers
                    .get(&signer_key_id)
                    .ok_or(AdminPluginLifecycleError::InvalidInput)?,
            )
        };
        let working_directory = std::env::current_dir()
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let artifact_root = working_directory
            .join(&self.artifact_root)
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        if !artifact_root.is_dir() || !artifact_root.starts_with(&working_directory) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        prepare_staging_parent(&artifact_root)?;
        let package_root = artifact_root
            .join(package_relative)
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if !package_root.starts_with(&artifact_root) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let executable = package_root
            .join(executable_relative)
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if !executable.starts_with(&package_root)
            || !tokio::fs::metadata(&executable)
                .await
                .is_ok_and(|metadata| metadata.is_file())
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let source_reference = serde_json::to_string(&serde_json::json!({
            "packagePath": canonical_relative_path(&artifact_root, &package_root)?,
            "executablePath": canonical_relative_path(&package_root, &executable)?,
        }))
        .map_err(|_| AdminPluginLifecycleError::Internal)?;
        if source_reference.len() > 2048 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let descriptor = PluginArtifactDescriptor::new(
            plugin_id,
            plugin_version,
            manifest,
            executable,
            package_root,
            sha256,
            signer_key_id,
            &signature_base64,
            "local_process".to_owned(),
            source_reference,
        )
        .and_then(|descriptor| descriptor.with_staging_parent(artifact_root.join(".runtime")))
        .map_err(map_sidecar_error)?;
        match key {
            Some(key) => VerifiedPluginArtifact::verify(descriptor, key).await,
            None => VerifiedPluginArtifact::verify_operator_managed(descriptor).await,
        }
        .map_err(map_sidecar_error)
    }

    /// Start a committed enabled generation or stop a committed disabled one.
    pub async fn apply(
        &self,
        repository: &PluginCatalogRepository,
        deployment: &PluginDeploymentRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        // HTTP deployment, periodic reconciliation and a lazy business call can
        // all observe the same missing generation. Serialize the entire apply
        // operation so a losing handshake cannot replace or stop its winner.
        let slot = {
            let mut slots = self.application_slots.lock().await;
            slots.retain(|_, slot| slot.strong_count() > 0);
            match slots.get(&deployment.plugin_id).and_then(Weak::upgrade) {
                Some(slot) => slot,
                None => {
                    let slot = Arc::new(tokio::sync::Mutex::new(()));
                    slots.insert(deployment.plugin_id.clone(), Arc::downgrade(&slot));
                    slot
                }
            }
        };
        let _application_guard = slot.lock().await;
        self.heartbeat(repository).await?;
        let result = self.apply_inner(repository, deployment).await;
        self.record_apply_result(repository, deployment, result.as_ref().err())
            .await?;
        result
    }

    async fn apply_inner(
        &self,
        repository: &PluginCatalogRepository,
        deployment: &PluginDeploymentRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        let plugin_id = PluginId::new(deployment.plugin_id.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if !deployment_is_current(repository, deployment).await? {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        if !deployment.enabled {
            let generation = u64::try_from(deployment.generation)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            self.supervisor
                .stop_through_generation(&plugin_id, generation)
                .await;
            return Ok(());
        }
        if deployment.granted_capabilities.is_empty() {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let grants = deployment
            .granted_capabilities
            .iter()
            .map(|grant| {
                CapabilityId::new(grant.id.clone())
                    .map(|id| (id, grant.version))
                    .map_err(|_| AdminPluginLifecycleError::InvalidInput)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let generation = u64::try_from(deployment.generation)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if let Ok(session) = self.supervisor.session(&plugin_id, generation).await {
            if session.health(generation).await.is_ok_and(|health| {
                health.status == af_plugin_contract::ConnectorHealthStatus::Healthy
            }) {
                return Ok(());
            }
            self.supervisor
                .stop_generation(&plugin_id, generation)
                .await;
        }
        let cache_guard = self.cache_operations.lock().await;
        self.report_state(
            repository,
            deployment,
            af_db::PluginRuntimeStatus::Downloading,
            None,
        )
        .await?;
        self.ensure_materialized(repository, &deployment.artifact)
            .await?;
        self.report_state(
            repository,
            deployment,
            af_db::PluginRuntimeStatus::Starting,
            None,
        )
        .await?;
        let artifact = match self.verify_artifact(&deployment.artifact).await {
            Ok(artifact) => artifact,
            Err(error)
                if matches!(
                    error,
                    AdminPluginLifecycleError::InvalidInput | AdminPluginLifecycleError::NotFound
                ) && self
                    .repair_materialized(repository, &deployment.artifact)
                    .await? =>
            {
                self.verify_artifact(&deployment.artifact).await?
            }
            Err(error) => return Err(error),
        };
        drop(cache_guard);
        artifact
            .validate_configuration(&deployment.config)
            .map_err(map_sidecar_error)?;
        if !deployment_is_current(repository, deployment).await? {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        self.supervisor
            .activate_with_policy(
                artifact,
                generation,
                grants,
                deployment.config.clone(),
                SidecarLimits::default(),
                self.isolation_policy,
            )
            .await
            .map_err(map_sidecar_error)?;
        let current = deployment_is_current(repository, deployment).await;
        ensure_deployment_current_after_activation(current, || async {
            self.supervisor
                .stop_generation(&plugin_id, generation)
                .await;
        })
        .await
    }

    /// Stop an active or failed process no newer than the committed generation.
    pub async fn stop_through_generation(
        &self,
        plugin_id: &str,
        generation: i64,
    ) -> Result<(), AdminPluginLifecycleError> {
        let plugin_id = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let generation =
            u64::try_from(generation).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        self.supervisor
            .stop_through_generation(&plugin_id, generation)
            .await;
        Ok(())
    }

    /// Stop an exact failed process generation without marking that desired
    /// generation disabled; a later reconciliation may retry it.
    pub async fn stop_generation(
        &self,
        plugin_id: &str,
        generation: i64,
    ) -> Result<(), AdminPluginLifecycleError> {
        let plugin_id = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let generation =
            u64::try_from(generation).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        self.supervisor
            .stop_generation(&plugin_id, generation)
            .await;
        Ok(())
    }

    /// Invoke one capability from a trusted host workflow. This deliberately
    /// has no HTTP route; call sites must first authenticate and authorize their
    /// own platform-specific operation.
    pub async fn invoke_capability(
        &self,
        repository: &PluginCatalogRepository,
        plugin_id: &str,
        generation: i64,
        invocation: &CapabilityExecutionContext,
        capability_id: &str,
        capability_version: u16,
        input: serde_json::Value,
    ) -> Result<CapabilityResult, AdminPluginLifecycleError> {
        let remaining = invocation
            .remaining_timeout()
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        tokio::time::timeout(
            remaining,
            self.invoke_capability_inner(
                repository,
                plugin_id,
                generation,
                invocation,
                capability_id,
                capability_version,
                input,
            ),
        )
        .await
        .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?
    }

    async fn invoke_capability_inner(
        &self,
        repository: &PluginCatalogRepository,
        plugin_id: &str,
        generation: i64,
        invocation: &CapabilityExecutionContext,
        capability_id: &str,
        capability_version: u16,
        input: serde_json::Value,
    ) -> Result<CapabilityResult, AdminPluginLifecycleError> {
        let plugin_id = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let generation =
            u64::try_from(generation).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let requested_generation =
            i64::try_from(generation).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let capability_id = CapabilityId::new(capability_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let deployment = repository
            .get_deployment(plugin_id.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !deployment.enabled
            || deployment.generation != requested_generation
            || deployment.artifact.revocation.is_some()
            || deployment.artifact.approval.is_none()
            || deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess
            || !deployment.granted_capabilities.iter().any(|grant| {
                grant.id == capability_id.as_str() && grant.version == capability_version
            })
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let current_session = self.supervisor.session(&plugin_id, generation).await;
        let session_is_running = match current_session {
            Ok(session) => session
                .is_process_running(generation)
                .await
                .unwrap_or(false),
            Err(_) => false,
        };
        if !session_is_running {
            self.apply(repository, &deployment).await?;
        }
        let latest = repository
            .get_deployment(plugin_id.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .ok_or(AdminPluginLifecycleError::NotFound)?;
        if !latest.enabled || latest.generation != requested_generation {
            if latest.enabled {
                self.supervisor
                    .stop_generation(&plugin_id, generation)
                    .await;
            } else {
                self.supervisor
                    .stop_through_generation(
                        &plugin_id,
                        u64::try_from(latest.generation)
                            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
                    )
                    .await;
            }
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let session = self
            .supervisor
            .session(&plugin_id, generation)
            .await
            .map_err(map_sidecar_error)?;
        let invocation = invocation
            .with_public_config(deployment.config)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        session
            .invoke_host_capability(
                generation,
                &invocation,
                &capability_id,
                capability_version,
                input,
            )
            .await
            .map_err(map_sidecar_error)
    }

    /// Restore every enabled desired deployment before the HTTP server accepts
    /// traffic. A bad or unavailable artifact leaves that plugin stopped and
    /// does not prevent unrelated plugins or the core service from starting.
    pub async fn reconcile(&self, repository: &PluginCatalogRepository) {
        if self.heartbeat(repository).await.is_err() {
            return;
        }
        if let Err(error) = self.reconcile_cleanup(repository).await {
            tracing::warn!(error_kind = ?error, "plugin cache cleanup could not be reconciled");
        }
        let mut before_plugin_id = None;
        loop {
            let query = match PluginDeploymentQuery::new(
                before_plugin_id.clone(),
                MAX_PLUGIN_DEPLOYMENT_HISTORY,
            ) {
                Ok(query) => query,
                Err(_) => return,
            };
            let deployments = match repository.list_deployments(query).await {
                Ok(deployments) => deployments,
                Err(error) => {
                    tracing::error!(error_kind = ?error, "plugin deployment reconciliation query failed");
                    return;
                }
            };
            let page_is_full = deployments.len() == MAX_PLUGIN_DEPLOYMENT_HISTORY as usize;
            for listed in deployments {
                before_plugin_id = Some(listed.plugin_id.clone());
                let deployment = match repository.get_deployment(&listed.plugin_id).await {
                    Ok(Some(deployment)) => deployment,
                    Ok(None) => {
                        if let Ok(plugin_id) = PluginId::new(listed.plugin_id.clone()) {
                            self.supervisor
                                .stop_through_generation(
                                    &plugin_id,
                                    u64::try_from(listed.generation).unwrap_or_default(),
                                )
                                .await;
                        }
                        continue;
                    }
                    Err(error) => {
                        tracing::warn!(
                            plugin_id = listed.plugin_id,
                            error_kind = ?error,
                            "plugin deployment could not be reloaded before reconciliation"
                        );
                        continue;
                    }
                };
                if let Err(error) = self.apply(repository, &deployment).await {
                    tracing::warn!(
                        plugin_id = deployment.plugin_id,
                        generation = deployment.generation,
                        error_kind = ?error,
                        "plugin deployment could not be reconciled"
                    );
                }
                match repository.get_deployment(&deployment.plugin_id).await {
                    Ok(Some(latest))
                        if latest.generation == deployment.generation
                            && latest.enabled == deployment.enabled => {}
                    Ok(Some(latest)) => {
                        let generation = u64::try_from(deployment.generation).unwrap_or_default();
                        let plugin_id = PluginId::new(deployment.plugin_id.clone());
                        if let Ok(plugin_id) = plugin_id {
                            if !latest.enabled {
                                self.supervisor
                                    .stop_through_generation(
                                        &plugin_id,
                                        u64::try_from(latest.generation).unwrap_or_default(),
                                    )
                                    .await;
                            } else if deployment.enabled {
                                self.supervisor
                                    .stop_generation(&plugin_id, generation)
                                    .await;
                            }
                        }
                    }
                    Ok(None) => {
                        if let Ok(plugin_id) = PluginId::new(deployment.plugin_id.clone()) {
                            self.supervisor
                                .stop_through_generation(
                                    &plugin_id,
                                    u64::try_from(deployment.generation).unwrap_or_default(),
                                )
                                .await;
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            plugin_id = deployment.plugin_id,
                            error_kind = ?error,
                            "plugin deployment could not be reloaded after reconciliation"
                        );
                        if let (Ok(plugin_id), Ok(generation)) = (
                            PluginId::new(deployment.plugin_id.clone()),
                            u64::try_from(deployment.generation),
                        ) {
                            self.supervisor
                                .stop_generation(&plugin_id, generation)
                                .await;
                        }
                    }
                }
            }
            if !page_is_full {
                return;
            }
        }
    }

    /// Stop every managed sidecar after the host has drained HTTP requests.
    pub async fn shutdown(&self) {
        self.supervisor.shutdown().await;
    }

    async fn verify_artifact(
        &self,
        record: &PluginArtifactRecord,
    ) -> Result<VerifiedPluginArtifact, AdminPluginLifecycleError> {
        if record.source_kind != PluginArtifactSourceKind::LocalProcess {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let source: LocalArtifactSource = serde_json::from_str(&record.source_reference)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let package_relative = safe_relative_path(&source.package_path)?;
        let executable_relative = safe_relative_path(&source.executable_path)?;
        let working_directory = std::env::current_dir()
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let artifact_root = working_directory
            .join(&self.artifact_root)
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::NotFound)?;
        if !artifact_root.is_dir() || !artifact_root.starts_with(&working_directory) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        prepare_staging_parent(&artifact_root)?;
        let package_root = artifact_root
            .join(package_relative)
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::NotFound)?;
        if !package_root.starts_with(&artifact_root) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let executable = package_root
            .join(executable_relative)
            .canonicalize()
            .map_err(|_| AdminPluginLifecycleError::NotFound)?;
        if !executable.starts_with(&package_root) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let key = if record.signer_key_id.is_empty() && record.signature.is_empty() {
            None
        } else {
            Some(
                self.trusted_signers
                    .get(&record.signer_key_id)
                    .ok_or(AdminPluginLifecycleError::Forbidden)?,
            )
        };
        let manifest: PluginManifest = serde_json::from_value(record.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let plugin_id = PluginId::new(record.plugin_id.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let plugin_version = PluginVersion::new(record.plugin_version.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let signature = if record.signature.is_empty() {
            String::new()
        } else {
            base64_signature_from_hex(&record.signature)?
        };
        let descriptor = PluginArtifactDescriptor::new(
            plugin_id,
            plugin_version,
            manifest,
            executable,
            package_root,
            record.digest.clone(),
            record.signer_key_id.clone(),
            &signature,
            "local_process".to_owned(),
            record.source_reference.clone(),
        )
        .and_then(|descriptor| descriptor.with_staging_parent(artifact_root.join(".runtime")))
        .map_err(map_sidecar_error)?;
        match key {
            Some(key) => VerifiedPluginArtifact::verify(descriptor, key).await,
            None => VerifiedPluginArtifact::verify_operator_managed(descriptor).await,
        }
        .map_err(map_sidecar_error)
    }
}

impl fmt::Debug for AdminPluginSidecarRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminPluginSidecarRuntime(<trusted paths and keys redacted>)")
    }
}

async fn deployment_is_current(
    repository: &PluginCatalogRepository,
    deployment: &PluginDeploymentRecord,
) -> Result<bool, AdminPluginLifecycleError> {
    Ok(repository
        .get_deployment(&deployment.plugin_id)
        .await
        .map_err(|_| AdminPluginLifecycleError::Internal)?
        .is_some_and(|current| {
            current.generation == deployment.generation
                && current.enabled == deployment.enabled
                && current.artifact.digest == deployment.artifact.digest
        }))
}

async fn ensure_deployment_current_after_activation<F, Fut>(
    current: Result<bool, AdminPluginLifecycleError>,
    stop_generation: F,
) -> Result<(), AdminPluginLifecycleError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    match current {
        Ok(true) => Ok(()),
        Ok(false) => {
            stop_generation().await;
            Err(AdminPluginLifecycleError::Conflict)
        }
        Err(error) => {
            stop_generation().await;
            Err(error)
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalArtifactSource {
    package_path: String,
    executable_path: String,
}

fn prepare_staging_parent(artifact_root: &Path) -> Result<(), AdminPluginLifecycleError> {
    let path = artifact_root.join(".runtime");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if let Err(error) = builder.create(&path)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(AdminPluginLifecycleError::RuntimeUnavailable);
    }
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> Result<PathBuf, AdminPluginLifecycleError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 2048
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    Ok(path.to_path_buf())
}

fn safe_posix_relative_path(value: &str) -> Result<PathBuf, AdminPluginLifecycleError> {
    if value.is_empty()
        || value.len() > 900
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || value.split('/').any(|component| {
            component.is_empty() || component == "." || component == ".." || component.len() > 255
        })
    {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    Ok(PathBuf::from(value))
}

fn canonical_relative_path(
    base: &Path,
    target: &Path,
) -> Result<String, AdminPluginLifecycleError> {
    let relative = target
        .strip_prefix(base)
        .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
    let components = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .map(str::to_owned)
                .ok_or(AdminPluginLifecycleError::InvalidInput),
            _ => Err(AdminPluginLifecycleError::InvalidInput),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.is_empty() {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    let value = components.join("/");
    if value.len() > 900 || value.chars().any(char::is_control) {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    Ok(value)
}

pub(crate) fn base64_signature_from_hex(value: &str) -> Result<String, AdminPluginLifecycleError> {
    if value.len() != 128 {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    let mut bytes = Vec::with_capacity(64);
    for pair in value.as_bytes().chunks_exact(2) {
        let pair =
            std::str::from_utf8(pair).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        bytes.push(
            u8::from_str_radix(pair, 16).map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
        );
    }
    Ok(BASE64.encode(bytes))
}

fn map_sidecar_error(error: SidecarError) -> AdminPluginLifecycleError {
    match error {
        SidecarError::UntrustedSigner => AdminPluginLifecycleError::Forbidden,
        SidecarError::InvalidArtifact
        | SidecarError::ArtifactDigestMismatch
        | SidecarError::InvalidArtifactSignature
        | SidecarError::InvalidConfiguration
        | SidecarError::CapabilityNotDeclared
        | SidecarError::CapabilityNotGranted
        | SidecarError::SensitivePayload => AdminPluginLifecycleError::InvalidInput,
        SidecarError::StartFailed
        | SidecarError::InvalidHandshake
        | SidecarError::FrameTooLarge
        | SidecarError::ProtocolViolation
        | SidecarError::ProcessExited
        | SidecarError::TimedOut
        | SidecarError::IsolationUnavailable
        | SidecarError::OperationFailed => AdminPluginLifecycleError::RuntimeUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdminPluginLifecycleError, base64_signature_from_hex,
        ensure_deployment_current_after_activation, safe_posix_relative_path, safe_relative_path,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn artifact_paths_must_be_relative_and_traversal_free() {
        assert!(safe_relative_path("packages/plugin-a").is_ok());
        assert!(safe_posix_relative_path("packages/plugin-a").is_ok());
        for value in ["", "../outside", ".", "/absolute/path", "C:\\outside"] {
            assert!(safe_relative_path(value).is_err(), "{value}");
        }
        for value in [
            "",
            "../outside",
            ".",
            "/absolute/path",
            "C:/outside",
            "a//b",
        ] {
            assert!(safe_posix_relative_path(value).is_err(), "{value}");
        }
    }

    #[test]
    fn stored_ed25519_signature_must_be_exactly_64_bytes() {
        assert!(base64_signature_from_hex(&"a1".repeat(64)).is_ok());
        assert!(base64_signature_from_hex(&"a1".repeat(63)).is_err());
        assert!(base64_signature_from_hex(&"zz".repeat(64)).is_err());
    }

    #[tokio::test]
    async fn failed_post_activation_database_check_stops_generation_and_preserves_error() {
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_by_callback = Arc::clone(&stopped);

        let result = ensure_deployment_current_after_activation(
            Err(AdminPluginLifecycleError::Internal),
            || async move {
                stopped_by_callback.store(true, Ordering::SeqCst);
            },
        )
        .await;

        assert_eq!(result, Err(AdminPluginLifecycleError::Internal));
        assert!(stopped.load(Ordering::SeqCst));
    }
}
