use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_billing::{
    BillingUsageDimensions, BillingUsageRecord, GroupPricingCache, PricingRatio, PricingRatios,
    TaskBillingPlan, TaskBillingReleaseSink, TaskBillingReservePort, TaskBillingSettlementPort,
    TaskBillingSettlementRequest, UsageRecordSink, XAI_VIDEO_PRICE_CARD_VERSION,
    XaiVideoPricingSnapshot,
};
use af_db::{
    AsyncTaskBillingAccept, AsyncTaskBillingClear, AsyncTaskBillingMark,
    AsyncTaskBillingMutationOutcome, AsyncTaskBillingPlan, AsyncTaskBillingPlanOutcome,
    AsyncTaskBillingRecord, AsyncTaskBillingRepository, AsyncTaskBillingResolution,
    AsyncTaskBillingSettlement, AsyncTaskBillingState, AsyncTaskCreateOutcome, AsyncTaskInputError,
    AsyncTaskPageCursor, AsyncTaskPageRecord, AsyncTaskRecord, AsyncTaskRepository,
    AsyncTaskRepositoryError, AsyncTaskSubmissionAccept, AsyncTaskSubmissionBegin,
    AsyncTaskSubmissionClaim, AsyncTaskSubmissionClaimOutcome, AsyncTaskSubmissionMutationOutcome,
    AsyncTaskSubmissionRecord, AsyncTaskSubmissionRelease, AsyncTaskSubmissionRepository,
    AsyncTaskSubmissionState, AsyncTaskTransition, AsyncTaskTransitionOutcome,
    AsyncTaskVideoResolution,
};
use af_domain::{
    AfError, AsyncTaskAttemptId, AsyncTaskBindingFingerprint, AsyncTaskId,
    AsyncTaskRequestFingerprint, AsyncTaskRequestId, BillingReservationId, ChannelId, CredentialId,
    GatewayPrincipal, GroupId, Protocol, TaskStatus, TaskSubmission, UserId,
};
use af_protocol::{
    CanonicalTaskPoll, CanonicalVideoGenerationRequest, VideoDuration, VideoResolution,
};
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
type PersistentTargetGroupFuture<'a> =
    Pin<Box<dyn Future<Output = Result<GroupId, AfError>> + Send + 'a>>;

