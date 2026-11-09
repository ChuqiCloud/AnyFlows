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
    EncryptedCredentialEnvelope, MerchantFinanceRepository, MerchantPoolRepository,
    MerchantRouteMember, MerchantRouteWrite, QuotaRepository, UsageLogRepository,
};
use af_domain::{
    AfError, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskRequestId, ChannelId, CredentialId,
    GatewayPrincipal, GroupId, MerchantApplication, MerchantModelWrite, MerchantPoolWrite,
    MerchantReview, MerchantServiceWrite, TaskProgress, TaskStatus, TokenId, UpstreamTaskId,
    UserId, UserRouteStrategy,
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
    polled_bindings: Mutex<Vec<(ChannelId, CredentialId)>>,
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
            polled_bindings: Mutex::new(Vec::new()),
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
        self.polled_bindings
            .lock()
            .expect("模拟绑定记录不能中毒")
            .push((
                claim.channel_id().expect("已受理渠道"),
                claim.credential_id().expect("已受理凭据"),
            ));
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

struct MerchantVideoFixture {
    fixture: Fixture,
    routes: UserRouteRuntime,
    pool_group: GroupId,
    merchants: [i64; 2],
    price_version: i64,
}

async fn merchant_video_fixture(with_cash: bool) -> Result<MerchantVideoFixture, Box<dyn Error>> {
    let mut fixture = fixture().await?;
    let pool_group = GroupId::new(insert(fixture.database.seed(),
        "INSERT INTO groups (name, display_name, flags) VALUES ('merchant-video-pool', '视频商户池', '{}')").await?)?;
    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO group_key_policies (group_id, selection_mode) VALUES (?, 'all_users')",
            [pool_group.get().into()],
        ))
        .await?;
    let admin = UserId::new(insert(fixture.database.seed(), format!(
        "INSERT INTO users (username,default_group_id,aff_code,settings,status,role) VALUES ('merchant-video-admin',{},'merchant-video-admin','{{}}',1,1)",
        fixture.principal.group_id().get())).await?)?;
    let pools =
        MerchantPoolRepository::new(fixture.database.pool().clone(), Duration::from_secs(5));
    let pool = pools
        .write_pool(
            admin,
            None,
            MerchantPoolWrite {
                group_id: pool_group.get(),
                name: "Merchant Video Pool".to_owned(),
                description: String::new(),
                enabled: true,
                commission_bps: 500,
                commission_inherit_global: false,
                min_ratio_micros: 100_000,
                max_ratio_micros: 5_000_000,
                settlement_delay_secs: 3600,
            },
        )
        .await?;
    let mut merchants = Vec::new();
    let mut first_price = None;
    for (index, ratio) in [(1, 500_000), (2, 1_000_000)] {
        let owner = UserId::new(insert(fixture.database.seed(), format!(
            "INSERT INTO users (username,default_group_id,aff_code,settings,status) VALUES ('merchant-video-{index}',{},'merchant-video-aff-{index}','{{}}',1)",
            fixture.principal.group_id().get())).await?)?;
        let merchant = pools
            .apply(
                owner,
                MerchantApplication {
                    name: format!("Merchant Video {index}"),
                    description: String::new(),
                },
            )
            .await?;
        let review = |expected_revision| MerchantReview {
            decision: "approve".to_owned(),
            reason: "Fixture approval".to_owned(),
            expected_revision,
        };
        pools
            .review_merchant(admin, merchant.id, review(None))
            .await?;
        let service = pools
            .write_service(
                owner,
                None,
                MerchantServiceWrite {
                    pool_id: pool.id,
                    name: format!("Merchant video supply {index}"),
                    description: String::new(),
                    base_url: "https://api.example.com/v1".to_owned(),
                    protocol: "xai_video".to_owned(),
                    api_key: Some("merchant-video-private".to_owned()),
                    default_ratio_micros: ratio,
                    models: vec![MerchantModelWrite {
                        model: UPSTREAM_MODEL.to_owned(),
                        upstream_model: UPSTREAM_MODEL.to_owned(),
                        ratio_micros: None,
                        context_window: None,
                        max_output_tokens: None,
                        supports_streaming: false,
                        supports_tools: false,
                        supports_vision: false,
                    }],
                    concurrency: Some(2),
                    timeout_secs: 60,
                },
                |_, _, _, _| {
                    EncryptedCredentialEnvelope::new("merchant-video", [1; 24], vec![2; 32])
                        .map_err(|_| af_domain::MerchantPoolError::Unavailable)
                },
            )
            .await?;
        let pending = pools.submit_service(owner, service.id).await?;
        let service = pools
            .review_service(
                admin,
                service.id,
                review(pending.draft.as_ref().map(|revision| revision.revision)),
            )
            .await?;
        if index == 1 {
            let revision = service
                .published
                .as_ref()
                .expect("published merchant video revision");
            let binding = fixture
                .database
                .seed()
                .query_one(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "SELECT channel_id, credential_id FROM merchant_service_revisions WHERE id = ?",
                    [revision.id.into()],
                ))
                .await?
                .expect("reviewed binding");
            fixture.channel_id = ChannelId::new(binding.try_get("", "channel_id")?)?;
            fixture.credential_id = CredentialId::new(binding.try_get("", "credential_id")?)?;
            first_price = service.active_price.map(|price| price.id);
        }
        merchants.push(merchant.id);
    }
    let repository =
        af_db::UserRouteRepository::new(fixture.database.pool().clone(), Duration::from_secs(5))?;
    let route = repository
        .write_merchant_route(
            fixture.principal.user_id(),
            None,
            MerchantRouteWrite {
                name: "Merchant video route".to_owned(),
                pool_id: pool.id,
                strategy: UserRouteStrategy::Ordered,
                enabled: true,
                max_merchant_attempts: 2,
                max_price_ratio_micros: None,
                members: merchants
                    .iter()
                    .enumerate()
                    .map(|(index, merchant)| MerchantRouteMember {
                        merchant_id: *merchant,
                        priority: index as i32,
                        weight: 1,
                        last_selected_at_millis: None,
                    })
                    .collect(),
                excluded_merchant_ids: Vec::new(),
                allow_pool_fallback: false,
                accept_new_merchants: false,
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
    if with_cash {
        fixture.database.seed().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO topup_orders (order_key,user_id,provider,status,amount_minor,currency,quota_amount,idempotency_key,version,paid_at) VALUES (?,?,'stripe',3,2000,'USD',10000000,?,1,CURRENT_TIMESTAMP)",
            ["11".repeat(16).into(), fixture.principal.user_id().get().into(), "22".repeat(16).into()])).await?;
        fixture.database.seed().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO wallet_ledger_entries (event_key,user_id,entry_type,quota_delta,balance_before,balance_after) VALUES (?,?,3,10000000,0,10000000)",
            ["33".repeat(16).into(), fixture.principal.user_id().get().into()])).await?;
    }
    fixture.group_pricing =
        GroupPricingCache::load_from_database(fixture.database.pool().clone()).await?;
    fixture.model_prices =
        ModelPriceCache::load_from_database(fixture.database.pool().clone()).await?;
    assert_eq!(
        fixture
            .database
            .seed()
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM model_prices".to_owned()
            ))
            .await?
            .expect("model price count")
            .try_get::<i64>("", "count")?,
        0
    );
    let routes = UserRouteRuntime::new(repository)
        .with_merchant_pools(pools)
        .with_group_model_policy(af_db::GroupModelPolicyRepository::new(
            fixture.database.pool().clone(),
            Duration::from_secs(5),
        )?);
    Ok(MerchantVideoFixture {
        fixture,
        routes,
        pool_group,
        merchants: [merchants[0], merchants[1]],
        price_version: first_price.expect("active sales price"),
    })
}

