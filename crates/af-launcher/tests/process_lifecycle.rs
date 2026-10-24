#![cfg(unix)]
use std::{fs, process::Stdio, time::Duration};
use tokio::{
    process::Command,
    time::{sleep, timeout},
};

async fn wait_event(root: &std::path::Path, event: &str) {
    timeout(Duration::from_secs(10), async {
        loop {
            if fs::read_to_string(root.join("events.jsonl"))
                .unwrap_or_default()
                .lines()
                .any(|line| {
                    serde_json::from_str::<serde_json::Value>(line)
                        .is_ok_and(|value| value["event"] == event)
                })
            {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

async fn lifecycle(force: bool) {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("af-launcher-lifecycle-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("preserved.txt"), "persistent data").unwrap();
    let source = include_str!("fixtures/program.py")
        .replace("__VERSION__", "\"1.0.0\"")
        .replace(
            "__ROOT__",
            &serde_json::to_string(root.to_str().unwrap()).unwrap(),
        )
        .replace("__UPDATE__", "False");
    let program = root.join("seed");
    fs::write(&program, source).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let mut launcher = Command::new(env!("CARGO_BIN_EXE_anyflows-launcher"))
        .arg("--program")
        .arg(program)
        .arg("--state-dir")
        .arg(root.join("state"))
        .arg("--stop-timeout-secs")
        .arg("3")
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    wait_event(&root, "ready").await;
    if force {
        launcher.kill().await.unwrap();
        wait_event(&root, "eof").await;
    } else {
        assert!(
            Command::new("kill")
                .arg("-TERM")
                .arg(launcher.id().unwrap().to_string())
                .status()
                .await
                .unwrap()
                .success()
        );
        assert!(
            timeout(Duration::from_secs(5), launcher.wait())
                .await
                .unwrap()
                .unwrap()
                .success()
        );
        wait_event(&root, "shutdown").await;
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("state/main.lock"))
        .unwrap();
    timeout(Duration::from_secs(3), async {
        loop {
            if fs2::FileExt::try_lock_exclusive(&lock).is_ok() {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    drop(lock);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn supervisor_stop_drains_and_reaps_its_child() {
    lifecycle(false).await;
}

#[tokio::test]
async fn supervisor_crash_closes_lifeline_and_does_not_leave_an_orphan() {
    lifecycle(true).await;
}

async fn environment_inheritance(explicit: bool) {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("af-launcher-env-{}", uuid::Uuid::new_v4()));
    let working = root.join("working directory");
    fs::create_dir_all(&working).unwrap();
    fs::write(working.join("preserved.txt"), "persistent data").unwrap();
    let source = include_str!("fixtures/program.py")
        .replace("__VERSION__", "\"1.0.0\"")
        .replace(
            "__ROOT__",
            &serde_json::to_string(working.to_str().unwrap()).unwrap(),
        )
        .replace("__UPDATE__", "False");
    let program = root.join("seed");
    fs::write(&program, source).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let env_path = if explicit {
        root.join("custom.env")
    } else {
        working.join(".env")
    };
    fs::write(
        env_path,
        "ANYFLOWS_TEST_PANEL_INJECTED_VALUE=dotenv-ignored\n\
         ANYFLOWS_TEST_DOTENV_DEFAULT_VALUE='dotenv default'\n\
         ANYFLOWS_LAUNCHER_ENDPOINT=invalid\nANYFLOWS_LAUNCHER_TOKEN=invalid\n",
    )
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_anyflows-launcher"));
    command
        .arg("--program")
        .arg(program)
        .arg("--state-dir")
        .arg(root.join("state"))
        .arg("--working-directory")
        .arg(&working)
        .arg("--stop-timeout-secs")
        .arg("3")
        .current_dir(&root)
        .env("ANYFLOWS_TEST_PANEL_INJECTED_VALUE", "panel injected")
        .env_remove("ANYFLOWS_TEST_DOTENV_DEFAULT_VALUE")
        .env("ANYFLOWS_LAUNCHER_TOKEN", "parent-ignored")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if explicit {
        command.args(["--env-file", "custom.env"]);
    }
    let launcher = command
        .args(["--", "--preserved-argument", "value with spaces"])
        .spawn()
        .unwrap();
    wait_event(&working, "ready").await;
    assert!(
        Command::new("kill")
            .arg("-TERM")
            .arg(launcher.id().unwrap().to_string())
            .status()
            .await
            .unwrap()
            .success()
    );
    let output = timeout(Duration::from_secs(5), launcher.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("managed stdout")
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("managed stderr")
    );
    let records: Vec<serde_json::Value> = fs::read_to_string(working.join("events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!records.is_empty());
    assert!(records.iter().all(|record| {
        record["panel_value"] == "panel injected"
            && record["dotenv_value"] == "dotenv default"
            && record["cwd"] == working.to_str().unwrap()
            && record["args"] == serde_json::json!(["--preserved-argument", "value with spaces"])
    }));
    assert!(records.iter().any(|record| record["event"] == "shutdown"));
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn default_dotenv_preserves_panel_environment_arguments_and_log_pipes() {
    environment_inheritance(false).await;
}

#[tokio::test]
async fn explicit_env_file_is_relative_to_launcher_directory() {
    environment_inheritance(true).await;
}
