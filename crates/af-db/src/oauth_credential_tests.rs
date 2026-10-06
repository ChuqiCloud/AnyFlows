use std::{error::Error, time::Duration};

use af_domain::{
    ChannelId, ChannelType, CredentialId, CredentialKind, CredentialQuotaDimension, Protocol,
    Status,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, PaginatorTrait, Set};

use super::{
    AdminChannelRepository, AdminChannelWriteRecord, AdminCredentialCreateOutcome,
    AdminCredentialDeleteOutcome, AdminCredentialWriteRecord, DatabaseOptions,
    MAX_OAUTH_REFRESH_CANDIDATES, MigrationOptions,
    OAuthCredentialExpirationProjectionUpdateOutcome, OAuthCredentialIdentityPatch,
    OAuthCredentialRefreshFailureKind, OAuthCredentialRefreshFailureUpdateOutcome,
    OAuthCredentialRefreshUpdateOutcome, OAuthCredentialRepository, OAuthCredentialRepositoryError,
    OAuthCredentialTokenUpdateOutcome,
    entity::{EncryptedJson, channels, credentials, scheduler_outbox_events},
};
use crate::EncryptedCredentialEnvelope;

#[tokio::test]
async fn oauth_token_update_preserves_scheduler_and_runtime_fields() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture, "primary").await?;
    let credential_id =
        create_credential(&fixture, channel_id, CredentialKind::Oauth, None, 0x11).await?;

    let mut active = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    active.status = Set(Status::AutoDisabled.code());
    active.rate_limited_at = Set(Some(now));
    active.rate_limit_reset_at = Set(Some(now + Duration::from_secs(60)));
    active.overload_until = Set(Some(now + Duration::from_secs(120)));
    active.temp_unschedulable_until = Set(Some(now + Duration::from_secs(180)));
    active.temp_unschedulable_reason = Set(Some("测试冷却".to_owned()));
    active.session_window_start = Set(Some(now));
    active.session_window_end = Set(Some(now + Duration::from_secs(3_600)));
    active.last_used_at = Set(Some(now));
    active.oauth_token_pending = Set(true);
    active.update(fixture.pool.connection()).await?;
    let before = credential_model(&fixture, credential_id).await?;
    let outbox_before = scheduler_outbox_events::Entity::find()
        .count(fixture.pool.connection())
        .await?;

    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                channel_id,
                credential_id,
                "codex",
                Some(1_800_000_000),
                envelope(0x22),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::Updated
    );
    let after = credential_model(&fixture, credential_id).await?;
    assert_ne!(after.secret, before.secret);
    assert_eq!(after.oauth_provider.as_deref(), Some("codex"));
    assert!(!after.oauth_token_pending);
    assert_eq!(after.oauth_expires_at_epoch_seconds, Some(1_800_000_000));
    assert_eq!(before.oauth_revision, 0);
    assert_eq!(after.oauth_revision, 1);
    let (key_id, nonce, ciphertext) = after.secret.envelope_parts()?;
    assert_eq!(key_id, "oauth-write-test-key");
    assert_eq!(nonce, [0x22; 24]);
    assert_eq!(ciphertext, vec![0x22; 32]);
    assert_eq!(
        scheduler_outbox_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        outbox_before + 1
    );

    // 除密文、provider、OAuth 版本与审计更新时间外，回调不得重写业务或运行字段。
    let mut normalized = after;
    normalized.secret = before.secret.clone();
    normalized.oauth_provider = before.oauth_provider.clone();
    normalized.oauth_token_pending = before.oauth_token_pending;
    normalized.oauth_revision = before.oauth_revision;
    normalized.oauth_expires_at_epoch_seconds = before.oauth_expires_at_epoch_seconds;
    normalized.updated_at = before.updated_at;
    assert_eq!(normalized, before);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn oauth_identity_patch_updates_only_present_fields() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture, "identity-patch").await?;
    let credential_id =
        create_credential(&fixture, channel_id, CredentialKind::Oauth, None, 0x23).await?;
    let mut active = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    active.oauth_account_key = Set(Some("manual-account".to_owned()));
    active.oauth_project_id = Set(Some("manual-project".to_owned()));
    active.update(fixture.pool.connection()).await?;

    let initial_identity =
        OAuthCredentialIdentityPatch::new(Some("token-account".to_owned()), None)?;
    let rendered = format!("{initial_identity:?}");
    assert!(!rendered.contains("token-account"));
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                channel_id,
                credential_id,
                "codex",
                Some(100),
                envelope(0x24),
                initial_identity,
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::Updated
    );
    let connected = credential_model(&fixture, credential_id).await?;
    assert_eq!(
        connected.oauth_account_key.as_deref(),
        Some("token-account")
    );
    assert_eq!(
        connected.oauth_project_id.as_deref(),
        Some("manual-project")
    );

    let candidate = fixture
        .oauth_repository
        .due_refresh_candidates(100, 1)
        .await?
        .pop()
        .expect("到期凭据必须形成刷新候选");
    let refreshed_identity =
        OAuthCredentialIdentityPatch::new(None, Some("discovered-project".to_owned()))?;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_refreshed_token_secret(candidate, 200, envelope(0x25), refreshed_identity,)
            .await?,
        OAuthCredentialRefreshUpdateOutcome::Updated
    );
    let refreshed = credential_model(&fixture, credential_id).await?;
    assert_eq!(
        refreshed.oauth_account_key.as_deref(),
        Some("token-account")
    );
    assert_eq!(
        refreshed.oauth_project_id.as_deref(),
        Some("discovered-project")
    );

    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn oauth_identity_patch_rejects_unsafe_values() {
    assert!(OAuthCredentialIdentityPatch::default().is_empty());
    for invalid in ["", " leading", "trailing ", "line\nbreak"] {
        assert_eq!(
            OAuthCredentialIdentityPatch::new(Some(invalid.to_owned()), None).unwrap_err(),
            OAuthCredentialRepositoryError::InvalidIdentity
        );
    }
    assert_eq!(
        OAuthCredentialIdentityPatch::new(None, Some("x".repeat(256)),).unwrap_err(),
        OAuthCredentialRepositoryError::InvalidIdentity
    );
}

