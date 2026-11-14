//! Trusted, signed, process-isolated sidecar transport.
//!
//! This supervisor isolates process lifetime and framing. It does not claim an
//! OS sandbox: approved sidecars run with the host account's OS permissions.

use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    CapabilityContext, CapabilityExecutionContext, CapabilityHandler, ConnectorRegistration,
    PlatformConnector, schema::CompiledJsonSchema,
};
use af_plugin_contract::{
    CapabilityAccess, CapabilityId, CapabilityResult, ConnectorError, ConnectorErrorCode,
    ConnectorHealth, ConnectorHealthStatus, ContractValidationError, ContractVersion,
    InvocationMetadata, PluginHostOperation, PluginHostRequest, PluginHostResponse,
    PluginHostResult, PluginId, PluginManifest, PluginVersion, SideEffectClass,
};
use async_trait::async_trait;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
#[cfg(unix)]
use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use ring::signature::{ED25519, UnparsedPublicKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::Mutex,
    time::timeout,
};

const SIDE_CAR_PROTOCOL_VERSION: u16 = 1;
const MAX_SIGNATURE_BYTES: usize = 64;
const MAX_PACKAGE_FILES: usize = 100_000;
const DEFAULT_MAX_FRAME_BYTES: usize = 1_048_576;
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SIGNATURE_DOMAIN: &[u8] = b"ANYFLOWS_PLUGIN_ARTIFACT_V2\0";

/// Validate the host-selected public configuration before it is persisted or
/// sent to a sidecar. Field-name screening only blocks common credential keys;
/// callers must still keep secret values out of public fields.
pub fn validate_public_plugin_configuration(
    schema: &Value,
    configuration: &Value,
) -> Result<(), ContractValidationError> {
    CompiledJsonSchema::compile(schema)?.validate(configuration)?;
    if contains_sensitive_fields(configuration) {
        return Err(ContractValidationError::InvalidConfiguration);
    }
    Ok(())
}

/// Ed25519 signer identity trusted by the host. Public keys are not secrets.
#[derive(Clone, Eq, PartialEq)]
pub struct TrustedPluginKey {
    key_id: String,
    public_key: [u8; 32],
}

impl TrustedPluginKey {
    pub fn new(key_id: impl Into<String>, public_key: [u8; 32]) -> Result<Self, SidecarError> {
        let key_id = key_id.into();
        if key_id.is_empty() || key_id.len() > 128 || key_id.chars().any(char::is_control) {
            return Err(SidecarError::InvalidArtifact);
        }
        Ok(Self { key_id, public_key })
    }

    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
}

impl fmt::Debug for TrustedPluginKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedPluginKey")
            .field("key_id", &self.key_id)
            .field("public_key", &"<redacted>")
            .finish()
    }
}

/// A local artifact explicitly approved by the host operator.
pub struct PluginArtifactDescriptor {
    plugin_id: PluginId,
    plugin_version: PluginVersion,
    manifest: PluginManifest,
    executable: PathBuf,
    package_root: PathBuf,
    sha256: String,
    signer_key_id: String,
    signature: Vec<u8>,
    source_kind: String,
    source_reference: String,
    staging_parent: Option<PathBuf>,
}

impl PluginArtifactDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_id: PluginId,
        plugin_version: PluginVersion,
        manifest: PluginManifest,
        executable: PathBuf,
        package_root: PathBuf,
        sha256: String,
        signer_key_id: String,
        signature_base64: &str,
        source_kind: String,
        source_reference: String,
    ) -> Result<Self, SidecarError> {
        if sha256.len() != 64
            || !sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || signer_key_id.is_empty() != signature_base64.is_empty()
            || signer_key_id.len() > 128
            || signer_key_id.chars().any(char::is_control)
            || signature_base64.len() > 128
            || !executable.is_absolute()
            || !package_root.is_absolute()
            || !matches!(source_kind.as_str(), "builtin" | "local_process")
            || source_reference.is_empty()
            || source_reference.len() > 2048
            || source_reference.chars().any(char::is_control)
            || manifest.plugin_id != plugin_id
            || manifest.plugin_version != plugin_version
            || manifest.validate().is_err()
        {
            return Err(SidecarError::InvalidArtifact);
        }
        let signature = BASE64
            .decode(signature_base64)
            .map_err(|_| SidecarError::InvalidArtifact)?;
        if !signature.is_empty() && signature.len() != MAX_SIGNATURE_BYTES {
            return Err(SidecarError::InvalidArtifact);
        }
        Ok(Self {
            plugin_id,
            plugin_version,
            manifest,
            executable,
            package_root,
            sha256,
            signer_key_id,
            signature,
            source_kind,
            source_reference,
            staging_parent: None,
        })
    }

    /// Select a host-owned executable staging directory. This path is host
    /// configuration, never publisher or API metadata.
    pub fn with_staging_parent(mut self, parent: PathBuf) -> Result<Self, SidecarError> {
        if !parent.is_absolute() {
            return Err(SidecarError::InvalidArtifact);
        }
        let metadata = fs::symlink_metadata(&parent).map_err(|_| SidecarError::InvalidArtifact)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(SidecarError::InvalidArtifact);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o077 != 0 {
                return Err(SidecarError::InvalidArtifact);
            }
        }
        let parent = parent
            .canonicalize()
            .map_err(|_| SidecarError::InvalidArtifact)?;
        self.staging_parent = Some(parent);
        Ok(self)
    }

    #[must_use]
    pub fn plugin_id(&self) -> &PluginId {
        &self.plugin_id
    }

    #[must_use]
    pub fn plugin_version(&self) -> &PluginVersion {
        &self.plugin_version
    }

    #[must_use]
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn signer_key_id(&self) -> &str {
        &self.signer_key_id
    }

    #[must_use]
    pub fn signature_base64(&self) -> String {
        BASE64.encode(&self.signature)
    }

    #[must_use]
    pub fn source_kind(&self) -> &str {
        &self.source_kind
    }

    #[must_use]
    pub fn source_reference(&self) -> &str {
        &self.source_reference
    }
}

impl fmt::Debug for PluginArtifactDescriptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginArtifactDescriptor")
            .field("plugin_id", &self.plugin_id)
            .field("plugin_version", &self.plugin_version)
            .field("manifest", &"<redacted>")
            .field("executable", &"<approved local path>")
            .field("package_root", &"<approved local path>")
            .field("sha256", &self.sha256)
            .field("signer_key_id", &self.signer_key_id)
            .field("signature", &"<redacted>")
            .finish()
    }
}

/// Host-approved artifact after package verification. Publisher-managed
/// artifacts additionally carry a verified signature; operator-managed ones
/// have an empty signer identity and signature.
pub struct VerifiedPluginArtifact {
    plugin_id: PluginId,
    plugin_version: PluginVersion,
    manifest: PluginManifest,
    config_validator: CompiledJsonSchema,
    executable: PathBuf,
    package_root: PathBuf,
    _staging_directory: tempfile::TempDir,
    sha256: String,
    signer_key_id: String,
    signature: Vec<u8>,
    source_kind: String,
    source_reference: String,
}

impl VerifiedPluginArtifact {
    /// Verify a local executable against its exact digest and a host trust key.
    /// No download, shell command, manifest-provided path, or implicit key trust
    /// is involved in this operation.
    pub async fn verify(
        descriptor: PluginArtifactDescriptor,
        trusted_key: &TrustedPluginKey,
    ) -> Result<Self, SidecarError> {
        if descriptor.signer_key_id != trusted_key.key_id {
            return Err(SidecarError::UntrustedSigner);
        }
        Self::verify_package(descriptor, Some(trusted_key)).await
    }

