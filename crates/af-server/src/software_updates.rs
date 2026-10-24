//! Fixed-origin release discovery and opt-in, node-local binary replacement.

use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_http::{
    SoftwareRelease, SoftwareStatus, SoftwareUpdateError as Error, SoftwareUpdateFuture,
    SoftwareUpdateService,
};
use af_httpclient::{HeaderMap, HeaderValue, HttpClientProvider, Method};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

const RELEASE_API: &str = "https://api.github.com/repos/ChuqiCloud/AnyFlows/releases?per_page=100";
const DOWNLOAD_ROOT: &str = "https://github.com/ChuqiCloud/AnyFlows/releases/download/";
const MAX_BINARY_BYTES: usize = 256 * 1024 * 1024;

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
}

pub(crate) fn web_updates_enabled(value: Option<&str>) -> bool {
    value.is_none_or(|value| !value.eq_ignore_ascii_case("false") && value != "0")
}

pub(crate) struct CommunitySoftwareUpdates {
    clients: HttpClientProvider,
    state: Mutex<SoftwareStatus>,
    executable: PathBuf,
    managed: Result<Option<af_launcher::ManagedContext>, af_launcher::ControlError>,
}

impl CommunitySoftwareUpdates {
    pub(crate) fn new(clients: HttpClientProvider, allow_install: bool) -> Self {
        let target = env!("AF_BUILD_TARGET");
        let container = af_launcher::in_container();
        let managed = af_launcher::ManagedContext::from_env();
        let executable = env::current_exe().unwrap_or_default();
        let reason = if container {
            Some("container")
        } else if managed.is_err() {
            Some("launcher_unavailable")
        } else if platform_name(target).is_none() {
            Some("unsupported_target")
        } else if !allow_install {
            Some("not_enabled")
        } else if !executable.is_file() {
            Some("executable_unavailable")
        } else {
            None
        };
        Self {
            clients,
            executable,
            managed: managed.clone(),
            state: Mutex::new(SoftwareStatus {
                edition: "community".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                core_version: env!("CARGO_PKG_VERSION").to_owned(),
                commit: option_env!("AF_BUILD_COMMIT")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                build_time: option_env!("AF_BUILD_TIME")
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                target: target.to_owned(),
                deployment: if container { "docker" } else { "binary" }.to_owned(),
                node_id: format!("process-{}-{}", std::process::id(), uuid::Uuid::new_v4()),
                launch_mode: if matches!(managed, Ok(Some(_))) {
                    "launcher"
                } else {
                    "direct"
                }
                .to_owned(),
                launcher_protocol: Some(af_launcher::PROTOCOL),
                launcher_version: None,
                update_phase: "idle".to_owned(),
                launcher_error: None,
                check_enabled: true,
                install_enabled: reason.is_none(),
                install_blocked_reason: reason.map(str::to_owned),
                restart_required: false,
                checked_at: None,
                latest: None,
                update_available: false,
            }),
        }
    }

    async fn check_inner(&self, state: &mut SoftwareStatus) -> Result<(), Error> {
        if state
            .checked_at
            .is_some_and(|at| now().saturating_sub(at) < 60)
        {
            return Ok(());
        }
        let bytes = self.fetch(RELEASE_API, 2 * 1024 * 1024).await?;
        let releases: Vec<Release> =
            serde_json::from_slice(&bytes).map_err(|_| Error::InvalidRelease)?;
        let current = Version::parse(&state.core_version).map_err(|_| Error::InvalidRelease)?;
        let release = select_release(releases, &current);
        state.latest = release.map(|release| {
            let package_name = package_name(&release.tag_name, &state.target);
            let package_name =
                package_name.filter(|name| release.assets.iter().any(|asset| asset.name == *name));
            SoftwareRelease {
                version: release.tag_name.trim_start_matches('v').to_owned(),
                url: format!(
                    "https://github.com/ChuqiCloud/AnyFlows/releases/tag/{}",
                    release.tag_name
                ),
                notes: release
                    .body
                    .unwrap_or_default()
                    .chars()
                    .take(32_768)
                    .collect(),
                published_at: release.published_at,
                package_url: package_name
                    .as_ref()
                    .map(|name| format!("{DOWNLOAD_ROOT}{}/{name}", release.tag_name)),
                package_name,
                tag: release.tag_name,
            }
        });
        state.update_available = state
            .latest
            .as_ref()
            .and_then(|r| Version::parse(&r.version).ok())
            .is_some_and(|v| v > current);
        state.checked_at = Some(now());
        Ok(())
    }

