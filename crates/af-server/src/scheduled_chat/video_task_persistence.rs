use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    AsyncTaskCreateOutcome, AsyncTaskInputError, AsyncTaskRecord, AsyncTaskRepository,
    AsyncTaskRepositoryError, AsyncTaskSubmissionAccept, AsyncTaskSubmissionBegin,
    AsyncTaskSubmissionClaim, AsyncTaskSubmissionClaimOutcome, AsyncTaskSubmissionMutationOutcome,
    AsyncTaskSubmissionRecord, AsyncTaskSubmissionRelease, AsyncTaskSubmissionRepository,
    AsyncTaskSubmissionState, AsyncTaskTransition, AsyncTaskTransitionOutcome,
};
use af_domain::{
    AfError, AsyncTaskAttemptId, AsyncTaskBindingFingerprint, AsyncTaskId,
    AsyncTaskRequestFingerprint, AsyncTaskRequestId, ChannelId, CredentialId, GatewayPrincipal,
    GroupId, Protocol, TaskSubmission, UserId,
};
use af_protocol::{CanonicalTaskPoll, CanonicalVideoGenerationRequest};
use af_relay::VideoTaskSubmissionDisposition;
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use super::{
    BoundVideoTaskSubmission, ScheduledChatService, VideoTaskRuntime,
    VideoTaskSubmissionRuntimeError,
};

type PersistentSubmissionFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PersistentSubmission, VideoTaskSubmissionRuntimeError>>
            + Send
            + 'a,
    >,
>;
type PersistentPollFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CanonicalTaskPoll, AfError>> + Send + 'a>>;

/// 持久化协调器实际需要的最小视频运行时端口。
trait PersistentVideoTaskRuntime: Send + Sync {
    fn submit<'a>(
        &'a self,
        group_id: GroupId,
        request: CanonicalVideoGenerationRequest,
        request_id: &'a str,
    ) -> PersistentSubmissionFuture<'a>;

    fn poll<'a>(
        &'a self,
        task: &'a AsyncTaskRecord,
        claim: &'a AsyncTaskSubmissionRecord,
        request_id: &'a str,
    ) -> PersistentPollFuture<'a>;
}

impl PersistentVideoTaskRuntime for ScheduledChatService {
    fn submit<'a>(
        &'a self,
        group_id: GroupId,
        request: CanonicalVideoGenerationRequest,
        request_id: &'a str,
    ) -> PersistentSubmissionFuture<'a> {
        Box::pin(async move {
            self.submit_video_task(group_id, request, request_id)
                .await
                .map(PersistentSubmission::from_bound)
        })
    }

    fn poll<'a>(
        &'a self,
        task: &'a AsyncTaskRecord,
        claim: &'a AsyncTaskSubmissionRecord,
        request_id: &'a str,
    ) -> PersistentPollFuture<'a> {
        Box::pin(async move {
            let binding = self.restore_video_task_binding(task, claim)?;
            self.poll_video_task(&binding, request_id).await
        })
    }
}

struct PersistentSubmission {
    target_group_id: GroupId,
    submission: TaskSubmission,
    upstream_model: String,
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_revision: u64,
    binding_fingerprint: AsyncTaskBindingFingerprint,
    attempt_timeout: std::time::Duration,
}

impl PersistentSubmission {
    fn from_bound(bound: BoundVideoTaskSubmission) -> Self {
        let (target_group_id, submission, binding) = bound.into_parts();
        Self {
            target_group_id,
            upstream_model: binding.expected_model().as_str().to_owned(),
            channel_id: binding.channel_id(),
            credential_id: binding.credential_id(),
            credential_revision: binding.credential_revision(),
            binding_fingerprint: binding.binding_fingerprint(),
            attempt_timeout: binding.attempt_timeout(),
            submission,
        }
    }
}

/// 持久化视频任务轮询的闭合结果。
pub(crate) enum PersistentVideoTaskPollOutcome {
    /// 数据库已经保存终态，本次没有再次访问上游。
    Terminal(AsyncTaskRecord),
    /// 本次轮询访问上游并使用 CAS 保存了最新状态。
    Updated {
        task: AsyncTaskRecord,
        poll: CanonicalTaskPoll,
    },
}

