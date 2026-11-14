//! Administrative lifecycle operations for verified plugin artifacts.
//!
//! This service owns authorization, persistent approvals/deployments and audit
//! attribution. Runtime activation is deliberately handled by a separate host
//! runtime coordinator.

use std::{
    fmt,
    sync::Arc,
    time::{Duration, SystemTime},
};

use af_db::{
    MAX_PLUGIN_DEPLOYMENT_HISTORY, PlatformAuditOutcome, PluginArtifactApproval,
    PluginArtifactCreate, PluginArtifactQuery, PluginArtifactRecord, PluginArtifactRevocation,
    PluginArtifactSourceKind, PluginCapabilityGrant, PluginCatalogError, PluginCatalogRepository,
    PluginDeploymentQuery, PluginDeploymentRecord, PluginDeploymentWrite,
};
use af_domain::PlatformPermission;
use af_plugin_contract::PluginManifest;
use af_plugin_runtime::{VerifiedPluginArtifact, validate_public_plugin_configuration};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde::Serialize;
use serde_json::{Value, json};
use thiserror::Error;

use crate::{
    AdminPluginSidecarRuntime, PlatformAuditEntry, PlatformAuditError, PlatformAuditService,
    PlatformPolicy, SessionPrincipal, SessionRole,
};

pub const DEFAULT_PLUGIN_CATALOG_PAGE_SIZE: u32 = 50;
const PLUGIN_PERMISSION: PlatformPermission = PlatformPermission::PluginsManage;
const SIMPLE_INSTALL_DEFAULT_CAPABILITIES: &[&str] =
    &["catalog.models.read", "catalog.models.search"];
pub const MAX_PLUGIN_PACKAGE_UPLOAD_BYTES: usize = 128 * 1024 * 1024;
const STALE_PLUGIN_UPLOAD_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// A bounded upload in host-owned storage. Dropping it removes unfinished data.
pub struct PluginPackageUpload {
    file: tokio::fs::File,
    temporary: tempfile::NamedTempFile,
    artifact_root: std::path::PathBuf,
    uploaded: usize,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl PluginPackageUpload {
    pub async fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), AdminPluginLifecycleError> {
        use tokio::io::AsyncWriteExt as _;
        self.uploaded = self
            .uploaded
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_PLUGIN_PACKAGE_UPLOAD_BYTES)
            .ok_or(AdminPluginLifecycleError::PayloadTooLarge)?;
        self.file
            .write_all(bytes)
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)
    }
}

/// A safe artifact view. The source reference and signature are intentionally
/// not represented here, so future HTTP adapters cannot accidentally serialize them.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifact {
    id: i64,
    plugin_id: String,
    plugin_version: String,
    digest: String,
    manifest: Value,
    source_kind: &'static str,
    signer_key_id: String,
    created_at: i64,
    approval: Option<AdminPluginArtifactApproval>,
    revocation: Option<AdminPluginArtifactRevocation>,
}

impl AdminPluginArtifact {
    fn from_record(record: PluginArtifactRecord) -> Self {
        let source_kind = match record.source_kind {
            PluginArtifactSourceKind::Builtin => "builtin",
            PluginArtifactSourceKind::LocalProcess => "local_process",
            PluginArtifactSourceKind::RemoteEndpoint => "remote_endpoint",
        };
        Self {
            id: record.id,
            plugin_id: record.plugin_id,
            plugin_version: record.plugin_version,
            digest: record.digest,
            manifest: record.manifest,
            source_kind,
            signer_key_id: record.signer_key_id,
            created_at: record.created_at,
            approval: record.approval.map(AdminPluginArtifactApproval::from),
            revocation: record.revocation.map(AdminPluginArtifactRevocation::from),
        }
    }

    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    #[must_use]
    pub fn plugin_version(&self) -> &str {
        &self.plugin_version
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    #[must_use]
    pub fn manifest(&self) -> &Value {
        &self.manifest
    }

    #[must_use]
    pub const fn source_kind(&self) -> &'static str {
        self.source_kind
    }

    #[must_use]
    pub fn signer_key_id(&self) -> &str {
        &self.signer_key_id
    }

    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    #[must_use]
    pub const fn approval(&self) -> Option<&AdminPluginArtifactApproval> {
        self.approval.as_ref()
    }

    #[must_use]
    pub const fn revocation(&self) -> Option<&AdminPluginArtifactRevocation> {
        self.revocation.as_ref()
    }
}

impl fmt::Debug for AdminPluginArtifact {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminPluginArtifact")
            .field("id", &self.id)
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("digest", &self.digest)
            .field("manifest", &"<redacted>")
            .field("source_kind", &self.source_kind)
            .field("signer_key_id", &self.signer_key_id)
            .field("created_at", &self.created_at)
            .field("approved", &self.approval.is_some())
            .field("revoked", &self.revocation.is_some())
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifactApproval {
    approved_by: i64,
    approved_at: i64,
    request_id: String,
}

impl From<PluginArtifactApproval> for AdminPluginArtifactApproval {
    fn from(value: PluginArtifactApproval) -> Self {
        Self {
            approved_by: value.approved_by.get(),
            approved_at: value.approved_at,
            request_id: value.request_id,
        }
    }
}

impl AdminPluginArtifactApproval {
    #[must_use]
    pub const fn approved_by(&self) -> i64 {
        self.approved_by
    }

    #[must_use]
    pub const fn approved_at(&self) -> i64 {
        self.approved_at
    }

    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifactRevocation {
    revoked_by: i64,
    revoked_at: i64,
    request_id: String,
}

impl From<PluginArtifactRevocation> for AdminPluginArtifactRevocation {
    fn from(value: PluginArtifactRevocation) -> Self {
        Self {
            revoked_by: value.revoked_by.get(),
            revoked_at: value.revoked_at,
            request_id: value.request_id,
        }
    }
}

impl AdminPluginArtifactRevocation {
    #[must_use]
    pub const fn revoked_by(&self) -> i64 {
        self.revoked_by
    }

