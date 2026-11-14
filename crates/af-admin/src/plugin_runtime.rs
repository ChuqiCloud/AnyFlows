//! Host-side reconciliation between durable plugin deployments and sidecar processes.

use std::{
    collections::BTreeMap,
    fmt,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use af_config::PluginTrustedSigner;
use af_db::{
    MAX_PLUGIN_DEPLOYMENT_HISTORY, PluginArtifactRecord, PluginArtifactSourceKind,
    PluginCatalogRepository, PluginDeploymentQuery, PluginDeploymentRecord,
};
use af_plugin_contract::{CapabilityId, CapabilityResult, PluginId, PluginManifest, PluginVersion};
use af_plugin_runtime::{
    CapabilityExecutionContext, PluginArtifactDescriptor, SidecarError, SidecarLimits,
    SidecarSupervisor, TrustedPluginKey, VerifiedPluginArtifact,
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;

use crate::AdminPluginLifecycleError;

/// Runtime owner for the optional, signed local-process plugin feature.
///
/// The artifact root and publisher keys come from host configuration. Nothing
/// in the database can add a signer or choose an absolute executable path.
pub struct AdminPluginSidecarRuntime {
    artifact_root: PathBuf,
    trusted_signers: BTreeMap<String, TrustedPluginKey>,
    supervisor: Arc<SidecarSupervisor>,
}

impl AdminPluginSidecarRuntime {
    /// Create the runtime from already validated application configuration.
    pub fn new(
        artifact_directory: &Path,
        signers: &[PluginTrustedSigner],
    ) -> Result<Self, AdminPluginLifecycleError> {
        if signers.is_empty()
            || artifact_directory.is_absolute()
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
        })
    }

    /// Start a committed enabled generation or stop a committed disabled one.
    pub async fn apply(
        &self,
        deployment: &PluginDeploymentRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        let plugin_id = PluginId::new(deployment.plugin_id.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
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
        let artifact = self.verify_artifact(&deployment.artifact).await?;
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
            self.supervisor.stop(&plugin_id).await;
        }
        self.supervisor
            .activate(artifact, generation, grants, SidecarLimits::default())
            .await
            .map_err(map_sidecar_error)?;
        Ok(())
    }

    /// Stop the currently active process after a committed revocation or failed activation.
    pub async fn stop(&self, plugin_id: &str) -> Result<(), AdminPluginLifecycleError> {
        let plugin_id = PluginId::new(plugin_id.to_owned())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        self.supervisor.stop(&plugin_id).await;
        Ok(())
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
            || deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess
            || !deployment.granted_capabilities.iter().any(|grant| {
                grant.id == capability_id.as_str() && grant.version == capability_version
            })
        {
            return Err(AdminPluginLifecycleError::Forbidden);
        }
        let session = self
            .supervisor
            .session(&plugin_id, generation)
            .await
            .map_err(map_sidecar_error)?;
        session
            .invoke_host_capability(
                generation,
                invocation,
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
            for deployment in deployments {
                before_plugin_id = Some(deployment.plugin_id.clone());
                if deployment.enabled {
                    if let Err(error) = self.apply(&deployment).await {
                        tracing::warn!(
                            plugin_id = deployment.plugin_id,
                            generation = deployment.generation,
                            error_kind = ?error,
                            "enabled plugin deployment could not be restored"
                        );
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
        let key = self
            .trusted_signers
            .get(&record.signer_key_id)
            .ok_or(AdminPluginLifecycleError::Forbidden)?;
        let manifest: PluginManifest = serde_json::from_value(record.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let plugin_id = PluginId::new(record.plugin_id.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let plugin_version = PluginVersion::new(record.plugin_version.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let signature = base64_signature_from_hex(&record.signature)?;
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
        .map_err(map_sidecar_error)?;
        VerifiedPluginArtifact::verify(descriptor, key)
            .await
            .map_err(map_sidecar_error)
    }
}

impl fmt::Debug for AdminPluginSidecarRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminPluginSidecarRuntime(<trusted paths and keys redacted>)")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LocalArtifactSource {
    package_path: String,
    executable_path: String,
}

fn safe_relative_path(value: &str) -> Result<PathBuf, AdminPluginLifecycleError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > 2048
        || value.chars().any(char::is_control)
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    Ok(path.to_path_buf())
}

fn base64_signature_from_hex(value: &str) -> Result<String, AdminPluginLifecycleError> {
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
        | SidecarError::OperationFailed => AdminPluginLifecycleError::RuntimeUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::{base64_signature_from_hex, safe_relative_path};

    #[test]
    fn artifact_paths_must_be_relative_and_traversal_free() {
        assert!(safe_relative_path("packages/plugin-a").is_ok());
        for value in ["", "../outside", ".", "/absolute/path", "C:\\outside"] {
            assert!(safe_relative_path(value).is_err(), "{value}");
        }
    }

    #[test]
    fn stored_ed25519_signature_must_be_exactly_64_bytes() {
        assert!(base64_signature_from_hex(&"a1".repeat(64)).is_ok());
        assert!(base64_signature_from_hex(&"a1".repeat(63)).is_err());
        assert!(base64_signature_from_hex(&"zz".repeat(64)).is_err());
    }
}