    /// Verify an artifact explicitly selected and managed by the host operator.
    /// This verifies package integrity, manifest and staging, but does not
    /// assert a publisher identity or accept a supplied publisher signature.
    pub async fn verify_operator_managed(
        descriptor: PluginArtifactDescriptor,
    ) -> Result<Self, SidecarError> {
        if !descriptor.signer_key_id.is_empty() || !descriptor.signature.is_empty() {
            return Err(SidecarError::InvalidArtifact);
        }
        Self::verify_package(descriptor, None).await
    }

    async fn verify_package(
        descriptor: PluginArtifactDescriptor,
        trusted_key: Option<&TrustedPluginKey>,
    ) -> Result<Self, SidecarError> {
        let config_validator = CompiledJsonSchema::compile(&descriptor.manifest.config_schema)
            .map_err(|_| SidecarError::InvalidArtifact)?;
        let package_root = tokio::fs::canonicalize(&descriptor.package_root)
            .await
            .map_err(|_| SidecarError::InvalidArtifact)?;
        let executable = tokio::fs::canonicalize(&descriptor.executable)
            .await
            .map_err(|_| SidecarError::InvalidArtifact)?;
        let executable_metadata = tokio::fs::metadata(&executable)
            .await
            .map_err(|_| SidecarError::InvalidArtifact)?;
        if !executable.starts_with(&package_root) || !executable_metadata.is_file() {
            return Err(SidecarError::InvalidArtifact);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if executable_metadata.permissions().mode() & 0o111 == 0 {
                return Err(SidecarError::InvalidArtifact);
            }
        }
        let root_for_hash = package_root.clone();
        let executable_for_hash = executable.clone();
        let actual_digest = tokio::task::spawn_blocking(move || {
            plugin_package_digest(&root_for_hash, &executable_for_hash)
        })
        .await
        .map_err(|_| SidecarError::InvalidArtifact)??;
        if actual_digest != descriptor.sha256 {
            return Err(SidecarError::ArtifactDigestMismatch);
        }
        let executable_relative_path = executable
            .strip_prefix(&package_root)
            .map_err(|_| SidecarError::InvalidArtifact)?
            .to_str()
            .ok_or(SidecarError::InvalidArtifact)?
            .replace('\\', "/");
        if executable_relative_path.is_empty()
            || executable_relative_path
                .split('/')
                .any(|component| component.is_empty() || component == "." || component == "..")
        {
            return Err(SidecarError::InvalidArtifact);
        }
        if let Some(trusted_key) = trusted_key {
            let signed_message = plugin_artifact_signature_payload(
                &descriptor.plugin_id,
                &descriptor.plugin_version,
                &actual_digest,
                &executable_relative_path,
                &descriptor.manifest,
            )?;
            UnparsedPublicKey::new(&ED25519, trusted_key.public_key)
                .verify(&signed_message, &descriptor.signature)
                .map_err(|_| SidecarError::InvalidArtifactSignature)?;
        }

        // Copy into a host-created private directory and verify the copied
        // bytes before execution. This prevents a writer outside the service
        // account from changing the source tree after verification; it is not
        // protection against another process running as the same account.
        let source_root = package_root.clone();
        let relative_executable = PathBuf::from(&executable_relative_path);
        let expected_digest = actual_digest.clone();
        let staging_parent = descriptor.staging_parent;
        let (staging_directory, staged_root, executable) = tokio::task::spawn_blocking(move || {
            stage_verified_package(
                &source_root,
                &relative_executable,
                &expected_digest,
                staging_parent.as_deref(),
            )
        })
        .await
        .map_err(|_| SidecarError::InvalidArtifact)??;

        Ok(Self {
            plugin_id: descriptor.plugin_id,
            plugin_version: descriptor.plugin_version,
            manifest: descriptor.manifest,
            config_validator,
            executable,
            package_root: staged_root,
            _staging_directory: staging_directory,
            sha256: actual_digest,
            signer_key_id: descriptor.signer_key_id,
            signature: descriptor.signature,
            source_kind: descriptor.source_kind,
            source_reference: descriptor.source_reference,
        })
    }

    #[must_use]
    pub fn plugin_id(&self) -> &PluginId {
        &self.plugin_id
    }

    #[must_use]
    pub fn plugin_version(&self) -> &PluginVersion {
        &self.plugin_version
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[must_use]
    pub fn signer_key_id(&self) -> &str {
        &self.signer_key_id
    }

    /// Return the verified Ed25519 signature for immutable catalog persistence,
    /// or an empty string for operator-managed artifacts. Debug output and
    /// public HTTP projections remain redacted.
    #[must_use]
    pub fn signature_base64(&self) -> String {
        BASE64.encode(&self.signature)
    }

    #[must_use]
    pub fn source_kind(&self) -> &str {
        &self.source_kind
    }

    #[must_use]
    pub fn source_reference(&self) -> &str {
        &self.source_reference
    }

    #[must_use]
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Validate operator-supplied deployment configuration against the
    /// verified manifest before launching the sidecar.
    pub fn validate_configuration(&self, configuration: &Value) -> Result<(), SidecarError> {
        self.config_validator
            .validate(configuration)
            .map_err(|_| SidecarError::InvalidConfiguration)?;
        if contains_sensitive_fields(configuration) {
            return Err(SidecarError::SensitivePayload);
        }
        Ok(())
    }
}

fn stage_verified_package(
    source_root: &Path,
    relative_executable: &Path,
    expected_digest: &str,
    staging_parent: Option<&Path>,
) -> Result<(tempfile::TempDir, PathBuf, PathBuf), SidecarError> {
    let mut builder = tempfile::Builder::new();
    builder.prefix("anyflows-plugin-");
    let staging_directory = match staging_parent {
        Some(parent) => {
            if parent.starts_with(source_root) {
                return Err(SidecarError::InvalidArtifact);
            }
            builder.tempdir_in(parent)
        }
        None => builder.tempdir(),
    }
    .map_err(|_| SidecarError::InvalidArtifact)?;
    let staged_root = staging_directory.path().to_path_buf();
    let executable = staged_root.join(relative_executable);
    let mut total_entries = 0_usize;
    let mut total_size = 0_u64;
    copy_package_directory(
        source_root,
        &staged_root,
        &executable,
        &mut total_entries,
        &mut total_size,
    )?;
    let actual_digest = plugin_package_digest(&staged_root, &executable)?;
    if actual_digest != expected_digest {
        return Err(SidecarError::ArtifactDigestMismatch);
    }
    Ok((staging_directory, staged_root, executable))
}

fn copy_package_directory(
    source: &Path,
    target: &Path,
    executable: &Path,
    total_entries: &mut usize,
    total_size: &mut u64,
) -> Result<(), SidecarError> {
    let mut pending = vec![(source.to_path_buf(), target.to_path_buf())];
    while let Some((source_directory, target_directory)) = pending.pop() {
        for entry in fs::read_dir(source_directory).map_err(|_| SidecarError::InvalidArtifact)? {
            *total_entries = (*total_entries).saturating_add(1);
            if *total_entries > MAX_PACKAGE_FILES {
                return Err(SidecarError::InvalidArtifact);
            }
            let entry = entry.map_err(|_| SidecarError::InvalidArtifact)?;
            let source_path = entry.path();
            let target_path = target_directory.join(entry.file_name());
            let metadata =
                fs::symlink_metadata(&source_path).map_err(|_| SidecarError::InvalidArtifact)?;
            if metadata.file_type().is_symlink() {
                return Err(SidecarError::InvalidArtifact);
            }
            if metadata.is_dir() {
                fs::create_dir(&target_path).map_err(|_| SidecarError::InvalidArtifact)?;
                pending.push((source_path, target_path));
            } else if metadata.is_file() {
                *total_size = (*total_size)
                    .checked_add(metadata.len())
                    .filter(|size| *size <= 512 * 1024 * 1024)
                    .ok_or(SidecarError::InvalidArtifact)?;
                let mut input =
                    fs::File::open(&source_path).map_err(|_| SidecarError::InvalidArtifact)?;
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target_path)
                    .map_err(|_| SidecarError::InvalidArtifact)?;
                let mut copied = 0_u64;
                let mut buffer = [0_u8; 64 * 1024];
                loop {
                    let read = input
                        .read(&mut buffer)
                        .map_err(|_| SidecarError::InvalidArtifact)?;
                    if read == 0 {
                        break;
                    }
                    copied = copied
                        .checked_add(read as u64)
                        .filter(|size| *size <= metadata.len())
                        .ok_or(SidecarError::InvalidArtifact)?;
                    output
                        .write_all(&buffer[..read])
                        .map_err(|_| SidecarError::InvalidArtifact)?;
                }
                if copied != metadata.len() {
                    return Err(SidecarError::InvalidArtifact);
                }
                fs::set_permissions(
                    &target_path,
                    sanitized_permissions(metadata.permissions(), target_path == executable),
                )
                .map_err(|_| SidecarError::InvalidArtifact)?;
            } else {
                return Err(SidecarError::InvalidArtifact);
            }
        }
    }
    Ok(())
}