    #[must_use]
    pub const fn revoked_at(&self) -> i64 {
        self.revoked_at
    }

    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

impl fmt::Debug for AdminPluginArtifactRevocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminPluginArtifactRevocation")
            .field("revoked_by", &self.revoked_by)
            .field("revoked_at", &self.revoked_at)
            .field("request_id", &self.request_id)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginArtifactPage {
    artifacts: Vec<AdminPluginArtifact>,
    next_before_id: Option<i64>,
}

impl AdminPluginArtifactPage {
    #[must_use]
    pub fn artifacts(&self) -> &[AdminPluginArtifact] {
        &self.artifacts
    }

    #[must_use]
    pub const fn next_before_id(&self) -> Option<i64> {
        self.next_before_id
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginDeployment {
    plugin_id: String,
    generation: i64,
    artifact: AdminPluginArtifact,
    enabled: bool,
    granted_capabilities: Vec<AdminPluginCapabilityGrant>,
    updated_by: i64,
    updated_at: i64,
    request_id: String,
}

impl AdminPluginDeployment {
    fn from_record(record: PluginDeploymentRecord) -> Self {
        Self {
            plugin_id: record.plugin_id,
            generation: record.generation,
            artifact: AdminPluginArtifact::from_record(record.artifact),
            enabled: record.enabled,
            granted_capabilities: record
                .granted_capabilities
                .into_iter()
                .map(AdminPluginCapabilityGrant::from)
                .collect(),
            updated_by: record.updated_by.get(),
            updated_at: record.updated_at,
            request_id: record.request_id,
        }
    }

    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    #[must_use]
    pub const fn generation(&self) -> i64 {
        self.generation
    }

    #[must_use]
    pub const fn artifact(&self) -> &AdminPluginArtifact {
        &self.artifact
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn granted_capabilities(&self) -> &[AdminPluginCapabilityGrant] {
        &self.granted_capabilities
    }

    #[must_use]
    pub const fn updated_by(&self) -> i64 {
        self.updated_by
    }

    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }

    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }
}

/// Safe public projection of one explicitly granted plugin capability.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginCapabilityGrant {
    id: String,
    version: u16,
}

impl From<PluginCapabilityGrant> for AdminPluginCapabilityGrant {
    fn from(value: PluginCapabilityGrant) -> Self {
        Self {
            id: value.id,
            version: value.version,
        }
    }
}

impl AdminPluginCapabilityGrant {
    pub fn new(id: String, version: u16) -> Result<Self, AdminPluginLifecycleError> {
        if id.is_empty() || id.len() > 128 || version == 0 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self { id, version })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }
}

impl fmt::Debug for AdminPluginDeployment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminPluginDeployment")
            .field("plugin_id", &self.plugin_id)
            .field("generation", &self.generation)
            .field("artifact", &self.artifact)
            .field("enabled", &self.enabled)
            .field("granted_capabilities", &self.granted_capabilities)
            .field("updated_by", &self.updated_by)
            .field("updated_at", &self.updated_at)
            .field("request_id", &self.request_id)
            .field("config", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminPluginDeploymentPage {
    deployments: Vec<AdminPluginDeployment>,
    next_before_plugin_id: Option<String>,
}

impl AdminPluginDeploymentPage {
    #[must_use]
    pub fn deployments(&self) -> &[AdminPluginDeployment] {
        &self.deployments
    }

    #[must_use]
    pub fn next_before_plugin_id(&self) -> Option<&str> {
        self.next_before_plugin_id.as_deref()
    }
}

/// Validated desired deployment state. Callers cannot set the actor, timestamp,
/// or request id; the service derives those from the authenticated request.
#[derive(Clone)]
pub struct PluginDeploymentCommand {
    plugin_id: String,
    artifact_digest: String,
    expected_generation: Option<i64>,
    enabled: bool,
    granted_capabilities: Vec<AdminPluginCapabilityGrant>,
    config: Value,
}

/// Administrator supplied artifact locator and optional publisher signature.
/// Integrity is always checked; configured publisher keys enable signature verification.
#[derive(Clone)]
pub struct PluginArtifactRegistrationCommand {
    plugin_id: String,
    plugin_version: String,
    manifest: Value,
    package_path: String,
    executable_path: String,
    sha256: String,
    signer_key_id: String,
    signature_base64: String,
}

impl PluginArtifactRegistrationCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_id: String,
        plugin_version: String,
        manifest: Value,
        package_path: String,
        executable_path: String,
        sha256: String,
        signer_key_id: String,
        signature_base64: String,
    ) -> Result<Self, AdminPluginLifecycleError> {
        if plugin_id.is_empty()
            || plugin_id.len() > 128
            || plugin_version.is_empty()
            || plugin_version.len() > 128
            || !manifest.is_object()
            || package_path.is_empty()
            || package_path.len() > 900
            || executable_path.is_empty()
            || executable_path.len() > 900
            || sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || signer_key_id.len() > 128
            || signer_key_id.is_empty() != signature_base64.is_empty()
            || (!signature_base64.is_empty() && signature_base64.len() != 88)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self {
            plugin_id,
            plugin_version,
            manifest,
            package_path,
            executable_path,
            sha256,
            signer_key_id,
            signature_base64,
        })
    }
}

impl fmt::Debug for PluginArtifactRegistrationCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginArtifactRegistrationCommand")
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("manifest", &"<redacted>")
            .field("package_path", &"<relative path>")
            .field("executable_path", &"<relative path>")
            .field("sha256", &self.sha256)
            .field("signer_key_id", &self.signer_key_id)
            .field("signature_base64", &"<redacted>")
            .finish()
    }
}