#[tokio::test]
async fn oauth_token_update_rejects_wrong_targets_without_changing_secrets()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first_channel = create_channel(&fixture, "first").await?;
    let second_channel = create_channel(&fixture, "second").await?;
    let oauth_id =
        create_credential(&fixture, first_channel, CredentialKind::Oauth, None, 0x31).await?;
    let api_key_id =
        create_credential(&fixture, first_channel, CredentialKind::ApiKey, None, 0x32).await?;
    let provider_id = create_credential(
        &fixture,
        first_channel,
        CredentialKind::Oauth,
        Some("claude_code"),
        0x33,
    )
    .await?;
    let deleted_id =
        create_credential(&fixture, first_channel, CredentialKind::Oauth, None, 0x34).await?;
    assert_eq!(
        fixture
            .admin_repository
            .delete_credential(first_channel, deleted_id)
            .await?,
        AdminCredentialDeleteOutcome::Deleted
    );

    let oauth_secret = credential_model(&fixture, oauth_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                second_channel,
                oauth_id,
                "codex",
                Some(1_800_000_000),
                envelope(0x41),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        credential_model(&fixture, oauth_id).await?.secret,
        oauth_secret
    );

    let api_key_secret = credential_model(&fixture, api_key_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                first_channel,
                api_key_id,
                "codex",
                Some(1_800_000_000),
                envelope(0x42),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        credential_model(&fixture, api_key_id).await?.secret,
        api_key_secret
    );

    let deleted_secret = credential_model(&fixture, deleted_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                first_channel,
                deleted_id,
                "codex",
                Some(1_800_000_000),
                envelope(0x43),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        credential_model(&fixture, deleted_id).await?.secret,
        deleted_secret
    );

    let provider_secret = credential_model(&fixture, provider_id).await?.secret;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                first_channel,
                provider_id,
                "codex",
                Some(1_800_000_000),
                envelope(0x44),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::ProviderMismatch
    );
    assert_eq!(
        credential_model(&fixture, provider_id).await?.secret,
        provider_secret
    );

    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                first_channel,
                CredentialId::new(i64::MAX)?,
                "codex",
                Some(1_800_000_000),
                envelope(0x45),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::TargetNotFound
    );
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                first_channel,
                oauth_id,
                "Codex",
                Some(1_800_000_000),
                envelope(0x46),
                OAuthCredentialIdentityPatch::default(),
            )
            .await
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidProvider
    );
    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                first_channel,
                oauth_id,
                "codex",
                Some(-1),
                envelope(0x47),
                OAuthCredentialIdentityPatch::default(),
            )
            .await
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidExpiration
    );
    assert_eq!(
        credential_model(&fixture, oauth_id).await?.secret,
        oauth_secret
    );
    assert_eq!(
        OAuthCredentialRepository::with_operation_timeout(fixture.pool.clone(), Duration::ZERO,)
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidConfiguration
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn due_refresh_candidates_are_bounded_stable_and_fail_closed() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let enabled_channel = create_channel(&fixture, "enabled").await?;
    let disabled_channel = create_channel(&fixture, "disabled").await?;
    let first_due =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x51).await?;
    let second_due =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x52).await?;
    let future =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x53).await?;
    let disabled =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x54).await?;
    let unschedulable =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x55).await?;
    let pending =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x5c).await?;
    let active_cooldown =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x56).await?;
    let elapsed_cooldown =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x57).await?;
    let missing_projection = create_credential(
        &fixture,
        enabled_channel,
        CredentialKind::Oauth,
        Some("codex"),
        0x58,
    )
    .await?;
    let root =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x59).await?;
    let child =
        create_credential(&fixture, enabled_channel, CredentialKind::Oauth, None, 0x5a).await?;
    let channel_disabled = create_credential(
        &fixture,
        disabled_channel,
        CredentialKind::Oauth,
        None,
        0x5b,
    )
    .await?;

    for (credential_id, expires_at, marker) in [
        (first_due, 90_i64, 0x61_u8),
        (second_due, 90, 0x62),
        (future, 101, 0x63),
        (disabled, 80, 0x64),
        (unschedulable, 70, 0x65),
        (pending, 65, 0x6b),
        (active_cooldown, 60, 0x66),
        (elapsed_cooldown, 50, 0x67),
        (root, 101, 0x68),
        (child, 40, 0x69),
        (channel_disabled, 30, 0x6a),
    ] {
        assert_eq!(
            fixture
                .oauth_repository
                .replace_token_secret(
                    if credential_id == channel_disabled {
                        disabled_channel
                    } else {
                        enabled_channel
                    },
                    credential_id,
                    "codex",
                    Some(expires_at),
                    envelope(marker),
                    OAuthCredentialIdentityPatch::default(),
                )
                .await?,
            OAuthCredentialTokenUpdateOutcome::Updated
        );
    }

    let mut disabled_model = credential_model(&fixture, disabled)
        .await?
        .into_active_model();
    disabled_model.status = Set(Status::Disabled.code());
    disabled_model.update(fixture.pool.connection()).await?;
    let mut unschedulable_model = credential_model(&fixture, unschedulable)
        .await?
        .into_active_model();
    unschedulable_model.schedulable = Set(false);
    unschedulable_model
        .update(fixture.pool.connection())
        .await?;
    let mut pending_model = credential_model(&fixture, pending)
        .await?
        .into_active_model();
    pending_model.oauth_token_pending = Set(true);
    pending_model.update(fixture.pool.connection()).await?;
    let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    let mut active_cooldown_model = credential_model(&fixture, active_cooldown)
        .await?
        .into_active_model();
    active_cooldown_model.temp_unschedulable_until = Set(Some(now + Duration::from_secs(300)));
    active_cooldown_model
        .update(fixture.pool.connection())
        .await?;
    let mut elapsed_cooldown_model = credential_model(&fixture, elapsed_cooldown)
        .await?
        .into_active_model();
    elapsed_cooldown_model.temp_unschedulable_until = Set(Some(now - Duration::from_secs(1)));
    elapsed_cooldown_model
        .update(fixture.pool.connection())
        .await?;
    let mut child_model = credential_model(&fixture, child).await?.into_active_model();
    // 影子只保留调度字段，OAuth 生命周期和真实并发均由母凭据承担。
    child_model.parent_id = Set(Some(root.get()));
    child_model.quota_dimension = Set(CredentialQuotaDimension::Spark.as_str().to_owned());
    child_model.concurrency = Set(None);
    child_model.oauth_provider = Set(None);
    child_model.oauth_account_key = Set(None);
    child_model.oauth_project_id = Set(None);
    child_model.oauth_token_pending = Set(false);
    child_model.update(fixture.pool.connection()).await?;
    let mut disabled_channel_model = channels::Entity::find_by_id(disabled_channel.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试渠道必须存在")
        .into_active_model();
    disabled_channel_model.status = Set(Status::Disabled.code());
    disabled_channel_model
        .update(fixture.pool.connection())
        .await?;

    let candidates = fixture
        .oauth_repository
        .due_refresh_candidates(100, MAX_OAUTH_REFRESH_CANDIDATES)
        .await?;
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.credential_id())
        .collect::<Vec<_>>();
    assert_eq!(candidate_ids, vec![elapsed_cooldown, first_due, second_due]);
    assert!(!candidate_ids.contains(&active_cooldown));
    assert!(!candidate_ids.contains(&pending));
    assert!(!candidate_ids.contains(&missing_projection));
    for candidate in &candidates {
        assert_eq!(candidate.channel_id(), enabled_channel);
        assert_eq!(candidate.provider(), "codex");
        assert_eq!(candidate.expected_revision(), 1);
        assert_eq!(
            candidate.expires_at_epoch_seconds(),
            if candidate.credential_id() == elapsed_cooldown {
                50
            } else {
                90
            }
        );
        let rendered = format!("{candidate:?}");
        assert!(!rendered.contains("oauth-write-test-key"));
        assert!(!rendered.contains("ciphertext"));
    }
    assert_eq!(
        fixture
            .oauth_repository
            .due_refresh_candidates(-1, 1)
            .await
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidCandidateQuery
    );
    assert_eq!(
        fixture
            .oauth_repository
            .due_refresh_candidates(100, 0)
            .await
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidCandidateQuery
    );
    assert_eq!(
        fixture
            .oauth_repository
            .due_refresh_candidates(100, MAX_OAUTH_REFRESH_CANDIDATES + 1)
            .await
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidCandidateQuery
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn expiration_projection_backfill_uses_stable_cursor_and_secret_cas()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture, "projection-backfill").await?;
    let first = create_credential(
        &fixture,
        channel_id,
        CredentialKind::Oauth,
        Some("codex"),
        0x69,
    )
    .await?;
    let second = create_credential(
        &fixture,
        channel_id,
        CredentialKind::Oauth,
        Some("codex"),
        0x6a,
    )
    .await?;

    let mut first_page = fixture
        .oauth_repository
        .missing_expiration_projection_candidates(None, 1)
        .await?;
    let duplicate = fixture
        .oauth_repository
        .missing_expiration_projection_candidates(None, 1)
        .await?
        .pop()
        .expect("并发读取必须观察同一空投影事实");
    let first_candidate = first_page.pop().expect("首个旧凭据必须进入回填");
    assert_eq!(first_candidate.credential_id(), first);
    assert_eq!(first_candidate.expected_revision(), 0);
    let rendered = format!("{first_candidate:?}");
    assert!(!rendered.contains("oauth-write-test-key"));
    assert!(!rendered.contains("ciphertext"));

    assert_eq!(
        fixture
            .oauth_repository
            .backfill_expiration_projection(first_candidate, 1_800_000_000)
            .await?,
        OAuthCredentialExpirationProjectionUpdateOutcome::Updated
    );
    assert_eq!(
        fixture
            .oauth_repository
            .backfill_expiration_projection(duplicate, 1_900_000_000)
            .await?,
        OAuthCredentialExpirationProjectionUpdateOutcome::Stale
    );
    let first_model = credential_model(&fixture, first).await?;
    assert_eq!(first_model.oauth_revision, 0);
    assert_eq!(
        first_model.oauth_expires_at_epoch_seconds,
        Some(1_800_000_000)
    );

    let second_page = fixture
        .oauth_repository
        .missing_expiration_projection_candidates(Some(first), 1)
        .await?;
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].credential_id(), second);
    assert_eq!(
        fixture
            .oauth_repository
            .missing_expiration_projection_candidates(None, 0)
            .await
            .unwrap_err(),
        OAuthCredentialRepositoryError::InvalidCandidateQuery
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_cas_preserves_runtime_changes_and_rejects_stale_secret()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture, "refresh-cas").await?;
    let credential_id =
        create_credential(&fixture, channel_id, CredentialKind::Oauth, None, 0x71).await?;
    fixture
        .oauth_repository
        .replace_token_secret(
            channel_id,
            credential_id,
            "codex",
            Some(100),
            envelope(0x72),
            OAuthCredentialIdentityPatch::default(),
        )
        .await?;
    let candidate = fixture
        .oauth_repository
        .due_refresh_candidates(100, 1)
        .await?
        .pop()
        .expect("到期凭据必须成为候选");

    let mut runtime_change = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    runtime_change.status = Set(Status::AutoDisabled.code());
    runtime_change.temp_unschedulable_reason = Set(Some("并发状态变化".to_owned()));
    runtime_change.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_refreshed_token_secret(
                candidate,
                200,
                envelope(0x73),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialRefreshUpdateOutcome::Updated
    );
    let refreshed = credential_model(&fixture, credential_id).await?;
    assert_eq!(refreshed.status, Status::AutoDisabled.code());
    assert_eq!(
        refreshed.temp_unschedulable_reason.as_deref(),
        Some("并发状态变化")
    );
    assert_eq!(refreshed.oauth_revision, 2);
    assert_eq!(refreshed.oauth_expires_at_epoch_seconds, Some(200));
    assert_eq!(refreshed.secret.envelope_parts()?.1, [0x73; 24]);

    let mut restore = refreshed.into_active_model();
    restore.status = Set(Status::Enabled.code());
    restore.schedulable = Set(true);
    restore.update(fixture.pool.connection()).await?;
    let stale_candidate = fixture
        .oauth_repository
        .due_refresh_candidates(200, 1)
        .await?
        .pop()
        .expect("刷新后的旧到期投影必须可复现候选");
    let mut concurrent_secret = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    concurrent_secret.secret = Set(encrypted_json(0x74));
    concurrent_secret.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .oauth_repository
            .replace_refreshed_token_secret(
                stale_candidate,
                300,
                envelope(0x75),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialRefreshUpdateOutcome::Stale
    );
    let after_stale = credential_model(&fixture, credential_id).await?;
    assert_eq!(after_stale.oauth_revision, 2);
    assert_eq!(after_stale.oauth_expires_at_epoch_seconds, Some(200));
    assert_eq!(after_stale.secret.envelope_parts()?.1, [0x74; 24]);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn refresh_failure_write_is_guarded_and_closes_runtime_state() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture, "refresh-failure").await?;
    let credential_id =
        create_credential(&fixture, channel_id, CredentialKind::Oauth, None, 0x81).await?;
    fixture
        .oauth_repository
        .replace_token_secret(
            channel_id,
            credential_id,
            "codex",
            Some(100),
            envelope(0x82),
            OAuthCredentialIdentityPatch::default(),
        )
        .await?;
    let mut candidates = Vec::with_capacity(3);
    for _ in 0..3 {
        candidates.push(
            fixture
                .oauth_repository
                .due_refresh_candidates(100, 1)
                .await?
                .pop()
                .expect("同一 OAuth 事实必须允许并发读取"),
        );
    }
    let stale_after_reauthorization = candidates.pop().unwrap();
    let revoked_candidate = candidates.pop().unwrap();
    let transient_candidate = candidates.pop().unwrap();

    let runtime_now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    let mut runtime = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    runtime.rate_limited_at = Set(Some(runtime_now));
    runtime.rate_limit_reset_at = Set(Some(runtime_now + Duration::from_secs(60)));
    runtime.overload_until = Set(Some(runtime_now + Duration::from_secs(30)));
    runtime.update(fixture.pool.connection()).await?;
    let before = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    assert_eq!(
        fixture
            .oauth_repository
            .record_refresh_failure(
                transient_candidate,
                OAuthCredentialRefreshFailureKind::Transient,
            )
            .await?,
        OAuthCredentialRefreshFailureUpdateOutcome::Recorded
    );
    let after = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
    let cooled = credential_model(&fixture, credential_id).await?;
    assert_eq!(cooled.status, Status::Enabled.code());
    assert_eq!(
        cooled.temp_unschedulable_reason.as_deref(),
        Some("oauth_refresh_transient")
    );
    let cooldown_until = cooled
        .temp_unschedulable_until
        .expect("临时刷新失败必须建立冷却");
    assert!(cooldown_until >= before + Duration::from_secs(5 * 60));
    assert!(cooldown_until <= after + Duration::from_secs(5 * 60));
    assert_eq!(cooled.rate_limited_at, Some(runtime_now));
    assert_eq!(
        cooled.rate_limit_reset_at,
        Some(runtime_now + Duration::from_secs(60))
    );
    assert_eq!(
        cooled.overload_until,
        Some(runtime_now + Duration::from_secs(30))
    );
    assert_eq!(cooled.oauth_revision, 1);
    assert!(
        fixture
            .oauth_repository
            .due_refresh_candidates(100, 1)
            .await?
            .is_empty()
    );

    assert_eq!(
        fixture
            .oauth_repository
            .record_refresh_failure(
                revoked_candidate,
                OAuthCredentialRefreshFailureKind::Revoked,
            )
            .await?,
        OAuthCredentialRefreshFailureUpdateOutcome::Recorded
    );
    let revoked = credential_model(&fixture, credential_id).await?;
    assert_eq!(revoked.status, Status::AutoDisabled.code());
    assert_eq!(revoked.oauth_revision, 1);
    assert!(revoked.rate_limited_at.is_none());
    assert!(revoked.rate_limit_reset_at.is_none());
    assert!(revoked.overload_until.is_none());
    assert!(revoked.temp_unschedulable_until.is_none());
    assert!(revoked.temp_unschedulable_reason.is_none());

    assert_eq!(
        fixture
            .oauth_repository
            .replace_token_secret(
                channel_id,
                credential_id,
                "codex",
                Some(200),
                envelope(0x83),
                OAuthCredentialIdentityPatch::default(),
            )
            .await?,
        OAuthCredentialTokenUpdateOutcome::Updated
    );
    let mut reauthorized = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    reauthorized.status = Set(Status::Enabled.code());
    reauthorized.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .oauth_repository
            .record_refresh_failure(
                stale_after_reauthorization,
                OAuthCredentialRefreshFailureKind::Revoked,
            )
            .await?,
        OAuthCredentialRefreshFailureUpdateOutcome::Stale
    );
    let current = credential_model(&fixture, credential_id).await?;
    assert_eq!(current.status, Status::Enabled.code());
    assert_eq!(current.oauth_revision, 2);
    assert_eq!(current.secret.envelope_parts()?.1, [0x83; 24]);

    let provider_guard = fixture
        .oauth_repository
        .due_refresh_candidates(200, 1)
        .await?
        .pop()
        .expect("重新授权后的凭据必须形成新候选");
    let mut switched = current.into_active_model();
    switched.oauth_provider = Set(Some("gemini".to_owned()));
    switched.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .oauth_repository
            .record_refresh_failure(provider_guard, OAuthCredentialRefreshFailureKind::Revoked)
            .await?,
        OAuthCredentialRefreshFailureUpdateOutcome::ProviderMismatch
    );

    let mut restored = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    restored.oauth_provider = Set(Some("codex".to_owned()));
    restored.update(fixture.pool.connection()).await?;
    let disabled_guard = fixture
        .oauth_repository
        .due_refresh_candidates(200, 1)
        .await?
        .pop()
        .expect("恢复 Provider 后必须重新成为候选");
    let mut disabled = credential_model(&fixture, credential_id)
        .await?
        .into_active_model();
    disabled.status = Set(Status::Disabled.code());
    disabled.update(fixture.pool.connection()).await?;
    assert_eq!(
        fixture
            .oauth_repository
            .record_refresh_failure(disabled_guard, OAuthCredentialRefreshFailureKind::Transient,)
            .await?,
        OAuthCredentialRefreshFailureUpdateOutcome::Stale
    );
    let disabled = credential_model(&fixture, credential_id).await?;
    assert_eq!(disabled.status, Status::Disabled.code());
    assert!(disabled.temp_unschedulable_until.is_none());

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    admin_repository: AdminChannelRepository,
    oauth_repository: OAuthCredentialRepository,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    Ok(Fixture {
        admin_repository: AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?,
        oauth_repository: OAuthCredentialRepository::new(pool.clone()),
        pool,
    })
}

