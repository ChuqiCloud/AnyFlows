use std::{
    collections::VecDeque,
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_billing::{
    DatabaseUsageRecordSink, GroupPricingCache, ModelPriceCache, TaskBillingReleaseSink,
    TaskBillingReservePort, TaskBillingSettlementError, TaskBillingSettlementFuture,
    TaskBillingSettlementPort, TaskBillingSettlementRequest, UsageRecordSink,
};
use af_db::{
    AsyncTaskBillingRepository, AsyncTaskRepository, AsyncTaskSubmissionRepository,
    QuotaRepository, UsageLogRepository,
};
use af_domain::{
    AfError, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskRequestId, ChannelId, CredentialId,
    GatewayPrincipal, GroupId, TaskProgress, TaskStatus, TokenId, UpstreamTaskId,
};
use af_protocol::{
    CanonicalTaskOutput, CanonicalTaskPoll, CanonicalVideoGenerationRequest, CanonicalVideoOutput,
    VideoDuration, VideoModel, VideoOutputUrl, VideoPrompt, VideoResolution,
};
use af_relay::VideoTaskSubmissionDisposition;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, Statement};

use crate::test_database::{SqliteTestDatabase, test_encrypted_envelope_json};

use super::*;

const REQUESTED_MODEL: &str = "video-public";
const UPSTREAM_MODEL: &str = "grok-imagine-video-1.5";

#[tokio::test]
async fn route_video_releases_cheapest_failure_and_bills_frozen_backup_group()
-> Result<(), Box<dyn Error>> {
    let (fixture, routes, backup) = user_route_fixture().await?;
    let mut success = successful_submission(&fixture);
    success.target_group_id = backup;
    let runtime = Arc::new(
        MockRuntime::new(vec![
            Err((
                af_domain::UpstreamError::overloaded().into(),
                VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
            )),
            Ok(success),
        ])
        .with_poll(successful_poll(8)),
    );
    let coordinator = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes);
    let request_id = async_request_id(0x81);
    let task = coordinator
        .submit(
            async_task_id(0x81),
            request_id,
            fixture.principal,
            request(),
            "video-route-failover",
        )
        .await?;
    assert_eq!(
        *runtime.submitted_groups.lock().unwrap(),
        vec![
            (fixture.principal.group_id(), fixture.principal.group_id()),
            (backup, backup)
        ],
    );
    let billing = fixture
        .billings
        .find(fixture.principal.user_id(), task.task_id())
        .await?
        .unwrap();
    assert_eq!(billing.target_group_id(), backup);
    assert_eq!(billing.ratios(), [2_000_000, 1_000_000, 1_000_000]);
    let reservations = fixture.database.seed().query_all(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "SELECT group_id, status FROM billing_reservations WHERE token_id = ? ORDER BY group_id",
        [fixture.principal.token_id().get().into()],
    )).await?;
    assert_eq!(reservations.len(), 2);
    assert_eq!(
        reservations[0].try_get::<i64>("", "group_id")?,
        fixture.principal.group_id().get()
    );
    assert_eq!(
        reservations[1].try_get::<i64>("", "group_id")?,
        backup.get()
    );
    assert_eq!(reservations[0].try_get::<i16>("", "status")?, 4);
    assert_eq!(reservations[1].try_get::<i16>("", "status")?, 1);

    // 停用路由不改变已经受理任务的重放及轮询绑定。
    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE user_routes SET enabled = false WHERE id = ?",
            [fixture.principal.user_route_id().unwrap().into()],
        ))
        .await?;
    let replay = coordinator
        .submit(
            async_task_id(0x82),
            request_id,
            fixture.principal,
            request(),
            "video-route-replay",
        )
        .await?;
    assert_eq!(replay.task_id(), task.task_id());
    assert_eq!(runtime.submission_count(), 2);
    coordinator
        .poll(
            fixture.principal.user_id(),
            task.task_id(),
            "video-route-poll",
        )
        .await?;
    assert_eq!(*runtime.polled_groups.lock().unwrap(), vec![backup]);
    let quota = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT frozen_quota FROM users WHERE id = ?",
            [fixture.principal.user_id().get().into()],
        ))
        .await?
        .unwrap();
    assert_eq!(quota.try_get::<i64>("", "frozen_quota")?, 0);
    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn route_video_does_not_failover_bad_request_or_unknown_acceptance()
