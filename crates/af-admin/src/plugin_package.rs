//! Bounded import of administrator-uploaded plugin ZIP packages.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use af_plugin_contract::PluginManifest;
use af_plugin_runtime::plugin_package_digest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::{CompressionMethod, ZipArchive};

use crate::{AdminPluginLifecycleError, plugin_lifecycle::MAX_PLUGIN_PACKAGE_UPLOAD_BYTES};

const MAX_EXPANDED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PACKAGE_ENTRIES: u64 = 100_000;
const MAX_METADATA_BYTES: u64 = 256 * 1024;
const MAX_PATH_BYTES: usize = 900;
const MAX_COMPONENT_BYTES: usize = 255;
const MAX_PATH_DEPTH: usize = 32;
const MAX_COMPRESSION_RATIO: u64 = 200;
const COMPRESSION_RATIO_ALLOWANCE: u64 = 1024 * 1024;
const METADATA_NAME: &str = "anyflows-plugin.json";

/// An imported package remains temporary until its catalog record commits.
/// Dropping it after a failed registration removes the entire extracted tree.
pub(crate) struct ImportedPluginPackage {
    pub(crate) plugin_id: String,
    pub(crate) plugin_version: String,
    pub(crate) manifest: Value,
    pub(crate) package_path: String,
    pub(crate) executable_path: String,
    pub(crate) digest: String,
    pub(crate) signer_key_id: String,
    pub(crate) signature_base64: String,
    directory: Option<PackageDirectoryGuard>,
}

impl ImportedPluginPackage {
    /// Keep files as soon as the durable artifact record refers to them.
    pub(crate) fn retain(mut self) {
        if let Some(directory) = self.directory.take() {
            directory.retain();
        }
    }
}

struct PackageDirectoryGuard {
    path: PathBuf,
    parent: PathBuf,
    remove_on_drop: bool,
}

impl PackageDirectoryGuard {
    fn retain(mut self) {
        self.remove_on_drop = false;
    }
}

