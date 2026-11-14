//! Host-selected process containment. Publisher metadata cannot relax this policy.

use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

use tokio::process::Command;

use crate::sidecar::SidecarError;

/// Execution policy selected by the host administrator.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SidecarIsolationPolicy {
    /// Apply available platform restrictions without adding an installation prerequisite.
    #[default]
    Auto,
    /// Explicitly allow a trusted plugin to use the host account's OS access.
    Trusted,
    /// Require no-new-privileges and per-process resource limits.
    Restricted,
    /// Require a filesystem and network sandbox as well as resource limits.
    RequireSandbox,
}

impl SidecarIsolationPolicy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Trusted => "trusted",
            Self::Restricted => "restricted",
            Self::RequireSandbox => "require_sandbox",
        }
    }
}

/// The containment actually used by a running process, rather than desired policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidecarIsolationLevel {
    Process,
    ResourceLimited,
    Sandboxed,
    JobObject,
}

impl SidecarIsolationLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Process => "process",
            Self::ResourceLimited => "resource_limited",
            Self::Sandboxed => "sandboxed",
            Self::JobObject => "job_object",
        }
    }
}

pub(crate) struct PreparedSidecarLaunch {
    pub command: Command,
    pub isolation: SidecarIsolationLevel,
}

#[cfg(windows)]
pub(crate) fn spawn_windows_sidecar(
    command: Command,
) -> Result<Box<dyn process_wrap::tokio::ChildWrapper>, SidecarError> {
    use process_wrap::tokio::{CommandWrap, CreationFlags, JobObject, KillOnDrop};
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    CommandWrap::from(command)
        .wrap(CreationFlags(CREATE_NO_WINDOW))
        .wrap(JobObject)
        .wrap(KillOnDrop)
        .spawn()
        .map_err(|_| SidecarError::StartFailed)
}

pub(crate) fn prepare_sidecar_launch(
    executable: &Path,
    package_root: &Path,
    policy: SidecarIsolationPolicy,
) -> Result<PreparedSidecarLaunch, SidecarError> {
    #[cfg(target_os = "linux")]
    {
        prepare_linux_launch(executable, package_root, policy, &SystemTools::discover())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = package_root;
        if matches!(
            policy,
            SidecarIsolationPolicy::Restricted | SidecarIsolationPolicy::RequireSandbox
        ) {
            return Err(SidecarError::IsolationUnavailable);
        }
        Ok(PreparedSidecarLaunch {
            command: Command::new(executable),
            isolation: if cfg!(windows) {
                SidecarIsolationLevel::JobObject
            } else {
                SidecarIsolationLevel::Process
            },
        })
    }
}