fn sanitized_permissions(mut permissions: fs::Permissions, executable: bool) -> fs::Permissions {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(if executable { 0o500 } else { 0o400 });
    }
    #[cfg(not(unix))]
    {
        let _ = executable;
        // Windows ACLs are inherited from the host-owned staging directory.
        // Avoid the readonly attribute, which prevents TempDir cleanup.
        permissions.set_readonly(false);
    }
    permissions
}

/// Compute the canonical SHA-256 digest used by signed plugin packages.
/// Directory entries are included so adding an empty directory cannot change
/// runtime-visible package layout without changing the signature.
pub fn plugin_package_digest(root: &Path, executable: &Path) -> Result<String, SidecarError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = BTreeMap::new();
    let mut total_size = 0_u64;
    let mut total_entries = 0_usize;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|_| SidecarError::InvalidArtifact)? {
            total_entries = total_entries.saturating_add(1);
            if total_entries > MAX_PACKAGE_FILES {
                return Err(SidecarError::InvalidArtifact);
            }
            let entry = entry.map_err(|_| SidecarError::InvalidArtifact)?;
            let path = entry.path();
            let metadata =
                std::fs::symlink_metadata(&path).map_err(|_| SidecarError::InvalidArtifact)?;
            if metadata.file_type().is_symlink() {
                return Err(SidecarError::InvalidArtifact);
            }
            if metadata.is_dir() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| SidecarError::InvalidArtifact)?
                    .to_str()
                    .ok_or(SidecarError::InvalidArtifact)?
                    .replace('\\', "/");
                if relative.is_empty()
                    || relative.len() > 4096
                    || files
                        .insert(relative, (0_u8, path.clone(), 0_u64))
                        .is_some()
                {
                    return Err(SidecarError::InvalidArtifact);
                }
                pending.push(path);
            } else if metadata.is_file() {
                total_size = total_size
                    .checked_add(metadata.len())
                    .filter(|size| *size <= 512 * 1024 * 1024)
                    .ok_or(SidecarError::InvalidArtifact)?;
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| SidecarError::InvalidArtifact)?
                    .to_str()
                    .ok_or(SidecarError::InvalidArtifact)?
                    .replace('\\', "/");
                if relative.is_empty() || relative.len() > 4096 {
                    return Err(SidecarError::InvalidArtifact);
                }
                if files
                    .insert(relative, (1_u8, path, metadata.len()))
                    .is_some()
                {
                    return Err(SidecarError::InvalidArtifact);
                }
            } else {
                return Err(SidecarError::InvalidArtifact);
            }
        }
    }
    let executable_relative = executable
        .strip_prefix(root)
        .map_err(|_| SidecarError::InvalidArtifact)?
        .to_str()
        .ok_or(SidecarError::InvalidArtifact)?
        .replace('\\', "/");
    if !files
        .get(&executable_relative)
        .is_some_and(|(kind, _, _)| *kind == 1)
    {
        return Err(SidecarError::InvalidArtifact);
    }
    let mut digest = Sha256::new();
    digest.update(b"ANYFLOWS_PLUGIN_PACKAGE_V2\0");
    for (relative, (kind, path, length)) in files {
        digest.update([kind]);
        digest.update((relative.len() as u64).to_le_bytes());
        digest.update(relative.as_bytes());
        if kind == 0 {
            continue;
        }
        digest.update(length.to_le_bytes());
        let mut file = std::fs::File::open(path).map_err(|_| SidecarError::InvalidArtifact)?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|_| SidecarError::InvalidArtifact)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Build the exact Ed25519 message that trusted plugin publishers must sign.
pub fn plugin_artifact_signature_payload(
    plugin_id: &PluginId,
    plugin_version: &PluginVersion,
    sha256: &str,
    executable_relative_path: &str,
    manifest: &PluginManifest,
) -> Result<Vec<u8>, SidecarError> {
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || executable_relative_path.is_empty()
        || executable_relative_path.len() > 4096
        || executable_relative_path.starts_with('/')
        || executable_relative_path.contains('\\')
        || executable_relative_path.chars().any(char::is_control)
        || executable_relative_path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(SidecarError::InvalidArtifact);
    }
    let manifest = serde_json::to_value(manifest).map_err(|_| SidecarError::InvalidArtifact)?;
    let manifest = canonicalize_json(manifest);
    let manifest = serde_json::to_vec(&manifest).map_err(|_| SidecarError::InvalidArtifact)?;
    let mut message = Vec::with_capacity(
        SIGNATURE_DOMAIN.len()
            + plugin_id.as_str().len()
            + plugin_version.as_str().len()
            + sha256.len()
            + executable_relative_path.len()
            + manifest.len()
            + 4,
    );
    message.extend_from_slice(SIGNATURE_DOMAIN);
    message.extend_from_slice(plugin_id.as_str().as_bytes());
    message.push(b'\n');
    message.extend_from_slice(plugin_version.as_str().as_bytes());
    message.push(b'\n');
    message.extend_from_slice(sha256.as_bytes());
    message.push(b'\n');
    message.extend_from_slice(executable_relative_path.as_bytes());
    message.push(b'\n');
    message.extend_from_slice(&manifest);
    Ok(message)
}

/// Hard ceilings for the host-controlled sidecar protocol.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SidecarLimits {
    pub max_frame_bytes: usize,
    pub startup_timeout: Duration,
    pub request_timeout: Duration,
}

impl Default for SidecarLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: DEFAULT_MAX_FRAME_BYTES,
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            request_timeout: MAX_REQUEST_TIMEOUT,
        }
    }
}

impl SidecarLimits {
    fn validate(self) -> Result<Self, SidecarError> {
        if !(1..=16 * 1024 * 1024).contains(&self.max_frame_bytes)
            || self.startup_timeout.is_zero()
            || self.startup_timeout > Duration::from_secs(30)
            || self.request_timeout.is_zero()
            || self.request_timeout > MAX_REQUEST_TIMEOUT
        {
            return Err(SidecarError::InvalidConfiguration);
        }
        Ok(self)
    }
}

struct SidecarProcess {
    child: Child,
    process_group_id: Option<i32>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_request_id: u64,
}

impl SidecarProcess {
    fn kill_process_group(&mut self) {
        let process_group_id = self.process_group_id.take();
        #[cfg(unix)]
        if let Some(process_group_id) = process_group_id {
            let _ = killpg(Pid::from_raw(process_group_id), Signal::SIGKILL);
        }
        #[cfg(not(unix))]
        let _ = process_group_id;
    }

    async fn terminate(&mut self) {
        self.kill_process_group();
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
    }
}

impl Drop for SidecarProcess {
    fn drop(&mut self) {
        self.kill_process_group();
        let _ = self.child.start_kill();
    }
}

struct InFlightSidecarExchange<'a> {
    process: &'a mut SidecarProcess,
    complete: bool,
}

