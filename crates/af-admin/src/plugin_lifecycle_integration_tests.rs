//! End-to-end coverage of the shared archive, real sidecar and user catalog path.
#![cfg(unix)]

use std::{error::Error, io::Write, path::Path, sync::Arc, time::Duration};

use af_db::{
    AdminUserCreateRecord, AdminUserLookupOutcome, AdminUserRepository, DatabaseOptions,
    DatabasePool, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MigrationOptions, PlatformAuditRepository, PluginCatalogRepository,
};
use serde_json::json;
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{
    AdminPluginLifecycleError, AdminPluginSidecarRuntime, DatabaseAdminPluginLifecycleService,
    DatabasePlatformAuditService, PluginCatalogExtensionQuery, SessionPrincipal, SessionRole,
};

const PLUGIN_ID: &str = "org.anyflows.example-catalog";

struct Fixture {
    pool: DatabasePool,
    root: tempfile::TempDir,
    first: DatabaseAdminPluginLifecycleService,
    second: DatabaseAdminPluginLifecycleService,
    operator: SessionPrincipal,
    first_reader: SessionPrincipal,
    second_reader: SessionPrincipal,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let working_directory = std::env::current_dir()?.canonicalize()?;
        // Every test has independent roots without changing process-global cwd.
        let root = tempfile::Builder::new()
            .prefix(".plugin-cluster-e2e-")
            .tempdir_in(&working_directory)?;
        let first_root = root.path().join("first");
        let second_root = root.path().join("second");
        let first_relative = first_root.strip_prefix(&working_directory)?;
        let second_relative = second_root.strip_prefix(&working_directory)?;
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let setup = InitialSetupRepository::new(pool.clone(), Duration::from_secs(10))?;
        let InitialSetupOutcome::Initialized { user_id } = setup
            .initialize(InitialSetupRecord::new(
                "plugin-integration-admin".to_owned(),
                "integration-fixture-password".to_owned(),
            ))
            .await?
        else {
            return Err("fresh integration database was already initialized".into());
        };
        let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(10))?;
        let AdminUserLookupOutcome::Found(operator) = users.get(user_id).await? else {
            return Err("initial administrator is missing".into());
        };
        let mut principals = vec![SessionPrincipal::new(user_id, SessionRole::Admin)];
        for index in 1..3 {
            let user = users
                .create(AdminUserCreateRecord::new(
                    format!("plugin-integration-{index}"),
                    None,
                    None,
                    SessionRole::User.to_database(),
                    1,
                    operator.default_group_id(),
                    0,
                    None,
                    None,
                ))
                .await?;
            principals.push(SessionPrincipal::new(user.user_id(), SessionRole::User));
        }
        let first = service(&pool, first_relative)?;
        let second = service(&pool, second_relative)?;
        Ok(Self {
            pool,
            root,
            first,
            second,
            operator: principals[0],
            first_reader: principals[1],
            second_reader: principals[2],
        })
    }

    async fn close(self) -> Result<(), Box<dyn Error>> {
        self.first.shutdown_runtime().await;
        self.second.shutdown_runtime().await;
        self.pool.close().await?;
        self.root.close()?;
        Ok(())
    }
}

fn service(
    pool: &DatabasePool,
    artifact_root: &Path,
) -> Result<DatabaseAdminPluginLifecycleService, Box<dyn Error>> {
    let runtime = Arc::new(AdminPluginSidecarRuntime::new(artifact_root, &[])?);
    let audit = Arc::new(DatabasePlatformAuditService::new(
        PlatformAuditRepository::new(pool.clone()),
    ));
    Ok(
        DatabaseAdminPluginLifecycleService::new(PluginCatalogRepository::new(pool.clone()), audit)
            .with_sidecar_runtime(Some(runtime)),
    )
}

fn example_package() -> Result<Vec<u8>, Box<dyn Error>> {
    example_package_with_entry(include_bytes!(
        "../../../tools/plugins/examples/catalog/plugin.py"
    ))
}