impl Drop for PackageDirectoryGuard {
    fn drop(&mut self) {
        if self.remove_on_drop
            && fs::symlink_metadata(&self.path)
                .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
            && self
                .path
                .canonicalize()
                .ok()
                .as_deref()
                .and_then(Path::parent)
                == Some(self.parent.as_path())
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackageEnvelope {
    manifest: PluginManifest,
    executable_path: String,
    signature: Option<PackageSignature>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackageSignature {
    signer_key_id: String,
    signature_base64: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageReceipt<'a> {
    schema_version: u8,
    plugin_id: &'a str,
    plugin_version: &'a str,
    manifest: &'a Value,
    executable_path: &'a str,
}

/// Import a ZIP previously streamed into the host-owned `.uploads` directory.
/// The caller retains ownership of the upload file and its cleanup guard.
pub(crate) async fn extract_plugin_package(
    upload: PathBuf,
    artifact_root: PathBuf,
) -> Result<ImportedPluginPackage, AdminPluginLifecycleError> {
    extract_plugin_package_at(upload, artifact_root, None).await
}

/// Restore an archive to its immutable catalog locator. Legacy digest-only
/// locators remain accepted; new imports also bind the public manifest.
pub(crate) async fn extract_plugin_package_at(
    upload: PathBuf,
    artifact_root: PathBuf,
    expected_package_path: Option<String>,
) -> Result<ImportedPluginPackage, AdminPluginLifecycleError> {
    tokio::task::spawn_blocking(move || {
        extract_package_at(&upload, &artifact_root, expected_package_path.as_deref())
    })
    .await
    .map_err(|_| AdminPluginLifecycleError::Internal)?
}

fn extract_package_at(
    upload: &Path,
    artifact_root: &Path,
    expected_package_path: Option<&str>,
) -> Result<ImportedPluginPackage, AdminPluginLifecycleError> {
    let root = controlled_directory(artifact_root)?;
    let uploads = controlled_directory(&root.join(".uploads"))?;
    let upload_metadata = fs::symlink_metadata(upload).map_err(|_| invalid())?;
    if !upload_metadata.is_file()
        || upload_metadata.file_type().is_symlink()
        || upload
            .parent()
            .and_then(|path| path.canonicalize().ok())
            .as_ref()
            != Some(&uploads)
    {
        return Err(invalid());
    }
    if upload_metadata.len() > MAX_PLUGIN_PACKAGE_UPLOAD_BYTES as u64 {
        return Err(AdminPluginLifecycleError::PayloadTooLarge);
    }
    let mut input = File::open(upload).map_err(|_| invalid())?;
    let expected_entries = bounded_zip_entry_count(&mut input, upload_metadata.len())?;
    let mut archive = ZipArchive::new(input).map_err(|_| invalid())?;
    // Also reject duplicate central names explicitly, including aliases that
    // would otherwise disappear from the reader's name-indexed view.
    if archive.has_duplicate_names()
        || archive.len() as u64 != expected_entries
        || archive.offset() != 0
    {
        return Err(invalid());
    }
    archive
        .validate_header_consistency()
        .map_err(|_| invalid())?;

    let packages = root.join("packages");
    create_private_directory(&packages)?;
    let packages = controlled_directory(&packages)?;
    if packages.parent() != Some(root.as_path()) {
        return Err(invalid());
    }
    let temporary_id = uuid::Uuid::new_v4().to_string();
    let directory = tempfile::Builder::new()
        .prefix(temporary_id.as_str())
        .rand_bytes(0)
        .tempdir_in(&packages)
        .map_err(|_| AdminPluginLifecycleError::Internal)?;
    set_directory_permissions(directory.path())?;
    let payload_root = directory.path().join("payload");
    create_private_directory(&payload_root)?;

    let mut metadata = None;
    let mut archive_paths = BTreeSet::new();
    let mut materialized = BTreeMap::new();
    let mut expanded_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|_| invalid())?;
        let raw_name = std::str::from_utf8(entry.name_raw()).map_err(|_| invalid())?;
        let name = if entry.is_dir() {
            raw_name.strip_suffix('/').ok_or_else(invalid)?
        } else {
            raw_name
        }
        .to_owned();
        validate_relative_path(&name)?;
        if !archive_paths.insert(name.to_lowercase())
            || entry.encrypted()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
            || !ordinary_entry_mode(entry.unix_mode(), entry.is_dir())
            || !safe_extra_fields(&entry.extra_data().unwrap_or_default())
            || entry.size()
                > entry
                    .compressed_size()
                    .saturating_mul(MAX_COMPRESSION_RATIO)
                    .saturating_add(COMPRESSION_RATIO_ALLOWANCE)
        {
            return Err(invalid());
        }
        expanded_bytes = expanded_bytes
            .checked_add(entry.size())
            .filter(|size| *size <= MAX_EXPANDED_BYTES)
            .ok_or_else(invalid)?;
        if entry.is_dir() {
            if entry.size() != 0 || (name != "payload" && !name.starts_with("payload/")) {
                return Err(invalid());
            }
            if let Some(relative) = name.strip_prefix("payload/") {
                ensure_payload_directories(&payload_root, relative, &mut materialized)?;
            }
            continue;
        }
        let expected_size = entry.size();
        if name == METADATA_NAME {
            if expected_size > MAX_METADATA_BYTES || metadata.is_some() {
                return Err(invalid());
            }
            let mut bytes = Vec::new();
            entry
                .take(MAX_METADATA_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid())?;
            if bytes.len() as u64 != expected_size {
                return Err(invalid());
            }
            metadata = Some(bytes);
            continue;
        }
        let relative = name.strip_prefix("payload/").ok_or_else(invalid)?;
        if let Some((parent, _)) = relative.rsplit_once('/') {
            ensure_payload_directories(&payload_root, parent, &mut materialized)?;
        }
        track_materialized_path(&mut materialized, relative, false)?;
        let target = payload_root.join(relative);
        let mut output = create_private_file(&target)?;
        copy_entry(&mut entry, &mut output, expected_size)?;
        output
            .flush()
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
    }

    let envelope: PackageEnvelope =
        serde_json::from_slice(&metadata.ok_or_else(invalid)?).map_err(|_| invalid())?;
    envelope.manifest.validate().map_err(|_| invalid())?;
    validate_relative_path(&envelope.executable_path)?;
    if !materialized
        .get(&envelope.executable_path.to_lowercase())
        .is_some_and(|(path, is_dir)| path == &envelope.executable_path && !is_dir)
    {
        return Err(invalid());
    }
    let (signer_key_id, signature_base64) = match envelope.signature {
        None => (String::new(), String::new()),
        Some(signature) => {
            if signature.signer_key_id.is_empty()
                || signature.signer_key_id.len() > 128
                || signature.signer_key_id.chars().any(char::is_control)
                || signature.signature_base64.len() > 4096
                || signature.signature_base64.chars().any(char::is_control)
            {
                return Err(invalid());
            }
            (signature.signer_key_id, signature.signature_base64)
        }
    };
    for (relative, is_directory) in materialized.values() {
        if !is_directory {
            set_file_permissions(
                &payload_root.join(relative),
                relative == &envelope.executable_path,
            )?;
        }
    }
    let executable = payload_root.join(&envelope.executable_path);
    let digest = plugin_package_digest(&payload_root, &executable).map_err(|_| invalid())?;
    let plugin_id = envelope.manifest.plugin_id.as_str().to_owned();
    let plugin_version = envelope.manifest.plugin_version.as_str().to_owned();
    // Serialize the public contract type rather than forwarding unmodeled JSON
    // members from the uploaded envelope into the catalog or runtime.
    let manifest = serde_json::to_value(&envelope.manifest).map_err(|_| invalid())?;
    let receipt = PackageReceipt {
        schema_version: 1,
        plugin_id: &plugin_id,
        plugin_version: &plugin_version,
        manifest: &manifest,
        executable_path: &envelope.executable_path,
    };
    let receipt_bytes = serde_json::to_vec(&receipt).map_err(|_| invalid())?;
    let mut receipt_file = create_private_file(&directory.path().join("import.json"))?;
    receipt_file
        .write_all(&receipt_bytes)
        .map_err(|_| AdminPluginLifecycleError::Internal)?;
    receipt_file
        .flush()
        .map_err(|_| AdminPluginLifecycleError::Internal)?;
    set_file_permissions(&directory.path().join("import.json"), false)?;

    let identity = format!("{digest}-{:x}", Sha256::digest(&receipt_bytes));
    let package_key = match expected_package_path {
        Some(path) if path == format!("packages/{digest}/payload") => digest.clone(),
        Some(path) if path == format!("packages/{identity}/payload") => identity,
        Some(_) => return Err(invalid()),
        None => identity,
    };
    let final_directory = packages.join(&package_key);
    let (package_directory, created_final_directory) = match fs::symlink_metadata(&final_directory)
    {
        Ok(_) => {
            verify_existing_package(
                &final_directory,
                &packages,
                &receipt_bytes,
                &digest,
                &envelope.executable_path,
            )?;
            (None, false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::rename(directory.path(), &final_directory) {
                Ok(()) => (
                    Some(PackageDirectoryGuard {
                        path: final_directory.clone(),
                        parent: packages.clone(),
                        remove_on_drop: true,
                    }),
                    true,
                ),
                Err(_) => {
                    // Another concurrent import may have atomically published
                    // the same package after our initial lookup.
                    verify_existing_package(
                        &final_directory,
                        &packages,
                        &receipt_bytes,
                        &digest,
                        &envelope.executable_path,
                    )?;
                    (None, false)
                }
            }
        }
        Err(_) => return Err(AdminPluginLifecycleError::Internal),
    };
    if created_final_directory {
        set_directory_permissions(&final_directory)?;
    }
    Ok(ImportedPluginPackage {
        plugin_id,
        plugin_version,
        manifest,
        package_path: format!("packages/{package_key}/payload"),
        executable_path: envelope.executable_path,
        digest,
        signer_key_id,
        signature_base64,
        directory: package_directory,
    })
}

fn verify_existing_package(
    path: &Path,
    packages: &Path,
    receipt: &[u8],
    digest: &str,
    executable_path: &str,
) -> Result<(), AdminPluginLifecycleError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| AdminPluginLifecycleError::Conflict)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AdminPluginLifecycleError::Conflict);
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| AdminPluginLifecycleError::Conflict)?;
    if canonical.parent() != Some(packages) {
        return Err(AdminPluginLifecycleError::Conflict);
    }
    let mut entries = fs::read_dir(&canonical)
        .map_err(|_| AdminPluginLifecycleError::Conflict)?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name())
                .map_err(|_| AdminPluginLifecycleError::Conflict)
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    if entries.len() != 2
        || entries[0] != std::ffi::OsStr::new("import.json")
        || entries[1] != std::ffi::OsStr::new("payload")
    {
        return Err(AdminPluginLifecycleError::Conflict);
    }
    let receipt_path = canonical.join("import.json");
    let receipt_metadata =
        fs::symlink_metadata(&receipt_path).map_err(|_| AdminPluginLifecycleError::Conflict)?;
    if !receipt_metadata.is_file()
        || receipt_metadata.file_type().is_symlink()
        || receipt_metadata.len() > MAX_METADATA_BYTES
        || fs::read(receipt_path).map_err(|_| AdminPluginLifecycleError::Conflict)? != receipt
    {
        return Err(AdminPluginLifecycleError::Conflict);
    }
    let payload = controlled_directory(&canonical.join("payload"))
        .map_err(|_| AdminPluginLifecycleError::Conflict)?;
    let executable = payload.join(executable_path);
    if !fs::symlink_metadata(&executable)
        .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        || !matches!(plugin_package_digest(&payload, &executable), Ok(actual) if actual == digest)
    {
        return Err(AdminPluginLifecycleError::Conflict);
    }
    Ok(())
}

