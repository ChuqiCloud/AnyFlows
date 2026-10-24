use super::*;

#[tokio::test]
async fn node_mismatch_and_disabled_installs_fail_before_network_or_file_access() {
    let clients = HttpClientProvider::new(af_httpclient::HttpClientConfig::default(), 4).unwrap();
    let updates = CommunitySoftwareUpdates::new(clients, false);
    assert!(matches!(
        updates
            .install("v999.0.0".to_owned(), "another-process".to_owned())
            .await,
        Err(Error::Conflict)
    ));
    let current = updates.status().await.unwrap();
    assert!(!current.install_enabled);
    assert!(matches!(
        updates
            .install("v999.0.0".to_owned(), current.node_id)
            .await,
        Err(Error::Disabled)
    ));
}

fn release(tag: &str, prerelease: bool) -> Release {
    Release {
        tag_name: tag.to_owned(),
        draft: false,
        prerelease,
        body: None,
        published_at: None,
        assets: vec![],
    }
}

#[test]
fn release_selection_uses_semver_and_keeps_stable_installations_on_stable_releases() {
    let candidates = || {
        vec![
            release("v0.10.0", false),
            release("v0.9.0", false),
            release("v1.0.0-beta.1", true),
            release("v99.0.0+unsafe", false),
            release("../../evil", false),
        ]
    };
    assert_eq!(
        select_release(candidates(), &Version::parse("0.9.0").unwrap())
            .unwrap()
            .tag_name,
        "v0.10.0"
    );
    assert_eq!(
        select_release(candidates(), &Version::parse("0.9.0-beta.1").unwrap())
            .unwrap()
            .tag_name,
        "v1.0.0-beta.1"
    );
    let mut draft = release("v99.0.0", false);
    draft.draft = true;
    assert!(select_release(vec![draft], &Version::parse("0.9.0").unwrap()).is_none());
}

#[test]
fn release_tags_reject_path_injection_and_nonrelease_identifiers() {
    for tag in [
        "v1.0.0-rc.0",
        "v1.0.0-preview.1",
        "v1.0.0-beta.1.2",
        "v1.0.0+abc",
        "1.0.0",
        "v1.0.0/../../x",
    ] {
        assert!(release_version(tag).is_none(), "{tag}");
    }
    for tag in ["v1.0.0", "v1.0.0-alpha.1", "v1.0.0-beta.2", "v1.0.0-rc.3"] {
        assert!(release_version(tag).is_some(), "{tag}");
    }
}

#[test]
fn package_matching_distinguishes_libc_os_architecture_and_extension() {
    for (target, name) in [
        ("x86_64-unknown-linux-gnu", "linux-amd64.tar.gz"),
        ("aarch64-unknown-linux-gnu", "linux-arm64.tar.gz"),
        ("x86_64-unknown-linux-musl", "linux-amd64-musl.tar.gz"),
        ("aarch64-unknown-linux-musl", "linux-arm64-musl.tar.gz"),
        ("x86_64-pc-windows-msvc", "windows-amd64.zip"),
        ("aarch64-pc-windows-msvc", "windows-arm64.zip"),
        ("x86_64-apple-darwin", "macos-amd64.tar.gz"),
        ("aarch64-apple-darwin", "macos-arm64.tar.gz"),
    ] {
        assert_eq!(
            package_name("v1.2.3", target).unwrap(),
            format!("anyflows-v1.2.3-{name}")
        );
    }
    assert!(package_name("v1.2.3", "s390x-unknown-linux-gnu").is_none());
    assert!(
        binary_name("v1.2.3", "x86_64-pc-windows-msvc")
            .unwrap()
            .ends_with(".exe")
    );
}

#[test]
fn redirects_and_checksums_are_exactly_scoped() {
    for url in [
        "http://github.com/ChuqiCloud/AnyFlows/releases/download/v1/file",
        "https://user@github.com/ChuqiCloud/AnyFlows/releases/download/x",
        "https://github.com/another/repo/releases/download/x",
        "https://evil.example/file",
        "https://127.0.0.1/file",
        "https://api.github.com/user",
        "https://release-assets.githubusercontent.com.evil.example/x",
    ] {
        assert!(!allowed_url(url), "{url}");
    }
    assert!(allowed_url(
        "https://release-assets.githubusercontent.com/github-production-release-asset/123?a=b"
    ));
    let digest = "a".repeat(64);
    assert_eq!(
        parse_checksum(format!("{digest}  binary\n").as_bytes(), "binary"),
        Some(digest.clone())
    );
    assert!(parse_checksum(format!("{digest}  another").as_bytes(), "binary").is_none());
    assert!(
        parse_checksum(
            format!("{digest}  binary\n{digest}  evil").as_bytes(),
            "binary"
        )
        .is_none()
    );
    assert!(parse_checksum(b"short  binary", "binary").is_none());
}

#[test]
fn staging_is_exclusive_preserves_backup_and_cleans_partial_downloads() {
    let directory = env::temp_dir().join(format!("af-update-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let executable = directory.join("anyflows");
    fs::write(&executable, b"old").unwrap();
    {
        let mut staged = StagedBinary::new(&executable).unwrap();
        assert!(StagedBinary::new(&executable).is_err());
        staged.write_all(b"new").unwrap();
        staged.commit(&executable).unwrap();
    }
    assert_eq!(fs::read(&executable).unwrap(), b"new");
    let backups: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("anyflows-backup-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(backups[0].path()).unwrap(), b"old");
    {
        let staged = StagedBinary::new(&executable).unwrap();
        assert!(staged.path.exists());
    }
    assert!(!directory.join(".anyflows-update.lock").exists());
    assert_eq!(fs::read_dir(&directory).unwrap().count(), 2);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn both_binary_start_modes_enable_web_installation_by_default() {
    assert!(web_updates_enabled(None));
    assert!(web_updates_enabled(Some("true")));
    assert!(!web_updates_enabled(Some("false")));
    assert!(!web_updates_enabled(Some("FALSE")));
    assert!(!web_updates_enabled(Some("0")));
}