    async fn fetch(&self, url: &str, limit: usize) -> Result<Vec<u8>, Error> {
        let mut body = Vec::new();
        let mut response = self.response(url).await?.into_bytes_stream();
        while let Some(chunk) = response.next_chunk().await {
            let chunk = chunk.map_err(|_| Error::Unavailable)?;
            if body.len().saturating_add(chunk.len()) > limit {
                return Err(Error::InvalidRelease);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }

    async fn response(&self, url: &str) -> Result<af_httpclient::HttpResponse, Error> {
        let client = self
            .clients
            .get(Some(Duration::from_secs(180)))
            .map_err(|_| Error::Unavailable)?;
        let mut url = url.to_owned();
        for _ in 0..5 {
            if !allowed_url(&url) {
                return Err(Error::InvalidRelease);
            }
            let mut headers = HeaderMap::new();
            headers.insert(
                "accept",
                HeaderValue::from_static("application/vnd.github+json"),
            );
            headers.insert(
                "x-github-api-version",
                HeaderValue::from_static("2022-11-28"),
            );
            let response = client
                .execute(Method::GET, &url, headers, None)
                .await
                .map_err(|_| Error::Unavailable)?;
            if response.status().is_redirection() {
                url = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or(Error::InvalidRelease)?
                    .to_owned();
                continue;
            }
            if !response.status().is_success() {
                return Err(Error::Unavailable);
            }
            return Ok(response);
        }
        Err(Error::Unavailable)
    }

    async fn install_inner(&self, state: &mut SoftwareStatus, tag: &str) -> Result<(), Error> {
        if !state.install_enabled {
            return Err(Error::Disabled);
        }
        self.check_inner(state).await?;
        let latest = state
            .latest
            .as_ref()
            .filter(|r| r.tag == tag && r.package_name.is_some() && state.update_available)
            .ok_or(Error::Conflict)?;
        let name = binary_name(&latest.tag, &state.target).ok_or(Error::InvalidRelease)?;
        // Re-read the selected release immediately before download. Never accept an arbitrary URL.
        let endpoint =
            format!("https://api.github.com/repos/ChuqiCloud/AnyFlows/releases/tags/{tag}");
        let release: Release = serde_json::from_slice(&self.fetch(&endpoint, 1024 * 1024).await?)
            .map_err(|_| Error::InvalidRelease)?;
        if release.draft || release.tag_name != tag {
            return Err(Error::InvalidRelease);
        }
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == name && a.size > 0 && a.size <= MAX_BINARY_BYTES as u64)
            .ok_or(Error::InvalidRelease)?;
        let checksum_name = format!("{name}.sha256");
        if !release.assets.iter().any(|a| a.name == checksum_name) {
            return Err(Error::InvalidRelease);
        }
        let checksum = self
            .fetch(&format!("{DOWNLOAD_ROOT}{tag}/{checksum_name}"), 1024)
            .await?;
        let expected = parse_checksum(&checksum, &name).ok_or(Error::InvalidRelease)?;
        let mut staged = StagedBinary::new(&self.executable).map_err(|_| Error::InstallFailed)?;
        let mut response = self
            .response(&format!("{DOWNLOAD_ROOT}{tag}/{name}"))
            .await?
            .into_bytes_stream();
        let mut digest = Sha256::new();
        let mut size = 0usize;
        while let Some(chunk) = response.next_chunk().await {
            let chunk = chunk.map_err(|_| Error::Unavailable)?;
            size = size
                .checked_add(chunk.len())
                .filter(|n| *n <= MAX_BINARY_BYTES && *n <= asset.size as usize)
                .ok_or(Error::InvalidRelease)?;
            digest.update(&chunk);
            staged.write_all(&chunk).map_err(|_| Error::InstallFailed)?;
        }
        if size != asset.size as usize || format!("{:x}", digest.finalize()) != expected {
            return Err(Error::InvalidRelease);
        }
        if let Ok(Some(context)) = &self.managed {
            staged
                .prepare(&self.executable)
                .map_err(|_| Error::InstallFailed)?;
            // Once handed to the supervisor, a lost acknowledgement must not delete its candidate.
            staged.retain = true;
            let reply = context
                .call(af_launcher::Action::Install {
                    candidate: staged
                        .path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or(Error::InstallFailed)?
                        .to_owned(),
                    tag: tag.to_owned(),
                    sha256: expected,
                    size: size as u64,
                })
                .await
                .map_err(|_| Error::Unavailable)?;
            if !reply.accepted {
                staged.retain = false;
                return Err(Error::Conflict);
            }
            state.launcher_version = Some(reply.launcher_version);
            state.update_phase = "restarting".to_owned();
            state.install_enabled = false;
            state.install_blocked_reason = Some("managed_restart".to_owned());
            return Ok(());
        }
        staged
            .commit(&self.executable)
            .map_err(|_| Error::InstallFailed)?;
        state.restart_required = true;
        state.install_enabled = false;
        state.install_blocked_reason = Some("restart_required".to_owned());
        Ok(())
    }
}

impl SoftwareUpdateService for CommunitySoftwareUpdates {
    fn status(&self) -> SoftwareUpdateFuture<'_> {
        Box::pin(async {
            let mut state = self.state.lock().await;
            if let Ok(Some(context)) = &self.managed {
                match context.call(af_launcher::Action::Status).await {
                    Ok(reply) if reply.accepted => {
                        state.launcher_version = Some(reply.launcher_version);
                        state.update_phase = reply.phase;
                        state.launcher_error = reply.error;
                    }
                    _ => {
                        state.launcher_error = Some("launcher_unavailable".to_owned());
                    }
                }
            }
            Ok(state.clone())
        })
    }
    fn check(&self) -> SoftwareUpdateFuture<'_> {
        Box::pin(async {
            let mut state = self.state.try_lock().map_err(|_| Error::Conflict)?;
            tokio::time::timeout(Duration::from_secs(30), self.check_inner(&mut state))
                .await
                .map_err(|_| Error::Unavailable)??;
            Ok(state.clone())
        })
    }
    fn install(&self, tag: String, expected_node_id: String) -> SoftwareUpdateFuture<'_> {
        Box::pin(async move {
            let mut state = self.state.try_lock().map_err(|_| Error::Conflict)?;
            if state.node_id != expected_node_id {
                return Err(Error::Conflict);
            }
            tokio::time::timeout(
                Duration::from_secs(240),
                self.install_inner(&mut state, &tag),
            )
            .await
            .map_err(|_| Error::Unavailable)??;
            Ok(state.clone())
        })
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn release_version(tag: &str) -> Option<Version> {
    let version = Version::parse(tag.strip_prefix('v')?).ok()?;
    if !version.build.is_empty() {
        return None;
    }
    if !version.pre.is_empty() {
        let (kind, number) = version.pre.as_str().split_once('.')?;
        if !["alpha", "beta", "rc"].contains(&kind)
            || number.parse::<u64>().ok()? == 0
            || number.contains('.')
        {
            return None;
        }
    }
    Some(version)
}