impl Drop for InFlightSidecarExchange<'_> {
    fn drop(&mut self) {
        if !self.complete {
            // A cancelled future can leave a partial request or unread
            // response. Never reuse that stream for another invocation.
            self.process.kill_process_group();
            let _ = self.process.child.start_kill();
        }
    }
}

/// One trusted, signed sidecar process. All requests are serialized on its
/// stdio stream; frame size, timeout, identity and capability schemas are
/// validated by the host. This is process fault isolation, not an OS sandbox.
pub struct SidecarSession {
    manifest: PluginManifest,
    deployment_generation: u64,
    granted_capabilities: BTreeMap<CapabilityId, u16>,
    public_config: Value,
    limits: SidecarLimits,
    process: Mutex<SidecarProcess>,
    // Drop the child process before removing its staged executable directory.
    artifact: Arc<VerifiedPluginArtifact>,
}

/// In-memory owner for verified sidecar processes. Durable desired state lives
/// in the host catalog; callers reconcile that state into this supervisor.
#[derive(Default)]
pub struct SidecarSupervisor {
    state: Mutex<SidecarSupervisorState>,
}

#[derive(Default)]
struct SidecarSupervisorState {
    sessions: BTreeMap<PluginId, Arc<SidecarSession>>,
    stopped_through: BTreeMap<PluginId, u64>,
    highest_generation: BTreeMap<PluginId, u64>,
    shutting_down: bool,
}

fn can_publish_generation(
    stopped_through: Option<u64>,
    highest_published: Option<u64>,
    current_generation: Option<u64>,
    candidate: u64,
) -> bool {
    !stopped_through.is_some_and(|stopped| stopped >= candidate)
        && !highest_published.is_some_and(|highest| candidate < highest)
        && !current_generation.is_some_and(|current| current >= candidate)
}

impl fmt::Debug for SidecarSupervisor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SidecarSupervisor(<processes redacted>)")
    }
}

impl SidecarSupervisor {
    /// Start and atomically publish a newer verified deployment. The previous
    /// process is stopped only after the replacement passes its handshake.
    pub async fn activate(
        &self,
        artifact: VerifiedPluginArtifact,
        generation: u64,
        granted_capabilities: Vec<(CapabilityId, u16)>,
        public_config: Value,
        limits: SidecarLimits,
    ) -> Result<Arc<SidecarSession>, SidecarError> {
        let session = SidecarSession::start(
            artifact,
            generation,
            granted_capabilities,
            public_config,
            limits,
        )
        .await?;
        if session.registration().is_err() {
            session.stop().await;
            return Err(SidecarError::CapabilityNotGranted);
        }
        let plugin_id = session.manifest.plugin_id.clone();
        let old_session = {
            let mut state = self.state.lock().await;
            let current_generation = state
                .sessions
                .get(&plugin_id)
                .map(|current| current.deployment_generation);
            if state.shutting_down
                || !can_publish_generation(
                    state.stopped_through.get(&plugin_id).copied(),
                    state.highest_generation.get(&plugin_id).copied(),
                    current_generation,
                    generation,
                )
            {
                drop(state);
                session.stop().await;
                return Err(SidecarError::InvalidConfiguration);
            }
            state
                .highest_generation
                .entry(plugin_id.clone())
                .and_modify(|highest| *highest = (*highest).max(generation))
                .or_insert(generation);
            state.sessions.insert(plugin_id, Arc::clone(&session))
        };
        if let Some(old_session) = old_session {
            old_session.stop().await;
        }
        Ok(session)
    }

    /// Stop the currently active process only when the caller still owns its
    /// deployment generation, preventing stale disable requests from stopping
    /// a newer deployment.
    pub async fn deactivate(
        &self,
        plugin_id: &PluginId,
        expected_generation: u64,
    ) -> Result<(), SidecarError> {
        let session = {
            let mut state = self.state.lock().await;
            let current = state
                .sessions
                .get(plugin_id)
                .ok_or(SidecarError::ProcessExited)?;
            if current.deployment_generation != expected_generation {
                return Err(SidecarError::InvalidConfiguration);
            }
            let stopped = state.stopped_through.entry(plugin_id.clone()).or_default();
            *stopped = (*stopped).max(expected_generation);
            state.sessions.remove(plugin_id)
        };
        if let Some(session) = session {
            session.stop().await;
        }
        Ok(())
    }

    /// Stop a failed session only if it is still the exact generation whose
    /// health check failed. A newer concurrently published deployment wins.
    pub async fn stop_generation(&self, plugin_id: &PluginId, generation: u64) -> bool {
        let session = {
            let mut state = self.state.lock().await;
            if state
                .sessions
                .get(plugin_id)
                .is_some_and(|current| current.deployment_generation == generation)
            {
                state.sessions.remove(plugin_id)
            } else {
                None
            }
        };
        if let Some(session) = session {
            session.stop().await;
            true
        } else {
            false
        }
    }

    /// Stop an older or failed deployment while preserving a process that was
    /// already activated for a newer committed generation.
    pub async fn stop_through_generation(
        &self,
        plugin_id: &PluginId,
        committed_generation: u64,
    ) -> bool {
        let session = {
            let mut state = self.state.lock().await;
            let stopped = state.stopped_through.entry(plugin_id.clone()).or_default();
            *stopped = (*stopped).max(committed_generation);
            if state
                .sessions
                .get(plugin_id)
                .is_some_and(|current| current.deployment_generation <= committed_generation)
            {
                state.sessions.remove(plugin_id)
            } else {
                None
            }
        };
        if let Some(session) = session {
            session.stop().await;
            true
        } else {
            false
        }
    }

    /// Return an active session only for the exact durable deployment version.
    pub async fn session(
        &self,
        plugin_id: &PluginId,
        generation: u64,
    ) -> Result<Arc<SidecarSession>, SidecarError> {
        let state = self.state.lock().await;
        let session = state
            .sessions
            .get(plugin_id)
            .filter(|session| session.deployment_generation == generation)
            .cloned()
            .ok_or(SidecarError::ProcessExited)?;
        Ok(session)
    }

    /// Stop all managed child processes during graceful host shutdown.
    pub async fn shutdown(&self) {
        let sessions = {
            let mut state = self.state.lock().await;
            state.shutting_down = true;
            std::mem::take(&mut state.sessions)
        };
        for session in sessions.into_values() {
            session.stop().await;
        }
    }
}

impl fmt::Debug for SidecarSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SidecarSession")
            .field("plugin_id", &self.manifest.plugin_id)
            .field("plugin_version", &self.manifest.plugin_version)
            .field("artifact_sha256", &self.artifact.sha256)
            .field("signer_key_id", &self.artifact.signer_key_id)
            .field("limits", &self.limits)
            .finish()
    }
}

