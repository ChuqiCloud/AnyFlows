use super::*;

fn directory() -> PathBuf {
    let path = env::temp_dir().join(format!("af-launcher-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}
fn selection(root: &Path, version: &str) -> Selection {
    let active = format!("versions/test-{version}/{}", binary_name());
    let program = root.join(&active);
    fs::create_dir_all(program.parent().unwrap()).unwrap();
    fs::write(program, b"binary").unwrap();
    Selection {
        protocol: PROTOCOL,
        active,
        previous: None,
        version: version.to_owned(),
    }
}

#[test]
fn immutable_selection_survives_restart_and_corruption_never_rolls_back_silently() {
    let root = directory();
    let first = selection(&root, "1.0.0");
    save_selection(&root, &first).unwrap();
    let mut second = selection(&root, "2.0.0");
    second.previous = Some(first.active);
    save_selection(&root, &second).unwrap();
    assert_eq!(load_selection(&root).unwrap().unwrap().version, "2.0.0");
    fs::write(root.join("state-00000000000000000003.json"), b"incomplete").unwrap();
    assert!(load_selection(&root).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn selection_paths_cannot_escape_the_version_directory() {
    let root = directory();
    for path in [
        "../outside",
        "versions/../outside",
        "/tmp/program",
        "versions/a/b/c",
    ] {
        assert!(selected_program(&root, path).is_err(), "{path}");
    }
    assert!(!single_component("../.anyflows-update-x"));
    assert!(!single_component("folder/file"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn candidate_size_hash_and_file_type_are_verified() {
    let root = directory();
    let candidate = root.join("candidate");
    fs::write(&candidate, b"candidate").unwrap();
    let hash = format!("{:x}", Sha256::digest(b"candidate"));
    assert!(verify_file(&candidate, &hash, 9).is_ok());
    assert!(verify_file(&candidate, &hash, 8).is_err());
    assert!(verify_file(&candidate, &"a".repeat(64), 9).is_err());
    assert!(verify_file(&root, &hash, 9).is_err());
    #[cfg(unix)]
    {
        let link = root.join("link");
        std::os::unix::fs::symlink(&candidate, &link).unwrap();
        assert!(verify_file(&link, &hash, 9).is_err());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_one_launcher_or_main_process_can_hold_a_runtime_lock() {
    let root = directory();
    let path = root.join("lock");
    let first = lock_file(&path).unwrap();
    assert!(lock_file(&path).is_err());
    drop(first);
    assert!(lock_file(&path).is_ok());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn managed_programs_require_a_compatible_protocol_version_and_platform() {
    let metadata = Identity {
        edition: "community".to_owned(),
        version: "1.0.0".to_owned(),
        target: "x86_64-test".to_owned(),
        launcher_protocol: Some(PROTOCOL),
    };
    assert!(verify_identity(&metadata, Some("1.0.0"), Some("x86_64-test")).is_ok());
    assert!(verify_identity(&metadata, Some("2.0.0"), None).is_err());
    assert!(verify_identity(&metadata, None, Some("aarch64-test")).is_err());
}

#[cfg(unix)]
fn program_fixture(root: &Path, name: &str, version: &str, update: bool) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let source = include_str!("../tests/fixtures/program.py")
        .replace("__VERSION__", &serde_json::to_string(version).unwrap())
        .replace(
            "__ROOT__",
            &serde_json::to_string(root.to_str().unwrap()).unwrap(),
        )
        .replace("__UPDATE__", if update { "True" } else { "False" });
    let program = root.join(name);
    fs::write(&program, source).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    program
}

#[cfg(unix)]
fn fixture_settings(root: &Path, program: PathBuf) -> Settings {
    Settings {
        program,
        state: root.join("state"),
        working_directory: root.to_owned(),
        env_file: None,
        environment: vec![],
        args: vec!["--preserved-argument".into()],
        stop_timeout: Duration::from_secs(3),
        ready_timeout: Duration::from_secs(3),
        select_only: false,
    }
}

#[cfg(unix)]
fn events(root: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(root.join("events.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[cfg(unix)]
async fn wait_event(root: &Path, version: &str, event: &str, count: usize) {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if events(root)
                .iter()
                .filter(|value| value["version"] == version && value["event"] == event)
                .count()
                >= count
            {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("managed fixture did not reach its expected lifecycle event");
}

#[cfg(unix)]
#[tokio::test]
async fn managed_update_drains_old_process_switches_version_and_survives_launcher_restart() {
    let root = directory();
    fs::write(root.join("preserved.txt"), "persistent data").unwrap();
    program_fixture(&root, "new-program", "2.0.0", false);
    let seed = program_fixture(&root, "seed", "1.0.0", true);
    let mut supervisor = Supervisor::new(fixture_settings(&root, seed.clone()))
        .await
        .unwrap();
    // Inject policy only into this unit-test coordinator. Shipped launchers always deny container updates.
    supervisor.updates_allowed = true;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(supervisor.run_with_stop(async {
        let _ = stopped.await;
    }));
    wait_event(&root, "2.0.0", "ready", 1).await;
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let records = events(&root);
    let old_stopped = records
        .iter()
        .position(|value| value["version"] == "1.0.0" && value["event"] == "shutdown")
        .unwrap();
    let new_started = records
        .iter()
        .position(|value| value["version"] == "2.0.0" && value["event"] == "start")
        .unwrap();
    assert!(old_stopped < new_started);
    assert!(
        records
            .iter()
            .all(|value| value["cwd"] == root.to_str().unwrap()
                && value["args"] == serde_json::json!(["--preserved-argument"]))
    );
    let selected = load_selection(&root.join("state")).unwrap().unwrap();
    assert_eq!(selected.version, "2.0.0");
    assert!(selected.previous.is_some());
    let supervisor = Supervisor::new(fixture_settings(&root, seed))
        .await
        .unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(supervisor.run_with_stop(async {
        let _ = stopped.await;
    }));
    wait_event(&root, "2.0.0", "ready", 2).await;
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(
        events(&root)
            .iter()
            .filter(|value| value["version"] == "1.0.0" && value["event"] == "start")
            .count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn failed_new_program_never_silently_reverts_to_an_older_database_client() {
    let root = directory();
    fs::write(root.join("preserved.txt"), "persistent data").unwrap();
    fs::write(root.join("fail-new"), b"fail after selection").unwrap();
    program_fixture(&root, "new-program", "2.0.0", false);
    let seed = program_fixture(&root, "seed", "1.0.0", true);
    let mut supervisor = Supervisor::new(fixture_settings(&root, seed))
        .await
        .unwrap();
    supervisor.updates_allowed = true;
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(supervisor.run_with_stop(async {
        let _ = stopped.await;
    }));
    wait_event(&root, "2.0.0", "failed", 1).await;
    stop.send(()).unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(
        load_selection(&root.join("state"))
            .unwrap()
            .unwrap()
            .version,
        "2.0.0"
    );
    assert_eq!(
        events(&root)
            .iter()
            .filter(|value| value["version"] == "1.0.0" && value["event"] == "start")
            .count(),
        1
    );
    fs::remove_dir_all(root).unwrap();
}
