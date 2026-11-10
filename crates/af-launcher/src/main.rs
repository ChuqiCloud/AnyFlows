//! Stable process supervisor. It never replaces itself or automatically rolls back a migrated database.
use af_launcher::{Action, CONTROL_ENV, MAX_MESSAGE_BYTES, PROTOCOL, Reply, Request};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{ExitCode, Stdio},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    time::{Instant, sleep},
};

const MAX_BINARY_BYTES: u64 = 256 * 1024 * 1024;

mod environment;

#[derive(Clone, Serialize, Deserialize)]
struct Selection {
    protocol: u32,
    active: String,
    previous: Option<String>,
    version: String,
}

struct Settings {
    program: PathBuf,
    state: PathBuf,
    working_directory: PathBuf,
    env_file: Option<PathBuf>,
    environment: Vec<(String, String)>,
    args: Vec<OsString>,
    stop_timeout: Duration,
    ready_timeout: Duration,
    select_only: bool,
}

#[derive(Deserialize)]
struct Identity {
    version: String,
    target: String,
    edition: String,
    launcher_protocol: Option<u32>,
}

struct Update {
    candidate: PathBuf,
    tag: String,
    digest: String,
    size: u64,
}

struct Running {
    child: Child,
    generation: String,
    started: Instant,
    ready_deadline: Instant,
}

struct Supervisor {
    updates_allowed: bool,
    settings: Settings,
    selection: Selection,
    target: String,
    token: String,
    listener: TcpListener,
    endpoint: std::net::SocketAddr,
    running: Option<Running>,
    phase: String,
    error: Option<String>,
    pending: Option<Update>,
    shutdown_at: Option<Instant>,
    stop_deadline: Option<Instant>,
    restart_at: Option<Instant>,
    failures: u32,
    _lock: File,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    if args.as_slice() == [OsString::from("--version")] {
        println!(
            "{}",
            serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"protocol":PROTOCOL,"target":env!("AF_LAUNCHER_TARGET")})
        );
        return ExitCode::SUCCESS;
    }
    if args.as_slice() == [OsString::from("--help")] {
        println!(
            "anyflows-launcher [--program PATH] [--state-dir PATH] [--working-directory PATH] [--env-file PATH] [--stop-timeout-secs N] [--ready-timeout-secs N] [-- SERVER_ARGS...]\n默认启动同目录 anyflows；继承父进程环境，工作目录 .env 补充默认值；显式 env-file 相对启动目录解析。日志沿用标准输出和错误管道。状态默认保存到启动目录 .anyflows-launcher。停止启动器会优雅停止主程序。启动器自身需停止后手动维护。"
        );
        return ExitCode::SUCCESS;
    }
    let result = async {
        let settings = Settings::parse(args)?;
        let supervisor = Supervisor::new(settings).await?;
        if supervisor.settings.select_only {
            println!(
                "{}",
                serde_json::json!({"selected_version":supervisor.selection.version})
            );
            Ok(())
        } else {
            supervisor.run().await
        }
    }
    .await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => {
            eprintln!("AnyFlows 启动器未完成：{code}");
            ExitCode::FAILURE
        }
    }
}

impl Settings {
    fn parse(args: Vec<OsString>) -> Result<Self, &'static str> {
        let cwd = env::current_dir().map_err(|_| "working_directory")?;
        let executable = env::current_exe().map_err(|_| "launcher_path")?;
        let mut settings = Self {
            program: executable.with_file_name(binary_name()),
            state: cwd.join(".anyflows-launcher"),
            working_directory: cwd.clone(),
            env_file: None,
            environment: vec![],
            args: vec![],
            stop_timeout: Duration::from_secs(330),
            ready_timeout: Duration::from_secs(330),
            select_only: false,
        };
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            if flag == "--" {
                settings.args.extend(args);
                break;
            }
            if flag == "--select-program" {
                settings.select_only = true;
                continue;
            }
            let value = args.next().ok_or("argument_value")?;
            match flag.to_str() {
                Some("--program") => settings.program = absolute(&cwd, Path::new(&value)),
                Some("--state-dir") => settings.state = absolute(&cwd, Path::new(&value)),
                Some("--env-file") => settings.env_file = Some(absolute(&cwd, Path::new(&value))),
                Some("--working-directory") => {
                    settings.working_directory = absolute(&cwd, Path::new(&value))
                }
                Some("--stop-timeout-secs" | "--ready-timeout-secs") => {
                    let seconds: u64 = value
                        .to_str()
                        .and_then(|v| v.parse().ok())
                        .filter(|v| (1..=3600).contains(v))
                        .ok_or("timeout")?;
                    if flag == "--stop-timeout-secs" {
                        settings.stop_timeout = Duration::from_secs(seconds);
                    } else {
                        settings.ready_timeout = Duration::from_secs(seconds);
                    }
                }
                _ => return Err("argument"),
            }
        }
        if !settings.working_directory.is_dir() {
            return Err("working_directory");
        }
        Ok(settings)
    }
}