impl PluginDeploymentCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_id: String,
        artifact_digest: String,
        expected_generation: Option<i64>,
        enabled: bool,
        granted_capabilities: Vec<AdminPluginCapabilityGrant>,
        config: Value,
    ) -> Result<Self, AdminPluginLifecycleError> {
        if plugin_id.is_empty()
            || plugin_id.len() > 128
            || artifact_digest.len() != 64
            || !artifact_digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || expected_generation.is_some_and(|generation| generation <= 0)
            || !config.is_object()
            || granted_capabilities.len() > af_db::MAX_PLUGIN_CAPABILITIES
            || (enabled && granted_capabilities.is_empty())
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let mut seen = std::collections::BTreeSet::new();
        if granted_capabilities
            .iter()
            .any(|grant| !seen.insert(grant.id.clone()))
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self {
            plugin_id,
            artifact_digest,
            expected_generation,
            enabled,
            granted_capabilities,
            config,
        })
    }

    #[must_use]
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }
}

impl fmt::Debug for PluginDeploymentCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginDeploymentCommand")
            .field("plugin_id", &self.plugin_id)
            .field("artifact_digest", &self.artifact_digest)
            .field("expected_generation", &self.expected_generation)
            .field("enabled", &self.enabled)
            .field("granted_capabilities", &self.granted_capabilities)
            .field("config", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AdminPluginLifecycleError {
    #[error("plugin lifecycle access is forbidden")]
    Forbidden,
    #[error("plugin lifecycle request is invalid")]
    InvalidInput,
    #[error("the simple installer only grants the standard public catalog capabilities")]
    CapabilityReviewRequired,
    #[error("plugin package upload exceeds the size limit")]
    PayloadTooLarge,
    #[error("plugin lifecycle record was not found")]
    NotFound,
    #[error("plugin lifecycle state conflicts with the request")]
    Conflict,
    #[error("plugin sidecar runtime is not configured or available")]
    RuntimeUnavailable,
    #[error("plugin lifecycle service is unavailable")]
    Internal,
}

/// Admin-only access to the durable plugin catalog and its optional sidecar runtime.
pub struct DatabaseAdminPluginLifecycleService {
    repository: PluginCatalogRepository,
    audit: Arc<dyn PlatformAuditService>,
    sidecar_runtime: Option<Arc<AdminPluginSidecarRuntime>>,
}

impl DatabaseAdminPluginLifecycleService {
    #[must_use]
    pub fn new(repository: PluginCatalogRepository, audit: Arc<dyn PlatformAuditService>) -> Self {
        Self {
            repository,
            audit,
            sidecar_runtime: None,
        }
    }

    #[must_use]
    pub fn with_sidecar_runtime(mut self, runtime: Option<Arc<AdminPluginSidecarRuntime>>) -> Self {
        self.sidecar_runtime = runtime;
        self
    }

    /// Return the narrowly scoped host capability port for trusted business
    /// extensions. It does not authorize callers or expose platform secrets.
    #[must_use]
    pub fn capability_runtime(&self) -> Option<Arc<AdminPluginSidecarRuntime>> {
        self.sidecar_runtime.clone()
    }

    /// Authorize before accepting package bytes, and reserve a bounded upload slot.
    pub async fn begin_package_upload(
        &self,
        principal: SessionPrincipal,
        request_id: &str,
    ) -> Result<PluginPackageUpload, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/install",
            "install",
            "plugin",
            None,
        )
        .await?;
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        let permit = runtime.acquire_installation_slot()?;
        let artifact_root = runtime.installation_root()?;
        let upload_directory = artifact_root.join(".uploads");
        match std::fs::create_dir(&upload_directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(&upload_directory)
                    .map_err(|_| AdminPluginLifecycleError::Internal)?;
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Err(AdminPluginLifecycleError::InvalidInput);
                }
            }
            Err(_) => return Err(AdminPluginLifecycleError::Internal),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&upload_directory, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| AdminPluginLifecycleError::Internal)?;
        }
        cleanup_stale_plugin_uploads(&upload_directory);
        let temporary = tempfile::Builder::new()
            .prefix(".plugin-upload-")
            .tempfile_in(&upload_directory)
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let file = tokio::fs::File::from_std(
            temporary
                .reopen()
                .map_err(|_| AdminPluginLifecycleError::Internal)?,
        );
        Ok(PluginPackageUpload {
            file,
            temporary,
            artifact_root,
            uploaded: 0,
            _permit: permit,
        })
    }

    /// One administrator action imports, authorizes and activates a package.
    pub async fn install_package(
        &self,
        principal: SessionPrincipal,
        upload: PluginPackageUpload,
        config: Value,
        expected_generation: Option<i64>,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/install",
            "install",
            "plugin",
            None,
        )
        .await?;
        let result = self
            .install_package_authorized(principal, upload, config, expected_generation, request_id)
            .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/install",
            "install",
            "plugin",
            None,
            None,
            result.as_ref().ok().map(|deployment| {
                json!({
                    "pluginId": deployment.plugin_id(),
                    "generation": deployment.generation(),
                    "digest": deployment.artifact().digest(),
                })
            }),
            result,
        )
        .await
    }

    async fn install_package_authorized(
        &self,
        principal: SessionPrincipal,
        mut upload: PluginPackageUpload,
        config: Value,
        expected_generation: Option<i64>,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        use tokio::io::AsyncWriteExt as _;
        if upload.uploaded == 0
            || !config.is_object()
            || serde_json::to_vec(&config)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?
                .len()
                > af_db::MAX_PLUGIN_CONFIG_BYTES
            || expected_generation.is_some_and(|generation| generation <= 0)
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        upload
            .file
            .flush()
            .await
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
        let imported = crate::plugin_package::extract_plugin_package(
            upload.temporary.path().to_path_buf(),
            upload.artifact_root.clone(),
        )
        .await?;
        let manifest: PluginManifest = serde_json::from_value(imported.manifest.clone())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        validate_public_plugin_configuration(&manifest.config_schema, &config)
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        let grants = manifest
            .capabilities
            .iter()
            .filter(|capability| {
                capability.access == af_plugin_contract::CapabilityAccess::Read
                    && capability.side_effect == af_plugin_contract::SideEffectClass::None
                    && SIMPLE_INSTALL_DEFAULT_CAPABILITIES.contains(&capability.id.as_str())
            })
            .map(|capability| {
                AdminPluginCapabilityGrant::new(
                    capability.id.as_str().to_owned(),
                    capability.version,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let requires_capability_review = grants.is_empty()
            || manifest.capabilities.iter().any(|capability| {
                !SIMPLE_INSTALL_DEFAULT_CAPABILITIES.contains(&capability.id.as_str())
            });
        let current = self
            .repository
            .get_deployment(&imported.plugin_id)
            .await
            .map_err(map_catalog_error)?;
        let current_generation = current.as_ref().map(|deployment| deployment.generation);
        if expected_generation.is_some_and(|expected| Some(expected) != current_generation) {
            return Err(AdminPluginLifecycleError::Conflict);
        }
        // Upload and Install is an explicit operator action. When replacing an
        // existing version, bind it to the generation just read so concurrent
        // installs still fail compare-and-swap rather than silently racing.
        let deployment_expected_generation = expected_generation.or(current_generation);
        let runtime = self
            .sidecar_runtime
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
        // A publisher unknown to this host is not a reason to block an
        // administrator-owned package, but it must never acquire a verified badge.
        let (signer_key_id, signature_base64) = if imported.signer_key_id.is_empty() {
            (String::new(), String::new())
        } else if runtime.recognizes_publisher(&imported.signer_key_id) {
            (
                imported.signer_key_id.clone(),
                imported.signature_base64.clone(),
            )
        } else {
            // Unknown publisher metadata does not block an explicit admin
            // upload, but the host records it as unverified rather than
            // presenting it as a trusted publisher.
            (String::new(), String::new())
        };
        let command = PluginArtifactRegistrationCommand::new(
            imported.plugin_id.clone(),
            imported.plugin_version.clone(),
            imported.manifest.clone(),
            imported.package_path.clone(),
            imported.executable_path.clone(),
            imported.digest.clone(),
            signer_key_id,
            signature_base64,
        )?;
        let artifact = self
            .register_artifact_candidate(principal, command, request_id)
            .await?;
        // The catalog now owns the source locator even if a later activation fails.
        imported.retain();
        if artifact.approval().is_none() {
            match self
                .approve_artifact(principal, artifact.id(), request_id)
                .await
            {
                Ok(()) => {}
                Err(AdminPluginLifecycleError::Conflict) => {
                    // Concurrent identical uploads can race to create the
                    // single durable approval row. Continue only if the
                    // winner approved the same artifact and it is not revoked.
                    let latest = self
                        .repository
                        .get_artifact(artifact.id())
                        .await
                        .map_err(map_catalog_error)?
                        .ok_or(AdminPluginLifecycleError::NotFound)?;
                    if latest.approval.is_none() || latest.revocation.is_some() {
                        return Err(AdminPluginLifecycleError::Conflict);
                    }
                }
                Err(error) => return Err(error),
            }
        }
        if requires_capability_review {
            return Err(AdminPluginLifecycleError::CapabilityReviewRequired);
        }
        let command = PluginDeploymentCommand::new(
            artifact.plugin_id().to_owned(),
            artifact.digest().to_owned(),
            deployment_expected_generation,
            true,
            grants,
            config,
        )?;
        self.deploy(principal, command, request_id).await
    }

    /// Reconcile durable enabled deployments before the host accepts traffic.
    pub async fn reconcile_runtime(&self) {
        if let Some(runtime) = &self.sidecar_runtime {
            runtime.reconcile(&self.repository).await;
            return;
        }
        let Ok(query) = PluginDeploymentQuery::new(None, MAX_PLUGIN_DEPLOYMENT_HISTORY) else {
            return;
        };
        if self
            .repository
            .list_deployments(query)
            .await
            .is_ok_and(|deployments| deployments.iter().any(|deployment| deployment.enabled))
        {
            tracing::warn!(
                "plugin deployments are configured as enabled while sidecar runtime is disabled"
            );
        }
    }

    /// Stop managed child processes after the HTTP server has drained requests.
    pub async fn shutdown_runtime(&self) {
        if let Some(runtime) = &self.sidecar_runtime {
            runtime.shutdown().await;
        }
    }

    async fn apply_runtime(
        &self,
        deployment: &PluginDeploymentRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        if deployment.artifact.source_kind != PluginArtifactSourceKind::LocalProcess {
            return if deployment.enabled {
                Err(AdminPluginLifecycleError::RuntimeUnavailable)
            } else {
                Ok(())
            };
        }
        if deployment.enabled && self.sidecar_runtime.is_none() {
            return Err(AdminPluginLifecycleError::RuntimeUnavailable);
        }
        if let Some(runtime) = &self.sidecar_runtime {
            if deployment.enabled {
                let latest = self
                    .repository
                    .get_deployment(&deployment.plugin_id)
                    .await
                    .map_err(map_catalog_error)?
                    .ok_or(AdminPluginLifecycleError::NotFound)?;
                if !latest.enabled || latest.generation != deployment.generation {
                    return Err(AdminPluginLifecycleError::Conflict);
                }
            }
            runtime.apply(&self.repository, deployment).await?;
            if deployment.enabled {
                let latest = match self.repository.get_deployment(&deployment.plugin_id).await {
                    Ok(Some(latest)) => latest,
                    Ok(None) => {
                        let _ = runtime
                            .stop_generation(&deployment.plugin_id, deployment.generation)
                            .await;
                        return Err(AdminPluginLifecycleError::NotFound);
                    }
                    Err(error) => {
                        let _ = runtime
                            .stop_generation(&deployment.plugin_id, deployment.generation)
                            .await;
                        return Err(map_catalog_error(error));
                    }
                };
                if !latest.enabled || latest.generation != deployment.generation {
                    let _ = runtime
                        .stop_generation(&deployment.plugin_id, deployment.generation)
                        .await;
                    return Err(AdminPluginLifecycleError::Conflict);
                }
            }
        }
        Ok(())
    }

    async fn reconcile_revocation_runtime(
        &self,
        artifact: &PluginArtifactRecord,
    ) -> Result<(), AdminPluginLifecycleError> {
        if artifact.source_kind != PluginArtifactSourceKind::LocalProcess {
            return Ok(());
        }
        let Some(deployment) = self
            .repository
            .get_deployment(&artifact.plugin_id)
            .await
            .map_err(map_catalog_error)?
        else {
            return Ok(());
        };
        if let Err(error) = self.apply_runtime(&deployment).await {
            if !deployment.enabled
                && deployment.artifact.digest == artifact.digest
                && let Some(runtime) = &self.sidecar_runtime
            {
                let _ = runtime
                    .stop_through_generation(&deployment.plugin_id, deployment.generation)
                    .await;
            }
            return Err(error);
        }
        Ok(())
    }

    async fn disable_after_activation_failure(
        &self,
        deployment: &PluginDeploymentRecord,
        principal: SessionPrincipal,
        request_id: &str,
    ) {
        if let Some(runtime) = &self.sidecar_runtime {
            let _ = runtime
                .stop_through_generation(&deployment.plugin_id, deployment.generation)
                .await;
        }
        let Some(updated_at) = current_timestamp().ok() else {
            return;
        };
        let compensation_request_id = format!("{request_id}-disabled");
        let compensation_result = self
            .repository
            .deploy(PluginDeploymentWrite {
                plugin_id: deployment.plugin_id.clone(),
                artifact_digest: deployment.artifact.digest.clone(),
                expected_generation: Some(deployment.generation),
                enabled: false,
                granted_capabilities: deployment.granted_capabilities.clone(),
                config: deployment.config.clone(),
                updated_by: principal.user_id(),
                updated_at,
                request_id: compensation_request_id,
            })
            .await;
        let audit = audit_entry(
            principal,
            &format!("{request_id}-disabled"),
            "/admin/plugins/{pluginId}/deployment",
            "disable_after_failed_activation",
            "plugin_deployment",
            Some(deployment.plugin_id.clone()),
            if compensation_result.is_ok() {
                PlatformAuditOutcome::Succeeded
            } else {
                PlatformAuditOutcome::Failed
            },
            None,
            Some(json!({
                "enabled": false,
                "generation": deployment.generation.saturating_add(1),
                "triggeringRequestId": request_id
            })),
        );
        let audit_failed = match audit {
            Ok(entry) => self.audit.record(entry).await.is_err(),
            Err(_) => true,
        };
        if audit_failed {
            tracing::warn!(
                plugin_id = deployment.plugin_id,
                generation = deployment.generation,
                "failed activation compensation could not be fully audited"
            );
        }
        if compensation_result.is_err() {
            tracing::warn!(
                plugin_id = deployment.plugin_id,
                generation = deployment.generation,
                "failed plugin deployment could not be compensated"
            );
        }
    }

    pub async fn list_artifacts(
        &self,
        principal: SessionPrincipal,
        before_id: Option<i64>,
        limit: u32,
    ) -> Result<AdminPluginArtifactPage, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let query = PluginArtifactQuery::new(before_id, limit).map_err(map_catalog_error)?;
        let records = self
            .repository
            .list_artifacts(query)
            .await
            .map_err(map_catalog_error)?;
        let next_before_id = (records.len() == limit as usize)
            .then(|| records.last().map(|item| item.id))
            .flatten();
        Ok(AdminPluginArtifactPage {
            artifacts: records
                .into_iter()
                .map(AdminPluginArtifact::from_record)
                .collect(),
            next_before_id,
        })
    }

    pub async fn list_deployments(
        &self,
        principal: SessionPrincipal,
        before_plugin_id: Option<String>,
        limit: u32,
    ) -> Result<AdminPluginDeploymentPage, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let query =
            PluginDeploymentQuery::new(before_plugin_id, limit).map_err(map_catalog_error)?;
        let records = self
            .repository
            .list_deployments(query)
            .await
            .map_err(map_catalog_error)?;
        let next_before_plugin_id = (records.len() == limit as usize)
            .then(|| records.last().map(|item| item.plugin_id.clone()))
            .flatten();
        Ok(AdminPluginDeploymentPage {
            deployments: records
                .into_iter()
                .map(AdminPluginDeployment::from_record)
                .collect(),
            next_before_plugin_id,
        })
    }

    pub async fn deployment_history(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        limit: u32,
    ) -> Result<Vec<AdminPluginDeployment>, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        let limit = limit.min(MAX_PLUGIN_DEPLOYMENT_HISTORY);
        self.repository
            .deployment_history(plugin_id, limit)
            .await
            .map(|records| {
                records
                    .into_iter()
                    .map(AdminPluginDeployment::from_record)
                    .collect()
            })
            .map_err(map_catalog_error)
    }

    pub async fn get_deployment(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
    ) -> Result<Option<AdminPluginDeployment>, AdminPluginLifecycleError> {
        self.authorize(principal).await?;
        self.repository
            .get_deployment(plugin_id)
            .await
            .map(|record| record.map(AdminPluginDeployment::from_record))
            .map_err(map_catalog_error)
    }

    /// Persist a host-verified artifact. Raw executable paths and trust keys are
    /// not accepted by this API; verification happens before this service call.
    pub async fn register_verified_artifact(
        &self,
        principal: SessionPrincipal,
        artifact: VerifiedPluginArtifact,
        request_id: &str,
    ) -> Result<AdminPluginArtifact, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            None,
        )
        .await?;
        let plugin_id = artifact.plugin_id().as_str().to_owned();
        let plugin_version = artifact.plugin_version().as_str().to_owned();
        let digest = artifact.sha256().to_owned();
        let result = self
            .persist_verified_artifact(artifact, principal.user_id(), request_id)
            .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            Some(plugin_id),
            None,
            Some(json!({ "pluginVersion": plugin_version, "digest": digest })),
            result,
        )
        .await
    }

    /// Verify a candidate using host-owned filesystem/key configuration, then
    /// persist it in the durable catalog under admin session and audit controls.
    pub async fn register_artifact_candidate(
        &self,
        principal: SessionPrincipal,
        command: PluginArtifactRegistrationCommand,
        request_id: &str,
    ) -> Result<AdminPluginArtifact, AdminPluginLifecycleError> {
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            None,
        )
        .await?;
        let plugin_id = command.plugin_id.clone();
        let plugin_version = command.plugin_version.clone();
        let digest = command.sha256.clone();
        let result = async {
            let runtime = self
                .sidecar_runtime
                .as_ref()
                .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?;
            let artifact = runtime
                .verify_candidate(
                    command.plugin_id,
                    command.plugin_version,
                    command.manifest,
                    command.package_path,
                    command.executable_path,
                    command.sha256,
                    command.signer_key_id,
                    command.signature_base64,
                )
                .await?;
            self.persist_verified_artifact(artifact, principal.user_id(), request_id)
                .await
        }
        .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts",
            "register",
            "plugin_artifact",
            Some(plugin_id),
            None,
            Some(json!({ "pluginVersion": plugin_version, "digest": digest })),
            result,
        )
        .await
    }

    async fn persist_verified_artifact(
        &self,
        artifact: VerifiedPluginArtifact,
        operator: af_domain::UserId,
        request_id: &str,
    ) -> Result<AdminPluginArtifact, AdminPluginLifecycleError> {
        let source_kind = match artifact.source_kind() {
            "builtin" => PluginArtifactSourceKind::Builtin,
            "local_process" => PluginArtifactSourceKind::LocalProcess,
            _ => return Err(AdminPluginLifecycleError::InvalidInput),
        };
        let signature = BASE64
            .decode(artifact.signature_base64().as_bytes())
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let input = PluginArtifactCreate {
            plugin_id: artifact.plugin_id().as_str().to_owned(),
            plugin_version: artifact.plugin_version().as_str().to_owned(),
            digest: artifact.sha256().to_owned(),
            manifest: serde_json::to_value(artifact.manifest())
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?,
            source_kind,
            source_reference: artifact.source_reference().to_owned(),
            signer_key_id: artifact.signer_key_id().to_owned(),
            signature,
            created_at: current_timestamp()?,
        };
        self.repository
            .register_artifact_audited(input, operator, request_id)
            .await
            .map(AdminPluginArtifact::from_record)
            .map_err(map_catalog_error)
    }

    pub async fn approve_artifact(
        &self,
        principal: SessionPrincipal,
        artifact_id: i64,
        request_id: &str,
    ) -> Result<(), AdminPluginLifecycleError> {
        let resource_id = artifact_id.to_string();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/approve",
            "approve",
            "plugin_artifact",
            Some(&resource_id),
        )
        .await?;
        let result = match current_timestamp() {
            Ok(approved_at) => self
                .repository
                .approve_artifact(PluginArtifactApproval {
                    artifact_id,
                    approved_by: principal.user_id(),
                    approved_at,
                    request_id: request_id.to_owned(),
                })
                .await
                .map_err(map_catalog_error),
            Err(error) => Err(error),
        };
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/approve",
            "approve",
            "plugin_artifact",
            Some(resource_id),
            None,
            Some(json!({ "artifactId": artifact_id })),
            result,
        )
        .await
    }

    pub async fn revoke_artifact(
        &self,
        principal: SessionPrincipal,
        artifact_id: i64,
        reason: String,
        request_id: &str,
    ) -> Result<(), AdminPluginLifecycleError> {
        let resource_id = artifact_id.to_string();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/revoke",
            "revoke",
            "plugin_artifact",
            Some(&resource_id),
        )
        .await?;
        let artifact = self
            .repository
            .get_artifact(artifact_id)
            .await
            .map_err(map_catalog_error);
        let artifact = match artifact {
            Ok(Some(artifact)) => Some(artifact),
            Ok(None) => None,
            Err(error) => {
                return self
                    .finish_mutation(
                        principal,
                        request_id,
                        "/admin/plugins/artifacts/{id}/revoke",
                        "revoke",
                        "plugin_artifact",
                        Some(resource_id),
                        None,
                        Some(json!({ "artifactId": artifact_id })),
                        Err(error),
                    )
                    .await;
            }
        };
        let Some(artifact) = artifact else {
            return self
                .finish_mutation(
                    principal,
                    request_id,
                    "/admin/plugins/artifacts/{id}/revoke",
                    "revoke",
                    "plugin_artifact",
                    Some(resource_id),
                    None,
                    Some(json!({ "artifactId": artifact_id })),
                    Err(AdminPluginLifecycleError::NotFound),
                )
                .await;
        };
        let result = match current_timestamp() {
            Ok(revoked_at) => self
                .repository
                .revoke_artifact(PluginArtifactRevocation {
                    artifact_id,
                    revoked_by: principal.user_id(),
                    revoked_at,
                    request_id: request_id.to_owned(),
                    reason,
                })
                .await
                .map_err(map_catalog_error),
            Err(error) => Err(error),
        };
        if result.is_ok()
            && let Err(error) = self.reconcile_revocation_runtime(&artifact).await
        {
            tracing::warn!(
                plugin_id = artifact.plugin_id,
                error_kind = ?error,
                "plugin runtime stopped after revocation reconciliation failed"
            );
        }
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/artifacts/{id}/revoke",
            "revoke",
            "plugin_artifact",
            Some(resource_id),
            None,
            Some(json!({ "artifactId": artifact_id })),
            result,
        )
        .await
    }

    pub async fn deploy(
        &self,
        principal: SessionPrincipal,
        command: PluginDeploymentCommand,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        let plugin_id = command.plugin_id.clone();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/deployment",
            "deploy",
            "plugin_deployment",
            Some(&plugin_id),
        )
        .await?;
        let config_validation = async {
            if !command.enabled {
                return Ok(());
            }
            let artifact = self
                .repository
                .get_artifact_by_digest(&command.plugin_id, &command.artifact_digest)
                .await
                .map_err(map_catalog_error)?
                .ok_or(AdminPluginLifecycleError::NotFound)?;
            let manifest: PluginManifest = serde_json::from_value(artifact.manifest)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            manifest
                .validate()
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
            validate_public_plugin_configuration(&manifest.config_schema, &command.config)
                .map_err(|_| AdminPluginLifecycleError::InvalidInput)
        }
        .await;
        let result = if command.enabled && self.sidecar_runtime.is_none() {
            Err(AdminPluginLifecycleError::RuntimeUnavailable)
        } else if let Err(error) = config_validation {
            Err(error)
        } else {
            match current_timestamp() {
                Ok(updated_at) => match self
                    .repository
                    .deploy(PluginDeploymentWrite {
                        plugin_id: command.plugin_id.clone(),
                        artifact_digest: command.artifact_digest.clone(),
                        expected_generation: command.expected_generation,
                        enabled: command.enabled,
                        granted_capabilities: command
                            .granted_capabilities
                            .iter()
                            .map(|grant| PluginCapabilityGrant {
                                id: grant.id.clone(),
                                version: grant.version,
                            })
                            .collect(),
                        config: command.config.clone(),
                        updated_by: principal.user_id(),
                        updated_at,
                        request_id: request_id.to_owned(),
                    })
                    .await
                    .map_err(map_catalog_error)
                {
                    Ok(record) => match self.apply_runtime(&record).await {
                        Ok(()) => Ok(AdminPluginDeployment::from_record(record)),
                        Err(error) => {
                            if record.enabled {
                                self.disable_after_activation_failure(
                                    &record, principal, request_id,
                                )
                                .await;
                            }
                            Err(error)
                        }
                    },
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            }
        };
        self.finish_mutation(principal, request_id, "/admin/plugins/{pluginId}/deployment", "deploy", "plugin_deployment", Some(plugin_id), None,
            Some(json!({ "artifactDigest": command.artifact_digest, "enabled": command.enabled, "generation": command.expected_generation })), result).await
    }

    pub async fn disable(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        expected_generation: i64,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        let resource_id = plugin_id.to_owned();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/disable",
            "disable",
            "plugin_deployment",
            Some(&resource_id),
        )
        .await?;
        let result = async {
            let current = self
                .repository
                .get_deployment(plugin_id)
                .await
                .map_err(map_catalog_error)?
                .ok_or(AdminPluginLifecycleError::NotFound)?;
            if current.generation != expected_generation {
                return Err(AdminPluginLifecycleError::Conflict);
            }
            if !current.enabled {
                self.apply_runtime(&current).await?;
                return Ok(AdminPluginDeployment::from_record(current));
            }
            let record = self
                .repository
                .deploy(PluginDeploymentWrite {
                    plugin_id: current.plugin_id.clone(),
                    artifact_digest: current.artifact.digest.clone(),
                    expected_generation: Some(expected_generation),
                    enabled: false,
                    granted_capabilities: current.granted_capabilities.clone(),
                    config: current.config.clone(),
                    updated_by: principal.user_id(),
                    updated_at: current_timestamp()?,
                    request_id: request_id.to_owned(),
                })
                .await
                .map_err(map_catalog_error)?;
            self.apply_runtime(&record).await?;
            Ok(AdminPluginDeployment::from_record(record))
        }
        .await;
        self.finish_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/disable",
            "disable",
            "plugin_deployment",
            Some(resource_id),
            None,
            Some(json!({ "enabled": false, "expectedGeneration": expected_generation })),
            result,
        )
        .await
    }

    pub async fn rollback(
        &self,
        principal: SessionPrincipal,
        plugin_id: &str,
        expected_generation: i64,
        target_generation: i64,
        request_id: &str,
    ) -> Result<AdminPluginDeployment, AdminPluginLifecycleError> {
        let resource_id = plugin_id.to_owned();
        self.authorize_mutation(
            principal,
            request_id,
            "/admin/plugins/{pluginId}/rollback",
            "rollback",
            "plugin_deployment",
            Some(&resource_id),
        )
        .await?;
        let result = match current_timestamp() {
            Ok(updated_at) => match self
                .repository
                .rollback(
                    plugin_id,
                    expected_generation,
                    target_generation,
                    principal.user_id(),
                    updated_at,
                    request_id.to_owned(),
                )
                .await
                .map_err(map_catalog_error)
            {
                Ok(record) => match self.apply_runtime(&record).await {
                    Ok(()) => Ok(AdminPluginDeployment::from_record(record)),
                    Err(error) => {
                        if record.enabled {
                            self.disable_after_activation_failure(&record, principal, request_id)
                                .await;
                        }
                        Err(error)
                    }
                },
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        self.finish_mutation(principal, request_id, "/admin/plugins/{pluginId}/rollback", "rollback", "plugin_deployment", Some(resource_id), None,
            Some(json!({ "expectedGeneration": expected_generation, "targetGeneration": target_generation })), result).await
    }

    async fn authorize(
        &self,
        principal: SessionPrincipal,
    ) -> Result<(), AdminPluginLifecycleError> {
        if principal.role() == SessionRole::Admin {
            PlatformPolicy::authorize(principal, PLUGIN_PERMISSION).map_err(map_audit_error)
        } else {
            Err(AdminPluginLifecycleError::Forbidden)
        }
    }

    async fn authorize_mutation(
        &self,
        principal: SessionPrincipal,
        request_id: &str,
        route: &'static str,
        operation: &'static str,
        resource: &'static str,
        resource_id: Option<&str>,
    ) -> Result<(), AdminPluginLifecycleError> {
        if self.authorize(principal).await.is_ok() {
            // Reject an unusable correlation id before any persistence call, so
            // a repository mutation cannot commit without an audit record.
            let _ = audit_entry(
                principal,
                request_id,
                route,
                operation,
                resource,
                resource_id.map(str::to_owned),
                PlatformAuditOutcome::Succeeded,
                None,
                None,
            )?;
            return Ok(());
        }
        let entry = audit_entry(
            principal,
            request_id,
            route,
            operation,
            resource,
            resource_id.map(str::to_owned),
            PlatformAuditOutcome::Denied,
            None,
            None,
        )?;
        self.audit.record(entry).await.map_err(map_audit_error)?;
        Err(AdminPluginLifecycleError::Forbidden)
    }

    #[allow(clippy::too_many_arguments)]
    async fn finish_mutation<T>(
        &self,
        principal: SessionPrincipal,
        request_id: &str,
        route: &'static str,
        operation: &'static str,
        resource: &'static str,
        resource_id: Option<String>,
        before: Option<Value>,
        after: Option<Value>,
        result: Result<T, AdminPluginLifecycleError>,
    ) -> Result<T, AdminPluginLifecycleError> {
        let succeeded = result.is_ok();
        let outcome = if succeeded {
            PlatformAuditOutcome::Succeeded
        } else {
            PlatformAuditOutcome::Failed
        };
        // Catalog facts commit atomically with mutations. A failure in this
        // additional outcome log must not mask success or a runtime failure.
        let audit = audit_entry(
            principal,
            request_id,
            route,
            operation,
            resource,
            resource_id,
            outcome,
            before,
            if succeeded { after } else { None },
        );
        let audit_result = match audit {
            Ok(entry) => self.audit.record(entry).await.map_err(map_audit_error),
            Err(error) => Err(error),
        };
        if let Err(error) = audit_result {
            tracing::warn!(
                request_id,
                route,
                operation,
                resource,
                succeeded,
                error_kind = ?error,
                "plugin lifecycle outcome audit failed; preserving the operation result"
            );
        }
        result
    }
}

fn cleanup_stale_plugin_uploads(directory: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten().take(10_000) {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".plugin-upload-")
        {
            continue;
        }
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        if metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age >= STALE_PLUGIN_UPLOAD_AGE)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

impl fmt::Debug for DatabaseAdminPluginLifecycleService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminPluginLifecycleService(<redacted>)")
    }
}