impl SidecarSession {
    pub async fn start(
        artifact: VerifiedPluginArtifact,
        deployment_generation: u64,
        granted_capabilities: Vec<(CapabilityId, u16)>,
        public_config: Value,
        limits: SidecarLimits,
    ) -> Result<Arc<Self>, SidecarError> {
        let mut limits = limits.validate()?;
        limits.request_timeout = limits.request_timeout.min(Duration::from_millis(u64::from(
            artifact.manifest.limits.request_timeout_ms,
        )));
        if deployment_generation == 0 {
            return Err(SidecarError::InvalidConfiguration);
        }
        if !artifact
            .manifest
            .contract_range
            .contains(ContractVersion::new(1, 0))
        {
            return Err(SidecarError::InvalidHandshake);
        }
        artifact.validate_configuration(&public_config)?;
        if granted_capabilities.iter().any(|(grant, version)| {
            !artifact.manifest.capabilities.iter().any(|capability| {
                capability.id == *grant
                    && capability.version == *version
                    && capability.access == CapabilityAccess::Read
                    && capability.side_effect == SideEffectClass::None
            }) || matches!(
                grant.as_str(),
                "external.account.profile.read" | "external.account.balance.read"
            )
        }) {
            return Err(SidecarError::CapabilityNotGranted);
        }
        if granted_capabilities
            .iter()
            .map(|(id, _)| id)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != granted_capabilities.len()
        {
            return Err(SidecarError::InvalidConfiguration);
        }
        for (capability_id, _) in &granted_capabilities {
            let declaration = artifact
                .manifest
                .capabilities
                .iter()
                .find(|capability| capability.id == *capability_id)
                .ok_or(SidecarError::CapabilityNotDeclared)?;
            for schema in [&declaration.input_schema, &declaration.output_schema] {
                CompiledJsonSchema::compile(
                    schema.as_ref().ok_or(SidecarError::CapabilityNotDeclared)?,
                )
                .map_err(|_| SidecarError::InvalidHandshake)?;
            }
        }
        let root_for_hash = artifact.package_root.clone();
        let executable_for_hash = artifact.executable.clone();
        let package_digest = tokio::task::spawn_blocking(move || {
            plugin_package_digest(&root_for_hash, &executable_for_hash)
        })
        .await
        .map_err(|_| SidecarError::InvalidArtifact)??;
        if package_digest != artifact.sha256 {
            return Err(SidecarError::ArtifactDigestMismatch);
        }
        let initial_manifest = artifact.manifest.clone();
        let artifact = Arc::new(artifact);
        let mut command = Command::new(&artifact.executable);
        command
            .current_dir(&artifact.package_root)
            .env_clear()
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        #[cfg(windows)]
        for variable in ["SystemRoot", "WINDIR", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(variable) {
                command.env(variable, value);
            }
        }

        let mut child = command.spawn().map_err(|_| SidecarError::StartFailed)?;
        #[cfg(unix)]
        let process_group_id = child.id().and_then(|id| i32::try_from(id).ok());
        #[cfg(not(unix))]
        let process_group_id = None;
        let stdin = child.stdin.take().ok_or(SidecarError::StartFailed)?;
        let stdout = child.stdout.take().ok_or(SidecarError::StartFailed)?;
        let session = Arc::new(Self {
            manifest: initial_manifest,
            limits,
            deployment_generation,
            granted_capabilities: granted_capabilities.into_iter().collect(),
            public_config,
            process: Mutex::new(SidecarProcess {
                child,
                process_group_id,
                stdin,
                stdout: BufReader::new(stdout),
                next_request_id: 1,
            }),
            artifact,
        });
        let response = match session
            .exchange(PluginHostOperation::Manifest, limits.startup_timeout, None)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                session.stop().await;
                return Err(error);
            }
        };
        let PluginHostResult::Manifest(manifest) = response else {
            session.stop().await;
            return Err(SidecarError::InvalidHandshake);
        };
        if manifest.validate().is_err()
            || manifest.plugin_id != session.artifact.plugin_id
            || manifest.plugin_version != session.artifact.plugin_version
            || manifest != session.artifact.manifest
        {
            session.stop().await;
            return Err(SidecarError::InvalidHandshake);
        }
        match session.health(deployment_generation).await {
            Ok(health) if health.status == ConnectorHealthStatus::Healthy => {}
            Ok(_) => {
                session.stop().await;
                return Err(SidecarError::InvalidHandshake);
            }
            Err(error) => {
                session.stop().await;
                return Err(error);
            }
        }
        Ok(session)
    }

    #[must_use]
    pub fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    /// Check child liveness without adding an extra protocol round trip to
    /// every capability invocation. Failed calls terminate the child and are
    /// restarted lazily before the next host operation.
    pub async fn is_process_running(
        &self,
        deployment_generation: u64,
    ) -> Result<bool, SidecarError> {
        self.ensure_generation(deployment_generation)?;
        let Ok(mut process) = self.process.try_lock() else {
            // A running exchange owns the process lock; admission to the next
            // exchange is bounded by that invocation's deadline.
            return Ok(true);
        };
        match process.child.try_wait() {
            Ok(Some(_)) => {
                process.terminate().await;
                Ok(false)
            }
            Ok(None) => Ok(true),
            Err(_) => {
                process.terminate().await;
                Ok(false)
            }
        }
    }

    pub async fn health(
        &self,
        deployment_generation: u64,
    ) -> Result<ConnectorHealth, SidecarError> {
        self.ensure_generation(deployment_generation)?;
        match self
            .exchange(
                PluginHostOperation::Health,
                self.limits.request_timeout,
                None,
            )
            .await?
        {
            PluginHostResult::Health(health) if health.validate().is_ok() => Ok(health),
            PluginHostResult::Error(_) => {
                self.stop().await;
                Err(SidecarError::OperationFailed)
            }
            _ => {
                self.stop().await;
                Err(SidecarError::ProtocolViolation)
            }
        }
    }

    pub async fn invoke(
        &self,
        deployment_generation: u64,
        invocation: &CapabilityContext<'_>,
        capability_id: &CapabilityId,
        capability_version: u16,
        input: Value,
    ) -> Result<CapabilityResult, SidecarError> {
        self.ensure_generation(deployment_generation)?;
        if self.granted_capabilities.get(capability_id) != Some(&capability_version) {
            return Err(SidecarError::CapabilityNotGranted);
        }
        let declaration = self
            .manifest
            .capabilities
            .iter()
            .find(|capability| capability.id == *capability_id)
            .ok_or(SidecarError::CapabilityNotDeclared)?;
        let sensitive_payload =
            contains_sensitive_fields(&self.public_config) || contains_sensitive_fields(&input);
        if sensitive_payload {
            return Err(SidecarError::SensitivePayload);
        }
        if declaration.access != CapabilityAccess::Read
            || declaration.side_effect != SideEffectClass::None
            || declaration.version != capability_version
            || declaration.input_schema.is_none()
            || declaration.output_schema.is_none()
        {
            return Err(SidecarError::CapabilityNotDeclared);
        }
        let input_validator = CompiledJsonSchema::compile(
            declaration
                .input_schema
                .as_ref()
                .ok_or(SidecarError::CapabilityNotDeclared)?,
        )
        .map_err(|_| SidecarError::InvalidHandshake)?;
        let output_validator = CompiledJsonSchema::compile(
            declaration
                .output_schema
                .as_ref()
                .ok_or(SidecarError::CapabilityNotDeclared)?,
        )
        .map_err(|_| SidecarError::InvalidHandshake)?;
        input_validator
            .validate(&input)
            .map_err(|_| SidecarError::InvalidConfiguration)?;
        crate::ensure_json_payload_within_limit(
            &input,
            self.manifest.limits.max_response_bytes as usize,
        )
        .map_err(|_| SidecarError::FrameTooLarge)?;
        match self
            .exchange(
                PluginHostOperation::InvokeCapability {
                    connection_id: invocation.connection_id().to_owned(),
                    public_config: self.public_config.clone(),
                    capability_id: capability_id.as_str().to_owned(),
                    capability_version,
                    input,
                },
                self.limits.request_timeout,
                Some(invocation.metadata()),
            )
            .await?
        {
            PluginHostResult::Capability(result) => {
                if crate::ensure_json_payload_within_limit(
                    &result.output,
                    self.manifest.limits.max_response_bytes as usize,
                )
                .is_err()
                {
                    self.stop().await;
                    return Err(SidecarError::FrameTooLarge);
                }
                if contains_sensitive_fields(&result.output) {
                    self.stop().await;
                    return Err(SidecarError::SensitivePayload);
                }
                if output_validator.validate(&result.output).is_err() {
                    self.stop().await;
                    return Err(SidecarError::ProtocolViolation);
                }
                Ok(result)
            }
            PluginHostResult::Error(_) => Err(SidecarError::OperationFailed),
            _ => {
                self.stop().await;
                Err(SidecarError::ProtocolViolation)
            }
        }
    }

    /// Invoke from another trusted host component without exposing a public
    /// HTTP capability endpoint or passing secret-bearing connector context.
    pub async fn invoke_host_capability(
        &self,
        deployment_generation: u64,
        invocation: &CapabilityExecutionContext,
        capability_id: &CapabilityId,
        capability_version: u16,
        input: Value,
    ) -> Result<CapabilityResult, SidecarError> {
        let context = CapabilityContext::from_execution_context(invocation);
        crate::ensure_json_payload_within_limit(&input, invocation.max_response_bytes as usize)
            .map_err(|_| SidecarError::FrameTooLarge)?;
        let result = self
            .invoke(
                deployment_generation,
                &context,
                capability_id,
                capability_version,
                input,
            )
            .await?;
        crate::ensure_json_payload_within_limit(
            &result.output,
            invocation.max_response_bytes as usize,
        )
        .map_err(|_| SidecarError::FrameTooLarge)?;
        Ok(result)
    }

    pub async fn stop(&self) {
        let mut process = self.process.lock().await;
        process.terminate().await;
    }

    async fn exchange(
        &self,
        operation: PluginHostOperation,
        requested_timeout: Duration,
        metadata: Option<&InvocationMetadata>,
    ) -> Result<PluginHostResult, SidecarError> {
        let deployment_generation = self.deployment_generation;
        if deployment_generation == 0 {
            return Err(SidecarError::InvalidConfiguration);
        }
        let admission_started_ms = now_unix_millis().ok_or(SidecarError::InvalidConfiguration)?;
        let bounded_timeout = requested_timeout.min(self.limits.request_timeout);
        let host_deadline = admission_started_ms.saturating_add(
            u64::try_from(bounded_timeout.as_millis())
                .map_err(|_| SidecarError::InvalidConfiguration)?,
        );
        let admission_deadline = metadata.map_or(host_deadline, |metadata| {
            metadata.deadline_unix_ms.min(host_deadline)
        });
        let remaining_ms = admission_deadline
            .checked_sub(admission_started_ms)
            .filter(|remaining| *remaining > 0)
            .ok_or(SidecarError::TimedOut)?;
        let mut process = timeout(Duration::from_millis(remaining_ms), self.process.lock())
            .await
            .map_err(|_| SidecarError::TimedOut)?;
        match process.child.try_wait() {
            Ok(Some(_)) => {
                process.terminate().await;
                return Err(SidecarError::ProcessExited);
            }
            Ok(None) => {}
            Err(_) => {
                process.terminate().await;
                return Err(SidecarError::OperationFailed);
            }
        }
        let request_number = process.next_request_id;
        process.next_request_id = request_number.saturating_add(1);
        let now_ms = now_unix_millis().ok_or(SidecarError::InvalidConfiguration)?;
        let request_timeout = requested_timeout
            .min(self.limits.request_timeout)
            .min(MAX_REQUEST_TIMEOUT);
        let configured_deadline = admission_deadline.min(
            now_ms.saturating_add(
                u64::try_from(request_timeout.as_millis())
                    .map_err(|_| SidecarError::InvalidConfiguration)?,
            ),
        );
        let (request_id, deadline_unix_ms, operation_timeout) = match metadata {
            Some(metadata) => {
                let deadline = metadata.deadline_unix_ms.min(configured_deadline);
                let remaining = deadline
                    .checked_sub(now_ms)
                    .filter(|remaining| *remaining > 0)
                    .ok_or(SidecarError::TimedOut)?;
                (
                    metadata.request_id.clone(),
                    deadline,
                    Duration::from_millis(remaining),
                )
            }
            None => (
                default_request_id(
                    &self.artifact.plugin_id,
                    deployment_generation,
                    request_number,
                ),
                configured_deadline,
                Duration::from_millis(
                    configured_deadline
                        .checked_sub(now_ms)
                        .filter(|remaining| *remaining > 0)
                        .ok_or(SidecarError::TimedOut)?,
                ),
            ),
        };
        let request = PluginHostRequest {
            protocol_version: SIDE_CAR_PROTOCOL_VERSION,
            plugin_id: self.artifact.plugin_id.clone(),
            plugin_version: self.artifact.plugin_version.clone(),
            artifact_sha256: self.artifact.sha256.clone(),
            deployment_generation,
            request_id: request_id.clone(),
            deadline_unix_ms,
            operation,
        };
        request
            .validate()
            .map_err(|_| SidecarError::InvalidConfiguration)?;
        let mut frame =
            serde_json::to_vec(&request).map_err(|_| SidecarError::ProtocolViolation)?;
        if frame.len() + 1 > self.limits.max_frame_bytes {
            return Err(SidecarError::FrameTooLarge);
        }
        frame.push(b'\n');
        let mut exchange_guard = InFlightSidecarExchange {
            process: &mut process,
            complete: false,
        };
        let exchange = async {
            exchange_guard
                .process
                .stdin
                .write_all(&frame)
                .await
                .map_err(|_| SidecarError::ProcessExited)?;
            exchange_guard
                .process
                .stdin
                .flush()
                .await
                .map_err(|_| SidecarError::ProcessExited)?;
            let frame = read_frame(
                &mut exchange_guard.process.stdout,
                self.limits.max_frame_bytes,
            )
            .await?;
            let response: PluginHostResponse =
                serde_json::from_slice(&frame).map_err(|_| SidecarError::ProtocolViolation)?;
            if response.protocol_version != SIDE_CAR_PROTOCOL_VERSION
                || response.request_id != request_id
                || response.plugin_id != self.artifact.plugin_id
                || response.plugin_version != self.artifact.plugin_version
                || response.artifact_sha256 != self.artifact.sha256
                || response.deployment_generation != deployment_generation
            {
                return Err(SidecarError::ProtocolViolation);
            }
            Ok(response.result)
        };
        match timeout(operation_timeout, exchange).await {
            Ok(Ok(result)) => {
                exchange_guard.complete = true;
                Ok(result)
            }
            Ok(Err(error)) => {
                exchange_guard.process.terminate().await;
                exchange_guard.complete = true;
                Err(error)
            }
            Err(_) => {
                exchange_guard.process.terminate().await;
                exchange_guard.complete = true;
                Err(SidecarError::TimedOut)
            }
        }
    }

    /// Build a registration only for declared generic read capabilities.
    pub fn registration(self: &Arc<Self>) -> Result<ConnectorRegistration, SidecarError> {
        let mut effective_manifest = self.manifest.clone();
        effective_manifest.capabilities.retain(|capability| {
            self.granted_capabilities.get(&capability.id) == Some(&capability.version)
        });
        if effective_manifest.capabilities.is_empty() {
            return Err(SidecarError::CapabilityNotGranted);
        }
        let connector: Arc<dyn PlatformConnector> = Arc::new(SidecarPlatformConnector {
            manifest: effective_manifest,
        });
        let mut registration = ConnectorRegistration::new(connector);
        for capability in &self.manifest.capabilities {
            if self.granted_capabilities.get(&capability.id) != Some(&capability.version) {
                continue;
            }
            if capability.id.as_str() == "external.account.profile.read"
                || capability.id.as_str() == "external.account.balance.read"
                || capability.access != CapabilityAccess::Read
                || capability.side_effect != SideEffectClass::None
                || capability.input_schema.is_none()
                || capability.output_schema.is_none()
            {
                return Err(SidecarError::CapabilityNotDeclared);
            }
            registration =
                registration.with_capability_handler(Arc::new(SidecarCapabilityHandler {
                    session: Arc::clone(self),
                    capability_id: capability.id.clone(),
                    capability_version: capability.version,
                }));
        }
        Ok(registration)
    }

    fn ensure_generation(&self, generation: u64) -> Result<(), SidecarError> {
        if generation == self.deployment_generation {
            Ok(())
        } else {
            Err(SidecarError::InvalidConfiguration)
        }
    }
}