-> Result<(), Box<dyn Error>> {
    for (error, disposition, expected_unknown) in [
        (
            af_domain::UpstreamError::BadRequest.into(),
            VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
            false,
        ),
        (
            af_domain::UpstreamError::overloaded().into(),
            VideoTaskSubmissionDisposition::AcceptanceUnknown,
            true,
        ),
    ] {
        let (fixture, routes, _) = user_route_fixture().await?;
        let runtime = Arc::new(MockRuntime::new(vec![Err((error, disposition))]));
        let coordinator = fixture
            .coordinator(runtime.clone())
            .with_user_route_runtime(routes);
        let request_id = async_request_id(0x91);
        let result = coordinator
            .submit(
                async_task_id(0x91),
                request_id,
                fixture.principal,
                request(),
                "video-route-terminal",
            )
            .await;
        if expected_unknown {
            assert!(matches!(
                result,
                Err(PersistentVideoTaskError::SubmissionOutcomeUnknown)
            ));
            assert!(matches!(
                coordinator
                    .submit(
                        async_task_id(0x92),
                        request_id,
                        fixture.principal,
                        request(),
                        "video-route-unknown-replay",
                    )
                    .await,
                Err(PersistentVideoTaskError::SubmissionOutcomeUnknown),
            ));
        } else {
            assert!(matches!(
                result,
                Err(PersistentVideoTaskError::Runtime(AfError::Upstream(
                    af_domain::UpstreamError::BadRequest
                )))
            ));
        }
        assert_eq!(runtime.submission_count(), 1);
        fixture.database.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn route_video_resumes_reserved_target_even_after_health_cooldown()
-> Result<(), Box<dyn Error>> {
    let (fixture, routes, backup) = user_route_fixture().await?;
    let mut success = successful_submission(&fixture);
    success.target_group_id = backup;
    let runtime = Arc::new(MockRuntime::new(vec![Ok(success)]));
    let coordinator = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes);
    let video = request();
    let request_id = async_request_id(0xa1);
    let claim = coordinator
        .claim(AsyncTaskSubmissionClaim::new(
            async_task_id(0xa1),
            request_id,
            fixture.principal,
            Protocol::XaiVideo,
            REQUESTED_MODEL.to_owned(),
            request_fingerprint(&video),
            Some(8),
            now()?,
        )?)
        .await?;
    let billing = coordinator
        .prepare_billing(&claim, backup, video.resolution())
        .await?;
    let original_reservation = billing.reservation_id();
    coordinator.reserve_billing(&claim, billing).await?;
    let repository =
        af_db::UserRouteRepository::new(fixture.database.pool().clone(), Duration::from_secs(5))?;
    for _ in 0..3 {
        repository
            .record_attempt(
                fixture.principal.user_route_id().unwrap(),
                backup,
                REQUESTED_MODEL,
                false,
                1,
                true,
            )
            .await?;
    }
    let task = coordinator
        .submit(
            async_task_id(0xa2),
            request_id,
            fixture.principal,
            video,
            "video-route-resume",
        )
        .await?;
    let resumed = fixture
        .billings
        .find(fixture.principal.user_id(), task.task_id())
        .await?
        .unwrap();
    assert_eq!(resumed.reservation_id(), original_reservation);
    assert_eq!(
        *runtime.submitted_groups.lock().unwrap(),
        vec![(backup, backup)]
    );
    fixture.database.close().await;
    Ok(())
}

async fn user_route_fixture() -> Result<(Fixture, UserRouteRuntime, GroupId), Box<dyn Error>> {
    let mut fixture = fixture().await?;
    let backup = GroupId::new(insert(
        fixture.database.seed(),
        "INSERT INTO groups (name, display_name, ratio_micros, flags) VALUES ('video-backup', '视频备用分组', 2000000, '{}')",
    ).await?)?;
    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO group_key_policies (group_id, selection_mode) VALUES (?, 'all_users')",
            [backup.get().into()],
        ))
        .await?;
    let repository =
        af_db::UserRouteRepository::new(fixture.database.pool().clone(), Duration::from_secs(5))?;
    let route = repository
        .create(
            fixture.principal.user_id(),
            af_db::UserRouteWrite {
                name: "视频智能路由".to_owned(),
                strategy: af_domain::UserRouteStrategy::PriceFirst,
                enabled: true,
                max_group_attempts: 2,
                max_price_ratio_micros: None,
                members: [fixture.principal.group_id(), backup]
                    .into_iter()
                    .enumerate()
                    .map(|(priority, group_id)| af_db::UserRouteMemberWrite {
                        group_id,
                        priority: priority as i32,
                        weight: 1,
                    })
                    .collect(),
            },
        )
        .await?;
    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE tokens SET group_id = NULL, user_route_id = ? WHERE id = ?",
            [route.id.into(), fixture.principal.token_id().get().into()],
        ))
        .await?;
    fixture.principal = fixture.principal.with_user_route_id(Some(route.id));
    fixture.group_pricing =
        GroupPricingCache::load_from_database(fixture.database.pool().clone()).await?;
    let runtime = UserRouteRuntime::new(repository).with_group_model_policy(
        af_db::GroupModelPolicyRepository::new(
            fixture.database.pool().clone(),
            Duration::from_secs(5),
        )?,
    );
    Ok((fixture, runtime, backup))
}