#[allow(clippy::too_many_arguments)]
fn audit_entry(
    principal: SessionPrincipal,
    request_id: &str,
    route: &'static str,
    operation: &'static str,
    resource: &'static str,
    resource_id: Option<String>,
    outcome: PlatformAuditOutcome,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<PlatformAuditEntry, AdminPluginLifecycleError> {
    PlatformAuditEntry::new(
        principal,
        PLUGIN_PERMISSION,
        route,
        operation,
        resource,
        resource_id,
        outcome,
        before,
        after,
        None,
        request_id,
    )
    .map_err(map_audit_error)
}

fn current_timestamp() -> Result<i64, AdminPluginLifecycleError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| AdminPluginLifecycleError::Internal)?
            .as_secs(),
    )
    .map_err(|_| AdminPluginLifecycleError::Internal)
}

fn map_catalog_error(error: PluginCatalogError) -> AdminPluginLifecycleError {
    match error {
        PluginCatalogError::InvalidInput => AdminPluginLifecycleError::InvalidInput,
        PluginCatalogError::NotFound => AdminPluginLifecycleError::NotFound,
        PluginCatalogError::Conflict => AdminPluginLifecycleError::Conflict,
        PluginCatalogError::Unavailable
        | PluginCatalogError::Timeout
        | PluginCatalogError::Invariant => AdminPluginLifecycleError::Internal,
    }
}

