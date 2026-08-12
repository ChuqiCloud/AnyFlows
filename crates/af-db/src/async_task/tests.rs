use std::{error::Error, time::Duration};

use af_domain::{
    AsyncTaskAttemptId, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskRequestFingerprint,
    AsyncTaskRequestId, BillingReservationId, ChannelId, CredentialId, GatewayPrincipal, GroupId,
    Protocol, Quota, TaskFailure, TaskFailureKind, TaskProgress, TaskState, TaskStatus, TokenId,
    UpstreamTaskId, UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ActiveModelTrait, Set, entity::prelude::Json};

use crate::{
    DatabaseOptions, MigrationOptions,
    entity::{
        EncryptedJson, HeaderOverrides, SensitiveJson, TokenHash, channels, credentials, groups,
        tokens, users,
    },
};

use super::{
    AsyncTaskBillingAccept, AsyncTaskBillingClear, AsyncTaskBillingMark,
    AsyncTaskBillingMutationOutcome, AsyncTaskBillingPlan, AsyncTaskBillingPlanOutcome,
    AsyncTaskBillingRecord, AsyncTaskBillingRepository, AsyncTaskBillingResolution,
    AsyncTaskBillingSettlement, AsyncTaskBillingState, AsyncTaskCreate, AsyncTaskCreateOutcome,
    AsyncTaskPageCursor, AsyncTaskRepository, AsyncTaskRepositoryError, AsyncTaskSubmissionAccept,
    AsyncTaskSubmissionBegin, AsyncTaskSubmissionClaim, AsyncTaskSubmissionClaimOutcome,
    AsyncTaskSubmissionMutationOutcome, AsyncTaskSubmissionRelease, AsyncTaskSubmissionRepository,
    AsyncTaskSubmissionState, AsyncTaskTransition, AsyncTaskTransitionOutcome,
    AsyncTaskVideoResolution,
};

const CREATED_AT: u64 = 1_800_000_000;
const UPDATED_AT: u64 = CREATED_AT + 30;