#[tokio::test]
async fn video_model_time_slot_is_frozen_before_submission_and_survives_replay()
-> Result<(), Box<dyn Error>> {
    let mut fixture = fixture().await?;
    let windows = |ratio| {
        serde_json::json!([{
            "weekdays": [1, 2, 3, 4, 5, 6, 7], "start_second": 0, "end_second": 86400,
            "ratio_micros": ratio, "models": [], "utc_offset_minutes": 0,
        }])
        .to_string()
    };
    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE groups SET peak_windows = ? WHERE id = ?",
            [
                windows(2_000_000).into(),
                fixture.principal.group_id().get().into(),
            ],
        ))
        .await?;
    fixture.database.seed().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO model_prices (model, billing_mode, input_price, output_price, cache_read_price, cache_creation_5m_price, cache_creation_1h_price, peak_windows, version) VALUES (?, 1, 0, 0, 0, 0, 0, ?, 1)",
        [REQUESTED_MODEL.into(), windows(500_000).into()],
    )).await?;
    fixture.group_pricing =
        GroupPricingCache::load_from_database(fixture.database.pool().clone()).await?;
    fixture.model_prices =
        ModelPriceCache::load_from_database(fixture.database.pool().clone()).await?;
    let runtime = Arc::new(MockRuntime::new(vec![Ok(successful_submission(&fixture))]));
    let request_id = async_request_id(0x73);
    let task = fixture
        .coordinator(runtime.clone())
        .submit(
            async_task_id(0x73),
            request_id,
            fixture.principal,
            request(),
            "video-model-peak",
        )
        .await?;
    let first = fixture
        .billings
        .find(fixture.principal.user_id(), task.task_id())
        .await?
        .expect("视频首次提交必须冻结模型时段倍率");
    assert_eq!(first.ratios(), [1_000_000, 1_000_000, 500_000]);

    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE model_prices SET peak_windows = ?, version = version + 1 WHERE model = ?",
            [windows(1_500_000).into(), REQUESTED_MODEL.into()],
        ))
        .await?;
    fixture.model_prices =
        ModelPriceCache::load_from_database(fixture.database.pool().clone()).await?;
    let restarted = fixture.coordinator(runtime.clone());
    let claim = fixture
        .submissions
        .find_by_request(fixture.principal.user_id(), request_id)
        .await?
        .expect("视频重放必须恢复首次提交 claim");
    let current =
        restarted.capture_pricing(&claim, fixture.principal.group_id(), request().resolution())?;
    assert_eq!(current.ratios().applied_peak().micros(), 1_500_000);
    let replay = restarted
        .submit(
            async_task_id(0x74),
            request_id,
            fixture.principal,
            request(),
            "video-model-peak-replay",
        )
        .await?;
    let frozen = fixture
        .billings
        .find(fixture.principal.user_id(), replay.task_id())
        .await?
        .expect("视频重放必须复用首次计费计划");
    assert_eq!(replay.task_id(), task.task_id());
    assert_eq!(frozen.ratios(), first.ratios());
    assert_eq!(frozen.upper_bound(), first.upper_bound());
    assert_eq!(runtime.submission_count(), 1);
    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn video_pricing_preserves_user_source_and_fixed_routing_group_on_replay()