/// 持久化协调器实际需要的最小视频运行时端口。
trait PersistentVideoTaskRuntime: Send + Sync {
    fn target_group<'a>(
        &'a self,
        group_id: GroupId,
        model: &'a str,
    ) -> PersistentTargetGroupFuture<'a>;

    fn submit<'a>(
        &'a self,
        group_id: GroupId,
        expected_target_group_id: GroupId,
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
    fn target_group<'a>(
        &'a self,
        group_id: GroupId,
        model: &'a str,
    ) -> PersistentTargetGroupFuture<'a> {
        Box::pin(async move {
            self.video_route(group_id, model)
                .await
                .map(|route| route.target_group_id())
        })
    }

    fn submit<'a>(
        &'a self,
        group_id: GroupId,
        expected_target_group_id: GroupId,
        request: CanonicalVideoGenerationRequest,
        request_id: &'a str,
    ) -> PersistentSubmissionFuture<'a> {
        Box::pin(async move {
            self.submit_video_task_to_target(
                group_id,
                expected_target_group_id,
                request,
                request_id,
            )
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
        /// 成功终态可用于计费审计的真实视频维度；非终态保持缺失。
        billing_dimensions: Option<BillingUsageDimensions>,
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
    #[error("视频任务成功结果当前无法重新取得")]
    ResultUnavailable,
    #[error("视频任务计费操作失败")]
    Billing,
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

/// 视频任务持久化协调器依赖的计费端口集合。
pub(crate) struct PersistentVideoTaskBillingPorts {
    reserve: Arc<dyn TaskBillingReservePort>,
    settlement: Arc<dyn TaskBillingSettlementPort>,
    release: Arc<dyn TaskBillingReleaseSink>,
    usage: Arc<dyn UsageRecordSink>,
}

impl PersistentVideoTaskBillingPorts {
    /// 组合预扣、结算、释放与用量持久化端口。
    pub(crate) fn new(
        reserve: Arc<dyn TaskBillingReservePort>,
        settlement: Arc<dyn TaskBillingSettlementPort>,
        release: Arc<dyn TaskBillingReleaseSink>,
        usage: Arc<dyn UsageRecordSink>,
    ) -> Self {
        Self {
            reserve,
            settlement,
            release,
            usage,
        }
    }
}

/// 在上游发网前持久化 claim，并在轮询时只恢复原始绑定的协调器。
pub(crate) struct PersistentVideoTaskCoordinator {
    runtime: Arc<dyn PersistentVideoTaskRuntime>,
    tasks: AsyncTaskRepository,
    submissions: AsyncTaskSubmissionRepository,
    billings: AsyncTaskBillingRepository,
    group_pricing: GroupPricingCache,
    billing_ports: PersistentVideoTaskBillingPorts,
}

impl PersistentVideoTaskCoordinator {
    /// 组合视频运行时、最终任务仓储和提交 claim 仓储。
    pub(crate) fn new(
        runtime: ScheduledChatService,
        tasks: AsyncTaskRepository,
        submissions: AsyncTaskSubmissionRepository,
        billings: AsyncTaskBillingRepository,
        group_pricing: GroupPricingCache,
        billing_ports: PersistentVideoTaskBillingPorts,
    ) -> Self {
        Self::with_runtime(
            Arc::new(runtime),
            tasks,
            submissions,
            billings,
            group_pricing,
            billing_ports,
        )
    }

    fn with_runtime(
        runtime: Arc<dyn PersistentVideoTaskRuntime>,
        tasks: AsyncTaskRepository,
        submissions: AsyncTaskSubmissionRepository,
        billings: AsyncTaskBillingRepository,
        group_pricing: GroupPricingCache,
        billing_ports: PersistentVideoTaskBillingPorts,
    ) -> Self {
        Self {
            runtime,
            tasks,
            submissions,
            billings,
            group_pricing,
            billing_ports,
        }
    }

    /// 只读取 owner-scoped 视频任务历史，不解析运行时绑定或访问上游。
    pub(crate) async fn list(
        &self,
        user_id: UserId,
        before: Option<AsyncTaskPageCursor>,
        limit: usize,
    ) -> Result<AsyncTaskPageRecord, PersistentVideoTaskError> {
        self.tasks
            .list(user_id, Protocol::XaiVideo, before, limit)
            .await
            .map_err(Into::into)
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
        let requested_duration_seconds = request.duration().map(|value| value.seconds());
        let requested_resolution = request.resolution().map(map_video_resolution);
        let claim = self
            .claim(AsyncTaskSubmissionClaim::new(
                task_id,
                request_id,
                principal,
                Protocol::XaiVideo,
                request.model().as_str().to_owned(),
                request_fingerprint,
                requested_duration_seconds,
                now()?,
            )?)
            .await?;
        match claim.state() {
            AsyncTaskSubmissionState::Accepted => {
                let billing = self.load_billing(&claim).await?;
                self.ensure_billing_submitted(&claim, billing).await?;
                return self.ensure_task(&claim).await;
            }
            AsyncTaskSubmissionState::Submitting => {
                if let Some(billing) = self
                    .billings
                    .find(claim.principal().user_id(), claim.task_id())
                    .await?
                    .filter(|billing| {
                        matches!(
                            billing.state(),
                            AsyncTaskBillingState::ReleasePending | AsyncTaskBillingState::Released
                        )
                    })
                {
                    self.finish_pre_submission_release(&claim, billing).await?;
                }
                return Err(PersistentVideoTaskError::SubmissionOutcomeUnknown);
            }
            AsyncTaskSubmissionState::Claimed => {}
        }

        let target_group_id = self
            .runtime
            .target_group(claim.principal().group_id(), claim.requested_model())
            .await
            .map_err(PersistentVideoTaskError::Runtime)?;
        let billing = self
            .prepare_billing(&claim, target_group_id, request.resolution())
            .await?;
        let billing = self.reserve_billing(&claim, billing).await?;
        let attempt_id = random_attempt_id()?;
        let begun = self.begin(&claim, attempt_id).await?;
        let submission = self
            .runtime
            .submit(
                claim.principal().group_id(),
                billing.target_group_id(),
                request,
                relay_request_id,
            )
            .await;
        match submission {
            Ok(submission) => {
                let accepted = self
                    .accept(&begun, attempt_id, submission, requested_resolution)
                    .await?;
                self.ensure_billing_submitted(&accepted, billing).await?;
                self.ensure_task(&accepted).await
            }
            Err(error)
                if error.disposition() == VideoTaskSubmissionDisposition::DefinitelyNotAccepted =>
            {
                self.release_pre_submission(&begun, attempt_id, billing)
                    .await?;
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
        let billing = self.load_billing(&claim).await?;
        let billing = self.ensure_billing_submitted(&claim, billing).await?;
        if task.status().is_terminal() {
            let expected_billing_state = match task.status() {
                TaskStatus::Succeeded => AsyncTaskBillingState::Settled,
                TaskStatus::Failed { .. } => AsyncTaskBillingState::Released,
                TaskStatus::Submitted { .. }
                | TaskStatus::Queued { .. }
                | TaskStatus::Running { .. } => return Err(PersistentVideoTaskError::Invariant),
            };
            if billing.state() != expected_billing_state {
                return Err(PersistentVideoTaskError::Invariant);
            }
            return Ok(PersistentVideoTaskPollOutcome::Terminal(task));
        }
        let poll = self
            .runtime
            .poll(&task, &claim, relay_request_id)
            .await
            .map_err(PersistentVideoTaskError::Runtime)?;
        let billing_dimensions = video_billing_dimensions(&claim, &poll)?;
        let expected_status = poll.status().clone();
        match &expected_status {
            TaskStatus::Succeeded => {
                let dimensions = billing_dimensions.ok_or(PersistentVideoTaskError::Invariant)?;
                self.complete_success_billing(&claim, billing, dimensions)
                    .await?;
            }
            TaskStatus::Failed { .. } => {
                self.complete_failure_billing(&claim, billing).await?;
            }
            TaskStatus::Submitted { .. }
            | TaskStatus::Queued { .. }
            | TaskStatus::Running { .. } => {}
        }
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
            billing_dimensions,
        })
    }

    /// 为已成功任务从原绑定重新取得短期结果，不推进状态或重复计费。
    pub(crate) async fn fetch_succeeded_output(
        &self,
        user_id: UserId,
        task_id: AsyncTaskId,
        relay_request_id: &str,
    ) -> Result<CanonicalTaskPoll, PersistentVideoTaskError> {
        let claim = self
            .submissions
            .find(user_id, task_id)
            .await?
            .ok_or(PersistentVideoTaskError::NotFound)?;
        if claim.state() != AsyncTaskSubmissionState::Accepted {
            return Err(PersistentVideoTaskError::SubmissionOutcomeUnknown);
        }
        let task = self
            .tasks
            .find(user_id, task_id)
            .await?
            .ok_or(PersistentVideoTaskError::NotFound)?;
        if !claim.matches_task(&task) || !matches!(task.status(), TaskStatus::Succeeded) {
            return Err(PersistentVideoTaskError::Invariant);
        }
        let billing = self.load_billing(&claim).await?;
        if billing.state() != AsyncTaskBillingState::Settled {
            return Err(PersistentVideoTaskError::Invariant);
        }

        // 只轮询已经固化的上游任务标识；这里绝不进入提交、冻结或结算路径。
        let poll = self
            .runtime
            .poll(&task, &claim, relay_request_id)
            .await
            .map_err(PersistentVideoTaskError::Runtime)?;
        if !matches!(poll.status(), TaskStatus::Succeeded) || poll.output().is_none() {
            return Err(PersistentVideoTaskError::ResultUnavailable);
        }
        Ok(poll)
    }

    async fn prepare_billing(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        target_group_id: GroupId,
        requested_resolution: Option<VideoResolution>,
    ) -> Result<AsyncTaskBillingRecord, PersistentVideoTaskError> {
        if let Some(existing) = self
            .billings
            .find(claim.principal().user_id(), claim.task_id())
            .await?
        {
            if existing.target_group_id() != target_group_id {
                return Err(PersistentVideoTaskError::Invariant);
            }
            if existing.state() != AsyncTaskBillingState::Released {
                return Ok(existing);
            }
            self.clear_released_billing(existing).await?;
        }

        let pricing = self.capture_pricing(claim, target_group_id, requested_resolution)?;
        let ratios = pricing.ratios();
        let write = AsyncTaskBillingPlan::new(
            claim.task_id(),
            claim.principal().user_id(),
            random_reservation_id()?,
            target_group_id,
            XAI_VIDEO_PRICE_CARD_VERSION,
            map_billing_resolution(pricing.resolution()),
            [
                ratios.group().micros(),
                ratios.group_model().micros(),
                ratios.applied_peak().micros(),
            ],
            pricing
                .maximum_upper_bound()
                .map_err(|_| PersistentVideoTaskError::Billing)?,
            now()?,
        )?;
        let task_id = claim.task_id();
        let user_id = claim.principal().user_id();
        match self.billings.plan(write).await {
            Ok(
                AsyncTaskBillingPlanOutcome::Created(record)
                | AsyncTaskBillingPlanOutcome::Existing(record),
            ) => Ok(record),
            Ok(AsyncTaskBillingPlanOutcome::NotFound) => Err(PersistentVideoTaskError::NotFound),
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => self
                .billings
                .find(user_id, task_id)
                .await?
                .ok_or(PersistentVideoTaskError::SubmissionOutcomeUnknown),
            Err(error) => Err(error.into()),
        }
    }

    fn capture_pricing(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        target_group_id: GroupId,
        requested_resolution: Option<VideoResolution>,
    ) -> Result<XaiVideoPricingSnapshot, PersistentVideoTaskError> {
        if self.group_pricing.is_stale() {
            return Err(PersistentVideoTaskError::Billing);
        }
        let snapshot = self
            .group_pricing
            .snapshot()
            .map_err(|_| PersistentVideoTaskError::Billing)?;
        let ratios = snapshot
            .ratios_for_request(
                claim.principal().group_id(),
                target_group_id,
                crate::utc_time::current_utc_day_second()
                    .ok_or(PersistentVideoTaskError::Invariant)?,
            )
            .map_err(|_| PersistentVideoTaskError::Billing)?;
        if self.group_pricing.is_stale() {
            return Err(PersistentVideoTaskError::Billing);
        }
        Ok(XaiVideoPricingSnapshot::new(requested_resolution, ratios))
    }

    async fn reserve_billing(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        mut billing: AsyncTaskBillingRecord,
    ) -> Result<AsyncTaskBillingRecord, PersistentVideoTaskError> {
        if billing.state() == AsyncTaskBillingState::Reserved {
            return Ok(billing);
        }
        if billing.state() != AsyncTaskBillingState::Planned {
            return Err(PersistentVideoTaskError::Invariant);
        }
        let plan = TaskBillingPlan::new(
            billing.reservation_id(),
            claim.principal(),
            billing.upper_bound(),
        )
        .map_err(|_| PersistentVideoTaskError::Billing)?;
        plan.reserve(self.billing_ports.reserve.as_ref())
            .await
            .map_err(|_| PersistentVideoTaskError::Billing)?;
        let command = || {
            AsyncTaskBillingMark::new(
                billing.task_id(),
                billing.user_id(),
                billing.version(),
                now()?,
            )
            .map_err(PersistentVideoTaskError::from)
        };
        billing = match self.billings.mark_reserved(command()?).await {
            Ok(outcome) => billing_mutation_record(outcome)?,
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let current = self.load_billing(claim).await?;
                if current.state() == AsyncTaskBillingState::Reserved {
                    current
                } else if current.state() == AsyncTaskBillingState::Planned
                    && current.version() == billing.version()
                {
                    billing_mutation_record(self.billings.mark_reserved(command()?).await?)?
                } else {
                    return Err(PersistentVideoTaskError::SubmissionOutcomeUnknown);
                }
            }
            Err(error) => return Err(error.into()),
        };
        Ok(billing)
    }

    async fn ensure_billing_submitted(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        billing: AsyncTaskBillingRecord,
    ) -> Result<AsyncTaskBillingRecord, PersistentVideoTaskError> {
        if billing.target_group_id()
            != claim
                .target_group_id()
                .ok_or(PersistentVideoTaskError::Invariant)?
        {
            return Err(PersistentVideoTaskError::Invariant);
        }
        if matches!(
            billing.state(),
            AsyncTaskBillingState::Submitted
                | AsyncTaskBillingState::SettlementPending
                | AsyncTaskBillingState::Settled
                | AsyncTaskBillingState::ReleasePending
                | AsyncTaskBillingState::Released
        ) {
            return Ok(billing);
        }
        if billing.state() != AsyncTaskBillingState::Reserved {
            return Err(PersistentVideoTaskError::Invariant);
        }
        let pricing = restore_pricing(&billing)?;
        let rate_microusd = pricing
            .rate_microusd(
                claim
                    .upstream_model()
                    .ok_or(PersistentVideoTaskError::Invariant)?,
            )
            .map_err(|_| PersistentVideoTaskError::Billing)?;
        let requested_duration = claim
            .video_duration_seconds()
            .map(VideoDuration::new)
            .transpose()
            .map_err(|_| PersistentVideoTaskError::Invariant)?;
        let fallback_quota = pricing
            .fallback_quota(rate_microusd, requested_duration)
            .map_err(|_| PersistentVideoTaskError::Billing)?;
        let command = || {
            AsyncTaskBillingAccept::new(
                billing.task_id(),
                billing.user_id(),
                billing.version(),
                rate_microusd,
                fallback_quota,
                now()?,
            )
            .map_err(PersistentVideoTaskError::from)
        };
        match self.billings.accept(command()?).await {
            Ok(outcome) => billing_mutation_record(outcome),
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let current = self.load_billing(claim).await?;
                if current.state() == AsyncTaskBillingState::Submitted
                    && current.rate_microusd() == Some(rate_microusd)
                    && current.fallback_quota() == Some(fallback_quota)
                {
                    Ok(current)
                } else {
                    billing_mutation_record(self.billings.accept(command()?).await?)
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn complete_success_billing(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        mut billing: AsyncTaskBillingRecord,
        dimensions: BillingUsageDimensions,
    ) -> Result<(), PersistentVideoTaskError> {
        let duration = dimensions
            .video_duration()
            .ok_or(PersistentVideoTaskError::Invariant)?;
        let pricing = restore_pricing(&billing)?;
        let rate_microusd = billing
            .rate_microusd()
            .ok_or(PersistentVideoTaskError::Invariant)?;
        let actual_quota = pricing
            .actual_quota(rate_microusd, duration)
            .map_err(|_| PersistentVideoTaskError::Billing)?;
        if billing.state() == AsyncTaskBillingState::Submitted {
            let command = || {
                AsyncTaskBillingSettlement::new(
                    billing.task_id(),
                    billing.user_id(),
                    billing.version(),
                    actual_quota,
                    duration.seconds(),
                    now()?,
                )
                .map_err(PersistentVideoTaskError::from)
            };
            billing = match self.billings.begin_settlement(command()?).await {
                Ok(outcome) => billing_mutation_record(outcome)?,
                Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                    let current = self.load_billing(claim).await?;
                    if current.state() == AsyncTaskBillingState::SettlementPending
                        && current.actual_quota() == Some(actual_quota)
                        && current.actual_duration_seconds() == Some(duration.seconds())
                    {
                        current
                    } else {
                        billing_mutation_record(self.billings.begin_settlement(command()?).await?)?
                    }
                }
                Err(error) => return Err(error.into()),
            };
        }
        if !matches!(
            billing.state(),
            AsyncTaskBillingState::SettlementPending | AsyncTaskBillingState::Settled
        ) || billing.actual_quota() != Some(actual_quota)
            || billing.actual_duration_seconds() != Some(duration.seconds())
        {
            return Err(PersistentVideoTaskError::Invariant);
        }
        if billing.state() == AsyncTaskBillingState::SettlementPending {
            let request = TaskBillingSettlementRequest::new(
                billing.reservation_id(),
                actual_quota,
                billing.upper_bound(),
            )
            .map_err(|_| PersistentVideoTaskError::Billing)?;
            self.billing_ports
                .settlement
                .settle(request)
                .await
                .map_err(|_| PersistentVideoTaskError::Billing)?;
            let command = || {
                AsyncTaskBillingMark::new(
                    billing.task_id(),
                    billing.user_id(),
                    billing.version(),
                    now()?,
                )
                .map_err(PersistentVideoTaskError::from)
            };
            billing = match self.billings.mark_settled(command()?).await {
                Ok(outcome) => billing_mutation_record(outcome)?,
                Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                    let current = self.load_billing(claim).await?;
                    if current.state() == AsyncTaskBillingState::Settled {
                        current
                    } else {
                        billing_mutation_record(self.billings.mark_settled(command()?).await?)?
                    }
                }
                Err(error) => return Err(error.into()),
            };
        }
        if billing.state() != AsyncTaskBillingState::Settled {
            return Err(PersistentVideoTaskError::Invariant);
        }
        self.billing_ports
            .usage
            .persist(BillingUsageRecord::for_per_call(
                billing.reservation_id(),
                claim.principal(),
                dimensions,
                actual_quota,
            ))
            .await
            .map_err(|_| PersistentVideoTaskError::Billing)
    }

    async fn complete_failure_billing(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        billing: AsyncTaskBillingRecord,
    ) -> Result<(), PersistentVideoTaskError> {
        let billing = self.begin_billing_release(claim, billing).await?;
        self.persist_billing_release(claim, billing)
            .await
            .map(|_| ())
    }

    async fn release_pre_submission(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        attempt_id: AsyncTaskAttemptId,
        billing: AsyncTaskBillingRecord,
    ) -> Result<(), PersistentVideoTaskError> {
        let billing = self.begin_billing_release(claim, billing).await?;
        let billing = self.persist_billing_release(claim, billing).await?;
        self.release(claim, attempt_id).await?;
        self.clear_released_billing(billing).await
    }

    async fn finish_pre_submission_release(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        billing: AsyncTaskBillingRecord,
    ) -> Result<(), PersistentVideoTaskError> {
        let attempt_id = claim
            .attempt_id()
            .ok_or(PersistentVideoTaskError::Invariant)?;
        let billing = self.persist_billing_release(claim, billing).await?;
        self.release(claim, attempt_id).await?;
        self.clear_released_billing(billing).await
    }

    async fn begin_billing_release(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        billing: AsyncTaskBillingRecord,
    ) -> Result<AsyncTaskBillingRecord, PersistentVideoTaskError> {
        if matches!(
            billing.state(),
            AsyncTaskBillingState::ReleasePending | AsyncTaskBillingState::Released
        ) {
            return Ok(billing);
        }
        if !matches!(
            billing.state(),
            AsyncTaskBillingState::Reserved | AsyncTaskBillingState::Submitted
        ) {
            return Err(PersistentVideoTaskError::Invariant);
        }
        let command = || {
            AsyncTaskBillingMark::new(
                billing.task_id(),
                billing.user_id(),
                billing.version(),
                now()?,
            )
            .map_err(PersistentVideoTaskError::from)
        };
        match self.billings.begin_release(command()?).await {
            Ok(outcome) => billing_mutation_record(outcome),
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let current = self.load_billing(claim).await?;
                if current.state() == AsyncTaskBillingState::ReleasePending {
                    Ok(current)
                } else {
                    billing_mutation_record(self.billings.begin_release(command()?).await?)
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn persist_billing_release(
        &self,
        claim: &AsyncTaskSubmissionRecord,
        billing: AsyncTaskBillingRecord,
    ) -> Result<AsyncTaskBillingRecord, PersistentVideoTaskError> {
        if billing.state() == AsyncTaskBillingState::Released {
            return Ok(billing);
        }
        if billing.state() != AsyncTaskBillingState::ReleasePending {
            return Err(PersistentVideoTaskError::Invariant);
        }
        self.billing_ports
            .release
            .release(billing.reservation_id())
            .await
            .map_err(|_| PersistentVideoTaskError::Billing)?;
        let command = || {
            AsyncTaskBillingMark::new(
                billing.task_id(),
                billing.user_id(),
                billing.version(),
                now()?,
            )
            .map_err(PersistentVideoTaskError::from)
        };
        match self.billings.mark_released(command()?).await {
            Ok(outcome) => billing_mutation_record(outcome),
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                let current = self.load_billing(claim).await?;
                if current.state() == AsyncTaskBillingState::Released {
                    Ok(current)
                } else {
                    billing_mutation_record(self.billings.mark_released(command()?).await?)
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn clear_released_billing(
        &self,
        billing: AsyncTaskBillingRecord,
    ) -> Result<(), PersistentVideoTaskError> {
        if billing.state() != AsyncTaskBillingState::Released {
            return Err(PersistentVideoTaskError::Invariant);
        }
        let command = || {
            AsyncTaskBillingClear::new(
                billing.task_id(),
                billing.user_id(),
                billing.version(),
                billing.reservation_id(),
                now()?,
            )
            .map_err(PersistentVideoTaskError::from)
        };
        match self.billings.clear_released(command()?).await {
            Ok(
                AsyncTaskBillingMutationOutcome::Applied(_)
                | AsyncTaskBillingMutationOutcome::NotFound,
            ) => Ok(()),
            Ok(AsyncTaskBillingMutationOutcome::Existing(_)) => {
                Err(PersistentVideoTaskError::Invariant)
            }
            Err(AsyncTaskRepositoryError::OutcomeUnknown) => {
                if self
                    .billings
                    .find(billing.user_id(), billing.task_id())
                    .await?
                    .is_none()
                {
                    Ok(())
                } else {
                    match self.billings.clear_released(command()?).await? {
                        AsyncTaskBillingMutationOutcome::Applied(_)
                        | AsyncTaskBillingMutationOutcome::NotFound => Ok(()),
                        AsyncTaskBillingMutationOutcome::Existing(_) => {
                            Err(PersistentVideoTaskError::Invariant)
                        }
                    }
                }
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn load_billing(
        &self,
        claim: &AsyncTaskSubmissionRecord,
    ) -> Result<AsyncTaskBillingRecord, PersistentVideoTaskError> {
        self.billings
            .find(claim.principal().user_id(), claim.task_id())
            .await?
            .ok_or(PersistentVideoTaskError::Invariant)
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
            .map_err(PersistentVideoTaskError::from)
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
        video_resolution: Option<AsyncTaskVideoResolution>,
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
                video_resolution,
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

fn billing_mutation_record(
    outcome: AsyncTaskBillingMutationOutcome,
) -> Result<AsyncTaskBillingRecord, PersistentVideoTaskError> {
    match outcome {
        AsyncTaskBillingMutationOutcome::Applied(record)
        | AsyncTaskBillingMutationOutcome::Existing(record) => Ok(record),
        AsyncTaskBillingMutationOutcome::NotFound => Err(PersistentVideoTaskError::NotFound),
    }
}

fn restore_pricing(
    billing: &AsyncTaskBillingRecord,
) -> Result<XaiVideoPricingSnapshot, PersistentVideoTaskError> {
    if billing.price_card_version() != XAI_VIDEO_PRICE_CARD_VERSION {
        return Err(PersistentVideoTaskError::Billing);
    }
    let [group, group_model, peak] = billing.ratios();
    let ratios = PricingRatios::new(
        PricingRatio::new(group).map_err(|_| PersistentVideoTaskError::Billing)?,
        PricingRatio::new(group_model).map_err(|_| PersistentVideoTaskError::Billing)?,
        PricingRatio::new(peak).map_err(|_| PersistentVideoTaskError::Billing)?,
    );
    Ok(XaiVideoPricingSnapshot::restore(
        restore_billing_resolution(billing.resolution()),
        ratios,
    ))
}

fn video_billing_dimensions(
    claim: &AsyncTaskSubmissionRecord,
    poll: &CanonicalTaskPoll,
) -> Result<Option<BillingUsageDimensions>, PersistentVideoTaskError> {
    if !matches!(poll.status(), TaskStatus::Succeeded) {
        return Ok(None);
    }
    let output = poll
        .output()
        .and_then(|output| output.as_video())
        .ok_or(PersistentVideoTaskError::Invariant)?;
    let resolution = claim.video_resolution().map(restore_video_resolution);
    Ok(Some(BillingUsageDimensions::with_video(
        output.duration(),
        resolution,
    )))
}

const fn map_video_resolution(resolution: VideoResolution) -> AsyncTaskVideoResolution {
    match resolution {
        VideoResolution::P480 => AsyncTaskVideoResolution::P480,
        VideoResolution::P720 => AsyncTaskVideoResolution::P720,
        VideoResolution::P1080 => AsyncTaskVideoResolution::P1080,
    }
}

const fn map_billing_resolution(resolution: VideoResolution) -> AsyncTaskBillingResolution {
    match resolution {
        VideoResolution::P480 => AsyncTaskBillingResolution::P480,
        VideoResolution::P720 => AsyncTaskBillingResolution::P720,
        VideoResolution::P1080 => AsyncTaskBillingResolution::P1080,
    }
}

const fn restore_billing_resolution(resolution: AsyncTaskBillingResolution) -> VideoResolution {
    match resolution {
        AsyncTaskBillingResolution::P480 => VideoResolution::P480,
        AsyncTaskBillingResolution::P720 => VideoResolution::P720,
        AsyncTaskBillingResolution::P1080 => VideoResolution::P1080,
    }
}

const fn restore_video_resolution(resolution: AsyncTaskVideoResolution) -> VideoResolution {
    match resolution {
        AsyncTaskVideoResolution::P480 => VideoResolution::P480,
        AsyncTaskVideoResolution::P720 => VideoResolution::P720,
        AsyncTaskVideoResolution::P1080 => VideoResolution::P1080,
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

fn random_reservation_id() -> Result<BillingReservationId, PersistentVideoTaskError> {
    for _ in 0..2 {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| PersistentVideoTaskError::Entropy)?;
        if let Ok(value) = BillingReservationId::new(bytes) {
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