impl Supervisor {
    async fn new(mut settings: Settings) -> Result<Self, &'static str> {
        if !settings.select_only {
            settings.environment =
                environment::defaults(&settings.working_directory, settings.env_file.as_deref())?;
        }
        fs::create_dir_all(&settings.state).map_err(|_| "state_directory")?;
        settings.state = settings
            .state
            .canonicalize()
            .map_err(|_| "state_directory")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&settings.state, fs::Permissions::from_mode(0o700))
                .map_err(|_| "state_permissions")?;
        }
        let lock = lock_file(&settings.state.join("launcher.lock"))?;
        let selected = load_selection(&settings.state)?;
        let active_identity = match &selected {
            Some(selection) => {
                Some(identity(&selected_program(&settings.state, &selection.active)?).await?)
            }
            None => None,
        };
        let seed = identity(&settings.program).await;
        let use_seed = settings.select_only
            || selected.is_none()
            || seed
                .as_ref()
                .ok()
                .zip(active_identity.as_ref())
                .is_some_and(|(seed, active)| {
                    semver::Version::parse(&seed.version)
                        .ok()
                        .zip(semver::Version::parse(&active.version).ok())
                        .is_some_and(|(seed, active)| seed > active)
                });
        let selection = if use_seed {
            let metadata = seed?;
            verify_identity(
                &metadata,
                None,
                active_identity
                    .as_ref()
                    .map(|identity| identity.target.as_str()),
            )?;
            let probe = lock_file(&settings.state.join("main.lock"))?;
            drop(probe);
            let program = settings
                .program
                .canonicalize()
                .map_err(|_| "program_missing")?;
            let relative = format!(
                "versions/bootstrap-{}/{}",
                uuid::Uuid::new_v4(),
                binary_name()
            );
            let destination = settings.state.join(&relative);
            fs::create_dir_all(destination.parent().ok_or("version_directory")?)
                .map_err(|_| "version_directory")?;
            fs::copy(&program, &destination).map_err(|_| "copy_program")?;
            File::open(&destination)
                .and_then(|file| file.sync_all())
                .map_err(|_| "sync_program")?;
            let selection = Selection {
                protocol: PROTOCOL,
                active: relative,
                previous: selected.map(|selection| selection.active),
                version: metadata.version,
            };
            save_selection(&settings.state, &selection)?;
            selection
        } else {
            selected.ok_or("state_missing")?
        };
        let metadata = identity(&selected_program(&settings.state, &selection.active)?).await?;
        verify_identity(&metadata, Some(&selection.version), None)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|_| "control_bind")?;
        let endpoint = listener.local_addr().map_err(|_| "control_bind")?;
        Ok(Self {
            updates_allowed: !af_launcher::in_container(),
            settings,
            selection,
            target: metadata.target,
            token: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            listener,
            endpoint,
            running: None,
            phase: "starting".to_owned(),
            error: None,
            pending: None,
            shutdown_at: None,
            stop_deadline: None,
            restart_at: Some(Instant::now()),
            failures: 0,
            _lock: lock,
        })
    }

    async fn start(&mut self) -> Result<(), &'static str> {
        // The child keeps this lock through full HTTP/billing/database shutdown, including parent death.
        let lock_path = self.settings.state.join("main.lock");
        let probe = match lock_file(&lock_path) {
            Ok(file) => file,
            Err(_) => {
                self.phase = "waiting_previous_process".to_owned();
                self.restart_at = Some(Instant::now() + Duration::from_secs(1));
                return Ok(());
            }
        };
        drop(probe);
        let program = selected_program(&self.settings.state, &self.selection.active)?;
        let generation = uuid::Uuid::new_v4().to_string();
        let mut command = Command::new(program);
        command
            .args(&self.settings.args)
            .current_dir(&self.settings.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        for (key, value) in &self.settings.environment {
            // Command inherits the parent environment, including non-Unicode values.
            // Never mutate the launcher's environment or replace panel-provided values.
            if env::var_os(key).is_none() {
                command.env(key, value);
            }
        }
        command
            .env(CONTROL_ENV[0], self.endpoint.to_string())
            .env(CONTROL_ENV[1], &self.token)
            .env(CONTROL_ENV[2], &generation)
            .env(CONTROL_ENV[3], &lock_path);
        let child = command.spawn().map_err(|_| "spawn_program")?;
        self.running = Some(Running {
            child,
            generation,
            started: Instant::now(),
            ready_deadline: Instant::now() + self.settings.ready_timeout,
        });
        self.phase = "starting".to_owned();
        self.restart_at = None;
        Ok(())
    }

    async fn run(self) -> Result<(), &'static str> {
        self.run_with_stop(shutdown_signal()).await
    }

    async fn run_with_stop(
        mut self,
        stop: impl std::future::Future<Output = ()>,
    ) -> Result<(), &'static str> {
        tokio::pin!(stop);
        loop {
            tokio::select! {
                _ = &mut stop => { self.stop().await; return Ok(()); }
                incoming = self.listener.accept() => {
                    let (stream, _) = incoming.map_err(|_| "control_accept")?;
                    if let Ok(Ok((request, mut stream))) = tokio::time::timeout(Duration::from_secs(2), read_request(stream)).await {
                        let reply = self.handle(request).await;
                        let _ = tokio::time::timeout(Duration::from_secs(2), write_reply(&mut stream, reply)).await;
                    }
                }
                _ = sleep(Duration::from_millis(100)) => {}
            }
            self.tick().await?;
        }
    }

    async fn handle(&mut self, request: Request) -> Reply {
        let owned = self.running.as_ref().is_some_and(|running| {
            running.child.id() == Some(request.pid) && running.generation == request.generation
        });
        if request.protocol != PROTOCOL || request.token != self.token || !owned {
            return Reply::new(false, &self.phase, Some("unauthorized"));
        }
        match request.action {
            Action::Status => Reply::new(true, &self.phase, self.error.as_deref()),
            Action::Ready => {
                if self.pending.is_none() {
                    self.phase = "idle".to_owned();
                    if self.error.as_deref() == Some("startup_timeout") {
                        self.error = None;
                    }
                }
                Reply::new(true, &self.phase, self.error.as_deref())
            }
            Action::Install {
                candidate,
                tag,
                sha256,
                size,
            } => {
                if !self.updates_allowed {
                    return Reply::new(false, &self.phase, Some("container"));
                }
                if self.pending.is_some() || self.phase != "idle" {
                    return Reply::new(false, &self.phase, Some("busy"));
                }
                let result = self.validate_update(candidate, tag, sha256, size).await;
                match result {
                    Ok(update) => {
                        self.pending = Some(update);
                        self.phase = "restarting".to_owned();
                        self.error = None;
                        self.shutdown_at = Some(Instant::now() + Duration::from_millis(1000));
                        Reply::new(true, &self.phase, None)
                    }
                    Err(code) => Reply::new(false, &self.phase, Some(code)),
                }
            }
        }
    }

    async fn validate_update(
        &self,
        candidate: String,
        tag: String,
        digest: String,
        size: u64,
    ) -> Result<Update, &'static str> {
        let name = candidate
            .strip_prefix(".anyflows-update-")
            .ok_or("candidate_name")?;
        let name = if cfg!(windows) {
            name.strip_suffix(".exe").ok_or("candidate_name")?
        } else {
            name
        };
        if uuid::Uuid::parse_str(name).is_err() || !single_component(&candidate) {
            return Err("candidate_name");
        }
        let version = tag
            .strip_prefix('v')
            .and_then(|v| semver::Version::parse(v).ok())
            .ok_or("version")?;
        if version <= semver::Version::parse(&self.selection.version).map_err(|_| "version")?
            || !version.build.is_empty()
        {
            return Err("version");
        }
        let active = selected_program(&self.settings.state, &self.selection.active)?;
        let candidate = active.parent().ok_or("candidate_path")?.join(candidate);
        verify_file(&candidate, &digest, size)?;
        let metadata = identity(&candidate).await?;
        verify_identity(&metadata, Some(&version.to_string()), Some(&self.target))?;
        Ok(Update {
            candidate,
            tag,
            digest,
            size,
        })
    }

    async fn tick(&mut self) -> Result<(), &'static str> {
        if self.restart_at.is_some_and(|at| Instant::now() >= at) && self.running.is_none() {
            self.start().await?;
        }
        if self.shutdown_at.is_some_and(|at| Instant::now() >= at) {
            self.shutdown_at = None;
            if let Some(update) = self.pending.as_ref()
                && let Err(code) = verify_file(&update.candidate, &update.digest, update.size)
            {
                self.error = Some(code.to_owned());
                self.phase = "idle".to_owned();
                self.pending = None;
                return Ok(());
            }
            if let Some(running) = self.running.as_mut() {
                send_shutdown(running, &self.token).await;
                self.stop_deadline = Some(Instant::now() + self.settings.stop_timeout);
            }
        }
        let exited = self
            .running
            .as_mut()
            .map(|running| running.child.try_wait())
            .transpose()
            .map_err(|_| "wait_program")?
            .flatten()
            .is_some();
        if exited {
            let running = self.running.take().ok_or("process_state")?;
            self.stop_deadline = None;
            if let Some(update) = self.pending.take() {
                match self.commit_update(update) {
                    Ok(()) => self.error = None,
                    Err(code) => self.error = Some(code.to_owned()),
                }
                self.failures = 0;
                self.restart_at = Some(Instant::now());
            } else {
                self.failures = if running.started.elapsed() >= Duration::from_secs(60) {
                    1
                } else {
                    self.failures.saturating_add(1)
                };
                if self.failures >= 5 {
                    self.phase = "failed".to_owned();
                    self.error = Some("restart_limit".to_owned());
                    self.restart_at = None;
                    eprintln!("AnyFlows 主程序连续退出，已暂停自动拉起；请检查日志后重启启动器。");
                } else {
                    self.phase = "backoff".to_owned();
                    self.restart_at =
                        Some(Instant::now() + Duration::from_secs(1u64 << self.failures.min(5)));
                }
            }
        }
        if self.stop_deadline.is_some_and(|at| Instant::now() >= at) {
            // No second process and no version switch while an old writer is still alive.
            self.stop_deadline = None;
            self.pending = None;
            self.phase = "stop_failed".to_owned();
            self.error = Some("shutdown_timeout".to_owned());
        }
        if self.phase == "starting"
            && self
                .running
                .as_ref()
                .is_some_and(|running| Instant::now() >= running.ready_deadline)
        {
            self.phase = "unhealthy".to_owned();
            self.error = Some("startup_timeout".to_owned());
        }
        Ok(())
    }

    fn commit_update(&mut self, update: Update) -> Result<(), &'static str> {
        verify_file(&update.candidate, &update.digest, update.size)?;
        let relative = format!(
            "versions/{}-{}/{}",
            update.tag,
            uuid::Uuid::new_v4(),
            binary_name()
        );
        let destination = self.settings.state.join(&relative);
        fs::create_dir_all(destination.parent().ok_or("version_directory")?)
            .map_err(|_| "version_directory")?;
        fs::rename(&update.candidate, &destination).map_err(|_| "move_candidate")?;
        let next = Selection {
            protocol: PROTOCOL,
            active: relative,
            previous: Some(self.selection.active.clone()),
            version: update.tag.trim_start_matches('v').to_owned(),
        };
        save_selection(&self.settings.state, &next)?;
        self.selection = next;
        Ok(())
    }

    async fn stop(&mut self) {
        self.pending = None;
        self.restart_at = None;
        self.phase = "stopping".to_owned();
        if let Some(running) = self.running.as_mut() {
            send_shutdown(running, &self.token).await;
            if tokio::time::timeout(self.settings.stop_timeout, running.child.wait())
                .await
                .is_err()
            {
                eprintln!("AnyFlows 主程序超过停止时限，正在终止受托管进程。");
                let _ = running.child.kill().await;
            }
        }
        self.running = None;
    }
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "anyflows.exe"
    } else {
        "anyflows"
    }
}
fn absolute(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}
fn single_component(path: &str) -> bool {
    let mut components = Path::new(path).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}