-> Result<(), Box<dyn Error>> {
    let mut fixture = fixture().await?;
    let source_group_id = insert(
        fixture.database.seed(),
        "INSERT INTO groups (name, display_name, flags) VALUES ('video-source', '视频用户来源组', '{}')",
    ).await?;
    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE users SET default_group_id = ? WHERE id = ?",
            [
                source_group_id.into(),
                fixture.principal.user_id().get().into(),
            ],
        ))
        .await?;
    fixture.database.seed().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO group_model_ratios (source_group_id, target_group_id, ratio_micros) VALUES (?, ?, ?)",
        [source_group_id.into(), fixture.principal.group_id().get().into(), 1_500_000_i64.into()],
    )).await?;
    fixture.principal = fixture
        .principal
        .with_billing_source_group_id(GroupId::new(source_group_id)?);
    fixture.group_pricing =
        GroupPricingCache::load_from_database(fixture.database.pool().clone()).await?;
    let runtime = Arc::new(MockRuntime::new(vec![Ok(successful_submission(&fixture))]));
    let coordinator = fixture.coordinator(runtime.clone());
    let request_id = async_request_id(0x71);
    let task = coordinator
        .submit(
            async_task_id(0x71),
            request_id,
            fixture.principal,
            request(),
            "video-source",
        )
        .await?;
    let billing = fixture
        .billings
        .find(fixture.principal.user_id(), task.task_id())
        .await?
        .expect("视频任务必须有冻结计费快照");
    assert_eq!(billing.ratios(), [1_000_000, 1_500_000, 1_000_000]);
    let claim = fixture
        .submissions
        .find_by_request(fixture.principal.user_id(), request_id)
        .await?
        .expect("视频任务必须保留提交来源");
    assert_eq!(
        claim.principal().billing_source_group_id().get(),
        source_group_id
    );
    assert_eq!(claim.principal().group_id(), fixture.principal.group_id());
    let replay = coordinator
        .submit(
            async_task_id(0x72),
            request_id,
            fixture.principal,
            request(),
            "video-source-replay",
        )
        .await?;
    assert_eq!(replay.task_id(), task.task_id());
    assert_eq!(runtime.submission_count(), 1);
    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn definite_failure_releases_claim_and_success_replay_does_not_resubmit()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let runtime = Arc::new(MockRuntime::new(vec![
        Err((
            AfError::Internal,
            VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
        )),
        Ok(successful_submission(&fixture)),
    ]));
    let coordinator = fixture.coordinator(runtime.clone());
    let request_id = async_request_id(0x21);

    assert!(matches!(
        coordinator
            .submit(
                async_task_id(0x11),
                request_id,
                fixture.principal,
                request(),
                "video-definite-first",
            )
            .await,
        Err(PersistentVideoTaskError::Runtime(AfError::Internal))
    ));
    let released_user = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT quota, used_quota, frozen_quota FROM users WHERE id = ?",
            [fixture.principal.user_id().get().into()],
        ))
        .await?
        .expect("测试用户必须存在");
    assert_eq!(released_user.try_get::<i64>("", "quota")?, 10_000_000);
    assert_eq!(released_user.try_get::<i64>("", "used_quota")?, 0);
    assert_eq!(released_user.try_get::<i64>("", "frozen_quota")?, 0);
    let released_token = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT remain_quota, used_quota FROM tokens WHERE id = ?",
            [fixture.principal.token_id().get().into()],
        ))
        .await?
        .expect("测试令牌必须存在");
    assert_eq!(
        released_token.try_get::<i64>("", "remain_quota")?,
        10_000_000
    );
    assert_eq!(released_token.try_get::<i64>("", "used_quota")?, 0);
    let task = match coordinator
        .submit(
            async_task_id(0x12),
            request_id,
            fixture.principal,
            request(),
            "video-definite-second",
        )
        .await
    {
        Ok(task) => task,
        Err(error) => {
            let claim_state = fixture
                .submissions
                .find_by_request(fixture.principal.user_id(), request_id)
                .await
                .map(|record| record.map(|record| record.state()));
            let task_exists = fixture
                .tasks
                .find_by_request(fixture.principal.user_id(), request_id)
                .await
                .map(|record| record.is_some());
            panic!(
                "确定失败重试未闭合：error={error:?}, claim_state={claim_state:?}, task_exists={task_exists:?}"
            );
        }
    };
    assert_eq!(runtime.submission_count(), 2);
    assert_eq!(task.status().state(), af_domain::TaskState::Submitted);

    let replay = coordinator
        .submit(
            async_task_id(0x13),
            request_id,
            fixture.principal,
            request(),
            "video-success-replay",
        )
        .await?;
    assert_eq!(replay.task_id(), task.task_id());
    assert_eq!(runtime.submission_count(), 2);

    let polled = coordinator
        .poll(fixture.principal.user_id(), task.task_id(), "video-poll")
        .await?;
    let PersistentVideoTaskPollOutcome::Updated {
        task,
        poll,
        billing_dimensions,
    } = polled
    else {
        panic!("活动任务必须访问轮询端口")
    };
    assert_eq!(task.status().progress().basis_points(), 2_500);
    assert_eq!(poll.status(), task.status());
    assert_eq!(billing_dimensions, None);
    assert_eq!(runtime.poll_count(), 1);

    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn unknown_submission_stays_claimed_by_original_attempt_and_never_resubmits()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let runtime = Arc::new(MockRuntime::new(vec![Err((
        AfError::Upstream(af_domain::UpstreamError::ProtocolError),
        VideoTaskSubmissionDisposition::AcceptanceUnknown,
    ))]));
    let coordinator = fixture.coordinator(runtime.clone());
    let request_id = async_request_id(0x31);

    for marker in [0x31, 0x32] {
        assert!(matches!(
            coordinator
                .submit(
                    async_task_id(marker),
                    request_id,
                    fixture.principal,
                    request(),
                    "video-unknown",
                )
                .await,
            Err(PersistentVideoTaskError::SubmissionOutcomeUnknown)
        ));
    }
    assert_eq!(runtime.submission_count(), 1);
    let claim = fixture
        .submissions
        .find_by_request(fixture.principal.user_id(), request_id)
        .await?
        .expect("结果未知 claim 必须保留");
    assert_eq!(claim.state(), af_db::AsyncTaskSubmissionState::Submitting);

    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn successful_terminal_uses_upstream_duration_and_keeps_missing_resolution()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let runtime = Arc::new(
        MockRuntime::new(vec![Ok(successful_submission(&fixture))]).with_poll(successful_poll(8)),
    );
    let coordinator = fixture.coordinator(runtime.clone());
    let task = coordinator
        .submit(
            async_task_id(0x41),
            async_request_id(0x41),
            fixture.principal,
            request_without_resolution(5),
            "video-success-without-resolution",
        )
        .await?;

    let polled = coordinator
        .poll(
            fixture.principal.user_id(),
            task.task_id(),
            "video-success-poll",
        )
        .await?;
    let PersistentVideoTaskPollOutcome::Updated {
        billing_dimensions, ..
    } = polled
    else {
        panic!("成功终态必须更新持久化任务")
    };
    let dimensions = billing_dimensions.expect("成功终态必须返回计费审计维度");
    assert_eq!(dimensions.video_duration().unwrap().seconds(), 8);
    assert_eq!(dimensions.video_resolution(), None);
    let user = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT quota, used_quota, frozen_quota FROM users WHERE id = ?",
            [fixture.principal.user_id().get().into()],
        ))
        .await?
        .expect("测试用户必须存在");
    assert_eq!(user.try_get::<i64>("", "quota")?, 9_680_000);
    assert_eq!(user.try_get::<i64>("", "used_quota")?, 320_000);
    assert_eq!(user.try_get::<i64>("", "frozen_quota")?, 0);
    let token = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT remain_quota, used_quota FROM tokens WHERE id = ?",
            [fixture.principal.token_id().get().into()],
        ))
        .await?
        .expect("成功视频令牌必须存在");
    assert_eq!(token.try_get::<i64>("", "remain_quota")?, 9_680_000);
    assert_eq!(token.try_get::<i64>("", "used_quota")?, 320_000);
    let usage = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT billing_mode, quota, video_duration_seconds, video_resolution FROM usage_logs WHERE user_id = ?",
            [fixture.principal.user_id().get().into()],
        ))
        .await?
        .expect("成功视频必须写入用量日志");
    assert_eq!(usage.try_get::<i16>("", "billing_mode")?, 3);
    assert_eq!(usage.try_get::<i64>("", "quota")?, 320_000);
    assert_eq!(usage.try_get::<i64>("", "video_duration_seconds")?, 8);
    assert_eq!(usage.try_get::<Option<i16>>("", "video_resolution")?, None);
    assert!(matches!(
        coordinator
            .poll(
                fixture.principal.user_id(),
                task.task_id(),
                "video-success-terminal-replay",
            )
            .await?,
        PersistentVideoTaskPollOutcome::Terminal(_)
    ));
    assert_eq!(runtime.poll_count(), 1);
    let refreshed = coordinator
        .fetch_succeeded_output(
            fixture.principal.user_id(),
            task.task_id(),
            "video-success-result-refresh",
        )
        .await?;
    let refreshed_video = refreshed
        .output()
        .and_then(|output| output.as_video())
        .expect("成功终态重取必须返回原任务的临时视频结果");
    assert_eq!(refreshed_video.duration().seconds(), 8);
    assert_eq!(runtime.poll_count(), 2);
    let usage_count = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS usage_count FROM usage_logs WHERE user_id = ?",
            [fixture.principal.user_id().get().into()],
        ))
        .await?
        .expect("用量计数必须返回一行");
    assert_eq!(usage_count.try_get::<i64>("", "usage_count")?, 1);

    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn unknown_settlement_result_replays_same_reservation_before_task_terminal()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let runtime = Arc::new(
        MockRuntime::new(vec![Ok(successful_submission(&fixture))]).with_poll(successful_poll(8)),
    );
    let settlement = Arc::new(UnknownOnceSettlementPort::new(fixture.quota.clone()));
    let coordinator = fixture.coordinator_with_settlement(runtime.clone(), settlement.clone());
    let task = coordinator
        .submit(
            async_task_id(0x45),
            async_request_id(0x45),
            fixture.principal,
            request_without_resolution(5),
            "video-settlement-unknown-submit",
        )
        .await?;

    assert!(matches!(
        coordinator
            .poll(
                fixture.principal.user_id(),
                task.task_id(),
                "video-settlement-unknown-first",
            )
            .await,
        Err(PersistentVideoTaskError::Billing)
    ));
    let stored = fixture
        .tasks
        .find(fixture.principal.user_id(), task.task_id())
        .await?
        .expect("结算结果未知时任务必须保留");
    assert!(!stored.status().is_terminal());
    let billing = fixture
        .billings
        .find(fixture.principal.user_id(), task.task_id())
        .await?
        .expect("结算参数必须持久化");
    assert_eq!(
        billing.state(),
        af_db::AsyncTaskBillingState::SettlementPending
    );
    assert_eq!(
        billing.actual_quota().map(af_domain::Quota::units),
        Some(320_000)
    );

    let replay = coordinator
        .poll(
            fixture.principal.user_id(),
            task.task_id(),
            "video-settlement-unknown-replay",
        )
        .await?;
    let PersistentVideoTaskPollOutcome::Updated { task, .. } = replay else {
        panic!("结算重放完成后必须推进任务终态")
    };
    assert_eq!(task.status(), &TaskStatus::Succeeded);
    assert_eq!(settlement.calls(), 2);
    assert_eq!(runtime.poll_count(), 2);

    fixture.database.close().await;
    Ok(())
}