#[cfg(target_os = "linux")]
struct SystemTools {
    setpriv: Option<PathBuf>,
    prlimit: Option<PathBuf>,
    bubblewrap: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
impl SystemTools {
    fn discover() -> Self {
        Self {
            setpriv: trusted_system_tool("setpriv"),
            prlimit: trusted_system_tool("prlimit"),
            bubblewrap: trusted_system_tool("bwrap"),
        }
    }
}

#[cfg(target_os = "linux")]
fn trusted_system_tool(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    // Only fixed system directories are considered. Never search a plugin-controlled PATH.
    ["/usr/bin", "/bin"].into_iter().find_map(|directory| {
        let path = Path::new(directory).join(name).canonicalize().ok()?;
        if !path.starts_with("/usr/bin") && !path.starts_with("/bin") {
            return None;
        }
        let metadata = std::fs::metadata(&path).ok()?;
        let mode = metadata.permissions().mode();
        (metadata.is_file() && metadata.uid() == 0 && mode & 0o6022 == 0 && mode & 0o111 != 0)
            .then_some(path)
    })
}

#[cfg(target_os = "linux")]
fn prepare_linux_launch(
    executable: &Path,
    package_root: &Path,
    policy: SidecarIsolationPolicy,
    tools: &SystemTools,
) -> Result<PreparedSidecarLaunch, SidecarError> {
    if policy == SidecarIsolationPolicy::Trusted {
        return Ok(PreparedSidecarLaunch {
            command: Command::new(executable),
            isolation: SidecarIsolationLevel::Process,
        });
    }
    let (Some(setpriv), Some(prlimit)) = (&tools.setpriv, &tools.prlimit) else {
        if policy == SidecarIsolationPolicy::Auto {
            return Ok(PreparedSidecarLaunch {
                command: Command::new(executable),
                isolation: SidecarIsolationLevel::Process,
            });
        }
        return Err(SidecarError::IsolationUnavailable);
    };
    if policy == SidecarIsolationPolicy::RequireSandbox && tools.bubblewrap.is_none() {
        return Err(SidecarError::IsolationUnavailable);
    }
    let mut command = Command::new(setpriv);
    command.arg("--no-new-privs").arg(prlimit);
    // Auto deliberately avoids a virtual-address ceiling: language runtimes can reserve
    // large address ranges without using that memory. Explicit restricted policies opt
    // into the stronger ceiling; a host can choose trusted mode for such runtimes.
    use nix::sys::resource::Resource;
    append_resource_limit(&mut command, "core", Resource::RLIMIT_CORE, 0)?;
    append_resource_limit(&mut command, "nofile", Resource::RLIMIT_NOFILE, 1024)?;
    append_resource_limit(&mut command, "fsize", Resource::RLIMIT_FSIZE, 67_108_864)?;
    if policy != SidecarIsolationPolicy::Auto {
        append_resource_limit(&mut command, "as", Resource::RLIMIT_AS, 4_294_967_296)?;
        append_resource_limit(&mut command, "cpu", Resource::RLIMIT_CPU, 300)?;
    }
    command.arg("--");
    if policy == SidecarIsolationPolicy::RequireSandbox {
        let bubblewrap = tools
            .bubblewrap
            .as_ref()
            .ok_or(SidecarError::IsolationUnavailable)?;
        let relative_executable = executable
            .strip_prefix(package_root)
            .map_err(|_| SidecarError::InvalidArtifact)?;
        if relative_executable
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(SidecarError::InvalidArtifact);
        }
        command.arg(bubblewrap).args([
            "--unshare-all",
            "--die-with-parent",
            "--new-session",
            "--clearenv",
            "--cap-drop",
            "ALL",
        ]);
        // Start from bwrap's empty root. Only runtime files and this verified package
        // are visible; the host's config, data directories and credential files are not.
        let runtime_directories = [
            "/usr/bin",
            "/usr/lib",
            "/usr/lib64",
            "/usr/share/locale",
            "/bin",
            "/lib",
            "/lib64",
        ];
        for directory in runtime_directories {
            if Path::new(directory).is_dir() {
                command.args(["--ro-bind", directory, directory]);
            }
        }
        if Path::new("/etc/ld.so.cache").is_file() {
            command.args(["--ro-bind", "/etc/ld.so.cache", "/etc/ld.so.cache"]);
        }
        // A service installed below a runtime-library directory must not expose its
        // own working directory through that system mount.
        if let Ok(host_directory) = std::env::current_dir().and_then(|path| path.canonicalize())
            && runtime_directories
                .iter()
                .any(|directory| host_directory.starts_with(directory))
        {
            command.arg("--tmpfs").arg(host_directory);
        }
        command.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp"]);
        command.arg("--ro-bind").arg(package_root).arg("/plugin");
        command.args([
            "--chdir",
            "/plugin",
            "--setenv",
            "PATH",
            "/usr/bin:/bin",
            "--setenv",
            "HOME",
            "/tmp",
            "--remount-ro",
            "/",
            "--",
        ]);
        command.arg(Path::new("/plugin").join(relative_executable));
        Ok(PreparedSidecarLaunch {
            command,
            isolation: SidecarIsolationLevel::Sandboxed,
        })
    } else {
        command.arg(executable);
        Ok(PreparedSidecarLaunch {
            command,
            isolation: SidecarIsolationLevel::ResourceLimited,
        })
    }
}

