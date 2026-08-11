use std::{
    collections::VecDeque,
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_db::{AsyncTaskRepository, AsyncTaskSubmissionRepository};
use af_domain::{
    AfError, AsyncTaskBindingFingerprint, AsyncTaskId, AsyncTaskRequestId, ChannelId, CredentialId,
    GatewayPrincipal, GroupId, TaskProgress, TaskStatus, TokenId, UpstreamTaskId,
};
use af_protocol::{
    CanonicalTaskPoll, CanonicalVideoGenerationRequest, VideoDuration, VideoModel, VideoPrompt,
    VideoResolution,
};
use af_relay::VideoTaskSubmissionDisposition;
use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::test_database::SqliteTestDatabase;

use super::*;

const REQUESTED_MODEL: &str = "video-public";
const UPSTREAM_MODEL: &str = "grok-imagine-video-1.5";

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
    let coordinator = PersistentVideoTaskCoordinator::with_runtime(
        runtime.clone(),
        fixture.tasks.clone(),
        fixture.submissions.clone(),
    );
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
    let PersistentVideoTaskPollOutcome::Updated { task, poll } = polled else {
        panic!("活动任务必须访问轮询端口")
    };
    assert_eq!(task.status().progress().basis_points(), 2_500);
    assert_eq!(poll.status(), task.status());
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
    let coordinator = PersistentVideoTaskCoordinator::with_runtime(
        runtime.clone(),
        fixture.tasks.clone(),
        fixture.submissions.clone(),
    );
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

struct MockRuntime {
    submissions:
        Mutex<VecDeque<Result<PersistentSubmission, (AfError, VideoTaskSubmissionDisposition)>>>,
    submission_count: AtomicUsize,
    poll_count: AtomicUsize,
}

impl MockRuntime {
    fn new(
        submissions: Vec<Result<PersistentSubmission, (AfError, VideoTaskSubmissionDisposition)>>,
    ) -> Self {
        Self {
            submissions: Mutex::new(submissions.into()),
            submission_count: AtomicUsize::new(0),
            poll_count: AtomicUsize::new(0),
        }
    }

    fn submission_count(&self) -> usize {
        self.submission_count.load(Ordering::SeqCst)
    }

    fn poll_count(&self) -> usize {
        self.poll_count.load(Ordering::SeqCst)
    }
}

impl PersistentVideoTaskRuntime for MockRuntime {
    fn submit<'a>(
        &'a self,
        _group_id: GroupId,
        _request: CanonicalVideoGenerationRequest,
        _request_id: &'a str,
    ) -> PersistentSubmissionFuture<'a> {
        self.submission_count.fetch_add(1, Ordering::SeqCst);
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
        _claim: &'a AsyncTaskSubmissionRecord,
        _request_id: &'a str,
    ) -> PersistentPollFuture<'a> {
        self.poll_count.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            CanonicalTaskPoll::new(
                TaskStatus::Running {
                    progress: TaskProgress::new(2_500).expect("测试进度必须有效"),
                },
                None,
            )
            .map_err(|_| AfError::Internal)
        })
    }
}

struct Fixture {
    database: SqliteTestDatabase,
    tasks: AsyncTaskRepository,
    submissions: AsyncTaskSubmissionRepository,
    principal: GatewayPrincipal,
    channel_id: ChannelId,
    credential_id: CredentialId,
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
            "INSERT INTO users (username, default_group_id, aff_code, settings) VALUES ('video-persistence', {group_id}, 'video-persistence-aff', '{{}}')"
        ),
    )
    .await?;
    let token_id = insert(
        database.seed(),
        format!(
            "INSERT INTO tokens (user_id, key_hash, key_prefix, name, group_id) VALUES ({user_id}, '{}', 'sk-video-persistence', '视频持久化令牌', {group_id})",
            "ab".repeat(32),
        ),
    )
    .await?;
    let channel_id = insert(
        database.seed(),
        "INSERT INTO channels (name, \"type\", protocol, model_mapping, param_override, header_override, settings) VALUES ('视频持久化渠道', 'xai', 'xai_video', '{}', '{}', '{}', '{}')",
    )
    .await?;
    let credential_id = insert(
        database.seed(),
        format!(
            "INSERT INTO credentials (channel_id, kind, secret) VALUES ({channel_id}, 'api_key', '{{}}')"
        ),
    )
    .await?;
    Ok(Fixture {
        tasks: AsyncTaskRepository::new(database.pool().clone(), Duration::from_secs(5))?,
        submissions: AsyncTaskSubmissionRepository::new(
            database.pool().clone(),
            Duration::from_secs(5),
        )?,
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
    Ok(i64::try_from(result.last_insert_id()).expect("SQLite 测试标识必须适配 i64"))
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

fn async_task_id(marker: u8) -> AsyncTaskId {
    AsyncTaskId::new([marker; 16]).expect("测试任务标识必须非零")
}

fn async_request_id(marker: u8) -> AsyncTaskRequestId {
    AsyncTaskRequestId::new([marker; 16]).expect("测试幂等标识必须非零")
}