async fn create_channel(fixture: &Fixture, name: &str) -> Result<ChannelId, Box<dyn Error>> {
    Ok(fixture
        .admin_repository
        .create_channel(AdminChannelWriteRecord::new(
            name.to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://api.example.com/v1".to_owned()),
            Some(af_domain::ChannelTimeout::new(60)?),
            Status::Enabled,
            10,
            20,
            true,
            Vec::new(),
            Vec::new(),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            None,
        ))
        .await?
        .channel_id())
}

async fn create_credential(
    fixture: &Fixture,
    channel_id: ChannelId,
    kind: CredentialKind,
    oauth_provider: Option<&str>,
    marker: u8,
) -> Result<CredentialId, Box<dyn Error>> {
    let AdminCredentialCreateOutcome::Created(record) = fixture
        .admin_repository
        .create_credential(
            channel_id,
            AdminCredentialWriteRecord::new(
                kind,
                Status::Enabled,
                Some(2),
                30,
                40,
                Some(5),
                Some(1_250_000),
                Some(875_000),
                true,
                None,
                CredentialQuotaDimension::Global,
                None,
                oauth_provider.map(str::to_owned),
                (kind == CredentialKind::Oauth).then(|| "account-key".to_owned()),
                (kind == CredentialKind::Oauth).then(|| "project-id".to_owned()),
            ),
            |_| Ok(envelope(marker)),
        )
        .await?
    else {
        panic!("测试渠道必须存在");
    };
    Ok(record.credential_id())
}

async fn credential_model(
    fixture: &Fixture,
    credential_id: CredentialId,
) -> Result<credentials::Model, Box<dyn Error>> {
    Ok(credentials::Entity::find_by_id(credential_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试凭据必须存在"))
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("oauth-write-test-key", [marker; 24], vec![marker; 32])
        .unwrap()
}

fn encrypted_json(marker: u8) -> EncryptedJson {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": "oauth-write-test-key",
        "nonce": URL_SAFE_NO_PAD.encode([marker; 24]),
        "ciphertext": URL_SAFE_NO_PAD.encode([marker; 32]),
    }))
    .unwrap()
}