fn invalid() -> AdminPluginLifecycleError {
    AdminPluginLifecycleError::InvalidInput
}

fn controlled_directory(path: &Path) -> Result<PathBuf, AdminPluginLifecycleError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid());
    }
    path.canonicalize()
        .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)
}

fn create_private_directory(path: &Path) -> Result<(), AdminPluginLifecycleError> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            controlled_directory(path)?;
        }
        Err(_) => return Err(AdminPluginLifecycleError::Internal),
    }
    set_directory_permissions(path)
}

fn set_directory_permissions(path: &Path) -> Result<(), AdminPluginLifecycleError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn create_private_file(path: &Path) -> Result<File, AdminPluginLifecycleError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|_| invalid())
}

fn set_file_permissions(path: &Path, executable: bool) -> Result<(), AdminPluginLifecycleError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if executable { 0o500 } else { 0o400 }),
        )
        .map_err(|_| AdminPluginLifecycleError::Internal)?;
    }
    #[cfg(not(unix))]
    let _ = (path, executable);
    Ok(())
}

fn validate_relative_path(path: &str) -> Result<(), AdminPluginLifecycleError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    let components = path.split('/').collect::<Vec<_>>();
    if components.len() > MAX_PATH_DEPTH {
        return Err(invalid());
    }
    for component in components {
        let windows_stem = component
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.len() > MAX_COMPONENT_BYTES
            || component.contains(['<', '>', '"', '|', '?', '*'])
            || component.ends_with(['.', ' '])
            || matches!(windows_stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || windows_stem.strip_prefix("COM").is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
            || windows_stem.strip_prefix("LPT").is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn ordinary_entry_mode(mode: Option<u32>, is_directory: bool) -> bool {
    match mode.map(|mode| mode & 0o170000) {
        None | Some(0) => true,
        Some(0o040000) => is_directory,
        Some(0o100000) => !is_directory,
        _ => false,
    }
}

fn safe_extra_fields(mut extra: &[u8]) -> bool {
    while !extra.is_empty() {
        if extra.len() < 4 {
            return false;
        }
        let kind = u16::from_le_bytes([extra[0], extra[1]]);
        let length = usize::from(u16::from_le_bytes([extra[2], extra[3]]));
        // PKWARE/Info-ZIP/ASi Unix fields can describe links. Ownership and
        // timestamps are unnecessary for a host-materialized executable tree.
        if matches!(kind, 0x000d | 0x5855 | 0x756e) || length > extra.len() - 4 {
            return false;
        }
        extra = &extra[4 + length..];
    }
    true
}

fn track_materialized_path(
    paths: &mut BTreeMap<String, (String, bool)>,
    path: &str,
    is_directory: bool,
) -> Result<bool, AdminPluginLifecycleError> {
    let key = path.to_lowercase();
    if let Some((existing, existing_directory)) = paths.get(&key) {
        if existing == path && is_directory && *existing_directory {
            return Ok(false);
        }
        return Err(invalid());
    }
    if paths.len() as u64 >= MAX_PACKAGE_ENTRIES {
        return Err(invalid());
    }
    paths.insert(key, (path.to_owned(), is_directory));
    Ok(true)
}

fn ensure_payload_directories(
    root: &Path,
    path: &str,
    materialized: &mut BTreeMap<String, (String, bool)>,
) -> Result<(), AdminPluginLifecycleError> {
    let mut relative = String::new();
    for component in path.split('/') {
        if !relative.is_empty() {
            relative.push('/');
        }
        relative.push_str(component);
        if track_materialized_path(materialized, &relative, true)? {
            create_private_directory(&root.join(&relative))?;
        }
    }
    Ok(())
}

fn copy_entry(
    input: &mut impl Read,
    output: &mut File,
    expected_size: u64,
) -> Result<(), AdminPluginLifecycleError> {
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|_| invalid())?;
        if read == 0 {
            break;
        }
        copied = copied
            .checked_add(read as u64)
            .filter(|count| *count <= expected_size)
            .ok_or_else(invalid)?;
        output
            .write_all(&buffer[..read])
            .map_err(|_| AdminPluginLifecycleError::Internal)?;
    }
    if copied != expected_size {
        return Err(invalid());
    }
    Ok(())
}