struct SidecarPlatformConnector {
    manifest: PluginManifest,
}

impl PlatformConnector for SidecarPlatformConnector {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn validate_config(&self, _config: &Value) -> Result<(), ConnectorError> {
        Ok(())
    }
}

struct SidecarCapabilityHandler {
    session: Arc<SidecarSession>,
    capability_id: CapabilityId,
    capability_version: u16,
}

#[async_trait]
impl CapabilityHandler for SidecarCapabilityHandler {
    fn capability_id(&self) -> &CapabilityId {
        &self.capability_id
    }

    async fn invoke(
        &self,
        invocation: &CapabilityContext<'_>,
        input: Value,
    ) -> Result<Value, ConnectorError> {
        self.session
            .invoke(
                self.session.deployment_generation,
                invocation,
                &self.capability_id,
                self.capability_version,
                input,
            )
            .await
            .map(|result| result.output)
            .map_err(|_| {
                ConnectorError::new(
                    ConnectorErrorCode::TemporarilyUnavailable,
                    af_plugin_contract::OperationOutcome::NotApplied,
                )
            })
    }
}

async fn read_frame<R>(reader: &mut R, max_frame_bytes: usize) -> Result<Vec<u8>, SidecarError>
where
    R: AsyncBufRead + Unpin,
{
    let mut frame = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|_| SidecarError::ProcessExited)?;
        if available.is_empty() {
            return Err(SidecarError::ProcessExited);
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(available.len(), |index| index + 1);
        if take > max_frame_bytes.saturating_sub(frame.len()) {
            return Err(SidecarError::FrameTooLarge);
        }
        frame.extend_from_slice(&available[..take]);
        reader.consume(take);
        if newline.is_some() {
            frame.pop();
            return Ok(frame);
        }
    }
}