/// 视频任务提交与轮询持久化协调错误；不携带请求或绑定敏感内容。
#[derive(Debug, Error)]
pub(crate) enum PersistentVideoTaskError {
    #[error("视频任务输入边界无效")]
    Input,
    #[error("视频任务持久化操作失败")]
    Repository(#[source] AsyncTaskRepositoryError),
    #[error("视频任务运行时执行失败")]
    Runtime(#[source] AfError),
    #[error("视频任务提交结果未知，禁止自动重提")]
    SubmissionOutcomeUnknown,
    #[error("视频任务不存在或绑定引用无效")]
    NotFound,
    #[error("视频任务随机标识生成失败")]
    Entropy,
    #[error("视频任务持久化内容违反不变量")]
    Invariant,
}

impl From<AsyncTaskRepositoryError> for PersistentVideoTaskError {
    fn from(error: AsyncTaskRepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<AsyncTaskInputError> for PersistentVideoTaskError {
    fn from(_error: AsyncTaskInputError) -> Self {
        Self::Input
    }
}

/// 在上游发网前持久化 claim，并在轮询时只恢复原始绑定的协调器。
pub(crate) struct PersistentVideoTaskCoordinator {
    runtime: Arc<dyn PersistentVideoTaskRuntime>,
    tasks: AsyncTaskRepository,
    submissions: AsyncTaskSubmissionRepository,
}

impl PersistentVideoTaskCoordinator {
    /// 组合视频运行时、最终任务仓储和提交 claim 仓储。
    pub(crate) fn new(
        runtime: ScheduledChatService,
        tasks: AsyncTaskRepository,
        submissions: AsyncTaskSubmissionRepository,
    ) -> Self {
        Self::with_runtime(Arc::new(runtime), tasks, submissions)
    }

    fn with_runtime(
        runtime: Arc<dyn PersistentVideoTaskRuntime>,
        tasks: AsyncTaskRepository,
        submissions: AsyncTaskSubmissionRepository,
    ) -> Self {
        Self {
            runtime,
            tasks,
            submissions,
        }
    }

    /// 幂等提交视频任务；`Submitting` 状态永远不会自动发起第二次上游请求。
    pub(crate) async fn submit(
        &self,
        task_id: AsyncTaskId,
        request_id: AsyncTaskRequestId,
        principal: GatewayPrincipal,
        request: CanonicalVideoGenerationRequest,
        relay_request_id: &str,
    ) -> Result<AsyncTaskRecord, PersistentVideoTaskError> {
        let request_fingerprint = request_fingerprint(&request);
        let claim = self
            .claim(AsyncTaskSubmissionClaim::new(
                task_id,
                request_id,
                principal,
                Protocol::XaiVideo,
                request.model().as_str().to_owned(),
                request_fingerprint,
                now()?,
            )?)
            .await?;
        match claim.state() {
            AsyncTaskSubmissionState::Accepted => return self.ensure_task(&claim).await,
            AsyncTaskSubmissionState::Submitting => {
                return Err(PersistentVideoTaskError::SubmissionOutcomeUnknown);
            }
            AsyncTaskSubmissionState::Claimed => {}
        }

        let attempt_id = random_attempt_id()?;
        let begun = self.begin(&claim, attempt_id).await?;
        let submission = self
            .runtime
            .submit(claim.principal().group_id(), request, relay_request_id)
            .await;
        match submission {
            Ok(submission) => {
                let accepted = self.accept(&begun, attempt_id, submission).await?;
                self.ensure_task(&accepted).await
            }
            Err(error)
                if error.disposition() == VideoTaskSubmissionDisposition::DefinitelyNotAccepted =>
            {
                self.release(&begun, attempt_id).await?;
                Err(PersistentVideoTaskError::Runtime(error.into_error()))
            }
            Err(_) => Err(PersistentVideoTaskError::SubmissionOutcomeUnknown),
        }
    }

    /// 从持久化绑定轮询任务，并使用最终任务版本执行一次状态 CAS。
    pub(crate) async fn poll(
        &self,
        user_id: UserId,
        task_id: AsyncTaskId,
        relay_request_id: &str,
    ) -> Result<PersistentVideoTaskPollOutcome, PersistentVideoTaskError> {
        let claim = self
            .submissions
            .find(user_id, task_id)
            .await?
            .ok_or(PersistentVideoTaskError::NotFound)?;
        if claim.state() != AsyncTaskSubmissionState::Accepted {
            return Err(PersistentVideoTaskError::SubmissionOutcomeUnknown);
        }
        let task = match self.tasks.find(user_id, task_id).await? {
            Some(task) => task,
            None => self.ensure_task(&claim).await?,
        };
        if !claim.matches_task(&task) {
            return Err(PersistentVideoTaskError::Invariant);
        }
        if task.status().is_terminal() {
            return Ok(PersistentVideoTaskPollOutcome::Terminal(task));
        }
        let poll = self
            .runtime
            .poll(&task, &claim, relay_request_id)
            .await
            .map_err(PersistentVideoTaskError::Runtime)?;
        let expected_status = poll.status().clone();
        let updated = self
            .transition(
                &task,
                &expected_status,
                AsyncTaskTransition::new(
                    task.task_id(),
                    task.principal().user_id(),
                    task.version(),
                    expected_status.clone(),
                    now()?,
                )?,
            )
            .await?;
        Ok(PersistentVideoTaskPollOutcome::Updated {
            task: updated,
            poll,
        })
    }

    async fn claim(
        &self,
        write: AsyncTaskSubmissionClaim,
    ) -> Result<AsyncTaskSubmissionRecord, PersistentVideoTaskError> {
        let user_id = write.principal().user_id();
        let request_id = write.request_id();
        let request_fingerprint = write.request_fingerprint();
        match self.submissions.claim(write).await {
            Ok(
                AsyncTaskSubmissionClaimOutcome::Created(record)
                | AsyncTaskSubmissionClaimOutcome::Existing(record),
            ) => Ok(record),
            Ok(AsyncTaskSubmissionClaimOutcome::NotFound) => {
                Err(PersistentVideoTaskError::NotFound)
            }
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let record = self
                    .submissions
                    .find_by_request(user_id, request_id)
                    .await?
                    .ok_or(PersistentVideoTaskError::SubmissionOutcomeUnknown)?;
                if record.request_fingerprint() != request_fingerprint {
                    return Err(PersistentVideoTaskError::Invariant);
                }
                Ok(record)
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn begin(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        attempt_id: AsyncTaskAttemptId,
    ) -> Result<AsyncTaskSubmissionRecord, PersistentVideoTaskError> {
        let command = || {
            AsyncTaskSubmissionBegin::new(
                claim.task_id(),
                claim.principal().user_id(),
                claim.version(),
                attempt_id,
                now()?,
            )
            .map_err(Into::into)
        };
        match self.submissions.begin(command()?).await {
            Ok(outcome) => mutation_record(outcome),
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let current = self
                    .submissions
                    .find(claim.principal().user_id(), claim.task_id())
                    .await?
                    .ok_or(PersistentVideoTaskError::NotFound)?;
                if current.state() == AsyncTaskSubmissionState::Submitting
                    && current.attempt_id() == Some(attempt_id)
                {
                    return Ok(current);
                }
                if current.state() != AsyncTaskSubmissionState::Claimed
                    || current.version() != claim.version()
                {
                    return Err(PersistentVideoTaskError::SubmissionOutcomeUnknown);
                }
                mutation_record(self.submissions.begin(command()?).await?)
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn release(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        attempt_id: AsyncTaskAttemptId,
    ) -> Result<(), PersistentVideoTaskError> {
        let command = || {
            AsyncTaskSubmissionRelease::new(
                claim.task_id(),
                claim.principal().user_id(),
                claim.version(),
                attempt_id,
                now()?,
            )
            .map_err(PersistentVideoTaskError::from)
        };
        let outcome = match self.submissions.release(command()?).await {
            Ok(outcome) => outcome,
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                self.submissions.release(command()?).await?
            }
            Err(error) => return Err(error.into()),
        };
        mutation_record(outcome).map(|_| ())
    }

    async fn accept(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        attempt_id: AsyncTaskAttemptId,
        submission: PersistentSubmission,
    ) -> Result<AsyncTaskSubmissionRecord, PersistentVideoTaskError> {
        let command = || {
            AsyncTaskSubmissionAccept::new(
                claim.task_id(),
                claim.principal().user_id(),
                claim.version(),
                attempt_id,
                submission.target_group_id,
                submission.upstream_model.clone(),
                submission.channel_id,
                submission.credential_id,
                submission.credential_revision,
                submission.submission.task_id().clone(),
                submission.binding_fingerprint,
                submission.attempt_timeout,
                submission.submission.status().clone(),
                now()?,
            )
            .map_err(PersistentVideoTaskError::from)
        };
        let outcome = match self.submissions.accept(command()?).await {
            Ok(outcome) => outcome,
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                self.submissions.accept(command()?).await?
            }
            Err(error) => return Err(error.into()),
        };
        mutation_record(outcome)
    }

    async fn ensure_task(
        &self,
        claim: &AsyncTaskSubmissionRecord,
    ) -> Result<AsyncTaskRecord, PersistentVideoTaskError> {
        if claim.state() != AsyncTaskSubmissionState::Accepted {
            return Err(PersistentVideoTaskError::SubmissionOutcomeUnknown);
        }
        if let Some(existing) = self
            .tasks
            .find_by_request(claim.principal().user_id(), claim.request_id())
            .await?
        {
            return if claim.matches_task(&existing) {
                Ok(existing)
            } else {
                Err(PersistentVideoTaskError::Invariant)
            };
        }
        let create = claim
            .task_create()?
            .ok_or(PersistentVideoTaskError::Invariant)?;
        match self.tasks.create(create).await {
            Ok(
                AsyncTaskCreateOutcome::Created(record) | AsyncTaskCreateOutcome::Existing(record),
            ) => {
                if claim.matches_task(&record) {
                    Ok(record)
                } else {
                    Err(PersistentVideoTaskError::Invariant)
                }
            }
            Ok(AsyncTaskCreateOutcome::NotFound) => Err(PersistentVideoTaskError::NotFound),
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let existing = self
                    .tasks
                    .find_by_request(claim.principal().user_id(), claim.request_id())
                    .await?
                    .ok_or(PersistentVideoTaskError::SubmissionOutcomeUnknown)?;
                if claim.matches_task(&existing) {
                    Ok(existing)
                } else {
                    Err(PersistentVideoTaskError::Invariant)
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn transition(
        &self,
        previous: &AsyncTaskRecord,
        expected_status: &af_domain::TaskStatus,
        command: AsyncTaskTransition,
    ) -> Result<AsyncTaskRecord, PersistentVideoTaskError> {
        match self.tasks.transition(command).await {
            Ok(
                AsyncTaskTransitionOutcome::Applied(record)
                | AsyncTaskTransitionOutcome::Existing(record),
            ) => Ok(record),
            Ok(AsyncTaskTransitionOutcome::NotFound) => Err(PersistentVideoTaskError::NotFound),
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let current = self
                    .tasks
                    .find(previous.principal().user_id(), previous.task_id())
                    .await?
                    .ok_or(PersistentVideoTaskError::NotFound)?;
                if current.status() == expected_status
                    && (current.version() == previous.version()
                        || current.version() == previous.version().saturating_add(1))
                {
                    Ok(current)
                } else {
                    Err(PersistentVideoTaskError::SubmissionOutcomeUnknown)
                }
            }
            Err(error) => Err(error.into()),
        }
    }
}

impl fmt::Debug for PersistentVideoTaskCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentVideoTaskCoordinator")
            .field("runtime", &"<动态端口>")
            .field("tasks", &self.tasks)
            .field("submissions", &self.submissions)
            .finish()
    }
}

fn mutation_record(
    outcome: AsyncTaskSubmissionMutationOutcome,
) -> Result<AsyncTaskSubmissionRecord, PersistentVideoTaskError> {
    match outcome {
        AsyncTaskSubmissionMutationOutcome::Applied(record)
        | AsyncTaskSubmissionMutationOutcome::Existing(record) => Ok(record),
        AsyncTaskSubmissionMutationOutcome::NotFound => Err(PersistentVideoTaskError::NotFound),
    }
}

fn request_fingerprint(request: &CanonicalVideoGenerationRequest) -> AsyncTaskRequestFingerprint {
    let mut digest = Sha256::new();
    digest.update(b"anyflows/video-task-request/v1\0");
    hash_bytes(&mut digest, request.model().as_str().as_bytes());
    hash_bytes(&mut digest, request.prompt().as_str().as_bytes());
    hash_optional_u64(
        &mut digest,
        request.duration().map(|value| u64::from(value.seconds())),
    );
    hash_optional_bytes(
        &mut digest,
        request
            .aspect_ratio()
            .map(|value| value.as_str().as_bytes()),
    );
    hash_optional_bytes(
        &mut digest,
        request.resolution().map(|value| value.as_str().as_bytes()),
    );
    AsyncTaskRequestFingerprint::new(digest.finalize().into())
}

fn hash_optional_bytes(digest: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            digest.update([1]);
            hash_bytes(digest, value);
        }
        None => digest.update([0]),
    }
}

fn hash_optional_u64(digest: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            digest.update([1]);
            digest.update(value.to_be_bytes());
        }
        None => digest.update([0]),
    }
}

fn hash_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update(
        u64::try_from(value.len())
            .expect("视频任务指纹字段长度必须适配 u64")
            .to_be_bytes(),
    );
    digest.update(value);
}

fn random_attempt_id() -> Result<AsyncTaskAttemptId, PersistentVideoTaskError> {
    for _ in 0..2 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| PersistentVideoTaskError::Entropy)?;
        if let Ok(value) = AsyncTaskAttemptId::new(bytes) {
            return Ok(value);
        }
    }
    Err(PersistentVideoTaskError::Entropy)
}

fn now() -> Result<u64, PersistentVideoTaskError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| PersistentVideoTaskError::Invariant)
}

#[cfg(test)]
#[path = "video_task_persistence_tests.rs"]
mod tests;