fn merchant_video_request() -> CanonicalVideoGenerationRequest {
    CanonicalVideoGenerationRequest::new(
        VideoModel::new(UPSTREAM_MODEL).expect("official video model"),
        VideoPrompt::new("private merchant video prompt").expect("video prompt"),
        Some(VideoDuration::new(8).expect("requested duration")),
        None,
        Some(VideoResolution::P720),
    )
}

fn merchant_successful_submission(fixture: &Fixture, pool_group: GroupId) -> PersistentSubmission {
    let mut result = successful_submission(fixture);
    result.target_group_id = pool_group;
    result
}

async fn merchant_video_reservation(
    fixture: &Fixture,
) -> Result<sea_orm::QueryResult, Box<dyn Error>> {
    Ok(fixture.database.seed().query_one(Statement::from_sql_and_values(DbBackend::Sqlite,
        "SELECT r.reservation_kind,r.funding_source,r.status AS reservation_status,r.reserved_quota,r.actual_quota,m.status AS merchant_status,m.merchant_id,m.pricing_version,m.sales_ratio_micros,m.commission_bps,m.gross_quota,m.commission_quota,m.net_quota FROM billing_reservations r INNER JOIN merchant_billing_reservations m ON m.reservation_key=r.idempotency_key WHERE r.token_id = ?",
        [fixture.principal.token_id().get().into()])).await?.expect("merchant-backed batch reservation"))
}

