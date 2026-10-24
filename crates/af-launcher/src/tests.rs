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