fn select_release(releases: Vec<Release>, current: &Version) -> Option<Release> {
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| release_version(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, r)| (!r.prerelease && v.pre.is_empty()) || !current.pre.is_empty())
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, r)| r)
}

fn platform_name(target: &str) -> Option<&'static str> {
    match target {
        "x86_64-unknown-linux-gnu" => Some("linux-amd64"),
        "aarch64-unknown-linux-gnu" => Some("linux-arm64"),
        "x86_64-unknown-linux-musl" => Some("linux-amd64-musl"),
        "aarch64-unknown-linux-musl" => Some("linux-arm64-musl"),
        "x86_64-pc-windows-msvc" => Some("windows-amd64"),
        "aarch64-pc-windows-msvc" => Some("windows-arm64"),
        "x86_64-apple-darwin" => Some("macos-amd64"),
        "aarch64-apple-darwin" => Some("macos-arm64"),
        _ => None,
    }
}

fn package_name(tag: &str, target: &str) -> Option<String> {
    let extension = if target.contains("windows") {
        "zip"
    } else {
        "tar.gz"
    };
    Some(format!(
        "anyflows-{tag}-{}.{extension}",
        platform_name(target)?
    ))
}

fn binary_name(tag: &str, target: &str) -> Option<String> {
    let extension = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    Some(format!(
        "anyflows-{tag}-{}{extension}",
        platform_name(target)?
    ))
}

fn parse_checksum(bytes: &[u8], name: &str) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut parts = text.split_whitespace();
    let digest = parts.next()?;
    if digest.len() != 64
        || !digest.bytes().all(|c| c.is_ascii_hexdigit())
        || parts.next()?.trim_start_matches('*') != name
        || parts.next().is_some()
    {
        return None;
    }
    Some(digest.to_ascii_lowercase())
}

