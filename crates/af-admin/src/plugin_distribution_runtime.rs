//! Node-local materialization of shared archives and generation-aware observations.

use std::path::{Component, Path, PathBuf};

use af_config::PluginTrustedSigner;
use af_db::{
    MAX_PLUGIN_DEPLOYMENT_HISTORY, PluginArtifactQuery, PluginArtifactRecord,
    PluginCatalogRepository, PluginDeploymentQuery, PluginDeploymentRecord, PluginRuntimeErrorCode,
    PluginRuntimeStateWrite, PluginRuntimeStatus,
};
use af_plugin_contract::{PluginId, PluginManifest, PluginVersion};
use af_plugin_runtime::{PluginArtifactDescriptor, SidecarIsolationPolicy, TrustedPluginKey};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::Deserialize;

use crate::plugin_lifecycle::current_timestamp;
use crate::plugin_operations::map_distribution_error;
use crate::{AdminPluginLifecycleError as Error, AdminPluginSidecarRuntime};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Source {
    package_path: String,
    executable_path: String,
}

impl AdminPluginSidecarRuntime {
    pub fn with_official_signers(mut self, signers: &[PluginTrustedSigner]) -> Result<Self, Error> {
        for signer in signers {
            let bytes: [u8; 32] = BASE64
                .decode(signer.public_key_base64())
                .map_err(|_| Error::InvalidInput)?
                .try_into()
                .map_err(|_| Error::InvalidInput)?;
            let key = TrustedPluginKey::new(signer.key_id().to_owned(), bytes)
                .map_err(|_| Error::InvalidInput)?;
            if self
                .trusted_signers
                .insert(signer.key_id().to_owned(), key)
                .is_some()
            {
                return Err(Error::InvalidInput);
            }
            self.official_signers.insert(signer.key_id().to_owned());
        }
        Ok(self)
    }

    #[must_use]
    pub fn with_isolation_policy(mut self, policy: SidecarIsolationPolicy) -> Self {
        self.isolation_policy = policy;
        self
    }

    pub fn with_configured_isolation(self, policy: &str) -> Result<Self, Error> {
        let policy = match policy {
            "auto" => SidecarIsolationPolicy::Auto,
            "trusted" => SidecarIsolationPolicy::Trusted,
            "restricted" => SidecarIsolationPolicy::Restricted,
            "require_sandbox" => SidecarIsolationPolicy::RequireSandbox,
            _ => return Err(Error::InvalidInput),
        };
        Ok(self.with_isolation_policy(policy))
    }

    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    #[must_use]
    pub const fn isolation_policy(&self) -> SidecarIsolationPolicy {
        self.isolation_policy
    }

    /// Verify the exact metadata signature against the configured official roots.
    /// The catalog's signer ID alone never confers the official badge.
    pub(crate) fn is_official_artifact(&self, record: &PluginArtifactRecord) -> bool {
        let check = || -> Result<(), Error> {
            if !self.official_signers.contains(&record.signer_key_id) {
                return Err(Error::Forbidden);
            }
            let key = self
                .trusted_signers
                .get(&record.signer_key_id)
                .ok_or(Error::Forbidden)?;
            let source: Source =
                serde_json::from_str(&record.source_reference).map_err(|_| Error::InvalidInput)?;
            let root = std::env::current_dir()
                .map_err(|_| Error::Internal)?
                .join(&self.artifact_root)
                .join(source.package_path);
            let executable = root.join(source.executable_path);
            let manifest: PluginManifest =
                serde_json::from_value(record.manifest.clone()).map_err(|_| Error::InvalidInput)?;
            let signature = crate::plugin_runtime::base64_signature_from_hex(&record.signature)?;
            let descriptor = PluginArtifactDescriptor::new(
                PluginId::new(record.plugin_id.clone()).map_err(|_| Error::InvalidInput)?,
                PluginVersion::new(record.plugin_version.clone())
                    .map_err(|_| Error::InvalidInput)?,
                manifest,
                executable,
                root,
                record.digest.clone(),
                record.signer_key_id.clone(),
                &signature,
                "local_process".to_owned(),
                record.source_reference.clone(),
            )
            .map_err(|_| Error::InvalidInput)?;
            descriptor
                .verify_publisher_signature(key)
                .map_err(|_| Error::Forbidden)
        };
        check().is_ok()
    }