#[tokio::test]
async fn merchant_video_cash_batch_settles_once_after_restart_and_route_disable()
-> Result<(), Box<dyn Error>> {
    let MerchantVideoFixture {
        fixture,
        routes,
        pool_group,
        merchants,
        price_version,
    } = merchant_video_fixture(true).await?;
    let runtime = Arc::new(
        MockRuntime::new(vec![Ok(merchant_successful_submission(
            &fixture, pool_group,
        ))])
        .with_poll(successful_poll(8)),
    );
    let coordinator = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes.clone());
    let request_id = async_request_id(0xc1);
    let task = coordinator
        .submit(
            async_task_id(0xc1),
            request_id,
            fixture.principal,
            merchant_video_request(),
            "merchant-video-submit",
        )
        .await?;
    assert_eq!(
        *runtime.submitted_groups.lock().unwrap(),
        vec![(pool_group, pool_group)]
    );
    let claim = fixture
        .submissions
        .find_by_request(fixture.principal.user_id(), request_id)
        .await?
        .expect("accepted merchant claim");
    assert_eq!(claim.principal().group_id(), fixture.principal.group_id());
    assert_ne!(claim.principal().group_id(), pool_group);
    assert_eq!(
        claim
            .principal()
            .merchant_billing_snapshot()
            .expect("merchant terms")
            .pricing_version(),
        price_version
    );
    let reserved = merchant_video_reservation(&fixture).await?;
    assert_eq!(reserved.try_get::<i16>("", "reservation_kind")?, 2);
    assert_eq!(reserved.try_get::<i16>("", "funding_source")?, 1);
    assert_eq!(
        reserved.try_get::<String>("", "merchant_status")?,
        "reserved"
    );
    assert_eq!(reserved.try_get::<i64>("", "merchant_id")?, merchants[0]);
    assert_eq!(
        reserved.try_get::<i64>("", "pricing_version")?,
        price_version
    );
    assert_eq!(reserved.try_get::<i64>("", "sales_ratio_micros")?, 500_000);
    assert_eq!(reserved.try_get::<i64>("", "commission_bps")?, 500);
    assert!(reserved.try_get::<i64>("", "reserved_quota")? >= 280_000);
    fixture
        .database
        .seed()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE user_routes SET enabled=false WHERE id=?",
            [fixture.principal.user_route_id().expect("route").into()],
        ))
        .await?;
    drop(coordinator);
    let restarted = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes);
    let replay = restarted
        .submit(
            async_task_id(0xc2),
            request_id,
            fixture.principal,
            merchant_video_request(),
            "merchant-video-replay",
        )
        .await?;
    assert_eq!(replay.task_id(), task.task_id());
    assert_eq!(runtime.submission_count(), 1);
    let polled = restarted
        .poll(
            fixture.principal.user_id(),
            task.task_id(),
            "merchant-video-poll",
        )
        .await?;
    assert!(
        matches!(polled, PersistentVideoTaskPollOutcome::Updated { task, .. } if matches!(task.status(), TaskStatus::Succeeded))
    );
    assert_eq!(
        *runtime.polled_bindings.lock().unwrap(),
        vec![(fixture.channel_id, fixture.credential_id)]
    );
    assert_eq!(*runtime.polled_groups.lock().unwrap(), vec![pool_group]);
    let settled = merchant_video_reservation(&fixture).await?;
    assert_eq!(settled.try_get::<i16>("", "reservation_status")?, 3);
    assert_eq!(settled.try_get::<String>("", "merchant_status")?, "settled");
    assert_eq!(settled.try_get::<i64>("", "actual_quota")?, 280_000);
    assert_eq!(settled.try_get::<i64>("", "gross_quota")?, 280_000);
    assert_eq!(settled.try_get::<i64>("", "commission_quota")?, 14_000);
    assert_eq!(settled.try_get::<i64>("", "net_quota")?, 266_000);
    let finance =
        MerchantFinanceRepository::new(fixture.database.pool().clone(), Duration::from_secs(5));
    let balance = finance.balance(merchants[0]).await?;
    assert_eq!(balance.total_gross_quota, 280_000);
    assert_eq!(balance.total_commission_quota, 14_000);
    assert_eq!(balance.pending_quota, 266_000);
    assert_eq!(finance.balance(merchants[1]).await?.total_gross_quota, 0);
    assert!(matches!(
        restarted
            .poll(
                fixture.principal.user_id(),
                task.task_id(),
                "merchant-video-terminal-replay"
            )
            .await?,
        PersistentVideoTaskPollOutcome::Terminal(_)
    ));
    assert_eq!(runtime.poll_count(), 1);
    assert_eq!(
        finance.entries(merchants[0], 100, None).await?.items.len(),
        1
    );
    let user = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT quota,used_quota,frozen_quota FROM users WHERE id=?",
            [fixture.principal.user_id().get().into()],
        ))
        .await?
        .expect("consumer wallet");
    assert_eq!(user.try_get::<i64>("", "quota")?, 9_720_000);
    assert_eq!(user.try_get::<i64>("", "used_quota")?, 280_000);
    assert_eq!(user.try_get::<i64>("", "frozen_quota")?, 0);
    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn merchant_video_unknown_acceptance_keeps_original_merchant_and_never_resubmits()
