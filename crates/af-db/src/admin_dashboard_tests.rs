use std::{error::Error, time::Duration};

use sea_orm::{ActiveModelTrait, JsonValue, Set, entity::prelude::TimeDateTimeWithTimeZone};

use super::{
    AdminDashboardRepository, AdminDashboardRepositoryConfigError, AdminDashboardRepositoryError,
    DatabaseOptions, MigrationOptions,
};
use crate::entity::{HeaderOverrides, SensitiveJson, channels, request_outcome_logs, usage_logs};

#[tokio::test]
async fn snapshot_uses_half_open_window_and_excludes_deleted_channels() -> Result<(), Box<dyn Error>>
{
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let start = TimeDateTimeWithTimeZone::from_unix_timestamp(1_000)?;
    let end = TimeDateTimeWithTimeZone::from_unix_timestamp(87_400)?;
    seed_principals(pool.connection()).await?;
    for (id, created_at, source, billing_mode, quota, first_token_ms, duration_ms) in [
        (
            1,
            start,
            1_i16,
            1_i16,
            10_i64,
            Some(500_i64),
            Some(5_000_i64),
        ),
        (
            2,
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_500)?,
            2,
            2,
            20,
            None,
            Some(12_000),
        ),
        (
            3,
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_750)?,
            1,
            3,
            30,
            Some(2_500),
            None,
        ),
        (4, end, 1, 1, 40, Some(9_000), Some(30_000)),
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
            organization_id: Set(None),
            organization_team_id: Set(None),
            billing_mode: Set(billing_mode),
            input_tokens: Set(1),
            output_tokens: Set(1),
            cache_read: Set(0),
            cache_creation_5m: Set(0),
            cache_creation_1h: Set(0),
            reasoning_tokens: Set(0),
            audio_input_tokens: Set(0),
            audio_output_tokens: Set(0),
            audio_duration_nanoseconds: Set(None),
            video_duration_seconds: Set(None),
            video_resolution: Set(None),
            request_id: Set(Some(format!("dashboard-outcome-{id}"))),
            model: Set(Some("gpt-test".to_owned())),
            protocol: Set(None),
            operation: Set(None),
            is_stream: Set(None),
            reasoning_effort: Set(None),
            reasoning_budget_tokens: Set(None),
            first_token_ms: Set(first_token_ms),
            duration_ms: Set(duration_ms),
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
            timeout_secs: Set(None),
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
    for (id, created_at, protocol, outcome, error_kind, channel_id) in [
        (1, start, "openai_chat", 1_i16, None, Some(1_i64)),
        (2, start, "openai_responses", 1, None, Some(1)),
        (
            3,
            TimeDateTimeWithTimeZone::from_unix_timestamp(1_500)?,
            "openai_chat",
            2,
            Some("upstream_network"),
            None,
        ),
        (4, end, "openai_chat", 1, None, Some(1)),
    ] {
        request_outcome_logs::ActiveModel {
            id: Set(id),
            request_id: Set(format!("dashboard-outcome-{id}")),
            protocol: Set(protocol.to_owned()),
            operation: Set("chat".to_owned()),
            model: Set("gpt-test".to_owned()),
            outcome: Set(outcome),
            error_kind: Set(error_kind.map(str::to_owned)),
            channel_id: Set(channel_id),
            duration_ms: Set(10),
            created_at: Set(created_at),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
    }

    let repository = AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2))?;
    let snapshot = repository.snapshot(1_000, 87_400).await?;
    assert_eq!(snapshot.request_count(), 3);
    assert_eq!(snapshot.quota_consumed(), 60);
    assert_eq!(snapshot.upstream_usage_count(), 2);
    assert_eq!(snapshot.estimated_usage_count(), 1);
    assert_eq!(snapshot.per_token_request_count(), 1);
    assert_eq!(snapshot.per_call_request_count(), 1);
    assert_eq!(snapshot.free_request_count(), 1);
    assert_eq!(snapshot.enabled_channel_count(), 1);
    assert_eq!(snapshot.disabled_channel_count(), 1);
    assert_eq!(snapshot.auto_disabled_channel_count(), 1);
    assert_eq!(snapshot.hourly().len(), 24);
    assert_eq!(snapshot.hourly()[0].period_start(), 1_000);
    assert_eq!(snapshot.hourly()[0].period_end(), 4_600);
    assert_eq!(snapshot.hourly()[0].request_count(), 3);
    assert_eq!(snapshot.hourly()[0].quota_consumed(), 60);
    assert!(
        snapshot.hourly()[1..]
            .iter()
            .all(|bucket| bucket.request_count() == 0 && bucket.quota_consumed() == 0)
    );
    let performance = snapshot.performance();
    assert_eq!(performance.first_token_sample_count(), 2);
    assert_eq!(performance.average_first_token_ms(), Some(1_500));
    assert_eq!(performance.slow_first_token_count(), 1);
    assert_eq!(performance.duration_sample_count(), 2);
    assert_eq!(performance.average_duration_ms(), Some(8_500));
    assert_eq!(performance.slow_request_count(), 1);
    assert_eq!(snapshot.outcome_request_count(), 3);
    assert_eq!(snapshot.successful_request_count(), 2);
    assert_eq!(snapshot.failed_request_count(), 1);
    assert_eq!(snapshot.other_success_count(), 0);
    assert_eq!(snapshot.failures().len(), 1);
    assert_eq!(
        snapshot.failures()[0].kind(),
        crate::RequestFailureKind::UpstreamNetwork
    );
    assert_eq!(snapshot.channel_flows().len(), 2);
    assert_eq!(snapshot.channel_flows()[0].request_count(), 1);
    assert_eq!(snapshot.flow_request_count(), 2);
    assert_eq!(snapshot.flow_quota_consumed(), 30);
    assert_eq!(snapshot.flow_paths().len(), 1);
    let path = &snapshot.flow_paths()[0];
    assert_eq!(path.user_id(), 1);
    assert_eq!(path.group_id(), 1);
    assert_eq!(path.group_name(), "看板");
    assert_eq!(path.channel_id().get(), 1);
    assert_eq!(path.channel_name(), "dashboard-1");
    assert_eq!(path.model(), "gpt-test");
    assert_eq!(path.request_count(), 2);
    assert_eq!(path.quota_consumed(), 30);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn service_levels_cover_full_outcomes_and_paginate_independently()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let start = TimeDateTimeWithTimeZone::from_unix_timestamp(1_000)?;
    for id in [1, 2] {
        channels::ActiveModel {
            id: Set(id),
            name: Set(format!("channel-{id}")),
            r#type: Set("openai".to_owned()),
            protocol: Set("openai_chat".to_owned()),
            base_url: Set(None),
            timeout_secs: Set(None),
            status: Set(1),
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
            deleted_at: Set((id == 2).then_some(start)),
        }
        .insert(pool.connection())
        .await?;
    }
    for (id, timestamp, model, outcome, error_kind, duration, channel) in [
        (1, 1_000, "gpt-test", 1, None, 1_000, Some(1)),
        (2, 4_600, "gpt-test", 1, None, 3_000, Some(1)),
        (
            3,
            1_000,
            "gpt-test",
            2,
            Some("upstream_network"),
            9_000,
            Some(1),
        ),
        (
            4,
            1_000,
            "gpt-test",
            2,
            Some("outcome_unknown"),
            9_000,
            Some(1),
        ),
        (
            5,
            1_000,
            "unknown-only",
            2,
            Some("outcome_unknown"),
            9_000,
            Some(2),
        ),
        (6, 999, "outside", 1, None, 1, Some(1)),
        (7, 87_400, "outside", 1, None, 1, Some(1)),
    ] {
        request_outcome_logs::ActiveModel {
            id: Set(id),
            request_id: Set(format!("sla-{id}")),
            protocol: Set("openai_chat".to_owned()),
            operation: Set("chat".to_owned()),
            model: Set(model.to_owned()),
            outcome: Set(outcome),
            error_kind: Set(error_kind.map(str::to_owned)),
            channel_id: Set(channel),
            duration_ms: Set(duration),
            created_at: Set(TimeDateTimeWithTimeZone::from_unix_timestamp(timestamp)?),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
    }
    for id in 10..35 {
        request_outcome_logs::ActiveModel {
            id: Set(id),
            request_id: Set(format!("sla-{id}")),
            protocol: Set("openai_chat".to_owned()),
            operation: Set("chat".to_owned()),
            model: Set(format!("model-{id}")),
            outcome: Set(2),
            error_kind: Set(Some("invalid_request".to_owned())),
            channel_id: Set(None),
            duration_ms: Set(100),
            created_at: Set(start),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
    }
    let repository = AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2))?;
    let query = crate::DashboardServiceLevelQuery {
        channels: false,
        search: String::new(),
        page: 1,
        page_size: 5,
        failures_first: false,
    };
    let report = repository
        .service_levels(1_000, 87_400, query.clone())
        .await?;
    assert_eq!(report.total, 27);
    assert_eq!(report.items.len(), 5);
    assert_eq!(report.unattributed_request_count, 25);
    let gpt = &report.items[0];
    assert_eq!(gpt.name, "gpt-test");
    assert_eq!(gpt.request_count, 4);
    assert_eq!(gpt.successful_request_count, 2);
    assert_eq!(gpt.failed_request_count, 1);
    assert_eq!(gpt.unknown_request_count, 1);
    assert_eq!(gpt.average_duration_ms, Some(2_000));
    assert_eq!(gpt.hourly.len(), 24);
    assert_eq!(gpt.hourly[0].successful_request_count, 1);
    assert_eq!(gpt.hourly[0].failed_request_count, 1);
    assert_eq!(gpt.hourly[0].unknown_request_count, 1);
    assert_eq!(gpt.hourly[1].successful_request_count, 1);
    assert!(
        gpt.hourly[2..]
            .iter()
            .all(|point| point.successful_request_count == 0)
    );
    let next = repository
        .service_levels(
            1_000,
            87_400,
            crate::DashboardServiceLevelQuery {
                page: 2,
                ..query.clone()
            },
        )
        .await?;
    assert_eq!(next.total, 27);
    assert!(
        next.items
            .iter()
            .all(|row| !report.items.iter().any(|first| first.key == row.key))
    );
    let unknown = repository
        .service_levels(
            1_000,
            87_400,
            crate::DashboardServiceLevelQuery {
                search: "UNKNOWN".to_owned(),
                ..query.clone()
            },
        )
        .await?;
    assert_eq!(unknown.total, 1);
    assert_eq!(unknown.items[0].failed_request_count, 0);
    assert_eq!(unknown.items[0].unknown_request_count, 1);
    assert_eq!(unknown.items[0].average_duration_ms, None);
    let channels = repository
        .service_levels(
            1_000,
            87_400,
            crate::DashboardServiceLevelQuery {
                channels: true,
                ..query.clone()
            },
        )
        .await?;
    assert_eq!(channels.total, 2);
    assert_eq!(channels.items[0].name, "channel-1");
    assert_eq!(channels.items[0].request_count, 4);
    assert_eq!(
        channels.items[1].name, "channel-2",
        "deleted channels retain historical attribution"
    );
    let sorted = repository
        .service_levels(
            1_000,
            87_400,
            crate::DashboardServiceLevelQuery {
                failures_first: true,
                page: 6,
                ..query.clone()
            },
        )
        .await?;
    assert_eq!(sorted.items.len(), 2);
    assert_eq!(
        sorted.items[1].name, "unknown-only",
        "unknown outcomes are not known failures"
    );
    let empty = repository
        .service_levels(
            1_000,
            87_400,
            crate::DashboardServiceLevelQuery {
                search: "no-match".to_owned(),
                ..query.clone()
            },
        )
        .await?;
    assert_eq!(empty.total, 0);
    assert!(empty.items.is_empty());
    assert_eq!(
        repository
            .service_levels(1_000, 87_399, query)
            .await
            .unwrap_err(),
        AdminDashboardRepositoryError::Invariant
    );
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
        repository.snapshot(10, 86_410).await.unwrap_err(),
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