    pub(crate) async fn heartbeat(
        &self,
        repository: &PluginCatalogRepository,
    ) -> Result<(), Error> {
        repository
            .distribution_repository()
            .upsert_node(&self.node_id, &self.host_label, current_timestamp()?)
            .await
            .map_err(map_distribution_error)
    }

    pub(crate) async fn report_state(
        &self,
        repository: &PluginCatalogRepository,
        deployment: &PluginDeploymentRecord,
        status: PluginRuntimeStatus,
        error_code: Option<PluginRuntimeErrorCode>,
    ) -> Result<(), Error> {
        let plugin_id =
            PluginId::new(deployment.plugin_id.clone()).map_err(|_| Error::InvalidInput)?;
        let isolation = match u64::try_from(deployment.generation) {
            Ok(generation) => self
                .supervisor
                .session(&plugin_id, generation)
                .await
                .ok()
                .map(|session| session.effective_isolation().as_str().to_owned()),
            Err(_) => None,
        }
        .unwrap_or_else(|| "unknown".to_owned());
        repository
            .distribution_repository()
            .record_state(PluginRuntimeStateWrite {
                node_id: self.node_id.clone(),
                plugin_id: deployment.plugin_id.clone(),
                generation: deployment.generation,
                digest: deployment.artifact.digest.clone(),
                status,
                isolation,
                error_code,
                updated_at: current_timestamp()?,
            })
            .await
            .map_err(map_distribution_error)?;
        Ok(())
    }

    pub(crate) async fn record_apply_result(
        &self,
        repository: &PluginCatalogRepository,
        deployment: &PluginDeploymentRecord,
        error: Option<&Error>,
    ) -> Result<(), Error> {
        let status = match (error, deployment.enabled) {
            (None, true) => PluginRuntimeStatus::Ready,
            (None, false) => PluginRuntimeStatus::Stopped,
            (Some(_), _) => PluginRuntimeStatus::Failed,
        };
        let code = error.map(|error| match error {
            Error::NotFound => PluginRuntimeErrorCode::ArtifactMissing,
            Error::InvalidInput => PluginRuntimeErrorCode::ConfigurationInvalid,
            Error::Forbidden => PluginRuntimeErrorCode::PublisherUntrusted,
            Error::RuntimeUnavailable => PluginRuntimeErrorCode::StartupFailed,
            _ => PluginRuntimeErrorCode::Unavailable,
        });
        self.report_state(repository, deployment, status, code)
            .await
    }

