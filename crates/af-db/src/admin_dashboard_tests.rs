use std::{error::Error, time::Duration};

use sea_orm::{ActiveModelTrait, JsonValue, Set, entity::prelude::TimeDateTimeWithTimeZone};

use super::{
    AdminDashboardRepository, AdminDashboardRepositoryConfigError, AdminDashboardRepositoryError,
    DatabaseOptions, MigrationOptions,
};
use crate::entity::{HeaderOverrides, SensitiveJson, channels, usage_logs};

#[tokio::test]
async fn snapshot_uses_half_open_window_and_excludes_deleted_channels() -> Result<(), Box<dyn Error>>
{
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let start = TimeDateTimeWithTimeZone::from_unix_timestamp(1_000)?;
    let end = TimeDateTimeWithTimeZone::from_unix_timestamp(2_000)?;
    seed_principals(pool.connection()).await?;
    for (id, created_at, source, billing_mode, quota) in [
        (1, start, 1_i16, 1_i16, 10_i64),
        (
            2,
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_500)?,
            2,
            2,
            20,
        ),
        (3, end, 1, 1, 30),
    ] {
        usage_logs::ActiveModel {
            id: Set(id),
            event_id: Set(crate::entity::BillingReservationKey::parse(&format!(
                "{id:032x}"
            ))?),
            event_type: Set(1),
            user_id: Set(1),
            token_id: Set(1),
            group_id: Set(1),
            billing_mode: Set(billing_mode),
            input_tokens: Set(1),
            output_tokens: Set(1),
            cache_read: Set(0),
            cache_creation_5m: Set(0),
            cache_creation_1h: Set(0),
            reasoning_tokens: Set(0),
            audio_input_tokens: Set(0),
            audio_output_tokens: Set(0),
            usage_source: Set(source),
            usage_semantics: Set(1),
            quota: Set(quota),
            created_at: Set(created_at),
        }
        .insert(pool.connection())
        .await?;
    }
    for (id, status, deleted_at) in [
        (1, 1_i16, None),
        (2, 2_i16, None),
        (3, 3_i16, None),
        (4, 1_i16, Some(end)),
    ] {
        channels::ActiveModel {
            id: Set(id),
            name: Set(format!("dashboard-{id}")),
            r#type: Set("openai".to_owned()),
            protocol: Set("openai_chat".to_owned()),
            base_url: Set(None),
            status: Set(status),
            weight: Set(1),
            priority: Set(0),
            auto_ban: Set(false),
            model_mapping: Set(JsonValue::Object(Default::default())),
            param_override: Set(JsonValue::Object(Default::default())),
            header_override: Set(HeaderOverrides::validate(JsonValue::Object(
                Default::default(),
            ))?),
            balance: Set(None),
            used_quota: Set(0),
            settings: Set(SensitiveJson::from(JsonValue::Object(Default::default()))),
            tag: Set(None),
            created_at: Set(start),
            updated_at: Set(start),
            deleted_at: Set(deleted_at),
        }
        .insert(pool.connection())
        .await?;
    }

    let repository = AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2))?;
    let snapshot = repository.snapshot(1_000, 2_000).await?;
    assert_eq!(snapshot.request_count(), 2);
    assert_eq!(snapshot.quota_consumed(), 30);
    assert_eq!(snapshot.upstream_usage_count(), 1);
    assert_eq!(snapshot.estimated_usage_count(), 1);
    assert_eq!(snapshot.per_token_request_count(), 1);
    assert_eq!(snapshot.free_request_count(), 1);
    assert_eq!(snapshot.enabled_channel_count(), 1);
    assert_eq!(snapshot.disabled_channel_count(), 1);
    assert_eq!(snapshot.auto_disabled_channel_count(), 1);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_window_and_closed_pool_fail_closed() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2))?;
    assert_eq!(
        repository.snapshot(10, 10).await.unwrap_err(),
        AdminDashboardRepositoryError::Invariant
    );
    pool.clone().close().await?;
    assert_eq!(
        repository.snapshot(10, 20).await.unwrap_err(),
        AdminDashboardRepositoryError::Query
    );
    Ok(())
}

#[test]
fn zero_lookup_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = runtime.block_on(crate::connect(&DatabaseOptions::new("sqlite::memory:")?))?;
    assert!(matches!(
        AdminDashboardRepository::new(pool.clone(), Duration::ZERO),
        Err(AdminDashboardRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

async fn seed_principals(connection: &sea_orm::DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    use crate::entity::{TokenHash, groups, tokens, users};

    groups::ActiveModel {
        id: Set(1),
        name: Set("dashboard".to_owned()),
        display_name: Set("看板".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    users::ActiveModel {
        id: Set(1),
        username: Set("dashboard-owner".to_owned()),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(1),
        quota: Set(1_000),
        aff_code: Set("dashboard-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    let now = TimeDateTimeWithTimeZone::from_unix_timestamp(1_000).unwrap();
    tokens::ActiveModel {
        id: Set(1),
        user_id: Set(1),
        key_hash: Set(TokenHash::parse(&format!("{:064x}", 1)).unwrap()),
        key_prefix: Set("sk-af-dashboard001".to_owned()),
        name: Set("dashboard-token".to_owned()),
        status: Set(1),
        group_id: Set(Some(1)),
        remain_quota: Set(1_000),
        unlimited_quota: Set(false),
        used_quota: Set(0),
        expired_at: Set(None),
        model_limits: Set(None),
        allow_ips: Set(None),
        cross_group_retry: Set(false),
        rate_limit_5h: Set(None),
        rate_limit_1d: Set(None),
        rate_limit_7d: Set(None),
        usage_5h: Set(0),
        usage_1d: Set(0),
        usage_7d: Set(0),
        window_5h_start: Set(now),
        window_1d_start: Set(now),
        window_7d_start: Set(now),
        max_requests: Set(None),
        used_requests: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    Ok(())
}