/// Bound the central-directory count before zip allocates one metadata object
/// per entry. This also retains the original count needed to reject duplicates.
fn bounded_zip_entry_count(file: &mut File, size: u64) -> Result<u64, AdminPluginLifecycleError> {
    if size > MAX_PLUGIN_PACKAGE_UPLOAD_BYTES as u64 {
        return Err(AdminPluginLifecycleError::PayloadTooLarge);
    }
    if size < 22 {
        return Err(invalid());
    }
    let tail_size = size.min(22 + u64::from(u16::MAX)) as usize;
    file.seek(SeekFrom::End(-(tail_size as i64)))
        .map_err(|_| invalid())?;
    let mut tail = vec![0_u8; tail_size];
    file.read_exact(&mut tail).map_err(|_| invalid())?;
    let end = (0..=tail_size - 22)
        .rev()
        .find(|offset| {
            tail.get(*offset..*offset + 4) == Some(b"PK\x05\x06".as_slice())
                && little_u16(&tail, *offset + 20)
                    .ok()
                    .is_some_and(|comment| *offset + 22 + usize::from(comment) == tail_size)
        })
        .ok_or_else(invalid)?;
    let end_offset = size - tail_size as u64 + end as u64;
    if little_u16(&tail, end + 4)? != 0 || little_u16(&tail, end + 6)? != 0 {
        return Err(invalid());
    }
    let entries_here = little_u16(&tail, end + 8)?;
    let entries_total = little_u16(&tail, end + 10)?;
    let directory_size = little_u32(&tail, end + 12)?;
    let directory_offset = little_u32(&tail, end + 16)?;
    let mut count = u64::from(entries_total);
    let mut directory_size = u64::from(directory_size);
    let mut directory_offset = u64::from(directory_offset);
    let mut directory_boundary = end_offset;
    let locator = end.checked_sub(20).and_then(|offset| tail.get(offset..end));
    if let Some(locator) = locator.filter(|locator| locator.starts_with(b"PK\x06\x07")) {
        if little_u32(locator, 4)? != 0 || little_u32(locator, 16)? != 1 {
            return Err(invalid());
        }
        let zip64_offset = little_u64(locator, 8)?;
        if zip64_offset
            .checked_add(56)
            .filter(|offset| *offset <= end_offset - 20)
            .is_none()
        {
            return Err(invalid());
        }
        file.seek(SeekFrom::Start(zip64_offset))
            .map_err(|_| invalid())?;
        let mut zip64 = [0_u8; 56];
        file.read_exact(&mut zip64).map_err(|_| invalid())?;
        let record_size = little_u64(&zip64, 4)?;
        if !zip64.starts_with(b"PK\x06\x06")
            || record_size < 44
            || zip64_offset
                .checked_add(12)
                .and_then(|offset| offset.checked_add(record_size))
                != Some(end_offset - 20)
            || little_u32(&zip64, 16)? != 0
            || little_u32(&zip64, 20)? != 0
            || little_u64(&zip64, 24)? != little_u64(&zip64, 32)?
        {
            return Err(invalid());
        }
        count = little_u64(&zip64, 32)?;
        let size64 = little_u64(&zip64, 40)?;
        let offset64 = little_u64(&zip64, 48)?;
        if (entries_total != u16::MAX && u64::from(entries_total) != count)
            || (entries_here != u16::MAX && u64::from(entries_here) != count)
            || (directory_size != u64::from(u32::MAX) && directory_size != size64)
            || (directory_offset != u64::from(u32::MAX) && directory_offset != offset64)
        {
            return Err(invalid());
        }
        directory_size = size64;
        directory_offset = offset64;
        directory_boundary = zip64_offset;
    } else if entries_here != entries_total || entries_total == u16::MAX {
        return Err(invalid());
    }
    if count == 0
        || count > MAX_PACKAGE_ENTRIES
        || directory_offset.checked_add(directory_size) != Some(directory_boundary)
    {
        return Err(invalid());
    }
    // Check every raw central record rather than trusting a count that could
    // leave hidden records outside the ZIP reader's indexed view.
    let mut cursor = directory_offset;
    for _ in 0..count {
        if cursor
            .checked_add(46)
            .filter(|end| *end <= directory_boundary)
            .is_none()
        {
            return Err(invalid());
        }
        file.seek(SeekFrom::Start(cursor)).map_err(|_| invalid())?;
        let mut header = [0_u8; 46];
        file.read_exact(&mut header).map_err(|_| invalid())?;
        if !header.starts_with(b"PK\x01\x02") || little_u16(&header, 34)? != 0 {
            return Err(invalid());
        }
        let suffix = u64::from(little_u16(&header, 28)?)
            + u64::from(little_u16(&header, 30)?)
            + u64::from(little_u16(&header, 32)?);
        cursor = cursor
            .checked_add(46 + suffix)
            .filter(|end| *end <= directory_boundary)
            .ok_or_else(invalid)?;
    }
    if cursor != directory_boundary {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(0)).map_err(|_| invalid())?;
    Ok(count)
}