#[cfg(target_os = "linux")]
fn append_resource_limit(
    command: &mut Command,
    name: &str,
    resource: nix::sys::resource::Resource,
    ceiling: u64,
) -> Result<(), SidecarError> {
    let (soft, hard) =
        nix::sys::resource::getrlimit(resource).map_err(|_| SidecarError::IsolationUnavailable)?;
    let (soft, hard) = reduced_resource_limit(soft, hard, ceiling);
    command.arg(format!("--{name}={soft}:{hard}"));
    Ok(())
}

#[cfg(target_os = "linux")]
fn reduced_resource_limit<T: Into<u64>>(soft: T, hard: T, ceiling: u64) -> (u64, u64) {
    let soft = soft.into();
    let hard = hard.into();
    (soft.min(hard).min(ceiling), hard.min(ceiling))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn tools() -> SystemTools {
        SystemTools {
            setpriv: Some(PathBuf::from("/usr/bin/setpriv")),
            prlimit: Some(PathBuf::from("/usr/bin/prlimit")),
            bubblewrap: Some(PathBuf::from("/usr/bin/bwrap")),
        }
    }

    #[test]
    fn resource_ceilings_never_raise_the_host_process_limits() {
        assert_eq!(reduced_resource_limit(256_u32, 512_u32, 1024), (256, 512));
        assert_eq!(
            reduced_resource_limit(u64::MAX, u64::MAX, 1024),
            (1024, 1024)
        );
        assert_eq!(reduced_resource_limit(0, u64::MAX, 0), (0, 0));
    }

    #[test]
    fn automatic_fallback_and_explicit_policy_do_not_confuse_requested_and_effective_isolation() {
        let empty = SystemTools {
            setpriv: None,
            prlimit: None,
            bubblewrap: None,
        };
        let executable = Path::new("/packages/example/bin/plugin");
        let package = Path::new("/packages/example");
        let launch =
            prepare_linux_launch(executable, package, SidecarIsolationPolicy::Auto, &empty)
                .unwrap();
        assert_eq!(launch.isolation, SidecarIsolationLevel::Process);
        assert!(matches!(
            prepare_linux_launch(
                executable,
                package,
                SidecarIsolationPolicy::Restricted,
                &empty
            ),
            Err(SidecarError::IsolationUnavailable)
        ));
        assert!(matches!(
            prepare_linux_launch(
                executable,
                package,
                SidecarIsolationPolicy::RequireSandbox,
                &empty
            ),
            Err(SidecarError::IsolationUnavailable)
        ));
    }

    #[test]
    fn sandbox_mounts_only_runtime_and_verified_package_and_has_no_network() {
        let launch = prepare_linux_launch(
            Path::new("/packages/example/bin/plugin"),
            Path::new("/packages/example"),
            SidecarIsolationPolicy::RequireSandbox,
            &tools(),
        )
        .unwrap();
        let arguments = launch
            .command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(launch.isolation, SidecarIsolationLevel::Sandboxed);
        assert!(arguments.contains(&"--no-new-privs".to_owned()));
        assert!(arguments.contains(&"--unshare-all".to_owned()));
        assert!(arguments.contains(&"--clearenv".to_owned()));
        assert!(arguments.contains(&"--remount-ro".to_owned()));
        assert_eq!(arguments.last().unwrap(), "/plugin/bin/plugin");
        assert!(
            !arguments
                .windows(3)
                .any(|arguments| arguments == ["--ro-bind", "/", "/"])
        );
        assert!(!arguments.contains(&"--share-net".to_owned()));
    }

    #[test]
    fn sandbox_cannot_run_an_entry_outside_its_verified_package() {
        assert!(matches!(
            prepare_linux_launch(
                Path::new("/other/plugin"),
                Path::new("/packages/example"),
                SidecarIsolationPolicy::RequireSandbox,
                &tools()
            ),
            Err(SidecarError::InvalidArtifact)
        ));
    }

    /// The dedicated Linux sandbox job opts in after provisioning Bubblewrap and
    /// user namespaces. Ordinary unit-test jobs need no sandbox prerequisites.
    #[tokio::test]
    async fn required_sandbox_hides_host_files_and_has_a_separate_network_namespace() {
        if std::env::var("ANYFLOWS_PLUGIN_SANDBOX_TEST")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }
        use std::os::unix::fs::PermissionsExt as _;

        let package = tempfile::tempdir().unwrap();
        let private_host_data = tempfile::tempdir().unwrap();
        let secret_path = private_host_data.path().join("host-only-value");
        std::fs::write(&secret_path, "host-only-test-value").unwrap();
        let entry = package.path().join("plugin");
        let quoted_secret = secret_path.to_string_lossy().replace('\'', "'\\''");
        let script = format!(
            "#!/bin/sh\nset -eu\nif [ -r '{quoted_secret}' ]; then exit 21; fi\nif ( : > /plugin/unexpected-write ) 2>/dev/null; then exit 22; fi\n: > /tmp/plugin-scratch\nprintf '%s\\n' \"$(readlink /proc/self/ns/net)\"\n"
        );
        std::fs::write(&entry, script).unwrap();
        std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o500)).unwrap();
        let mut launch = prepare_sidecar_launch(
            &entry,
            package.path(),
            SidecarIsolationPolicy::RequireSandbox,
        )
        .unwrap();
        assert_eq!(launch.isolation, SidecarIsolationLevel::Sandboxed);
        launch
            .command
            .env_clear()
            .current_dir(package.path())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(15), launch.command.output())
                .await
                .unwrap()
                .unwrap();
        assert!(
            output.status.success(),
            "required filesystem restrictions must execute successfully"
        );
        let host_namespace = std::fs::read_link("/proc/self/ns/net").unwrap();
        let plugin_namespace = String::from_utf8(output.stdout).unwrap();
        assert_ne!(plugin_namespace.trim(), host_namespace.to_str().unwrap());
        assert!(!package.path().join("unexpected-write").exists());
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use std::{path::PathBuf, process::Stdio, time::Duration};

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use tokio::{
        io::{AsyncBufReadExt as _, BufReader},
        time::timeout,
    };
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    use super::*;

    fn powershell() -> PathBuf {
        PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root is available"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe")
    }

    fn encoded_script(script: &str) -> String {
        STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn windows_reports_job_containment_and_rejects_unsupported_sandbox_requirements() {
        let executable = powershell();
        let launch = prepare_sidecar_launch(
            &executable,
            executable.parent().unwrap(),
            SidecarIsolationPolicy::Auto,
        )
        .unwrap();
        assert_eq!(launch.isolation, SidecarIsolationLevel::JobObject);
        for policy in [
            SidecarIsolationPolicy::Restricted,
            SidecarIsolationPolicy::RequireSandbox,
        ] {
            assert!(matches!(
                prepare_sidecar_launch(&executable, executable.parent().unwrap(), policy),
                Err(SidecarError::IsolationUnavailable)
            ));
        }
    }

    #[tokio::test]
    async fn job_object_stops_a_real_descendant_process() {
        let powershell = powershell();
        let leaf = encoded_script("Start-Sleep -Seconds 60");
        let parent = encoded_script(&format!(
            "$child = Start-Process -FilePath ([System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName) -ArgumentList '-NoLogo','-NoProfile','-NonInteractive','-EncodedCommand','{leaf}' -WindowStyle Hidden -PassThru; [Console]::Out.WriteLine($child.Id); [Console]::Out.Flush(); Start-Sleep -Seconds 60"
        ));
        let mut command = Command::new(&powershell);
        command
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-EncodedCommand",
                &parent,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = spawn_windows_sidecar(command).unwrap();
        let stdout = child.stdout().take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        timeout(Duration::from_secs(15), reader.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        let descendant: u32 = line
            .trim()
            .parse()
            .expect("parent reports its child PID after spawning");
        assert!(descendant > 0);
        child.start_kill().unwrap();
        timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap();
        let status = Command::new(&powershell)
            .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &format!("$ErrorActionPreference='SilentlyContinue'; if (Get-Process -Id {descendant}) {{ exit 1 }} else {{ exit 0 }}")])
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .status().await.unwrap();
        assert!(
            status.success(),
            "the descendant must be terminated with the sidecar job"
        );
    }
}