fn lock_file(path: &Path) -> Result<File, &'static str> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| "lock_file")?;
    file.try_lock_exclusive().map_err(|_| "already_running")?;
    Ok(file)
}

fn selected_program(root: &Path, active: &str) -> Result<PathBuf, &'static str> {
    let path = Path::new(active);
    if path.is_absolute()
        || !path.starts_with("versions")
        || path.components().count() != 3
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("active_path");
    }
    let program = root
        .join(path)
        .canonicalize()
        .map_err(|_| "active_missing")?;
    if !program.starts_with(root.join("versions")) || !program.is_file() {
        return Err("active_path");
    }
    Ok(program)
}

fn manifests(root: &Path) -> Result<Vec<(u64, PathBuf)>, &'static str> {
    let mut files = vec![];
    for entry in fs::read_dir(root).map_err(|_| "state_read")? {
        let entry = entry.map_err(|_| "state_read")?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(sequence) = name
            .strip_prefix("state-")
            .and_then(|s| s.strip_suffix(".json"))
            .and_then(|s| s.parse::<u64>().ok())
        {
            files.push((sequence, entry.path()));
        }
    }
    files.sort_by_key(|(sequence, _)| *sequence);
    Ok(files)
}

fn load_selection(root: &Path) -> Result<Option<Selection>, &'static str> {
    let Some((_, path)) = manifests(root)?.pop() else {
        return Ok(None);
    };
    let bytes = fs::read(path).map_err(|_| "state_read")?;
    if bytes.len() > MAX_MESSAGE_BYTES as usize {
        return Err("state_invalid");
    }
    let state: Selection = serde_json::from_slice(&bytes).map_err(|_| "state_invalid")?;
    if state.protocol != PROTOCOL {
        return Err("state_protocol");
    }
    selected_program(root, &state.active)?;
    Ok(Some(state))
}