struct UnknownOnceSettlementPort {
    inner: Arc<QuotaRepository>,
    calls: AtomicUsize,
}

impl UnknownOnceSettlementPort {
    fn new(inner: Arc<QuotaRepository>) -> Self {
        Self {
            inner,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl TaskBillingSettlementPort for UnknownOnceSettlementPort {
    fn settle<'a>(
        &'a self,
        request: TaskBillingSettlementRequest,
    ) -> TaskBillingSettlementFuture<'a> {
        Box::pin(async move {
            TaskBillingSettlementPort::settle(self.inner.as_ref(), request).await?;
            if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Err(TaskBillingSettlementError::OutcomeUnknown)
            } else {
                Ok(())
            }
        })
    }
}

struct MockRuntime {
    submissions:
        Mutex<VecDeque<Result<PersistentSubmission, (AfError, VideoTaskSubmissionDisposition)>>>,
    submission_count: AtomicUsize,
    poll_count: AtomicUsize,
    poll: Mutex<CanonicalTaskPoll>,
    submitted_groups: Mutex<Vec<(GroupId, GroupId)>>,
    polled_groups: Mutex<Vec<GroupId>>,
}

impl MockRuntime {
    fn new(
        submissions: Vec<Result<PersistentSubmission, (AfError, VideoTaskSubmissionDisposition)>>,
    ) -> Self {
        Self {
            submissions: Mutex::new(submissions.into()),
            submission_count: AtomicUsize::new(0),
            poll_count: AtomicUsize::new(0),
            poll: Mutex::new(running_poll()),
            submitted_groups: Mutex::new(Vec::new()),
            polled_groups: Mutex::new(Vec::new()),
        }
    }