fn allowed_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    match url.host_str() {
        Some("api.github.com") => url
            .path()
            .starts_with("/repos/ChuqiCloud/AnyFlows/releases"),
        Some("github.com") => url
            .path()
            .starts_with("/ChuqiCloud/AnyFlows/releases/download/"),
        Some("release-assets.githubusercontent.com") => true,
        _ => false,
    }
}

struct StagedBinary {
    file: Option<fs::File>,
    path: PathBuf,
    lock: PathBuf,
    _lock_file: fs::File,
    retain: bool,
}
impl StagedBinary {
    fn new(executable: &Path) -> std::io::Result<Self> {
        let parent = executable.parent().ok_or(std::io::ErrorKind::NotFound)?;
        let lock = parent.join(".anyflows-update.lock");
        let lock_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)?;
        let path = parent.join(format!(
            ".anyflows-update-{}{}",
            uuid::Uuid::new_v4(),
            if cfg!(windows) { ".exe" } else { "" }
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => Ok(Self {
                file: Some(file),
                path,
                lock,
                _lock_file: lock_file,
                retain: false,
            }),
            Err(error) => {
                drop(lock_file);
                let _ = fs::remove_file(lock);
                Err(error)
            }
        }
    }
    fn commit(&mut self, executable: &Path) -> std::io::Result<()> {
        self.prepare(executable)?;
        let backup = executable.with_file_name(format!(
            "anyflows-backup-{}{}",
            uuid::Uuid::new_v4(),
            if cfg!(windows) { ".exe" } else { "" }
        ));
        #[cfg(not(windows))]
        {
            fs::copy(executable, &backup)?;
            fs::File::open(&backup)?.sync_all()?;
            fs::rename(&self.path, executable)?;
            // Replacement has committed; a directory-sync failure must not report an uninstalled binary.
            if let Some(parent) = executable.parent()
                && let Ok(directory) = fs::File::open(parent)
            {
                let _ = directory.sync_all();
            }
        }
        #[cfg(windows)]
        {
            fs::rename(executable, &backup)?;
            if let Err(error) = fs::rename(&self.path, executable) {
                let _ = fs::rename(&backup, executable);
                return Err(error);
            }
        }
        Ok(())
    }
    fn prepare(&mut self, executable: &Path) -> std::io::Result<()> {
        if let Some(file) = self.file.take() {
            file.set_permissions(fs::metadata(executable)?.permissions())?;
            file.sync_all()?;
        }
        Ok(())
    }
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.file
            .as_mut()
            .ok_or(std::io::ErrorKind::BrokenPipe)?
            .write_all(bytes)
    }
}
impl Drop for StagedBinary {
    fn drop(&mut self) {
        self.file = None;
        if !self.retain {
            let _ = fs::remove_file(&self.path);
        }
        let _ = fs::remove_file(&self.lock);
    }
}

/// Handle update/version commands before loading application configuration or connecting to a database.
pub async fn software_update_command() -> Option<std::process::ExitCode> {
    let args: Vec<String> = env::args().skip(1).collect();
    if !matches!(
        args.first().map(String::as_str),
        Some("update" | "--version" | "version")
    ) {
        return None;
    }
    let Ok(clients) = HttpClientProvider::new(af_httpclient::HttpClientConfig::default(), 4) else {
        return Some(std::process::ExitCode::FAILURE);
    };
    let updates = Arc::new(CommunitySoftwareUpdates::new(clients, true));
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["--version" | "version"] => updates.status().await,
        ["update", "check"] => updates.check().await,
        ["update", "install", tag, "--yes"] => {
            let node = updates.state.lock().await.node_id.clone();
            updates.install((*tag).to_owned(), node).await
        }
        _ => {
            eprintln!(
                "用法：anyflows update check | anyflows update install <版本标签> --yes\n升级前备份数据库并停止当前节点；安装完成后由管理员重启。仅升级当前二进制，不修改配置和数据。"
            );
            return Some(std::process::ExitCode::FAILURE);
        }
    };
    Some(match result {
        Ok(status) => match serde_json::to_string_pretty(&status) {
            Ok(json) => {
                println!("{json}");
                std::process::ExitCode::SUCCESS
            }
            Err(_) => std::process::ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("软件更新未完成：{error:?}");
            std::process::ExitCode::FAILURE
        }
    })
}
