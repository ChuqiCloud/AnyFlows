use std::{error::Error, time::Duration};

use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};
use sea_orm::{
    ActiveModelTrait, EntityTrait, JsonValue, QueryOrder, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use super::{
    AdminUsageLogRecord, AdminUsageLogRepository, AdminUsageLogRepositoryConfigError,
    AdminUsageLogRepositoryError, DatabaseOptions, MigrationOptions, UsageLogBillingMode,
    UsageLogRepository, UsageLogSemantics, UsageLogSource, UsageLogUsage, UsageLogVideoResolution,
    UsageLogWrite,
};
use crate::entity::{TokenHash, groups, tokens, usage_logs, users};

#[tokio::test]
async fn list_uses_newest_first_cursor_and_preserves_usage_dimensions() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let first = fixture.repository.list(None, 2).await?;
    let (logs, next_cursor) = first.into_parts();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].quota(), 300);
    assert_eq!(logs[1].quota(), 200);
    assert_eq!(next_cursor, Some(logs[1].id()));

    let own = fixture
        .repository
        .list_for_user(fixture.user_id, None, 10)
        .await?;
    assert_eq!(own.into_parts().0.len(), 3);
    let unrelated = fixture
        .repository
        .list_for_user(UserId::new(fixture.user_id.get() + 1_000)?, None, 10)
        .await?;
    assert!(unrelated.into_parts().0.is_empty());

    let newest = &logs[0];
    assert_eq!(newest.user_id(), fixture.user_id);
    assert_eq!(newest.username(), "usage-owner");
    assert_eq!(newest.token_id(), fixture.token_id);
    assert_eq!(newest.group_id(), fixture.group_id);
    assert_eq!(newest.billing_mode(), 3);
    assert_eq!(newest.usage_source(), 1);
    assert_eq!(newest.usage_semantics(), 2);
    assert_eq!(newest.usage().input_tokens(), 30);
    assert_eq!(newest.usage().output_tokens(), 3);
    assert_eq!(newest.usage().cache_read(), 6);
    assert_eq!(newest.usage().cache_creation_5m(), 3);
    assert_eq!(newest.usage().cache_creation_1h(), 0);
    assert_eq!(newest.usage().reasoning_tokens(), 9);
    assert_eq!(newest.usage().audio_input_tokens(), 0);
    assert_eq!(newest.usage().audio_output_tokens(), 0);
    assert_eq!(newest.audio_duration_nanoseconds(), Some(3_500_000_000));
    assert_eq!(newest.video_duration_seconds(), Some(8));
    assert_eq!(
        newest.video_resolution(),
        Some(UsageLogVideoResolution::P720)
    );
    assert_eq!(newest.event_id().len(), 32);
    assert!(newest.created_at() > 0);
    assert_eq!(format!("{newest:?}"), "AdminUsageLogRecord(<redacted>)");

    let second = fixture.repository.list(next_cursor, 2).await?;
    let (logs, next_cursor) = second.into_parts();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].quota(), 100);
    assert_eq!(next_cursor, None);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_pagination_corrupt_state_and_closed_pool_fail_closed() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    for (before, limit) in [(Some(0), 1), (None, 0), (None, 101)] {
        assert_eq!(
            fixture.repository.list(before, limit).await.unwrap_err(),
            AdminUsageLogRepositoryError::Invariant
        );
    }

    let mut broken = usage_logs::Entity::find()
        .order_by_desc(usage_logs::Column::Id)
        .one(fixture.pool.connection())
        .await?
        .expect("测试用量日志必须存在");
    broken.usage_source = 9;
    assert_eq!(
        AdminUsageLogRecord::try_from_model(broken, "usage-owner".to_owned()).unwrap_err(),
        AdminUsageLogRepositoryError::Invariant
    );

    fixture.pool.clone().close().await?;
    assert_eq!(
        fixture.repository.list(None, 1).await.unwrap_err(),
        AdminUsageLogRepositoryError::Query
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
        AdminUsageLogRepository::new(pool.clone(), Duration::ZERO),
        Err(AdminUsageLogRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminUsageLogRepository,
    user_id: UserId,
    token_id: TokenId,
    group_id: GroupId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("usage-admin".to_owned()),
        display_name: Set("用量审计".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("usage-owner".to_owned()),
        email: Set(None),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(group.id),
        quota: Set(10_000),
        aff_code: Set("usage-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&format!("{:064x}", 91))?),
        key_prefix: Set("sk-af-usage0000001".to_owned()),
        name: Set("usage-token".to_owned()),
        status: Set(1),
        group_id: Set(Some(group.id)),
        remain_quota: Set(10_000),
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
    .insert(pool.connection())
    .await?;
    let user_id = UserId::new(user.id)?;
    let token_id = TokenId::new(token.id)?;
    let group_id = GroupId::new(group.id)?;
    let principal = GatewayPrincipal::new(token_id, user_id, group_id);
    let writer = UsageLogRepository::new(pool.clone());
    for marker in 1_u8..=3 {
        let mut event_id = [0_u8; 16];
        event_id[15] = marker;
        let usage = UsageLogUsage::new(
            i64::from(marker) * 10,
            i64::from(marker),
            i64::from(marker) * 2,
            i64::from(marker),
            0,
            i64::from(marker) * 3,
            0,
            0,
        )?;
        let write = UsageLogWrite::new(
            BillingReservationId::new(event_id)?,
            principal,
            match marker {
                2 => UsageLogBillingMode::Free,
                3 => UsageLogBillingMode::PerCall,
                _ => UsageLogBillingMode::PerToken,
            },
            usage,
            if marker == 2 {
                UsageLogSource::Estimated
            } else {
                UsageLogSource::Upstream
            },
            if marker == 3 {
                UsageLogSemantics::CacheSeparated
            } else {
                UsageLogSemantics::Inclusive
            },
            Quota::new(i64::from(marker) * 100)?,
        );
        let write = if marker == 3 {
            write
                .with_audio_duration_nanoseconds(Some(3_500_000_000))?
                .with_video_dimensions(Some(8), Some(UsageLogVideoResolution::P720))?
                .with_call_observation(
                    "request-admin-3",
                    "gpt-test",
                    af_domain::Protocol::OpenAiResponses,
                    af_domain::Operation::Responses,
                    true,
                    Some(5),
                    Some(4_096),
                    Some(120),
                    640,
                )?
        } else {
            write
        };
        writer.record(&write).await?;
    }
    Ok(Fixture {
        repository: AdminUsageLogRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        user_id,
        token_id,
        group_id,
    })
}
