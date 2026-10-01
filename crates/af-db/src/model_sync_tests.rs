use std::{error::Error, time::Duration};

use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol, Status, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use super::{
    AdminModelCreateRecord, AdminModelLifecycleRecord, AdminModelModalitiesRecord,
    AdminModelRepository, AdminModelVisibilityRecord, AdminModelWriteRecord, DatabaseOptions,
    DiscoveredModelRecord, MODEL_SYNC_PREVIEW_TTL_SECONDS, MigrationOptions,
    MissingModelImportItemRecord, ModelDiscoveryTargetLookup, ModelSyncApplyItemRecord,
    ModelSyncPreviewWrite, ModelSyncRelationRecord, ModelSyncRepository, ModelSyncRepositoryError,
};
use crate::entity::{
    ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, channel_models, channels,
    credentials, groups, model_sync_items, model_sync_runs, models, users,
};

const PREVIEW_ID: &str = "018f8e52-7e21-7f47-a540-3f1bb2785594";

#[tokio::test]
async fn missing_detection_uses_active_channel_references_and_metadata()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    attach_model(&fixture, "existing-model").await?;
    attach_model(&fixture, "missing-model").await?;
    fixture
        .models
        .create(model_create("existing-model", "Existing"))
        .await?;

    let page = fixture.sync.list_missing_models(None, 20).await?;
    assert_eq!(page.models().len(), 1);
    assert_eq!(page.models()[0].model(), "missing-model");
    assert_eq!(page.models()[0].channel_count(), 1);
    assert_eq!(
        page.models()[0].channels()[0].channel_name(),
        "sync-channel"
    );
    assert_eq!(page.next_cursor(), None);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn preview_persists_four_relations_without_promoting_upstream_hints()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for model in ["configured-existing", "configured-missing", "not-reported"] {
        attach_model(&fixture, model).await?;
    }
    fixture
        .models
        .create(model_create("configured-existing", "Existing"))
        .await?;
    let now = current_unix();
    let preview = fixture
        .sync
        .create_preview(preview_write(
            &fixture,
            now + MODEL_SYNC_PREVIEW_TTL_SECONDS,
            vec![
                discovered("configured-existing", Some("Existing Hint")),
                discovered("configured-missing", Some("Missing Hint")),
                discovered("upstream-new", Some("New Hint")),
            ],
        ))
        .await?;

    assert_eq!(preview.preview_id(), PREVIEW_ID);
    assert_eq!(preview.items().len(), 4);
    let relations = preview
        .items()
        .iter()
        .map(|item| (item.canonical_model(), item.relation()))
        .collect::<Vec<_>>();
    assert_eq!(
        relations,
        [
            ("configured-existing", ModelSyncRelationRecord::Existing),
            (
                "configured-missing",
                ModelSyncRelationRecord::MissingMetadata
            ),
            ("not-reported", ModelSyncRelationRecord::NotReported),
            (
                "upstream-new",
                ModelSyncRelationRecord::DiscoveredUnconfigured
            ),
        ]
    );
    let missing = preview
        .items()
        .iter()
        .find(|item| item.canonical_model() == "configured-missing")
        .expect("缺失候选必须存在");
    assert_eq!(missing.display_name_hint(), Some("Missing Hint"));
    assert_eq!(missing.context_window_hint(), Some(128_000));
    assert_eq!(
        missing.supported_methods(),
        ["countTokens", "generateContent"]
    );
    assert_eq!(
        models::Entity::find()
            .filter(models::Column::Model.eq("configured-missing"))
            .count(fixture.pool.connection())
            .await?,
        0,
        "预览不得把上游提示直接写成权威元数据"
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn apply_creates_hidden_drafts_atomically_and_rejects_replay() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    attach_model(&fixture, "configured-missing").await?;
    let now = current_unix();
    let preview = fixture
        .sync
        .create_preview(preview_write(
            &fixture,
            now + MODEL_SYNC_PREVIEW_TTL_SECONDS,
            vec![
                discovered("configured-missing", None),
                discovered("upstream-new", None),
            ],
        ))
        .await?;
    let applicable = preview
        .items()
        .iter()
        .filter(|item| item.relation().is_applicable())
        .map(|item| {
            ModelSyncApplyItemRecord::new(
                item.item_id(),
                draft_fields(&format!("{} Draft", item.canonical_model())),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let created = fixture
        .sync
        .apply_preview(PREVIEW_ID, fixture.admin_user_id, now, applicable)
        .await?;
    assert_eq!(created.len(), 2);
    assert!(created.iter().all(|model| {
        model.visibility() == AdminModelVisibilityRecord::Hidden
            && model.lifecycle() == AdminModelLifecycleRecord::Draft
    }));
    assert_eq!(
        channel_models::Entity::find()
            .filter(channel_models::Column::ChannelId.eq(fixture.channel_id.get()))
            .count(fixture.pool.connection())
            .await?,
        2,
        "应用同步结果必须同时加入来源渠道路由"
    );
    for model in ["configured-missing", "upstream-new"] {
        assert!(
            channel_models::Entity::find()
                .filter(channel_models::Column::ChannelId.eq(fixture.channel_id.get()))
                .filter(channel_models::Column::Model.eq(model))
                .one(fixture.pool.connection())
                .await?
                .is_some(),
            "同步后的渠道路由缺少 {model}"
        );
    }
    let run = model_sync_runs::Entity::find()
        .filter(model_sync_runs::Column::PreviewId.eq(PREVIEW_ID))
        .one(fixture.pool.connection())
        .await?
        .expect("同步运行必须存在");
    assert_eq!(run.state, 2);
    assert!(run.applied_at.is_some());
    assert_eq!(
        model_sync_items::Entity::find()
            .filter(model_sync_items::Column::RunId.eq(run.id))
            .filter(model_sync_items::Column::AppliedModelId.is_not_null())
            .count(fixture.pool.connection())
            .await?,
        2
    );
    assert_eq!(
        fixture
            .sync
            .apply_preview(
                PREVIEW_ID,
                fixture.admin_user_id,
                now,
                vec![ModelSyncApplyItemRecord::new(
                    preview.items()[0].item_id(),
                    draft_fields("Replay"),
                )?],
            )
            .await
            .unwrap_err(),
        ModelSyncRepositoryError::AlreadyApplied
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_model_conflict_rolls_back_every_selected_item() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let now = current_unix();
    let preview = fixture
        .sync
        .create_preview(preview_write(
            &fixture,
            now + MODEL_SYNC_PREVIEW_TTL_SECONDS,
            vec![
                discovered("conflict-model", None),
                discovered("rollback-model", None),
            ],
        ))
        .await?;
    fixture
        .models
        .create(model_create("conflict-model", "Concurrent"))
        .await?;
    let selected = preview
        .items()
        .iter()
        .map(|item| {
            ModelSyncApplyItemRecord::new(item.item_id(), draft_fields(item.canonical_model()))
        })
        .collect::<Result<Vec<_>, _>>()?;

    assert_eq!(
        fixture
            .sync
            .apply_preview(PREVIEW_ID, fixture.admin_user_id, now, selected)
            .await
            .unwrap_err(),
        ModelSyncRepositoryError::Conflict
    );
    assert_eq!(
        models::Entity::find()
            .filter(models::Column::Model.eq("rollback-model"))
            .count(fixture.pool.connection())
            .await?,
        0
    );
    let run = model_sync_runs::Entity::find()
        .filter(model_sync_runs::Column::PreviewId.eq(PREVIEW_ID))
        .one(fixture.pool.connection())
        .await?
        .expect("冲突后预览仍应存在");
    assert_eq!(run.state, 1);
    assert!(run.applied_at.is_none());

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_import_creates_hidden_drafts_atomically() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for model in ["missing-a", "missing-b"] {
        attach_model(&fixture, model).await?;
    }

    let created = fixture
        .sync
        .import_missing_models(vec![
            missing_import("missing-a", "Missing A")?,
            missing_import("missing-b", "Missing B")?,
        ])
        .await?;

    assert_eq!(created.len(), 2);
    assert_eq!(
        created
            .iter()
            .map(|model| model.model())
            .collect::<Vec<_>>(),
        ["missing-a", "missing-b"]
    );
    assert!(created.iter().all(|model| {
        model.visibility() == AdminModelVisibilityRecord::Hidden
            && model.lifecycle() == AdminModelLifecycleRecord::Draft
    }));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_import_conflicts_roll_back_the_whole_batch() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for model in ["existing-model", "rollback-model"] {
        attach_model(&fixture, model).await?;
    }
    fixture
        .models
        .create(model_create("existing-model", "Existing"))
        .await?;

    assert_eq!(
        fixture
            .sync
            .import_missing_models(vec![
                missing_import("rollback-model", "Rollback")?,
                missing_import("existing-model", "Existing")?,
            ])
            .await
            .unwrap_err(),
        ModelSyncRepositoryError::Conflict
    );
    assert_eq!(
        models::Entity::find()
            .filter(models::Column::Model.eq("rollback-model"))
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_import_rejects_stale_references_and_duplicates() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    attach_model(&fixture, "still-missing").await?;

    assert_eq!(
        fixture
            .sync
            .import_missing_models(vec![
                missing_import("still-missing", "Still Missing")?,
                missing_import("not-referenced", "Not Referenced")?,
            ])
            .await
            .unwrap_err(),
        ModelSyncRepositoryError::Conflict
    );
    assert_eq!(
        models::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );

    assert_eq!(
        fixture
            .sync
            .import_missing_models(vec![
                missing_import("still-missing", "First")?,
                missing_import("still-missing", "Duplicate")?,
            ])
            .await
            .unwrap_err(),
        ModelSyncRepositoryError::Invariant
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_import_allows_replacing_soft_deleted_metadata() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    attach_model(&fixture, "deleted-model").await?;
    let existing = fixture
        .models
        .create(model_create("deleted-model", "Deleted"))
        .await?;
    let deleted_at = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    models::Entity::update_many()
        .filter(models::Column::Id.eq(existing.model_id().get()))
        .col_expr(
            models::Column::DeletedAt,
            sea_orm::sea_query::Expr::value(deleted_at),
        )
        .exec(fixture.pool.connection())
        .await?;

    let created = fixture
        .sync
        .import_missing_models(vec![missing_import("deleted-model", "Replacement")?])
        .await?;
    assert_eq!(created.len(), 1);
    assert_ne!(created[0].model_id(), existing.model_id());
    assert_eq!(
        models::Entity::find()
            .filter(models::Column::Model.eq("deleted-model"))
            .count(fixture.pool.connection())
            .await?,
        2
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn discovery_target_excludes_pending_oauth_credentials() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let pending = credentials::ActiveModel {
        channel_id: Set(fixture.channel_id.get()),
        kind: Set(CredentialKind::Oauth.as_str().to_owned()),
        secret: Set(EncryptedJson::from_envelope(serde_json::json!({
            "version": 1,
            "algorithm": "xchacha20poly1305",
            "key_id": "oauth-token-pending",
            "nonce": URL_SAFE_NO_PAD.encode([0_u8; 24]),
            "ciphertext": URL_SAFE_NO_PAD.encode([0_u8; 16]),
        }))?),
        status: Set(Status::Enabled.code()),
        schedulable: Set(true),
        oauth_provider: Set(Some("codex".to_owned())),
        oauth_token_pending: Set(true),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;
    assert!(matches!(
        fixture
            .sync
            .load_discovery_target(fixture.channel_id)
            .await?,
        ModelDiscoveryTargetLookup::Unavailable
    ));

    credentials::ActiveModel {
        id: Set(pending.id),
        oauth_token_pending: Set(false),
        ..Default::default()
    }
    .update(fixture.pool.connection())
    .await?;
    assert!(matches!(
        fixture
            .sync
            .load_discovery_target(fixture.channel_id)
            .await?,
        ModelDiscoveryTargetLookup::Found(_)
    ));

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    sync: ModelSyncRepository,
    models: AdminModelRepository,
    channel_id: ChannelId,
    admin_user_id: UserId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    groups::ActiveModel {
        id: Set(1),
        name: Set("sync-group".to_owned()),
        display_name: Set("同步测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    users::ActiveModel {
        id: Set(1),
        username: Set("sync-admin".to_owned()),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(1),
        quota: Set(1_000),
        aff_code: Set("sync-admin-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let channel = channels::ActiveModel {
        name: Set("sync-channel".to_owned()),
        r#type: Set(ChannelType::OpenAi.as_str().to_owned()),
        protocol: Set(Protocol::OpenAiChat.as_str().to_owned()),
        base_url: Set(Some(ChannelBaseUrl::parse("https://api.example.com/v1")?)),
        status: Set(1),
        model_mapping: Set(serde_json::json!({})),
        param_override: Set(serde_json::json!({})),
        header_override: Set(HeaderOverrides::validate(serde_json::json!({}))?),
        settings: Set(SensitiveJson::from(serde_json::json!({}))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        sync: ModelSyncRepository::with_timeout(pool.clone(), Duration::from_secs(2))?,
        models: AdminModelRepository::new(pool.clone(), Duration::from_secs(2))?,
        channel_id: ChannelId::new(channel.id)?,
        admin_user_id: UserId::new(1)?,
        pool,
    })
}

async fn attach_model(fixture: &Fixture, model: &str) -> Result<(), sea_orm::DbErr> {
    channel_models::ActiveModel {
        channel_id: Set(fixture.channel_id.get()),
        model: Set(model.to_owned()),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;
    Ok(())
}

fn preview_write(
    fixture: &Fixture,
    expires_at: i64,
    discovered: Vec<DiscoveredModelRecord>,
) -> ModelSyncPreviewWrite {
    ModelSyncPreviewWrite::new(
        PREVIEW_ID.to_owned(),
        fixture.admin_user_id,
        fixture.channel_id,
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        expires_at,
        discovered,
    )
    .expect("固定预览写入必须有效")
}

fn discovered(model: &str, display_name: Option<&str>) -> DiscoveredModelRecord {
    DiscoveredModelRecord::new(
        model.to_owned(),
        model.to_owned(),
        display_name.map(str::to_owned),
        Some("上游证据".to_owned()),
        Some(128_000),
        Some(128_000),
        Some(8_192),
        vec!["countTokens".to_owned(), "generateContent".to_owned()],
    )
    .expect("固定发现记录必须有效")
}

fn model_create(model: &str, display_name: &str) -> AdminModelCreateRecord {
    AdminModelCreateRecord::new(
        model.to_owned(),
        AdminModelWriteRecord::new(
            display_name.to_owned(),
            "provider".to_owned(),
            None,
            None,
            Vec::new(),
            None,
            AdminModelModalitiesRecord::new(true, false, false, false),
            AdminModelModalitiesRecord::new(true, false, false, false),
            false,
            false,
            AdminModelVisibilityRecord::Public,
            AdminModelLifecycleRecord::Active,
        ),
    )
}

fn draft_fields(display_name: &str) -> AdminModelWriteRecord {
    AdminModelWriteRecord::new(
        display_name.to_owned(),
        "provider".to_owned(),
        None,
        None,
        Vec::new(),
        None,
        AdminModelModalitiesRecord::new(true, false, false, false),
        AdminModelModalitiesRecord::new(true, false, false, false),
        false,
        false,
        AdminModelVisibilityRecord::Hidden,
        AdminModelLifecycleRecord::Draft,
    )
}

fn missing_import(
    model: &str,
    display_name: &str,
) -> Result<MissingModelImportItemRecord, ModelSyncRepositoryError> {
    MissingModelImportItemRecord::new(model.to_owned(), draft_fields(display_name))
}

fn current_unix() -> i64 {
    sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc().unix_timestamp()
}