    /// Download from the shared database, verify the archive, then run the same
    /// bounded importer as an administrator upload. No node-to-node secrets are required.
    pub(crate) async fn ensure_materialized(
        &self,
        repository: &PluginCatalogRepository,
        artifact: &PluginArtifactRecord,
    ) -> Result<(), Error> {
        let source: Source =
            serde_json::from_str(&artifact.source_reference).map_err(|_| Error::InvalidInput)?;
        let root = self.installation_root()?;
        let relative = controlled_relative(&source.package_path)?;
        match tokio::fs::symlink_metadata(root.join(&relative)).await {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                return Ok(());
            }
            Ok(_) => return Err(Error::InvalidInput),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Error::Internal),
        }
        let distribution = repository.distribution_repository();
        let archive = distribution
            .load_package(artifact.id)
            .await
            .map_err(map_distribution_error)?
            .ok_or(Error::NotFound)?;
        let upload_root = root.join(".uploads");
        create_private_directory(&upload_root)?;
        let temporary = tempfile::Builder::new()
            .prefix(".plugin-replica-")
            .tempfile_in(&upload_root)
            .map_err(|_| Error::Internal)?;
        tokio::fs::write(temporary.path(), &archive)
            .await
            .map_err(|_| Error::Internal)?;
        let imported = crate::plugin_package::extract_plugin_package_at(
            temporary.path().to_path_buf(),
            root,
            Some(source.package_path.clone()),
        )
        .await?;
        if imported.plugin_id != artifact.plugin_id
            || imported.plugin_version != artifact.plugin_version
            || imported.digest != artifact.digest
            || imported.manifest != artifact.manifest
            || imported.package_path != source.package_path
            || imported.executable_path != source.executable_path
        {
            return Err(Error::InvalidInput);
        }
        // Publisher validation is repeated by verify_artifact before activation.
        imported.retain();
        Ok(())
    }

    pub(crate) async fn reconcile_cleanup(
        &self,
        repository: &PluginCatalogRepository,
    ) -> Result<(), Error> {
        let distribution = repository.distribution_repository();
        for request in distribution
            .list_uninstall_requests()
            .await
            .map_err(map_distribution_error)?
        {
            if self.cleaned_uninstalls.lock().await.get(&request.plugin_id)
                == Some(&request.generation)
            {
                continue;
            }
            let Some(current) = repository
                .get_deployment(&request.plugin_id)
                .await
                .map_err(|_| Error::Internal)?
            else {
                continue;
            };
            if current.enabled || current.generation != request.generation {
                continue;
            }
            self.stop_through_generation(&request.plugin_id, request.generation)
                .await?;
            let mut cursor = None;
            loop {
                let query = PluginArtifactQuery::new(cursor, MAX_PLUGIN_DEPLOYMENT_HISTORY)
                    .map_err(|_| Error::InvalidInput)?;
                let artifacts = repository
                    .list_artifacts(query)
                    .await
                    .map_err(|_| Error::Internal)?;
                let full = artifacts.len() == MAX_PLUGIN_DEPLOYMENT_HISTORY as usize;
                for artifact in artifacts {
                    cursor = Some(artifact.id);
                    if artifact.plugin_id == request.plugin_id {
                        self.cleanup_materialized(repository, &artifact).await?;
                    }
                }
                if !full {
                    break;
                }
            }
            self.report_state(repository, &current, PluginRuntimeStatus::Stopped, None)
                .await?;
            self.cleaned_uninstalls
                .lock()
                .await
                .insert(request.plugin_id, request.generation);
        }
        for request in distribution
            .list_cleanup_requests()
            .await
            .map_err(map_distribution_error)?
        {
            if self
                .collected_artifacts
                .lock()
                .await
                .contains(&request.artifact_id)
            {
                continue;
            }
            let Some(artifact) = repository
                .get_artifact(request.artifact_id)
                .await
                .map_err(|_| Error::Internal)?
            else {
                continue;
            };
            if artifact.revocation.is_none() {
                continue;
            }
            distribution
                .remove_package(artifact.id)
                .await
                .map_err(map_distribution_error)?;
            self.cleanup_materialized(repository, &artifact).await?;
            self.collected_artifacts.lock().await.insert(artifact.id);
        }
        Ok(())
    }

    /// Called only under cache_operations after verification failed. A damaged
    /// importer-owned replica can be recreated once from its immutable archive;
    /// manually registered operator directories are never removed here.
    pub(crate) async fn repair_materialized(
        &self,
        repository: &PluginCatalogRepository,
        artifact: &PluginArtifactRecord,
    ) -> Result<bool, Error> {
        let source: Source =
            serde_json::from_str(&artifact.source_reference).map_err(|_| Error::InvalidInput)?;
        let Some(key) = managed_package_key(&source.package_path) else {
            return Ok(false);
        };
        if repository
            .distribution_repository()
            .package_metadata(artifact.id)
            .await
            .map_err(map_distribution_error)?
            .is_none()
        {
            return Ok(false);
        }
        let root = self.installation_root()?;
        let packages = root.join("packages");
        let target = packages.join(key);
        tokio::task::spawn_blocking(move || remove_managed_package(&root, &packages, &target))
            .await
            .map_err(|_| Error::Internal)??;
        self.ensure_materialized(repository, artifact).await?;
        Ok(true)
    }

    /// Remove only importer-owned cache directories. Manual registrations never
    /// authorize deleting arbitrary operator-provided paths.
    pub(crate) async fn cleanup_materialized(
        &self,
        repository: &PluginCatalogRepository,
        artifact: &PluginArtifactRecord,
    ) -> Result<bool, Error> {
        if artifact.source_kind != af_db::PluginArtifactSourceKind::LocalProcess {
            return Ok(false);
        }
        let _cache_guard = self.cache_operations.lock().await;
        let source: Source =
            serde_json::from_str(&artifact.source_reference).map_err(|_| Error::InvalidInput)?;
        let Some(key) = managed_package_key(&source.package_path) else {
            return Ok(false);
        };
        let mut cursor = None;
        loop {
            let query = PluginDeploymentQuery::new(cursor.clone(), MAX_PLUGIN_DEPLOYMENT_HISTORY)
                .map_err(|_| Error::InvalidInput)?;
            let deployments = repository
                .list_deployments(query)
                .await
                .map_err(|_| Error::Internal)?;
            let full = deployments.len() == MAX_PLUGIN_DEPLOYMENT_HISTORY as usize;
            for deployment in deployments {
                cursor = Some(deployment.plugin_id.clone());
                let uses_same_package =
                    serde_json::from_str::<Source>(&deployment.artifact.source_reference)
                        .ok()
                        .is_some_and(|active| {
                            controlled_relative(&active.package_path).ok()
                                == controlled_relative(&source.package_path).ok()
                        });
                if deployment.enabled && uses_same_package {
                    return Ok(false);
                }
            }
            if !full {
                break;
            }
        }
        let root = self.installation_root()?;
        let packages = root.join("packages");
        let target = packages.join(key);
        tokio::task::spawn_blocking(move || remove_managed_package(&root, &packages, &target))
            .await
            .map_err(|_| Error::Internal)?
    }
}