    fn with_poll(mut self, poll: CanonicalTaskPoll) -> Self {
        self.poll = Mutex::new(poll);
        self
    }

    fn submission_count(&self) -> usize {
        self.submission_count.load(Ordering::SeqCst)
    }

    fn poll_count(&self) -> usize {
        self.poll_count.load(Ordering::SeqCst)
    }
}

impl PersistentVideoTaskRuntime for MockRuntime {
    fn target_group<'a>(
        &'a self,
        group_id: GroupId,
        _model: &'a str,
    ) -> PersistentTargetGroupFuture<'a> {
        Box::pin(async move { Ok(group_id) })
    }

    fn submit<'a>(
        &'a self,
        group_id: GroupId,
        expected_target_group_id: GroupId,
        _request: CanonicalVideoGenerationRequest,
        _request_id: &'a str,
    ) -> PersistentSubmissionFuture<'a> {
        self.submission_count.fetch_add(1, Ordering::SeqCst);
        self.submitted_groups
            .lock()
            .expect("模拟分组记录不能中毒")
            .push((group_id, expected_target_group_id));
        let result = self
            .submissions
            .lock()
            .expect("模拟提交队列锁不能中毒")
            .pop_front()
            .expect("模拟提交结果不能为空");
        Box::pin(async move {
            result.map_err(|(error, disposition)| {
                VideoTaskSubmissionRuntimeError::new(error, disposition)
            })
        })
    }

    fn poll<'a>(
        &'a self,
        _task: &'a AsyncTaskRecord,
        claim: &'a AsyncTaskSubmissionRecord,
        _request_id: &'a str,
    ) -> PersistentPollFuture<'a> {
        self.poll_count.fetch_add(1, Ordering::SeqCst);
        self.polled_groups
            .lock()
            .expect("模拟轮询记录不能中毒")
            .push(claim.target_group_id().expect("已受理任务必须保存目标分组"));
        let poll = self.poll.lock().expect("模拟轮询结果锁不能中毒").clone();
        Box::pin(async move { Ok(poll) })
    }
}

