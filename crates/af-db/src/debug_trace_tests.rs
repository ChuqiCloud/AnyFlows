use std::{error::Error, time::Duration};

use sea_orm::{
    ActiveModelTrait, ConnectionTrait, EntityTrait, PaginatorTrait, Set,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
};

use crate::{
    DatabaseOptions, DebugTraceAttemptWrite, DebugTraceFailureKind, DebugTraceListQuery,
    DebugTraceOperation, DebugTraceOutcome, DebugTraceProtocol, DebugTraceRepository,
    DebugTraceRepositoryError, DebugTraceSettingsWrite, DebugTraceWrite, MigrationOptions,
    entity::{TokenHash, debug_trace_attempts, debug_traces, groups, tokens, users},
};

#[tokio::test]
async fn settings_default_to_disabled_and_increment_version() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let settings = fixture.repository.settings().await?;
    assert!(!settings.enabled());
    assert_eq!(settings.sample_per_million(), 10_000);
    assert_eq!(settings.retention_hours(), 24);
    assert_eq!(settings.version(), 1);

    let updated = fixture
        .repository
        .update_settings(DebugTraceSettingsWrite::new(true, 250_000, 72)?)
        .await?;
    assert!(updated.enabled());
    assert_eq!(updated.sample_per_million(), 250_000);
    assert_eq!(updated.retention_hours(), 72);
    assert_eq!(updated.version(), 2);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn storage_round_trip_preserves_order_filters_and_cursor() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = fixture
        .repository
        .insert(successful_trace(&fixture, "request-a", "gpt-5.5")?)
        .await?;
    let second = fixture
        .repository
        .insert(failed_trace(&fixture, "request-b", "claude-opus-4-8")?)
        .await?;
    let third = fixture
        .repository
        .insert(successful_trace(&fixture, "request-c", "gpt-5.5")?)
        .await?;

    let page = fixture
        .repository
        .list(DebugTraceListQuery::new(None, 2, None, None)?)
        .await?;
    assert_eq!(
        page.traces()
            .iter()
            .map(|trace| trace.id())
            .collect::<Vec<_>>(),
        vec![third.id(), second.id()]
    );
    assert_eq!(page.next_cursor(), Some(second.id()));
    let next = fixture
        .repository
        .list(DebugTraceListQuery::new(page.next_cursor(), 2, None, None)?)
        .await?;
    assert_eq!(next.traces().len(), 1);
    assert_eq!(next.traces()[0].id(), first.id());
    assert_eq!(next.next_cursor(), None);

    let failed = fixture
        .repository
        .list(DebugTraceListQuery::new(
            None,
            10,
            Some(DebugTraceOutcome::Failed),
            Some("claude-opus-4-8".to_owned()),
        )?)
        .await?;
    assert_eq!(failed.traces().len(), 1);
    assert_eq!(failed.traces()[0].id(), second.id());

    let detail = fixture.repository.detail(first.id()).await?;
    assert_eq!(detail.attempts().len(), 2);
    assert_eq!(detail.attempts()[0].candidate_index(), 0);
    assert_eq!(detail.attempts()[1].candidate_index(), 1);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn duplicate_request_is_rejected_without_orphan_attempts() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .insert(successful_trace(&fixture, "request-duplicate", "gpt-5.5")?)
        .await?;
    assert_eq!(
        fixture
            .repository
            .insert(failed_trace(&fixture, "request-duplicate", "gpt-5.5")?)
            .await,
        Err(DebugTraceRepositoryError::Query)
    );
    assert_eq!(
        debug_traces::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        debug_trace_attempts::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        2
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn corrupted_settings_fail_closed_and_prune_cascades_attempts() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let trace = fixture
        .repository
        .insert(successful_trace(&fixture, "request-expired", "gpt-5.5")?)
        .await?;
    let model = debug_traces::Entity::find_by_id(trace.id())
        .one(fixture.pool.connection())
        .await?
        .expect("追踪主记录必须存在");
    let mut active: debug_traces::ActiveModel = model.into();
    active.created_at = Set(TimeDateTimeWithTimeZone::now_utc() - Duration::from_secs(7_200));
    active.update(fixture.pool.connection()).await?;
    assert_eq!(fixture.repository.prune(1).await?, 1);
    assert_eq!(
        debug_traces::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        debug_trace_attempts::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );

    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA ignore_check_constraints = ON")
        .await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE debug_trace_settings SET version = 0 WHERE id = 1")
        .await?;
    assert_eq!(
        fixture.repository.settings().await,
        Err(DebugTraceRepositoryError::Invariant)
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA ignore_check_constraints = OFF")
        .await?;
    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: crate::DatabasePool,
    repository: DebugTraceRepository,
    user_id: i64,
    token_id: i64,
    group_id: i64,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("debug-trace-group".to_owned()),
        display_name: Set("调试追踪测试分组".to_owned()),
        flags: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("debug-trace-user".to_owned()),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("debug-trace-aff".to_owned()),
        settings: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&format!("{:064x}", 43_u8))?),
        key_prefix: Set("sk-af-debug-trace".to_owned()),
        name: Set("debug-trace-token".to_owned()),
        status: Set(1),
        group_id: Set(Some(group.id)),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        repository: DebugTraceRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        user_id: user.id,
        token_id: token.id,
        group_id: group.id,
    })
}

fn successful_trace(
    fixture: &Fixture,
    request_id: &str,
    model: &str,
) -> Result<DebugTraceWrite, crate::DebugTraceWriteError> {
    DebugTraceWrite::new(
        request_id.to_owned(),
        fixture.user_id,
        fixture.token_id,
        fixture.group_id,
        model.to_owned(),
        DebugTraceProtocol::OpenAiChat,
        DebugTraceProtocol::OpenAiResponses,
        DebugTraceOperation::Chat,
        DebugTraceOutcome::Succeeded,
        35,
        vec![
            DebugTraceAttemptWrite::succeeded(1, 12, 22, 15)?,
            DebugTraceAttemptWrite::failed(
                0,
                11,
                21,
                DebugTraceFailureKind::RateLimited,
                None,
                true,
                20,
            )?,
        ],
    )
}

fn failed_trace(
    fixture: &Fixture,
    request_id: &str,
    model: &str,
) -> Result<DebugTraceWrite, crate::DebugTraceWriteError> {
    DebugTraceWrite::new(
        request_id.to_owned(),
        fixture.user_id,
        fixture.token_id,
        fixture.group_id,
        model.to_owned(),
        DebugTraceProtocol::Anthropic,
        DebugTraceProtocol::Anthropic,
        DebugTraceOperation::Chat,
        DebugTraceOutcome::Failed,
        50,
        vec![DebugTraceAttemptWrite::failed(
            0,
            13,
            23,
            DebugTraceFailureKind::ServerError,
            Some(503),
            false,
            50,
        )?],
    )
}