fn now_unix_millis() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn default_request_id(plugin_id: &PluginId, generation: u64, request_number: u64) -> String {
    let mut digest = Sha256::new();
    digest.update(plugin_id.as_str().as_bytes());
    let short_id = digest
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("sc-{short_id}-{generation}-{request_number}")
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Object(fields) => {
            let mut fields = fields.into_iter().collect::<Vec<_>>();
            fields.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize_json(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize_json).collect()),
        scalar => scalar,
    }
}

fn contains_sensitive_fields(value: &Value) -> bool {
    match value {
        Value::Object(fields) => fields
            .iter()
            .any(|(name, value)| is_sensitive_field_name(name) || contains_sensitive_fields(value)),
        Value::Array(items) => items.iter().any(contains_sensitive_fields),
        _ => false,
    }
}

fn is_sensitive_field_name(name: &str) -> bool {
    let normalized = name
        .bytes()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(char::from)
        .collect::<String>()
        .to_ascii_lowercase();
    [
        "password",
        "secret",
        "credential",
        "authorization",
        "apikey",
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "privatekey",
        "cookie",
        "bearer",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
        || normalized == "token"
        || normalized.ends_with("token")
}

/// Stable process, trust and transport failures; never contains paths or output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SidecarError {
    #[error("plugin artifact is invalid")]
    InvalidArtifact,
    #[error("plugin artifact signer is not trusted")]
    UntrustedSigner,
    #[error("plugin artifact digest does not match")]
    ArtifactDigestMismatch,
    #[error("plugin artifact signature is invalid")]
    InvalidArtifactSignature,
    #[error("sidecar configuration is invalid")]
    InvalidConfiguration,
    #[error("sidecar process could not be started")]
    StartFailed,
    #[error("sidecar handshake failed")]
    InvalidHandshake,
    #[error("sidecar protocol frame is too large")]
    FrameTooLarge,
    #[error("sidecar protocol violation")]
    ProtocolViolation,
    #[error("sidecar process exited")]
    ProcessExited,
    #[error("sidecar operation timed out")]
    TimedOut,
    #[error("sidecar operation failed")]
    OperationFailed,
    #[error("sidecar did not declare the requested read capability")]
    CapabilityNotDeclared,
    #[error("sidecar capability was not granted by host policy")]
    CapabilityNotGranted,
    #[error("sidecar payload contains a credential-like field")]
    SensitivePayload,
}

#[cfg(test)]
mod tests {
    use af_plugin_contract::{
        CapabilityAccess, CapabilityDeclaration, CapabilityId, ConnectorLimits, ContractVersion,
        ContractVersionRange, PluginId, PluginManifest, PluginVersion, SideEffectClass,
    };
    use base64::Engine as _;
    use ring::signature::{Ed25519KeyPair, KeyPair as _};
    use serde_json::json;

    use super::{
        PluginArtifactDescriptor, SidecarError, TrustedPluginKey, VerifiedPluginArtifact,
        can_publish_generation, canonicalize_json, contains_sensitive_fields, default_request_id,
        is_sensitive_field_name, plugin_artifact_signature_payload, plugin_package_digest,
        validate_public_plugin_configuration,
    };

    fn signing_test_manifest() -> PluginManifest {
        PluginManifest {
            plugin_id: PluginId::new("org.example.signature-test").unwrap(),
            display_name: "Signature test".to_owned(),
            plugin_version: PluginVersion::new("1.0.0").unwrap(),
            contract_range: ContractVersionRange::new(
                ContractVersion::new(1, 0),
                ContractVersion::new(1, 0),
            )
            .unwrap(),
            capabilities: vec![CapabilityDeclaration {
                id: CapabilityId::new("catalog.models.search").unwrap(),
                version: 1,
                access: CapabilityAccess::Read,
                side_effect: SideEffectClass::None,
                write_semantics: None,
                input_schema: Some(json!({ "type": "object" })),
                output_schema: Some(json!({ "type": "object" })),
            }],
            authentication_modes: vec!["none".to_owned()],
            limits: ConnectorLimits {
                max_concurrency: 1,
                request_timeout_ms: 1_000,
                max_response_bytes: 1_024,
                max_page_size: 1,
            },
            config_schema: json!({ "type": "object" }),
        }
    }