struct Fixture {
    database: SqliteTestDatabase,
    tasks: AsyncTaskRepository,
    submissions: AsyncTaskSubmissionRepository,
    billings: AsyncTaskBillingRepository,
    group_pricing: GroupPricingCache,
    model_prices: ModelPriceCache,
    quota: Arc<QuotaRepository>,
    reserve: Arc<dyn TaskBillingReservePort>,
    settlement: Arc<dyn TaskBillingSettlementPort>,
    release: Arc<dyn TaskBillingReleaseSink>,
    usage: Arc<dyn UsageRecordSink>,
    principal: GatewayPrincipal,
    channel_id: ChannelId,
    credential_id: CredentialId,
}

impl Fixture {
    fn coordinator(
        &self,
        runtime: Arc<dyn PersistentVideoTaskRuntime>,
    ) -> PersistentVideoTaskCoordinator {
        self.coordinator_with_settlement(runtime, Arc::clone(&self.settlement))
    }

    fn coordinator_with_settlement(
        &self,
        runtime: Arc<dyn PersistentVideoTaskRuntime>,
        settlement: Arc<dyn TaskBillingSettlementPort>,
    ) -> PersistentVideoTaskCoordinator {
        PersistentVideoTaskCoordinator::with_runtime(
            runtime,
            self.tasks.clone(),
            self.submissions.clone(),
            self.billings.clone(),
            self.group_pricing.clone(),
            self.model_prices.clone(),
            PersistentVideoTaskBillingPorts::new(
                Arc::clone(&self.reserve),
                settlement,
                Arc::clone(&self.release),
                Arc::clone(&self.usage),
            ),
        )
    }
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let database = SqliteTestDatabase::new("video-persistence").await;
    let group_id = insert(
        database.seed(),
        "INSERT INTO groups (name, display_name, flags) VALUES ('video-persistence', '视频持久化测试', '{}')",
    )
    .await?;
    let user_id = insert(
        database.seed(),
        format!(
            "INSERT INTO users (username, default_group_id, aff_code, quota, settings, status) VALUES ('video-persistence', {group_id}, 'video-persistence-aff', 10000000, '{{}}', 1)"
        ),
    )
    .await?;
    let token_id = insert(
        database.seed(),
        format!(
            "INSERT INTO tokens (user_id, key_hash, key_prefix, name, status, group_id, remain_quota) VALUES ({user_id}, '{}', 'sk-video-persistence', '视频持久化令牌', 1, {group_id}, 10000000)",
            "ab".repeat(32),
        ),
    )
    .await?;
    let channel_id = insert(
        database.seed(),
        "INSERT INTO channels (name, \"type\", protocol, model_mapping, param_override, header_override, settings) VALUES ('视频持久化渠道', 'xai', 'xai_video', '{}', '{}', '{}', '{}')",
    )
    .await?;
    let credential_id = insert_statement(
        database.seed(),
        Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO credentials (channel_id, kind, secret) VALUES (?, 'api_key', ?)",
            [channel_id.into(), test_encrypted_envelope_json(0x51).into()],
        ),
    )
    .await?;
    let pool = database.pool().clone();
    let quota = Arc::new(QuotaRepository::new(pool.clone()));
    let reserve: Arc<dyn TaskBillingReservePort> = quota.clone();
    let settlement: Arc<dyn TaskBillingSettlementPort> = quota.clone();
    let release: Arc<dyn TaskBillingReleaseSink> = quota.clone();
    let usage: Arc<dyn UsageRecordSink> = Arc::new(DatabaseUsageRecordSink::new(
        UsageLogRepository::new(pool.clone()),
    ));
    Ok(Fixture {
        tasks: AsyncTaskRepository::new(database.pool().clone(), Duration::from_secs(5))?,
        submissions: AsyncTaskSubmissionRepository::new(
            database.pool().clone(),
            Duration::from_secs(5),
        )?,
        billings: AsyncTaskBillingRepository::new(pool.clone(), Duration::from_secs(5))?,
        group_pricing: GroupPricingCache::load_from_database(pool.clone()).await?,
        model_prices: ModelPriceCache::load_from_database(pool).await?,
        quota: quota.clone(),
        reserve,
        settlement,
        release,
        usage,
        database,
        principal: GatewayPrincipal::new(
            TokenId::new(token_id)?,
            af_domain::UserId::new(user_id)?,
            GroupId::new(group_id)?,
        ),
        channel_id: ChannelId::new(channel_id)?,
        credential_id: CredentialId::new(credential_id)?,
    })
}