fn example_package_with_entry(entry: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tools/plugins/examples/catalog/manifest.json"
    ))?;
    let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let file = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .unix_permissions(0o644);
    writer.start_file("anyflows-plugin.json", file)?;
    writer.write_all(&serde_json::to_vec(&json!({
        "manifest": manifest,
        "executablePath": "plugin.py"
    }))?)?;
    writer.add_directory("payload/", file.unix_permissions(0o755))?;
    writer.start_file("payload/plugin.py", file.unix_permissions(0o755))?;
    writer.write_all(entry)?;
    writer.start_file("payload/manifest.json", file)?;
    writer.write_all(include_bytes!(
        "../../../tools/plugins/examples/catalog/manifest.json"
    ))?;
    writer.start_file("payload/models.json", file)?;
    writer.write_all(include_bytes!(
        "../../../tools/plugins/examples/catalog/models.json"
    ))?;
    writer.start_file("payload/anyflows_plugin.py", file)?;
    writer.write_all(include_bytes!(
        "../../../tools/plugins/sdk/python/anyflows_plugin.py"
    ))?;
    Ok(writer.finish()?.into_inner())
}

#[tokio::test]
async fn package_registration_waits_for_cache_collection_before_importing()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let runtime = fixture
        .first
        .capability_runtime()
        .ok_or("runtime is missing")?;
    let mut upload = fixture
        .first
        .begin_package_upload(fixture.operator, "cache-ownership-upload")
        .await?;
    upload.write_chunk(&example_package()?).await?;
    {
        // Cache collection and recovery own this lock while changing a replica.
        // An install must not publish a temporary directory that another import
        // could retain before the original owner's failure removes it.
        let collection = runtime.cache_operations.lock().await;
        let installation = fixture.first.install_package(
            fixture.operator,
            upload,
            json!({ "unsupportedSetting": true }),
            None,
            "cache-ownership-install",
        );
        tokio::pin!(installation);
        assert!(
            tokio::time::timeout(Duration::from_millis(250), &mut installation)
                .await
                .is_err(),
            "installation must wait until cache collection releases the replica"
        );
        assert!(
            !fixture.root.path().join("first/packages").exists(),
            "a blocked install must not expose a temporary package"
        );
        drop(collection);
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(10), &mut installation).await?,
            Err(AdminPluginLifecycleError::InvalidInput)
        ));
    }
    assert_eq!(
        std::fs::read_dir(fixture.root.path().join("first/packages"))?.count(),
        0,
        "rejected configuration must release its unregistered cache directory"
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_apply_of_the_same_generation_keeps_the_winning_session_healthy()
-> Result<(), Box<dyn Error>> {
    if !Path::new("/usr/bin/python3").is_file() {
        eprintln!("Skipping real Python catalog fixture: /usr/bin/python3 is unavailable");
        return Ok(());
    }
    let fixture = Fixture::new().await?;
    let repository = PluginCatalogRepository::new(fixture.pool.clone());
    let mut upload = fixture
        .first
        .begin_package_upload(fixture.operator, "concurrent-upload")
        .await?;
    upload.write_chunk(&example_package()?).await?;
    let installed = fixture
        .first
        .install_package(
            fixture.operator,
            upload,
            json!({}),
            None,
            "concurrent-install",
        )
        .await?;
    let deployment = repository
        .get_deployment(PLUGIN_ID)
        .await?
        .ok_or("deployment is missing")?;
    let runtime = fixture
        .second
        .capability_runtime()
        .ok_or("runtime is missing")?;

    // HTTP activation, reconciliation and a lazy capability request can all
    // observe a missing session before any of them finishes the handshake.
    let (first, second, third) = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::join!(
            runtime.apply(&repository, &deployment),
            runtime.apply(&repository, &deployment),
            runtime.apply(&repository, &deployment),
        )
    })
    .await?;
    first?;
    second?;
    third?;

    let overview = fixture.first.runtime_overview(fixture.operator).await?;
    let state = overview
        .states
        .iter()
        .find(|state| state.node_id == runtime.node_id() && state.plugin_id == PLUGIN_ID)
        .ok_or("second node did not report its runtime state")?;
    assert_eq!(state.generation, installed.generation());
    assert_eq!(state.digest, installed.artifact().digest());
    assert_eq!(state.status, "ready");
    let catalog = fixture
        .second
        .catalog_models(
            fixture.second_reader,
            PLUGIN_ID,
            PluginCatalogExtensionQuery::new(None, None, 24)?,
            "concurrent-catalog",
        )
        .await?;
    assert_eq!(catalog.models.len(), 2);
    let current = repository
        .get_deployment(PLUGIN_ID)
        .await?
        .ok_or("deployment is missing")?;
    assert!(current.enabled);
    assert_eq!(current.generation, installed.generation());
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn shared_package_recovers_after_a_transient_start_failure_without_a_new_generation()
-> Result<(), Box<dyn Error>> {
    if !Path::new("/usr/bin/python3").is_file() {
        eprintln!("Skipping real Python catalog fixture: /usr/bin/python3 is unavailable");
        return Ok(());
    }
    let fixture = Fixture::new().await?;
    let repository = PluginCatalogRepository::new(fixture.pool.clone());
    let marker = fixture.root.path().join("failed-first-start");
    // The package itself is immutable. Only this fixture-owned marker outside
    // its tree changes, causing exactly the first real sidecar process to exit.
    let entry = format!(
        concat!(
            "#!/usr/bin/python3\n",
            "from pathlib import Path\n",
            "marker = Path({})\n",
            "if not marker.exists():\n",
            "    marker.write_text('failed-once', encoding='utf-8')\n",
            "    raise SystemExit(1)\n",
            "exec(compile({}, __file__, 'exec'))\n",
        ),
        serde_json::to_string(marker.to_str().ok_or("marker path is not UTF-8")?)?,
        serde_json::to_string(include_str!(
            "../../../tools/plugins/examples/catalog/plugin.py"
        ))?,
    );
    let mut upload = fixture
        .first
        .begin_package_upload(fixture.operator, "recovery-upload")
        .await?;
    upload
        .write_chunk(&example_package_with_entry(entry.as_bytes())?)
        .await?;
    let failed = fixture
        .first
        .install_package(
            fixture.operator,
            upload,
            json!({}),
            None,
            "recovery-install",
        )
        .await;
    assert!(failed.is_err(), "the first sidecar handshake must fail");
    assert!(marker.is_file(), "the sidecar ran the failing first launch");
    let desired = repository
        .get_deployment(PLUGIN_ID)
        .await?
        .ok_or("failed activation lost the desired deployment")?;
    assert!(desired.enabled, "one node failure preserves shared intent");
    assert!(
        repository
            .distribution_repository()
            .load_package(desired.artifact.id)
            .await?
            .is_some()
    );
    let runtime = fixture
        .first
        .capability_runtime()
        .ok_or("runtime is missing")?;
    let failed_overview = fixture.first.runtime_overview(fixture.operator).await?;
    assert!(failed_overview.states.iter().any(|state| {
        state.node_id == runtime.node_id()
            && state.plugin_id == PLUGIN_ID
            && state.generation == desired.generation
            && state.status == "failed"
    }));

    // Reuse the same runtime and supervisor, so a poisoned stopped-through
    // generation cannot be hidden by constructing a fresh service.
    tokio::time::timeout(Duration::from_secs(30), fixture.first.reconcile_runtime()).await?;
    let recovered = repository
        .get_deployment(PLUGIN_ID)
        .await?
        .ok_or("deployment is missing after reconciliation")?;
    assert!(recovered.enabled);
    assert_eq!(recovered.generation, desired.generation);
    let overview = fixture.first.runtime_overview(fixture.operator).await?;
    assert!(overview.states.iter().any(|state| {
        state.node_id == runtime.node_id()
            && state.plugin_id == PLUGIN_ID
            && state.generation == desired.generation
            && state.digest == desired.artifact.digest
            && state.status == "ready"
    }));
    let catalog = fixture
        .first
        .catalog_models(
            fixture.first_reader,
            PLUGIN_ID,
            PluginCatalogExtensionQuery::new(None, None, 24)?,
            "recovery-catalog",
        )
        .await?;
    assert_eq!(catalog.models.len(), 2);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn installed_catalog_distributes_to_another_node_and_obeys_disable_revoke_and_uninstall()
-> Result<(), Box<dyn Error>> {
    if !Path::new("/usr/bin/python3").is_file() {
        eprintln!("Skipping real Python catalog fixture: /usr/bin/python3 is unavailable");
        return Ok(());
    }
    let fixture = Fixture::new().await?;
    let repository = PluginCatalogRepository::new(fixture.pool.clone());
    let mut upload = fixture
        .first
        .begin_package_upload(fixture.operator, "cluster-upload-1")
        .await?;
    upload.write_chunk(&example_package()?).await?;
    let installed = fixture
        .first
        .install_package(
            fixture.operator,
            upload,
            json!({}),
            None,
            "cluster-install-1",
        )
        .await?;
    assert_eq!(installed.plugin_id(), PLUGIN_ID);
    assert!(
        !installed.artifact().is_official(),
        "an unsigned example never gains official status"
    );
    let artifact_id = installed.artifact().id();
    let artifact = repository
        .get_artifact(artifact_id)
        .await?
        .ok_or("artifact is missing")?;
    let source: serde_json::Value = serde_json::from_str(&artifact.source_reference)?;
    let relative_package = source["packagePath"]
        .as_str()
        .ok_or("package locator is missing")?;
    let first_package = fixture.root.path().join("first").join(relative_package);
    let second_package = fixture.root.path().join("second").join(relative_package);
    assert!(first_package.is_dir());
    assert!(
        !second_package.exists(),
        "the second node starts without a copied package"
    );
    assert!(
        repository
            .distribution_repository()
            .load_package(artifact_id)
            .await?
            .is_some()
    );

    let sources = fixture
        .first
        .catalog_sources(fixture.first_reader, None)
        .await?;
    assert_eq!(sources.sources.len(), 1);
    assert_eq!(sources.sources[0].plugin_id, PLUGIN_ID);
    let first_page = fixture
        .first
        .catalog_models(
            fixture.first_reader,
            PLUGIN_ID,
            PluginCatalogExtensionQuery::new(None, None, 1)?,
            "cluster-catalog-first",
        )
        .await?;
    assert_eq!(first_page.models.len(), 1);
    assert_eq!(first_page.models[0].id, "example-text-small");
    assert!(first_page.next_cursor.is_some());

    fixture.second.reconcile_runtime().await;
    assert!(
        second_package.is_dir(),
        "the second node materializes the verified shared archive"
    );
    let second_page = fixture
        .second
        .catalog_models(
            fixture.second_reader,
            PLUGIN_ID,
            PluginCatalogExtensionQuery::new(None, first_page.next_cursor.clone(), 1)?,
            "cluster-catalog-second",
        )
        .await?;
    assert_eq!(second_page.models.len(), 1);
    assert_eq!(second_page.models[0].id, "example-vision");
    let overview = fixture.first.runtime_overview(fixture.operator).await?;
    assert_eq!(overview.nodes.iter().filter(|node| node.live).count(), 2);
    assert_eq!(
        overview
            .states
            .iter()
            .filter(|state| state.plugin_id == PLUGIN_ID
                && state.generation == installed.generation()
                && state.status == "ready")
            .count(),
        2
    );

    // A damaged node-local cache must be restored from the immutable archive,
    // rather than leaving an otherwise enabled deployment permanently failed.
    let second_runtime = fixture
        .second
        .capability_runtime()
        .ok_or("runtime is missing")?;
    second_runtime
        .stop_generation(PLUGIN_ID, installed.generation())
        .await?;
    let damaged = second_package.join("models.json");
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&damaged, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::write(&damaged, b"damaged replica")?;
    fixture.second.reconcile_runtime().await;
    assert_eq!(
        std::fs::read(&damaged)?,
        include_bytes!("../../../tools/plugins/examples/catalog/models.json")
    );
    assert!(
        repository
            .get_deployment(PLUGIN_ID)
            .await?
            .is_some_and(|deployment| deployment.enabled)
    );

    let disabled = fixture
        .first
        .disable(
            fixture.operator,
            PLUGIN_ID,
            installed.generation(),
            "cluster-disable-1",
        )
        .await?;
    fixture.second.reconcile_runtime().await;
    assert!(matches!(
        fixture
            .second
            .catalog_models(
                fixture.second_reader,
                PLUGIN_ID,
                PluginCatalogExtensionQuery::new(None, None, 24)?,
                "cluster-disabled-read",
            )
            .await,
        Err(AdminPluginLifecycleError::Forbidden)
    ));
    assert!(
        fixture
            .first
            .catalog_sources(fixture.first_reader, None)
            .await?
            .sources
            .is_empty()
    );

    let restored = fixture
        .first
        .rollback(
            fixture.operator,
            PLUGIN_ID,
            disabled.generation(),
            installed.generation(),
            "cluster-rollback-1",
        )
        .await?;
    assert!(restored.enabled());
    fixture.second.reconcile_runtime().await;
    let search = fixture
        .first
        .catalog_models(
            fixture.operator,
            PLUGIN_ID,
            PluginCatalogExtensionQuery::new(Some("vision".to_owned()), None, 24)?,
            "cluster-search-after-rollback",
        )
        .await?;
    assert_eq!(search.models.len(), 1);
    assert_eq!(search.models[0].id, "example-vision");
    fixture
        .first
        .revoke_artifact(
            fixture.operator,
            artifact_id,
            "integration revocation".to_owned(),
            "cluster-revoke-1",
        )
        .await?;
    assert!(matches!(
        fixture
            .first
            .catalog_models(
                fixture.first_reader,
                PLUGIN_ID,
                PluginCatalogExtensionQuery::new(None, None, 24)?,
                "cluster-revoked-read",
            )
            .await,
        Err(AdminPluginLifecycleError::Forbidden)
    ));
    let current = repository
        .get_deployment(PLUGIN_ID)
        .await?
        .ok_or("deployment is missing")?;
    let removed = fixture
        .first
        .uninstall(
            fixture.operator,
            PLUGIN_ID,
            current.generation,
            "cluster-uninstall-1",
        )
        .await?;
    fixture.second.reconcile_runtime().await;
    assert!(!first_package.exists());
    assert!(!second_package.exists());
    let overview = fixture.first.runtime_overview(fixture.operator).await?;
    assert_eq!(
        overview
            .states
            .iter()
            .filter(|state| state.plugin_id == PLUGIN_ID
                && state.generation == removed.generation
                && state.status == "stopped")
            .count(),
        2
    );

    let cleanup = fixture
        .first
        .cleanup_artifact(fixture.operator, artifact_id, "cluster-cleanup-1")
        .await?;
    assert!(cleanup.archive_removed);
    assert!(
        repository
            .distribution_repository()
            .load_package(artifact_id)
            .await?
            .is_none()
    );
    assert!(
        repository.get_artifact(artifact_id).await?.is_some(),
        "cleanup preserves audit identity"
    );
    fixture.first.reconcile_runtime().await;
    let mut reupload = fixture
        .first
        .begin_package_upload(fixture.operator, "cluster-revoked-reupload")
        .await?;
    reupload.write_chunk(&example_package()?).await?;
    assert!(matches!(
        fixture
            .first
            .install_package(
                fixture.operator,
                reupload,
                json!({}),
                Some(removed.generation),
                "cluster-revoked-reinstall",
            )
            .await,
        Err(AdminPluginLifecycleError::Conflict)
    ));
    assert!(
        !first_package.exists(),
        "reuploading a revoked artifact cannot resurrect its collected cache"
    );
    assert!(
        repository
            .distribution_repository()
            .load_package(artifact_id)
            .await?
            .is_none(),
        "a rejected reupload preserves the archive cleanup tombstone"
    );
    assert!(
        repository.deployment_history(PLUGIN_ID, 100).await?.len() >= 3,
        "uninstall preserves immutable history"
    );
    fixture.close().await?;
    Ok(())
}