pub(crate) fn runtime_host_label() -> String {
    let candidate = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "AnyFlows node".to_owned());
    let label: String = candidate
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(*ch, '-' | '_' | '.'))
        .take(128)
        .collect();
    if label.is_empty() {
        "AnyFlows node".to_owned()
    } else {
        label
    }
}

fn controlled_relative(value: &str) -> Result<PathBuf, Error> {
    if value.is_empty()
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(Error::InvalidInput);
    }
    let path = Path::new(value);
    if path.is_absolute()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(Error::InvalidInput);
    }
    Ok(path.to_path_buf())
}

fn managed_package_key(value: &str) -> Option<&str> {
    let key = value.strip_prefix("packages/")?.strip_suffix("/payload")?;
    let valid_hash = |hash: &str| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if valid_hash(key)
        || key
            .split_once('-')
            .is_some_and(|(digest, receipt)| valid_hash(digest) && valid_hash(receipt))
    {
        Some(key)
    } else {
        None
    }
}

fn create_private_directory(path: &Path) -> Result<(), Error> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = std::fs::symlink_metadata(path).map_err(|_| Error::Internal)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                Ok(())
            } else {
                Err(Error::InvalidInput)
            }
        }
        Err(_) => Err(Error::Internal),
    }
}

fn remove_managed_package(root: &Path, packages: &Path, target: &Path) -> Result<bool, Error> {
    let metadata = match std::fs::symlink_metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(Error::Internal),
    };
    let canonical_root = root.canonicalize().map_err(|_| Error::Internal)?;
    let packages_metadata = std::fs::symlink_metadata(packages).map_err(|_| Error::Internal)?;
    let canonical_packages = packages.canonicalize().map_err(|_| Error::Internal)?;
    let canonical_target = target.canonicalize().map_err(|_| Error::Internal)?;
    if packages_metadata.file_type().is_symlink()
        || !packages_metadata.is_dir()
        || metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || canonical_packages.parent() != Some(canonical_root.as_path())
        || canonical_target.parent() != Some(canonical_packages.as_path())
    {
        return Err(Error::InvalidInput);
    }
    std::fs::remove_dir_all(&canonical_target).map_err(|_| Error::Internal)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_content_addressed_importer_cache_paths_can_be_removed() {
        let digest = "a".repeat(64);
        let receipt = "b".repeat(64);
        assert!(managed_package_key(&format!("packages/{digest}/payload")).is_some());
        assert!(managed_package_key(&format!("packages/{digest}-{receipt}/payload")).is_some());
        for path in [
            "packages/../payload",
            "custom/payload",
            "packages/plugin/payload",
            "packages/a/payload/extra",
        ] {
            assert!(managed_package_key(path).is_none());
        }
        assert!(controlled_relative("../outside").is_err());
        assert!(controlled_relative("C:/outside").is_err());
    }
}