fn save_selection(root: &Path, selection: &Selection) -> Result<(), &'static str> {
    let files = manifests(root)?;
    let next = files
        .last()
        .map(|(sequence, _)| sequence.saturating_add(1))
        .unwrap_or(1);
    let temporary = root.join(format!(".state-{}", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "state_write")?;
    let bytes = serde_json::to_vec(selection).map_err(|_| "state_write")?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| "state_write")?;
    drop(file);
    // A new immutable name gives atomic commit on both Unix and Windows without replacing an open file.
    fs::rename(&temporary, root.join(format!("state-{next:020}.json")))
        .map_err(|_| "state_commit")?;
    #[cfg(unix)]
    if let Ok(directory) = File::open(root) {
        let _ = directory.sync_all();
    }
    for (_, path) in files.iter().rev().skip(7) {
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn verify_file(path: &Path, expected: &str, size: u64) -> Result<(), &'static str> {
    if size == 0
        || size > MAX_BINARY_BYTES
        || expected.len() != 64
        || !expected.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("candidate_digest");
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| "candidate_missing")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() != size {
        return Err("candidate_size");
    }
    let mut file = File::open(path).map_err(|_| "candidate_read")?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|_| "candidate_read")?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > size {
            return Err("candidate_size");
        }
        digest.update(&buffer[..count]);
    }
    if total != size || format!("{:x}", digest.finalize()) != expected.to_ascii_lowercase() {
        return Err("candidate_digest");
    }
    Ok(())
}