fn little_u16(bytes: &[u8], offset: usize) -> Result<u16, AdminPluginLifecycleError> {
    let bytes = bytes.get(offset..offset + 2).ok_or_else(invalid)?;
    Ok(u16::from_le_bytes(bytes.try_into().map_err(|_| invalid())?))
}

fn little_u32(bytes: &[u8], offset: usize) -> Result<u32, AdminPluginLifecycleError> {
    let bytes = bytes.get(offset..offset + 4).ok_or_else(invalid)?;
    Ok(u32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?))
}

fn little_u64(bytes: &[u8], offset: usize) -> Result<u64, AdminPluginLifecycleError> {
    let bytes = bytes.get(offset..offset + 8).ok_or_else(invalid)?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| invalid())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use std::{error::Error, io::Cursor};
    use zip::{ZipWriter, write::SimpleFileOptions};

    fn envelope() -> Value {
        serde_json::json!({
            "manifest": {
                "pluginId": "org.example.upload",
                "pluginVersion": "1.0.0",
                "contractRange": { "minimum": { "major": 1, "minor": 0 }, "maximum": { "major": 1, "minor": 2 } },
                "authenticationModes": ["none"],
                "limits": { "maxConcurrency": 2, "requestTimeoutMs": 1000, "maxResponseBytes": 4096, "maxPageSize": 20 },
                "configSchema": { "type": "object", "additionalProperties": false },
                "capabilities": [{ "id": "catalog.models.read", "version": 1, "access": "read", "sideEffect": "none" }]
            },
            "executablePath": "bin/plugin"
        })
    }

    fn archive(
        metadata: &[u8],
        extra: &[(&str, &[u8])],
        compression: CompressionMethod,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(compression);
        writer.start_file(METADATA_NAME, options)?;
        writer.write_all(metadata)?;
        writer.start_file("payload/bin/plugin", options)?;
        writer.write_all(b"not executed during import")?;
        for (name, bytes) in extra {
            writer.start_file(*name, options)?;
            writer.write_all(bytes)?;
        }
        Ok(writer.finish()?.into_inner())
    }

    fn write_upload(root: &Path, bytes: &[u8]) -> Result<PathBuf, Box<dyn Error>> {
        let uploads = root.join(".uploads");
        fs::create_dir_all(&uploads)?;
        let file = uploads.join(format!("{}.zip", uuid::Uuid::new_v4()));
        fs::write(&file, bytes)?;
        Ok(file)
    }

    fn assert_no_packages(root: &Path) -> Result<(), Box<dyn Error>> {
        let packages = root.join("packages");
        if packages.exists() {
            assert_eq!(fs::read_dir(packages)?.count(), 0);
        }
        Ok(())
    }

    #[tokio::test]
    async fn unsigned_import_computes_digest_and_cleans_uncommitted_payload()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let mut metadata = envelope();
        metadata["manifest"]["unmodeledMember"] = serde_json::json!({ "ignored": true });
        let bytes = archive(
            &serde_json::to_vec(&metadata)?,
            &[("payload/data/example.json", b"{}")],
            CompressionMethod::Deflated,
        )?;
        let upload = write_upload(root.path(), &bytes)?;
        let imported = extract_plugin_package(upload.clone(), root.path().to_path_buf()).await?;
        assert_eq!(imported.plugin_id, "org.example.upload");
        assert_eq!(imported.plugin_version, "1.0.0");
        assert!(imported.signer_key_id.is_empty());
        assert!(imported.signature_base64.is_empty());
        assert!(imported.manifest.get("unmodeledMember").is_none());
        let payload = root.path().join(&imported.package_path);
        assert_eq!(
            imported.digest,
            plugin_package_digest(&payload, &payload.join(&imported.executable_path))?
        );
        assert_eq!(fs::read(payload.join("data/example.json"))?, b"{}");
        assert!(!payload.join(METADATA_NAME).exists());
        assert!(
            upload.exists(),
            "the HTTP layer retains ownership of its upload"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(payload.join("bin/plugin"))?
                    .permissions()
                    .mode()
                    & 0o7777,
                0o500
            );
            assert_eq!(
                fs::metadata(payload.join("data/example.json"))?
                    .permissions()
                    .mode()
                    & 0o7777,
                0o400
            );
            assert_eq!(
                fs::metadata(payload.join("bin"))?.permissions().mode() & 0o7777,
                0o700
            );
        }
        drop(imported);
        assert!(!payload.exists());
        assert_no_packages(root.path())?;
        Ok(())
    }

    #[tokio::test]
    async fn optional_publisher_signature_is_preserved_for_runtime_verification()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let mut metadata = envelope();
        metadata["signature"] = serde_json::json!({
            "signerKeyId": "official-release",
            "signatureBase64": BASE64.encode([0_u8; 64])
        });
        let upload = write_upload(
            root.path(),
            &archive(
                &serde_json::to_vec(&metadata)?,
                &[],
                CompressionMethod::Stored,
            )?,
        )?;
        let imported = extract_plugin_package(upload, root.path().to_path_buf()).await?;
        assert_eq!(imported.signer_key_id, "official-release");
        assert_eq!(BASE64.decode(imported.signature_base64)?.len(), 64);
        Ok(())
    }

    #[tokio::test]
    async fn self_signature_metadata_does_not_block_custom_package_installation()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let mut metadata = envelope();
        metadata["signature"] = serde_json::json!({
            "signerKeyId": "my-private-key-label",
            "signatureBase64": "not-an-official-signature-format"
        });
        let upload = write_upload(
            root.path(),
            &archive(
                &serde_json::to_vec(&metadata)?,
                &[],
                CompressionMethod::Stored,
            )?,
        )?;
        let imported = extract_plugin_package(upload, root.path().to_path_buf()).await?;
        assert_eq!(imported.signer_key_id, "my-private-key-label");
        assert_eq!(
            imported.signature_base64,
            "not-an-official-signature-format"
        );
        Ok(())
    }

    #[tokio::test]
    async fn retained_import_survives_guard_drop() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let upload = write_upload(
            root.path(),
            &archive(
                &serde_json::to_vec(&envelope())?,
                &[],
                CompressionMethod::Stored,
            )?,
        )?;
        let imported = extract_plugin_package(upload, root.path().to_path_buf()).await?;
        let payload = root.path().join(&imported.package_path);
        imported.retain();
        assert!(payload.join("bin/plugin").is_file());
        Ok(())
    }

    #[tokio::test]
    async fn identical_imports_reuse_path_and_distinct_metadata_uses_separate_receipts()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let metadata = serde_json::to_vec(&envelope())?;
        let first_upload = write_upload(
            root.path(),
            &archive(&metadata, &[], CompressionMethod::Stored)?,
        )?;
        let first = extract_plugin_package(first_upload, root.path().to_path_buf()).await?;
        let expected_path = first.package_path.clone();
        let expected_digest = first.digest.clone();
        let expected_manifest = first.manifest.clone();
        first.retain();

        let second_upload = write_upload(
            root.path(),
            &archive(&metadata, &[], CompressionMethod::Stored)?,
        )?;
        let second = extract_plugin_package(second_upload, root.path().to_path_buf()).await?;
        assert_eq!(second.package_path, expected_path);
        second.retain();
        assert_eq!(fs::read_dir(root.path().join("packages"))?.count(), 1);

        let mut conflicting = envelope();
        conflicting["manifest"]["displayName"] =
            Value::String("Different plugin metadata".to_owned());
        let upload = write_upload(
            root.path(),
            &archive(
                &serde_json::to_vec(&conflicting)?,
                &[],
                CompressionMethod::Stored,
            )?,
        )?;
        let distinct = extract_plugin_package(upload, root.path().to_path_buf()).await?;
        assert_ne!(distinct.package_path, expected_path);
        assert_eq!(distinct.digest, expected_digest);
        assert_eq!(distinct.manifest, conflicting["manifest"]);
        distinct.retain();
        assert!(
            root.path()
                .join(&expected_path)
                .join("bin/plugin")
                .is_file()
        );
        let original_receipt: Value = serde_json::from_slice(&fs::read(
            root.path()
                .join(&expected_path)
                .parent()
                .ok_or("missing receipt directory")?
                .join("import.json"),
        )?)?;
        assert_eq!(original_receipt["manifest"], expected_manifest);
        assert_eq!(fs::read_dir(root.path().join("packages"))?.count(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn zip_paths_cannot_escape_or_alias_package_tree() -> Result<(), Box<dyn Error>> {
        let metadata = serde_json::to_vec(&envelope())?;
        for name in [
            "../outside",
            "payload/../outside",
            "/payload/absolute",
            "payload\\backslash",
            "payload/C:alternate",
            "payload/invalid*.name",
            "payload/bin/plugin.",
            "payload/NUL.txt",
            "payload/BIN/another",
            "unrecognized-file",
        ] {
            let root = tempfile::tempdir()?;
            let upload = write_upload(
                root.path(),
                &archive(&metadata, &[(name, b"rejected")], CompressionMethod::Stored)?,
            )?;
            assert!(
                matches!(
                    extract_plugin_package(upload, root.path().to_path_buf()).await,
                    Err(AdminPluginLifecycleError::InvalidInput)
                ),
                "accepted {name}"
            );
            assert_no_packages(root.path())?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn duplicate_raw_zip_paths_are_rejected_before_index_deduplication()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let mut bytes = archive(
            &serde_json::to_vec(&envelope())?,
            &[("payload/copy-a", b"first"), ("payload/copy-b", b"other")],
            CompressionMethod::Stored,
        )?;
        let needle = b"payload/copy-b";
        for index in 0..=bytes.len() - needle.len() {
            if bytes.get(index..index + needle.len()) == Some(needle.as_slice()) {
                bytes[index + needle.len() - 1] = b'a';
            }
        }
        let upload = write_upload(root.path(), &bytes)?;
        assert!(matches!(
            extract_plugin_package(upload, root.path().to_path_buf()).await,
            Err(AdminPluginLifecycleError::InvalidInput)
        ));
        assert_no_packages(root.path())?;
        Ok(())
    }

    #[tokio::test]
    async fn symlinks_and_special_unix_entries_are_rejected() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer.start_file(METADATA_NAME, options)?;
        writer.write_all(&serde_json::to_vec(&envelope())?)?;
        writer.start_file("payload/bin/plugin", options)?;
        writer.write_all(b"binary")?;
        writer.add_symlink("payload/link", "bin/plugin", options)?;
        let upload = write_upload(root.path(), &writer.finish()?.into_inner())?;
        assert!(matches!(
            extract_plugin_package(upload, root.path().to_path_buf()).await,
            Err(AdminPluginLifecycleError::InvalidInput)
        ));
        assert_no_packages(root.path())?;

        let mut bytes = archive(
            &serde_json::to_vec(&envelope())?,
            &[],
            CompressionMethod::Stored,
        )?;
        for index in 0..bytes.len().saturating_sub(46) {
            if bytes.get(index..index + 4) == Some(b"PK\x01\x02".as_slice()) {
                bytes[index + 5] = 3; // Unix creator; encode a FIFO in external attributes.
                bytes[index + 38..index + 42].copy_from_slice(&(0o010600_u32 << 16).to_le_bytes());
                break;
            }
        }
        let upload = write_upload(root.path(), &bytes)?;
        assert!(matches!(
            extract_plugin_package(upload, root.path().to_path_buf()).await,
            Err(AdminPluginLifecycleError::InvalidInput)
        ));
        assert_no_packages(root.path())?;
        Ok(())
    }

    #[tokio::test]
    async fn missing_executable_oversized_metadata_and_compression_bombs_are_rejected()
    -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let mut missing = envelope();
        missing["executablePath"] = Value::String("missing".to_owned());
        let metadata_bomb = vec![b' '; MAX_METADATA_BYTES as usize + 1];
        let repeated = vec![0_u8; 2 * 1024 * 1024];
        let cases = [
            archive(
                &serde_json::to_vec(&missing)?,
                &[],
                CompressionMethod::Stored,
            )?,
            archive(&metadata_bomb, &[], CompressionMethod::Stored)?,
            archive(
                &serde_json::to_vec(&envelope())?,
                &[("payload/bomb", &repeated)],
                CompressionMethod::Deflated,
            )?,
        ];
        for bytes in cases {
            let upload = write_upload(root.path(), &bytes)?;
            assert!(matches!(
                extract_plugin_package(upload, root.path().to_path_buf()).await,
                Err(AdminPluginLifecycleError::InvalidInput)
            ));
            assert_no_packages(root.path())?;
        }
        Ok(())
    }

    #[test]
    fn link_metadata_and_excessive_path_depth_are_rejected() {
        assert!(!safe_extra_fields(&[0x0d, 0, 0, 0]));
        assert!(!safe_extra_fields(&[0x6e, 0x75, 0, 0]));
        assert!(!safe_extra_fields(&[1, 0, 4, 0, 1]));
        assert!(validate_relative_path(&vec!["nested"; MAX_PATH_DEPTH + 1].join("/")).is_err());
    }
}