    fn signed_fixture(
        entrypoint: &str,
    ) -> (
        tempfile::TempDir,
        PluginArtifactDescriptor,
        TrustedPluginKey,
    ) {
        let package = tempfile::tempdir().unwrap();
        let binary_directory = package.path().join("bin");
        std::fs::create_dir(&binary_directory).unwrap();
        for name in ["catalog-plugin", "other-program"] {
            let path = binary_directory.join(name);
            std::fs::write(&path, name.as_bytes()).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let manifest = signing_test_manifest();
        let digest =
            plugin_package_digest(package.path(), &binary_directory.join("catalog-plugin"))
                .unwrap();
        let payload = plugin_artifact_signature_payload(
            &manifest.plugin_id,
            &manifest.plugin_version,
            &digest,
            "bin/catalog-plugin",
            &manifest,
        )
        .unwrap();
        let signer = Ed25519KeyPair::from_seed_unchecked(&[42_u8; 32]).unwrap();
        let signature = super::BASE64.encode(signer.sign(&payload).as_ref());
        let key = TrustedPluginKey::new(
            "test-publisher",
            signer.public_key().as_ref().try_into().unwrap(),
        )
        .unwrap();
        let root = package.path().canonicalize().unwrap();
        let descriptor = PluginArtifactDescriptor::new(
            manifest.plugin_id.clone(),
            manifest.plugin_version.clone(),
            manifest,
            root.join(entrypoint),
            root,
            digest,
            "test-publisher".to_owned(),
            &signature,
            "local_process".to_owned(),
            "test-fixture".to_owned(),
        )
        .unwrap();
        (package, descriptor, key)
    }

    fn with_trust_metadata(
        descriptor: PluginArtifactDescriptor,
        signer_key_id: &str,
        signature_base64: &str,
    ) -> Result<PluginArtifactDescriptor, SidecarError> {
        PluginArtifactDescriptor::new(
            descriptor.plugin_id,
            descriptor.plugin_version,
            descriptor.manifest,
            descriptor.executable,
            descriptor.package_root,
            descriptor.sha256,
            signer_key_id.to_owned(),
            signature_base64,
            descriptor.source_kind,
            descriptor.source_reference,
        )
    }

    #[test]
    fn artifact_trust_metadata_must_be_empty_or_a_complete_signature_pair() {
        let (_package, descriptor, _key) = signed_fixture("bin/catalog-plugin");
        assert!(with_trust_metadata(descriptor, "", "").is_ok());
        let (_package, descriptor, _key) = signed_fixture("bin/catalog-plugin");
        let signature = descriptor.signature_base64();
        assert!(with_trust_metadata(descriptor, "", &signature).is_err());
        let (_package, descriptor, _key) = signed_fixture("bin/catalog-plugin");
        assert!(with_trust_metadata(descriptor, "test-publisher", "").is_err());
        let (_package, descriptor, _key) = signed_fixture("bin/catalog-plugin");
        assert!(with_trust_metadata(descriptor, "test-publisher", "not-base64").is_err());
    }

    #[tokio::test]
    async fn operator_managed_verification_preserves_staging_and_configuration_checks() {
        let (package, descriptor, _key) = signed_fixture("bin/catalog-plugin");
        let descriptor = with_trust_metadata(descriptor, "", "").unwrap();
        let verified = VerifiedPluginArtifact::verify_operator_managed(descriptor)
            .await
            .unwrap();
        assert!(verified.signer_key_id().is_empty());
        assert!(verified.signature_base64().is_empty());
        assert!(verified.validate_configuration(&json!({})).is_ok());
        assert!(
            verified
                .validate_configuration(&json!("wrong type"))
                .is_err()
        );
        std::fs::write(package.path().join("bin/catalog-plugin"), b"replacement").unwrap();
        assert_eq!(
            std::fs::read(&verified.executable).unwrap(),
            b"catalog-plugin"
        );
        assert_eq!(
            plugin_package_digest(&verified.package_root, &verified.executable).unwrap(),
            verified.sha256
        );
    }

    #[tokio::test]
    async fn operator_managed_verification_rejects_modified_package_bytes() {
        let (package, descriptor, _key) = signed_fixture("bin/catalog-plugin");
        let descriptor = with_trust_metadata(descriptor, "", "").unwrap();
        std::fs::write(package.path().join("bin/catalog-plugin"), b"replacement").unwrap();
        assert_eq!(
            VerifiedPluginArtifact::verify_operator_managed(descriptor)
                .await
                .err(),
            Some(SidecarError::ArtifactDigestMismatch),
        );
    }

    #[tokio::test]
    async fn signed_and_operator_managed_verification_cannot_bypass_each_other() {
        let (_package, descriptor, _key) = signed_fixture("bin/catalog-plugin");
        assert_eq!(
            VerifiedPluginArtifact::verify_operator_managed(descriptor)
                .await
                .err(),
            Some(SidecarError::InvalidArtifact),
        );
        let (_package, descriptor, key) = signed_fixture("bin/catalog-plugin");
        let descriptor = with_trust_metadata(descriptor, "", "").unwrap();
        assert_eq!(
            VerifiedPluginArtifact::verify(descriptor, &key).await.err(),
            Some(SidecarError::UntrustedSigner),
        );
    }

    #[tokio::test]
    async fn verification_rejects_a_different_entrypoint_in_the_same_signed_package() {
        let (_package, descriptor, key) = signed_fixture("bin/other-program");
        assert_eq!(
            VerifiedPluginArtifact::verify(descriptor, &key).await.err(),
            Some(SidecarError::InvalidArtifactSignature),
        );
    }

    #[tokio::test]
    async fn verified_staging_retains_signed_bytes_when_the_source_is_replaced() {
        let (package, descriptor, key) = signed_fixture("bin/catalog-plugin");
        let verified = VerifiedPluginArtifact::verify(descriptor, &key)
            .await
            .unwrap();
        std::fs::write(package.path().join("bin/catalog-plugin"), b"replacement").unwrap();
        assert_eq!(
            std::fs::read(&verified.executable).unwrap(),
            b"catalog-plugin"
        );
        assert_eq!(
            plugin_package_digest(&verified.package_root, &verified.executable).unwrap(),
            verified.sha256
        );
        let staged_root = verified.package_root.clone();
        drop(verified);
        assert!(!staged_root.exists());
    }

    #[test]
    fn sensitive_field_screening_checks_nested_objects_without_rejecting_token_counts() {
        assert!(contains_sensitive_fields(&json!({
            "settings": [{ "refresh_token": "secret" }]
        })));
        assert!(contains_sensitive_fields(
            &json!({ "Authorization": "Bearer value" })
        ));
        assert!(!contains_sensitive_fields(
            &json!({ "tokenCount": 42, "page": 1 })
        ));
        assert!(is_sensitive_field_name("api-key"));
    }

    #[test]
    fn public_deployment_configuration_rejects_credential_field_names() {
        assert!(
            validate_public_plugin_configuration(
                &json!({ "type": "object" }),
                &json!({ "region": "west" }),
            )
            .is_ok()
        );
        let error = validate_public_plugin_configuration(
            &json!({ "type": "object" }),
            &json!({ "provider": { "apiKey": "secret-value" } }),
        )
        .unwrap_err();
        assert!(!error.to_string().contains("secret-value"));
    }

    #[test]
    fn canonical_json_sorts_nested_object_keys() {
        let canonical = canonicalize_json(json!({ "z": { "b": 1, "a": 2 }, "a": 0 }));
        assert_eq!(
            serde_json::to_string(&canonical).unwrap(),
            r#"{"a":0,"z":{"a":2,"b":1}}"#
        );
    }

    #[test]
    fn signed_artifact_payload_binds_the_executable_entrypoint() {
        let plugin_id = PluginId::new("org.example.signature-test").unwrap();
        let plugin_version = PluginVersion::new("1.0.0").unwrap();
        let manifest = signing_test_manifest();
        let first = plugin_artifact_signature_payload(
            &plugin_id,
            &plugin_version,
            &"a".repeat(64),
            "bin/catalog-plugin",
            &manifest,
        )
        .unwrap();
        let second = plugin_artifact_signature_payload(
            &plugin_id,
            &plugin_version,
            &"a".repeat(64),
            "bin/other-program",
            &manifest,
        )
        .unwrap();
        assert_ne!(first, second);
        assert!(
            plugin_artifact_signature_payload(
                &plugin_id,
                &plugin_version,
                &"a".repeat(64),
                "../outside",
                &manifest,
            )
            .is_err()
        );
    }

    #[test]
    fn package_digest_changes_when_an_empty_directory_is_added() {
        let package = tempfile::tempdir().unwrap();
        let binary_directory = package.path().join("bin");
        std::fs::create_dir(&binary_directory).unwrap();
        let executable = binary_directory.join("catalog-plugin");
        std::fs::write(&executable, b"signed content").unwrap();
        let original = plugin_package_digest(package.path(), &executable).unwrap();
        std::fs::create_dir(package.path().join("empty-data")).unwrap();
        let changed = plugin_package_digest(package.path(), &executable).unwrap();
        assert_ne!(original, changed);
    }

    #[test]
    fn default_request_id_is_short_for_maximum_length_plugin_ids() {
        let plugin_id = PluginId::new(format!("org.{}", "x".repeat(120))).unwrap();
        let request_id = default_request_id(&plugin_id, u64::MAX, u64::MAX);
        assert!(request_id.len() <= 128);
    }

    #[test]
    fn stale_or_stopped_generation_cannot_be_published() {
        assert!(!can_publish_generation(None, Some(8), None, 7));
        assert!(!can_publish_generation(Some(8), Some(8), None, 8));
        assert!(!can_publish_generation(None, Some(8), Some(8), 8));
        assert!(can_publish_generation(Some(8), Some(8), None, 9));
        assert!(can_publish_generation(None, Some(8), Some(8), 9));
    }
}