async fn identity(program: &Path) -> Result<Identity, &'static str> {
    let mut command = Command::new(program);
    command
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for name in CONTROL_ENV {
        command.env_remove(name);
    }
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .map_err(|_| "identity_timeout")?
        .map_err(|_| "identity_process")?;
    if !output.status.success() || output.stdout.len() > MAX_MESSAGE_BYTES as usize {
        return Err("identity_invalid");
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "identity_invalid")
}

fn verify_identity(
    identity: &Identity,
    version: Option<&str>,
    target: Option<&str>,
) -> Result<(), &'static str> {
    if identity.edition != "community"
        || identity.launcher_protocol != Some(PROTOCOL)
        || semver::Version::parse(&identity.version).is_err()
        || version.is_some_and(|v| v != identity.version)
        || target.is_some_and(|v| v != identity.target)
    {
        return Err("identity_mismatch");
    }
    Ok(())
}

async fn read_request(stream: TcpStream) -> Result<(Request, TcpStream), ()> {
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    (&mut reader)
        .take(MAX_MESSAGE_BYTES + 1)
        .read_until(b'\n', &mut bytes)
        .await
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_MESSAGE_BYTES {
        return Err(());
    }
    let request = serde_json::from_slice(&bytes).map_err(|_| ())?;
    Ok((request, reader.into_inner()))
}
async fn write_reply(stream: &mut TcpStream, reply: Reply) -> Result<(), ()> {
    let mut bytes = serde_json::to_vec(&reply).map_err(|_| ())?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await.map_err(|_| ())
}
async fn send_shutdown(running: &mut Running, token: &str) {
    if let Some(input) = running.child.stdin.as_mut() {
        let mut bytes = serde_json::to_vec(
            &serde_json::json!({"protocol":PROTOCOL,"token":token,"command":"shutdown"}),
        )
        .unwrap_or_default();
        bytes.push(b'\n');
        let _ = tokio::time::timeout(Duration::from_secs(2), input.write_all(&bytes)).await;
    }
}
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            return;
        };
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests;