fn map_audit_error(error: PlatformAuditError) -> AdminPluginLifecycleError {
    match error {
        PlatformAuditError::InvalidInput => AdminPluginLifecycleError::InvalidInput,
        PlatformAuditError::Forbidden => AdminPluginLifecycleError::Forbidden,
        PlatformAuditError::Internal => AdminPluginLifecycleError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_projection_never_serializes_storage_reference_or_signature() {
        let artifact = AdminPluginArtifact {
            id: 7,
            plugin_id: "org.example.catalog".to_owned(),
            plugin_version: "1.0.0".to_owned(),
            digest: "a".repeat(64),
            manifest: json!({ "pluginId": "org.example.catalog" }),
            source_kind: "local_process",
            signer_key_id: "publisher-1".to_owned(),
            created_at: 1_800_000_000,
            approval: None,
            revocation: None,
        };

        let serialized = serde_json::to_value(&artifact).unwrap();
        assert!(serialized.get("sourceReference").is_none());
        assert!(serialized.get("signature").is_none());
        assert!(serialized.get("signatureBase64").is_none());
        let debug = format!("{artifact:?}");
        assert!(!debug.contains("sourceReference"));
        assert!(!debug.contains("signature"));
    }

    #[test]
    fn deployment_command_debug_redacts_configuration() {
        let command = PluginDeploymentCommand::new(
            "org.example.catalog".to_owned(),
            "a".repeat(64),
            None,
            true,
            vec![AdminPluginCapabilityGrant::new("catalog.models.read".to_owned(), 1).unwrap()],
            json!({ "credential": "sensitive-value" }),
        )
        .unwrap();

        let debug = format!("{command:?}");
        assert!(!debug.contains("sensitive-value"));
        assert!(debug.contains("<redacted>"));
    }
}