#[tokio::test]
async fn owner_page_is_newest_first_stable_and_protocol_scoped() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for (task_marker, request_marker, created_at) in [
        (0x11, 0x21, CREATED_AT),
        (0x12, 0x22, CREATED_AT + 1),
        (0x13, 0x23, CREATED_AT + 1),
    ] {
        fixture
            .repository
            .create(AsyncTaskCreate::new(
                task_id(task_marker),
                request_id(request_marker),
                fixture.first.principal,
                Protocol::XaiVideo,
                "gpt-video".to_owned(),
                "grok-video".to_owned(),
                fixture.channel_id,
                fixture.credential_id,
                u64::MAX - 7,
                UpstreamTaskId::new(format!("upstream-{task_marker}"))?,
                submitted(),
                created_at,
            )?)
            .await?;
    }
    fixture
        .repository
        .create(fixture.task(&fixture.second, 0x14, 0x24, "gpt-video", "grok-video")?)
        .await?;
    fixture
        .repository
        .create(AsyncTaskCreate::new(
            task_id(0x15),
            request_id(0x25),
            fixture.first.principal,
            Protocol::OpenAiChat,
            "gpt-chat".to_owned(),
            "gpt-chat-upstream".to_owned(),
            fixture.channel_id,
            fixture.credential_id,
            u64::MAX - 7,
            UpstreamTaskId::new("upstream-chat-task")?,
            submitted(),
            CREATED_AT + 2,
        )?)
        .await?;

    let first = fixture
        .repository
        .list(
            fixture.first.principal.user_id(),
            Protocol::XaiVideo,
            None,
            2,
        )
        .await?;
    let (tasks, cursor) = first.into_parts();
    assert_eq!(
        tasks.iter().map(|task| task.task_id()).collect::<Vec<_>>(),
        vec![task_id(0x13), task_id(0x12)]
    );
    let cursor = cursor.expect("仍有第三个 owner-scoped 任务时必须返回游标");
    assert_eq!(cursor.task_id(), task_id(0x12));

    let second = fixture
        .repository
        .list(
            fixture.first.principal.user_id(),
            Protocol::XaiVideo,
            Some(cursor),
            2,
        )
        .await?;
    let (tasks, cursor) = second.into_parts();
    assert_eq!(
        tasks.iter().map(|task| task.task_id()).collect::<Vec<_>>(),
        vec![task_id(0x11)]
    );
    assert!(cursor.is_none());

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn owner_page_rejects_invalid_limits_and_cursor_positions() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for limit in [0, super::repository::MAX_ASYNC_TASK_PAGE_SIZE + 1] {
        assert!(matches!(
            fixture
                .repository
                .list(
                    fixture.first.principal.user_id(),
                    Protocol::XaiVideo,
                    None,
                    limit,
                )
                .await,
            Err(AsyncTaskRepositoryError::Invariant)
        ));
    }
    let forged = AsyncTaskPageCursor::new(task_id(0xfe));
    let (tasks, cursor) = fixture
        .repository
        .list(
            fixture.first.principal.user_id(),
            Protocol::XaiVideo,
            Some(forged),
            20,
        )
        .await?
        .into_parts();
    assert!(tasks.is_empty());
    assert!(cursor.is_none());

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn create_is_idempotent_per_owner_and_rejects_payload_conflicts() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let first_write = fixture.task(&fixture.first, 0x11, 0x21, "gpt-video", "grok-video")?;
    let first = fixture.repository.create(first_write).await?;
    let AsyncTaskCreateOutcome::Created(first) = first else {
        panic!("首次写入必须创建任务")
    };
    assert_eq!(first.version(), 1);
    assert_eq!(first.status().state(), TaskState::Submitted);

    let replay = fixture
        .repository
        .create(fixture.task(&fixture.first, 0x11, 0x21, "gpt-video", "grok-video")?)
        .await?;
    assert!(matches!(replay, AsyncTaskCreateOutcome::Existing(_)));

    let conflict = fixture
        .repository
        .create(fixture.task(
            &fixture.first,
            0x11,
            0x21,
            "gpt-video",
            "different-upstream-model",
        )?)
        .await;
    assert!(matches!(conflict, Err(AsyncTaskRepositoryError::Conflict)));

    let other_owner = fixture
        .repository
        .create(fixture.task(&fixture.second, 0x12, 0x21, "gpt-video", "grok-video")?)
        .await?;
    assert!(matches!(other_owner, AsyncTaskCreateOutcome::Created(_)));
    assert!(
        fixture
            .repository
            .find(fixture.second.principal.user_id(), task_id(0x11))
            .await?
            .is_none()
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn create_rejects_mismatched_owner_and_binding_references() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let wrong_owner = GatewayPrincipal::new(
        fixture.first.principal.token_id(),
        fixture.second.principal.user_id(),
        fixture.first.principal.group_id(),
    );
    let write = AsyncTaskCreate::new(
        task_id(0x31),
        request_id(0x41),
        wrong_owner,
        Protocol::XaiVideo,
        "gpt-video".to_owned(),
        "grok-video".to_owned(),
        fixture.channel_id,
        fixture.credential_id,
        u64::MAX - 7,
        UpstreamTaskId::new("upstream-owner-mismatch")?,
        submitted(),
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create(write).await?,
        AsyncTaskCreateOutcome::NotFound
    ));

    let missing_credential = AsyncTaskCreate::new(
        task_id(0x32),
        request_id(0x42),
        fixture.first.principal,
        Protocol::XaiVideo,
        "gpt-video".to_owned(),
        "grok-video".to_owned(),
        fixture.channel_id,
        CredentialId::new(i64::MAX)?,
        u64::MAX - 7,
        UpstreamTaskId::new("upstream-binding-mismatch")?,
        submitted(),
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create(missing_credential).await?,
        AsyncTaskCreateOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn state_machine_applies_only_declared_forward_transitions() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .create(fixture.task(&fixture.first, 0x51, 0x61, "gpt-video", "grok-video")?)
        .await?;

    let queued = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            1,
            TaskStatus::Queued {
                progress: TaskProgress::new(100)?,
            },
            UPDATED_AT,
        )?)
        .await?;
    let AsyncTaskTransitionOutcome::Applied(queued) = queued else {
        panic!("submitted 必须允许推进到 queued")
    };
    assert_eq!(queued.version(), 2);

    let running = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            2,
            TaskStatus::Running {
                progress: TaskProgress::new(5_000)?,
            },
            UPDATED_AT + 1,
        )?)
        .await?;
    assert!(matches!(running, AsyncTaskTransitionOutcome::Applied(_)));

    let succeeded = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            3,
            TaskStatus::Succeeded,
            UPDATED_AT + 2,
        )?)
        .await?;
    let AsyncTaskTransitionOutcome::Applied(succeeded) = succeeded else {
        panic!("running 必须允许推进到 succeeded")
    };
    assert_eq!(succeeded.version(), 4);
    assert_eq!(succeeded.terminal_at(), Some(UPDATED_AT + 2));

    let replay = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            3,
            TaskStatus::Succeeded,
            UPDATED_AT + 3,
        )?)
        .await?;
    assert!(matches!(replay, AsyncTaskTransitionOutcome::Existing(_)));

    let rollback = fixture
        .repository
        .transition(transition(
            0x51,
            fixture.first.principal.user_id(),
            4,
            TaskStatus::Running {
                progress: TaskProgress::new(9_000)?,
            },
            UPDATED_AT + 4,
        )?)
        .await;
    assert!(matches!(rollback, Err(AsyncTaskRepositoryError::Conflict)));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn state_machine_rejects_version_conflicts_and_normalizes_terminal_failure_replays()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .create(fixture.task(&fixture.first, 0x71, 0x81, "gpt-video", "grok-video")?)
        .await?;

    let wrong_version = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            2,
            TaskStatus::Running {
                progress: TaskProgress::new(500)?,
            },
            UPDATED_AT,
        )?)
        .await;
    assert!(matches!(
        wrong_version,
        Err(AsyncTaskRepositoryError::Conflict)
    ));

    let running = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            1,
            TaskStatus::Running {
                progress: TaskProgress::new(500)?,
            },
            UPDATED_AT,
        )?)
        .await?;
    assert!(matches!(running, AsyncTaskTransitionOutcome::Applied(_)));

    let same_version_different_state = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            1,
            failed("first failure reason")?,
            UPDATED_AT + 1,
        )?)
        .await;
    assert!(matches!(
        same_version_different_state,
        Err(AsyncTaskRepositoryError::Conflict)
    ));

    let failed_outcome = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            2,
            failed("first failure reason")?,
            UPDATED_AT + 2,
        )?)
        .await?;
    assert!(matches!(
        failed_outcome,
        AsyncTaskTransitionOutcome::Applied(_)
    ));

    let replay = fixture
        .repository
        .transition(transition(
            0x71,
            fixture.first.principal.user_id(),
            2,
            failed("different redacted reason")?,
            UPDATED_AT + 3,
        )?)
        .await?;
    assert!(matches!(replay, AsyncTaskTransitionOutcome::Existing(_)));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn public_debug_output_hides_models_and_upstream_task_identifiers()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let write = fixture.task(
        &fixture.first,
        0x91,
        0xa1,
        "private-requested-model",
        "private-upstream-model",
    )?;
    let write_debug = format!("{write:?}");
    assert!(!write_debug.contains("private-requested-model"));
    assert!(!write_debug.contains("private-upstream-model"));
    assert!(!write_debug.contains("upstream-task-secret"));

    let AsyncTaskCreateOutcome::Created(record) = fixture.repository.create(write).await? else {
        panic!("首次写入必须创建任务")
    };
    let record_debug = format!("{record:?}");
    assert!(!record_debug.contains(record.requested_model()));
    assert!(!record_debug.contains(record.upstream_model()));
    assert!(!record_debug.contains(record.upstream_task_id().as_str()));
    assert_eq!(record.credential_revision(), u64::MAX - 7);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn submission_claim_serializes_attempts_and_recovers_accepted_binding()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = fixture
        .submissions
        .claim(submission_claim(&fixture, 0xb1, 0xc1, 0xd1)?)
        .await?;
    let AsyncTaskSubmissionClaimOutcome::Created(first) = first else {
        panic!("首次提交必须创建 claim")
    };
    assert_eq!(first.state(), AsyncTaskSubmissionState::Claimed);

    let replay = fixture
        .submissions
        .claim(submission_claim(&fixture, 0xb2, 0xc1, 0xd1)?)
        .await?;
    let AsyncTaskSubmissionClaimOutcome::Existing(replay) = replay else {
        panic!("相同幂等请求必须复用原 claim")
    };
    assert_eq!(replay.task_id(), first.task_id());
    assert!(matches!(
        fixture
            .submissions
            .claim(submission_claim(&fixture, 0xb3, 0xc1, 0xd2)?)
            .await,
        Err(AsyncTaskRepositoryError::Conflict)
    ));

    let first_attempt = AsyncTaskAttemptId::new([0xe1; 16])?;
    let begun = fixture
        .submissions
        .begin(AsyncTaskSubmissionBegin::new(
            first.task_id(),
            first.principal().user_id(),
            first.version(),
            first_attempt,
            UPDATED_AT,
        )?)
        .await?;
    let AsyncTaskSubmissionMutationOutcome::Applied(begun) = begun else {
        panic!("claim 必须被首个尝试所有者领取")
    };
    assert_eq!(begun.state(), AsyncTaskSubmissionState::Submitting);
    assert!(matches!(
        fixture
            .submissions
            .begin(AsyncTaskSubmissionBegin::new(
                begun.task_id(),
                begun.principal().user_id(),
                begun.version(),
                AsyncTaskAttemptId::new([0xe2; 16])?,
                UPDATED_AT + 1,
            )?)
            .await,
        Err(AsyncTaskRepositoryError::Conflict)
    ));

    let released = fixture
        .submissions
        .release(AsyncTaskSubmissionRelease::new(
            begun.task_id(),
            begun.principal().user_id(),
            begun.version(),
            first_attempt,
            UPDATED_AT + 1,
        )?)
        .await?;
    let AsyncTaskSubmissionMutationOutcome::Applied(released) = released else {
        panic!("确定未提交后必须释放 claim")
    };
    let second_attempt = AsyncTaskAttemptId::new([0xe2; 16])?;
    let begun = fixture
        .submissions
        .begin(AsyncTaskSubmissionBegin::new(
            released.task_id(),
            released.principal().user_id(),
            released.version(),
            second_attempt,
            UPDATED_AT + 2,
        )?)
        .await?;
    let AsyncTaskSubmissionMutationOutcome::Applied(begun) = begun else {
        panic!("释放后的 claim 必须允许新尝试领取")
    };

    let accept = AsyncTaskSubmissionAccept::new(
        begun.task_id(),
        begun.principal().user_id(),
        begun.version(),
        second_attempt,
        begun.principal().group_id(),
        "grok-video".to_owned(),
        fixture.channel_id,
        fixture.credential_id,
        u64::MAX - 7,
        UpstreamTaskId::new("upstream-task-accepted")?,
        AsyncTaskBindingFingerprint::new([0xf1; 32]),
        Duration::from_secs(5),
        Some(AsyncTaskVideoResolution::P720),
        submitted(),
        UPDATED_AT + 3,
    )?;
    let accepted = fixture.submissions.accept(accept).await?;
    let AsyncTaskSubmissionMutationOutcome::Applied(accepted) = accepted else {
        panic!("上游接受事实必须持久化")
    };
    assert_eq!(accepted.state(), AsyncTaskSubmissionState::Accepted);
    assert_eq!(
        accepted.video_resolution(),
        Some(AsyncTaskVideoResolution::P720)
    );
    let task_create = accepted
        .task_create()?
        .expect("已接受 claim 必须可恢复任务写入");
    let task = fixture.repository.create(task_create).await?;
    let AsyncTaskCreateOutcome::Created(task) = task else {
        panic!("恢复绑定必须创建最终任务")
    };
    assert!(accepted.matches_task(&task));
    assert_eq!(
        fixture
            .repository
            .find_by_request(task.principal().user_id(), accepted.request_id())
            .await?
            .expect("必须可按幂等键读取最终任务")
            .task_id(),
        task.task_id()
    );
    let debug = format!("{accepted:?}");
    for secret in ["grok-video", "upstream-task-accepted", &"f1".repeat(32)] {
        assert!(!debug.contains(secret));
    }

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn state_machine_allows_only_monotonic_progress_within_active_state()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .create(fixture.task(&fixture.first, 0xd1, 0xe1, "gpt-video", "grok-video")?)
        .await?;
    let progressed = fixture
        .repository
        .transition(transition(
            0xd1,
            fixture.first.principal.user_id(),
            1,
            TaskStatus::Submitted {
                progress: TaskProgress::new(250)?,
            },
            UPDATED_AT,
        )?)
        .await?;
    let AsyncTaskTransitionOutcome::Applied(progressed) = progressed else {
        panic!("同状态进度增长必须写入")
    };
    assert_eq!(progressed.status().progress().basis_points(), 250);
    assert!(matches!(
        fixture
            .repository
            .transition(transition(
                0xd1,
                fixture.first.principal.user_id(),
                progressed.version(),
                TaskStatus::Submitted {
                    progress: TaskProgress::new(249)?,
                },
                UPDATED_AT + 1,
            )?)
            .await,
        Err(AsyncTaskRepositoryError::Conflict)
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn billing_state_machine_replays_same_facts_and_clears_released_plans()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    assert!(matches!(
        fixture
            .submissions
            .claim(submission_claim(&fixture, 0xe1, 0xe1, 0xe1)?)
            .await?,
        AsyncTaskSubmissionClaimOutcome::Created(_)
    ));
    let reservation_id = BillingReservationId::new([0x51; 16])?;
    let plan = || {
        AsyncTaskBillingPlan::new(
            task_id(0xe1),
            fixture.first.principal.user_id(),
            reservation_id,
            fixture.first.principal.group_id(),
            1,
            AsyncTaskBillingResolution::P720,
            [1_000_000; 3],
            Quota::new(1_050_000).expect("测试冻结上界必须有效"),
            CREATED_AT,
        )
    };
    let AsyncTaskBillingPlanOutcome::Created(planned) = fixture.billings.plan(plan()?).await?
    else {
        panic!("首次计费计划必须创建")
    };
    assert_eq!(planned.state(), AsyncTaskBillingState::Planned);
    assert!(matches!(
        fixture.billings.plan(plan()?).await?,
        AsyncTaskBillingPlanOutcome::Existing(_)
    ));

    let reserved = billing_record(
        fixture
            .billings
            .mark_reserved(AsyncTaskBillingMark::new(
                planned.task_id(),
                planned.user_id(),
                planned.version(),
                CREATED_AT + 1,
            )?)
            .await?,
    )?;
    assert_eq!(reserved.state(), AsyncTaskBillingState::Reserved);
    assert!(matches!(
        fixture
            .billings
            .mark_reserved(AsyncTaskBillingMark::new(
                planned.task_id(),
                planned.user_id(),
                planned.version(),
                CREATED_AT + 1,
            )?)
            .await?,
        AsyncTaskBillingMutationOutcome::Existing(_)
    ));

    let submitted = billing_record(
        fixture
            .billings
            .accept(AsyncTaskBillingAccept::new(
                reserved.task_id(),
                reserved.user_id(),
                reserved.version(),
                140_000,
                Quota::new(560_000)?,
                CREATED_AT + 2,
            )?)
            .await?,
    )?;
    let pending = billing_record(
        fixture
            .billings
            .begin_settlement(AsyncTaskBillingSettlement::new(
                submitted.task_id(),
                submitted.user_id(),
                submitted.version(),
                Quota::new(560_000)?,
                8,
                CREATED_AT + 3,
            )?)
            .await?,
    )?;
    assert_eq!(pending.state(), AsyncTaskBillingState::SettlementPending);
    assert_eq!(pending.actual_duration_seconds(), Some(8));
    let settled = billing_record(
        fixture
            .billings
            .mark_settled(AsyncTaskBillingMark::new(
                pending.task_id(),
                pending.user_id(),
                pending.version(),
                CREATED_AT + 4,
            )?)
            .await?,
    )?;
    assert_eq!(settled.state(), AsyncTaskBillingState::Settled);

    assert!(matches!(
        fixture
            .submissions
            .claim(submission_claim(&fixture, 0xe2, 0xe2, 0xe2)?)
            .await?,
        AsyncTaskSubmissionClaimOutcome::Created(_)
    ));
    let release_plan = AsyncTaskBillingPlan::new(
        task_id(0xe2),
        fixture.first.principal.user_id(),
        BillingReservationId::new([0x52; 16])?,
        fixture.first.principal.group_id(),
        1,
        AsyncTaskBillingResolution::P480,
        [1_000_000; 3],
        Quota::new(600_000)?,
        CREATED_AT,
    )?;
    let AsyncTaskBillingPlanOutcome::Created(release_planned) =
        fixture.billings.plan(release_plan).await?
    else {
        panic!("释放路径计费计划必须创建")
    };
    let release_reserved = billing_record(
        fixture
            .billings
            .mark_reserved(AsyncTaskBillingMark::new(
                release_planned.task_id(),
                release_planned.user_id(),
                release_planned.version(),
                CREATED_AT + 1,
            )?)
            .await?,
    )?;
    let release_pending = billing_record(
        fixture
            .billings
            .begin_release(AsyncTaskBillingMark::new(
                release_reserved.task_id(),
                release_reserved.user_id(),
                release_reserved.version(),
                CREATED_AT + 2,
            )?)
            .await?,
    )?;
    let released = billing_record(
        fixture
            .billings
            .mark_released(AsyncTaskBillingMark::new(
                release_pending.task_id(),
                release_pending.user_id(),
                release_pending.version(),
                CREATED_AT + 3,
            )?)
            .await?,
    )?;
    assert!(matches!(
        fixture
            .billings
            .clear_released(AsyncTaskBillingClear::new(
                released.task_id(),
                released.user_id(),
                released.version(),
                released.reservation_id(),
                CREATED_AT + 4,
            )?)
            .await?,
        AsyncTaskBillingMutationOutcome::Applied(_)
    ));
    assert!(
        fixture
            .billings
            .find(fixture.first.principal.user_id(), task_id(0xe2))
            .await?
            .is_none()
    );

    fixture.pool.close().await?;
    Ok(())
}

fn billing_record(
    outcome: AsyncTaskBillingMutationOutcome,
) -> Result<AsyncTaskBillingRecord, AsyncTaskRepositoryError> {
    match outcome {
        AsyncTaskBillingMutationOutcome::Applied(record)
        | AsyncTaskBillingMutationOutcome::Existing(record) => Ok(record),
        AsyncTaskBillingMutationOutcome::NotFound => Err(AsyncTaskRepositoryError::Invariant),
    }
}

struct Owner {
    principal: GatewayPrincipal,
}

struct Fixture {
    pool: crate::DatabasePool,
    repository: AsyncTaskRepository,
    submissions: AsyncTaskSubmissionRepository,
    billings: AsyncTaskBillingRepository,
    first: Owner,
    second: Owner,
    channel_id: ChannelId,
    credential_id: CredentialId,
}

impl Fixture {
    fn task(
        &self,
        owner: &Owner,
        task_marker: u8,
        request_marker: u8,
        requested_model: &str,
        upstream_model: &str,
    ) -> Result<AsyncTaskCreate, Box<dyn Error>> {
        Ok(AsyncTaskCreate::new(
            task_id(task_marker),
            request_id(request_marker),
            owner.principal,
            Protocol::XaiVideo,
            requested_model.to_owned(),
            upstream_model.to_owned(),
            self.channel_id,
            self.credential_id,
            u64::MAX - 7,
            UpstreamTaskId::new("upstream-task-secret")?,
            submitted(),
            CREATED_AT,
        )?)
    }
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let first = insert_owner(&pool, "first", 0x11).await?;
    let second = insert_owner(&pool, "second", 0x22).await?;
    let channel = channels::ActiveModel {
        name: Set("异步任务测试渠道".to_owned()),
        r#type: Set("xai".to_owned()),
        protocol: Set("xai_video".to_owned()),
        model_mapping: Set(Json::Object(Default::default())),
        param_override: Set(Json::Object(Default::default())),
        header_override: Set(HeaderOverrides::validate(Json::Object(Default::default()))?),
        settings: Set(SensitiveJson::from(Json::Object(Default::default()))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let credential = credentials::ActiveModel {
        channel_id: Set(channel.id),
        kind: Set("api_key".to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope(
            "async-task",
        ))?),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        repository: AsyncTaskRepository::new(pool.clone(), Duration::from_secs(5))?,
        submissions: AsyncTaskSubmissionRepository::new(pool.clone(), Duration::from_secs(5))?,
        billings: AsyncTaskBillingRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        first,
        second,
        channel_id: ChannelId::new(channel.id)?,
        credential_id: CredentialId::new(credential.id)?,
    })
}

fn submission_claim(
    fixture: &Fixture,
    task_marker: u8,
    request_marker: u8,
    fingerprint_marker: u8,
) -> Result<AsyncTaskSubmissionClaim, crate::AsyncTaskInputError> {
    AsyncTaskSubmissionClaim::new(
        task_id(task_marker),
        request_id(request_marker),
        fixture.first.principal,
        Protocol::XaiVideo,
        "gpt-video".to_owned(),
        AsyncTaskRequestFingerprint::new([fingerprint_marker; 32]),
        None,
        CREATED_AT,
    )
}

async fn insert_owner(
    pool: &crate::DatabasePool,
    marker: &str,
    hash_marker: u8,
) -> Result<Owner, Box<dyn Error>> {
    let group = groups::ActiveModel {
        name: Set(format!("async-task-{marker}")),
        display_name: Set(format!("异步任务测试-{marker}")),
        flags: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set(format!("async-task-{marker}")),
        default_group_id: Set(group.id),
        aff_code: Set(format!("async-task-aff-{marker}")),
        settings: Set(Json::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&format!("{hash_marker:02x}").repeat(32))?),
        key_prefix: Set(format!("sk-{marker}")),
        name: Set(format!("异步任务令牌-{marker}")),
        group_id: Set(Some(group.id)),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Owner {
        principal: GatewayPrincipal::new(
            TokenId::new(token.id)?,
            UserId::new(user.id)?,
            GroupId::new(group.id)?,
        ),
    })
}

fn transition(
    marker: u8,
    user_id: UserId,
    version: u64,
    status: TaskStatus,
    observed_at: u64,
) -> Result<AsyncTaskTransition, crate::AsyncTaskInputError> {
    AsyncTaskTransition::new(task_id(marker), user_id, version, status, observed_at)
}

fn submitted() -> TaskStatus {
    TaskStatus::Submitted {
        progress: TaskProgress::ZERO,
    }
}

fn failed(reason: &str) -> Result<TaskStatus, af_domain::TaskFailureError> {
    Ok(TaskStatus::Failed {
        failure: TaskFailure::with_reason(TaskFailureKind::Upstream, reason)?,
    })
}

fn task_id(marker: u8) -> AsyncTaskId {
    AsyncTaskId::new([marker; 16]).expect("测试任务标识必须非零")
}

fn request_id(marker: u8) -> AsyncTaskRequestId {
    AsyncTaskRequestId::new([marker; 16]).expect("测试幂等标识必须非零")
}

fn encrypted_envelope(marker: &str) -> Json {
    Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            ("key_id".to_owned(), Json::String(marker.to_owned())),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0x42; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0xa5; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}