-> Result<(), Box<dyn Error>> {
    let MerchantVideoFixture {
        fixture,
        routes,
        pool_group,
        merchants,
        ..
    } = merchant_video_fixture(true).await?;
    let runtime = Arc::new(MockRuntime::new(vec![
        Err((
            AfError::Upstream(af_domain::UpstreamError::ProtocolError),
            VideoTaskSubmissionDisposition::AcceptanceUnknown,
        )),
        Ok(merchant_successful_submission(&fixture, pool_group)),
    ]));
    let coordinator = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes.clone());
    let request_id = async_request_id(0xd1);
    assert!(matches!(
        coordinator
            .submit(
                async_task_id(0xd1),
                request_id,
                fixture.principal,
                merchant_video_request(),
                "merchant-video-unknown"
            )
            .await,
        Err(PersistentVideoTaskError::SubmissionOutcomeUnknown)
    ));
    drop(coordinator);
    let restarted = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes);
    assert!(matches!(
        restarted
            .submit(
                async_task_id(0xd2),
                request_id,
                fixture.principal,
                merchant_video_request(),
                "merchant-video-unknown-replay"
            )
            .await,
        Err(PersistentVideoTaskError::SubmissionOutcomeUnknown)
    ));
    assert_eq!(runtime.submission_count(), 1);
    let claim = fixture
        .submissions
        .find_by_request(fixture.principal.user_id(), request_id)
        .await?
        .expect("unknown acceptance claim");
    assert_eq!(claim.state(), AsyncTaskSubmissionState::Submitting);
    assert_eq!(
        claim
            .principal()
            .merchant_billing_snapshot()
            .expect("frozen terms")
            .merchant_id(),
        merchants[0]
    );
    assert_eq!(
        claim.principal().merchant_channel_id(),
        Some(fixture.channel_id)
    );
    let reservation = merchant_video_reservation(&fixture).await?;
    assert_eq!(reservation.try_get::<i16>("", "reservation_status")?, 1);
    assert_eq!(
        reservation.try_get::<String>("", "merchant_status")?,
        "reserved"
    );
    let finance =
        MerchantFinanceRepository::new(fixture.database.pool().clone(), Duration::from_secs(5));
    assert_eq!(finance.balance(merchants[0]).await?.total_gross_quota, 0);
    assert_eq!(finance.balance(merchants[1]).await?.total_gross_quota, 0);
    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn merchant_video_duration_above_request_refunds_cash_without_proceeds()