async fn insert(
    connection: &DatabaseConnection,
    sql: impl Into<String>,
) -> Result<i64, sea_orm::DbErr> {
    let sql = sql.into();
    let result = connection.execute_unprepared(&sql).await?;
    Ok(inserted_id(result))
}

async fn insert_statement(
    connection: &DatabaseConnection,
    statement: Statement,
) -> Result<i64, sea_orm::DbErr> {
    let result = connection.execute(statement).await?;
    Ok(inserted_id(result))
}

fn inserted_id(result: sea_orm::ExecResult) -> i64 {
    i64::try_from(result.last_insert_id()).expect("SQLite 测试标识必须适配 i64")
}

fn successful_submission(fixture: &Fixture) -> PersistentSubmission {
    PersistentSubmission {
        target_group_id: fixture.principal.group_id(),
        submission: TaskSubmission::new(
            UpstreamTaskId::new("video-upstream-task").expect("测试上游任务标识必须有效"),
            TaskStatus::Submitted {
                progress: TaskProgress::ZERO,
            },
        ),
        upstream_model: UPSTREAM_MODEL.to_owned(),
        channel_id: fixture.channel_id,
        credential_id: fixture.credential_id,
        credential_revision: 0x0102_0304_0506_0708,
        binding_fingerprint: AsyncTaskBindingFingerprint::new([0x42; 32]),
        attempt_timeout: Duration::from_secs(5),
    }
}

fn request() -> CanonicalVideoGenerationRequest {
    CanonicalVideoGenerationRequest::new(
        VideoModel::new(REQUESTED_MODEL).expect("测试模型必须有效"),
        VideoPrompt::new("private prompt").expect("测试提示词必须有效"),
        Some(VideoDuration::new(8).expect("测试时长必须有效")),
        None,
        Some(VideoResolution::P720),
    )
}

fn request_without_resolution(seconds: u8) -> CanonicalVideoGenerationRequest {
    CanonicalVideoGenerationRequest::new(
        VideoModel::new(REQUESTED_MODEL).expect("测试模型必须有效"),
        VideoPrompt::new("private prompt").expect("测试提示词必须有效"),
        Some(VideoDuration::new(seconds).expect("测试时长必须有效")),
        None,
        None,
    )
}

fn running_poll() -> CanonicalTaskPoll {
    CanonicalTaskPoll::new(
        TaskStatus::Running {
            progress: TaskProgress::new(2_500).expect("测试进度必须有效"),
        },
        None,
    )
    .expect("测试运行态必须有效")
}

fn successful_poll(seconds: u8) -> CanonicalTaskPoll {
    let output = CanonicalVideoOutput::new(
        VideoOutputUrl::new("https://video.example/result.mp4?signature=private")
            .expect("测试结果地址必须有效"),
        VideoDuration::new(seconds).expect("测试输出时长必须有效"),
        VideoModel::new(UPSTREAM_MODEL).expect("测试上游模型必须有效"),
    );
    CanonicalTaskPoll::new(
        TaskStatus::Succeeded,
        Some(CanonicalTaskOutput::Video(output)),
    )
    .expect("测试成功终态必须有效")
}

fn async_task_id(marker: u8) -> AsyncTaskId {
    AsyncTaskId::new([marker; 16]).expect("测试任务标识必须非零")
}

fn async_request_id(marker: u8) -> AsyncTaskRequestId {
    AsyncTaskRequestId::new([marker; 16]).expect("测试幂等标识必须非零")
}