-> Result<(), Box<dyn Error>> {
    let MerchantVideoFixture {
        fixture,
        routes,
        pool_group,
        merchants,
        ..
    } = merchant_video_fixture(true).await?;
    let runtime = Arc::new(
        MockRuntime::new(vec![Ok(merchant_successful_submission(
            &fixture, pool_group,
        ))])
        .with_poll(successful_poll(15)),
    );
    let coordinator = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes);
    let task = coordinator
        .submit(
            async_task_id(0xe1),
            async_request_id(0xe1),
            fixture.principal,
            merchant_video_request(),
            "merchant-video-duration-submit",
        )
        .await?;
    let outcome = coordinator
        .poll(
            fixture.principal.user_id(),
            task.task_id(),
            "merchant-video-duration-poll",
        )
        .await?;
    assert!(
        matches!(outcome, PersistentVideoTaskPollOutcome::Updated { task, billing_dimensions: None, .. } if matches!(task.status(), TaskStatus::Failed { .. }))
    );
    let reservation = merchant_video_reservation(&fixture).await?;
    assert_eq!(reservation.try_get::<i16>("", "reservation_status")?, 4);
    assert_eq!(
        reservation.try_get::<String>("", "merchant_status")?,
        "canceled"
    );
    assert_eq!(reservation.try_get::<i64>("", "gross_quota")?, 0);
    assert_eq!(reservation.try_get::<i64>("", "commission_quota")?, 0);
    assert_eq!(reservation.try_get::<i64>("", "net_quota")?, 0);
    let finance =
        MerchantFinanceRepository::new(fixture.database.pool().clone(), Duration::from_secs(5));
    assert_eq!(finance.balance(merchants[0]).await?.total_gross_quota, 0);
    assert!(
        finance
            .entries(merchants[0], 100, None)
            .await?
            .items
            .is_empty()
    );
    let user = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT quota,used_quota,frozen_quota FROM users WHERE id=?",
            [fixture.principal.user_id().get().into()],
        ))
        .await?
        .expect("refunded consumer wallet");
    assert_eq!(user.try_get::<i64>("", "quota")?, 10_000_000);
    assert_eq!(user.try_get::<i64>("", "used_quota")?, 0);
    assert_eq!(user.try_get::<i64>("", "frozen_quota")?, 0);
    assert!(matches!(
        coordinator
            .poll(
                fixture.principal.user_id(),
                task.task_id(),
                "merchant-video-duration-terminal"
            )
            .await?,
        PersistentVideoTaskPollOutcome::Terminal(_)
    ));
    assert_eq!(runtime.poll_count(), 1);
    fixture.database.close().await;
    Ok(())
}

#[tokio::test]
async fn merchant_video_batch_cannot_reserve_promotional_wallet_balance()
-> Result<(), Box<dyn Error>> {
    let MerchantVideoFixture {
        fixture,
        routes,
        pool_group,
        ..
    } = merchant_video_fixture(false).await?;
    let runtime = Arc::new(MockRuntime::new(vec![Ok(merchant_successful_submission(
        &fixture, pool_group,
    ))]));
    let coordinator = fixture
        .coordinator(runtime.clone())
        .with_user_route_runtime(routes);
    assert!(matches!(
        coordinator
            .submit(
                async_task_id(0xf1),
                async_request_id(0xf1),
                fixture.principal,
                merchant_video_request(),
                "merchant-video-no-cash"
            )
            .await,
        Err(PersistentVideoTaskError::Billing)
    ));
    assert_eq!(runtime.submission_count(), 0);
    let count = fixture
        .database
        .seed()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM billing_reservations WHERE token_id=?",
            [fixture.principal.token_id().get().into()],
        ))
        .await?
        .expect("reservation count");
    assert_eq!(count.try_get::<i64>("", "count")?, 0);
    fixture.database.close().await;
    Ok(())
}
