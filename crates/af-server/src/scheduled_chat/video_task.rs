use std::{fmt, future::Future, pin::Pin, time::Duration};

use af_adapter::{RelayContext, get_task_adaptor};
use af_billing::{PricingRatio, PricingRatios, XaiVideoPricingSnapshot};
use af_db::{
    AsyncTaskRecord, AsyncTaskSubmissionRecord, SchedulerRuntimeCredentialRecord,
    SchedulerRuntimeHeader, SchedulerRuntimeTargetRecord,
};
use af_domain::{
    AfError, AsyncTaskBindingFingerprint, ChannelAutoBanRules, ChannelId, ChannelTimeout,
    CredentialId, GroupId, Protocol, TaskSubmission, UpstreamTaskId,
};
use af_protocol::{CanonicalTaskPoll, CanonicalVideoGenerationRequest, VideoModel};
use af_relay::{
    RelayAttemptReport, RelayError, RelayStateMachine, VideoTaskSubmissionCandidate,
    VideoTaskSubmissionDisposition, VideoTaskTarget, relay_video_task_poll,
    relay_video_task_submission,
};
use af_scheduler::{IndexedRoutePlan, RouteWaitKind};

use crate::{
    adaptor_credential::build_adaptor_credential,
    auto_ban_feedback::{ChannelAutoBanAttemptTarget, persist_channel_auto_ban_feedback},
    credential_feedback::{CredentialAttemptTarget, persist_credential_feedback},
    credential_order::{
        CredentialLoad, order_credentials, order_credentials_by_load,
        order_credentials_by_load_and_health, order_credentials_with_health,
    },
    health_runtime::{RuntimeHealthState, persist_scheduler_health_feedback},
};

use super::{ScheduledChatService, header_overrides, map_scheduler_error, target_matches_protocol};

mod fingerprint;

use fingerprint::binding_fingerprint;

/// xAI 视频任务提交计划的异步结果。
pub type VideoTaskSubmissionFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<BoundVideoTaskSubmission, VideoTaskSubmissionRuntimeError>>
            + Send
            + 'a,
    >,
>;

/// xAI 视频任务轮询的异步结果。
pub type VideoTaskPollFuture<'a> =
    Pin<Box<dyn Future<Output = Result<CanonicalTaskPoll, AfError>> + Send + 'a>>;

/// 不依赖公开 HTTP 路由的视频任务调度运行时。
///
/// 当前契约只负责提交前故障转移和提交后原目标轮询，不包含任务持久化、公开 API 或计费。
pub trait VideoTaskRuntime: Send + Sync {
    /// 在指定分组的当前快照中提交任务，并返回后续轮询必须持有的绑定。
    fn submit_video_task<'a>(
        &'a self,
        group_id: GroupId,
        request: CanonicalVideoGenerationRequest,
        request_id: &'a str,
    ) -> VideoTaskSubmissionFuture<'a>;

    /// 只使用提交成功时绑定的渠道、凭据和模型轮询任务。
    fn poll_video_task<'a>(
        &'a self,
        binding: &'a VideoTaskBinding,
        request_id: &'a str,
    ) -> VideoTaskPollFuture<'a>;
}

/// 视频运行时提交失败，并保留持久化 claim 所需的接受确定性。
pub struct VideoTaskSubmissionRuntimeError {
    error: AfError,
    disposition: VideoTaskSubmissionDisposition,
}

impl VideoTaskSubmissionRuntimeError {
    pub(super) fn new(error: AfError, disposition: VideoTaskSubmissionDisposition) -> Self {
        Self { error, disposition }
    }

    /// 返回是否可以安全释放提交 claim。
    #[must_use]
    pub const fn disposition(&self) -> VideoTaskSubmissionDisposition {
        self.disposition
    }

    /// 消费包装并返回统一业务错误。
    #[must_use]
    pub fn into_error(self) -> AfError {
        self.error
    }
}

impl fmt::Debug for VideoTaskSubmissionRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoTaskSubmissionRuntimeError")
            .field("error", &self.error)
            .field("disposition", &self.disposition)
            .finish()
    }
}

impl fmt::Display for VideoTaskSubmissionRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("视频任务运行时提交失败")
    }
}

impl std::error::Error for VideoTaskSubmissionRuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl From<AfError> for VideoTaskSubmissionRuntimeError {
    fn from(error: AfError) -> Self {
        Self::new(error, VideoTaskSubmissionDisposition::DefinitelyNotAccepted)
    }
}

/// 提交成功后必须随任务一起保存的原目标绑定。
#[derive(Clone)]
pub struct VideoTaskBinding {
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_revision: u64,
    upstream_task_id: UpstreamTaskId,
    expected_model: VideoModel,
    binding_fingerprint: AsyncTaskBindingFingerprint,
    target_snapshot: BoundVideoTargetSnapshot,
}

impl VideoTaskBinding {
    /// 返回任务提交成功的渠道标识。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回任务提交成功的凭据标识。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }

    /// 返回提交时完整密文封套派生的凭据版本。
    #[must_use]
    pub const fn credential_revision(&self) -> u64 {
        self.credential_revision
    }

    /// 返回后续轮询必须复用的上游任务标识。
    #[must_use]
    pub const fn upstream_task_id(&self) -> &UpstreamTaskId {
        &self.upstream_task_id
    }

    /// 返回提交时完成模型映射后的预期上游模型。
    #[must_use]
    pub const fn expected_model(&self) -> &VideoModel {
        &self.expected_model
    }

    /// 返回完整运行时目标的脱敏 SHA-256 绑定指纹。
    #[must_use]
    pub const fn binding_fingerprint(&self) -> AsyncTaskBindingFingerprint {
        self.binding_fingerprint
    }

    /// 返回提交时固化的账号并发等待时间。
    #[must_use]
    pub const fn attempt_timeout(&self) -> Duration {
        self.target_snapshot.attempt_timeout
    }
}

impl fmt::Debug for VideoTaskBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoTaskBinding")
            .field("channel_id", &self.channel_id)
            .field("credential_id", &self.credential_id)
            .field("credential_revision", &"<已脱敏>")
            .field("upstream_task_id", &self.upstream_task_id)
            .field("expected_model", &"<已脱敏>")
            .field("binding_fingerprint", &self.binding_fingerprint)
            .finish()
    }
}

/// 视频任务提交结果及其不可变原目标绑定。
pub struct BoundVideoTaskSubmission {
    target_group_id: GroupId,
    submission: TaskSubmission,
    binding: VideoTaskBinding,
}

impl BoundVideoTaskSubmission {
    /// 返回调度计划实际命中的计费分组，供后续持久化切片复用。
    #[must_use]
    pub const fn target_group_id(&self) -> GroupId {
        self.target_group_id
    }

    /// 返回规范任务提交句柄。
    #[must_use]
    pub const fn submission(&self) -> &TaskSubmission {
        &self.submission
    }

    /// 返回后续轮询必须持有的原目标绑定。
    #[must_use]
    pub const fn binding(&self) -> &VideoTaskBinding {
        &self.binding
    }

    /// 消费结果并拆分任务句柄和原目标绑定。
    #[must_use]
    pub fn into_parts(self) -> (GroupId, TaskSubmission, VideoTaskBinding) {
        (self.target_group_id, self.submission, self.binding)
    }
}

impl fmt::Debug for BoundVideoTaskSubmission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundVideoTaskSubmission")
            .field("target_group_id", &self.target_group_id)
            .field("submission", &self.submission)
            .field("binding", &self.binding)
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
struct BoundVideoTargetSnapshot {
    channel_type: af_domain::ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    headers: Vec<SchedulerRuntimeHeader>,
    auto_ban_rules: ChannelAutoBanRules,
    pool_mode: bool,
    credential: SchedulerRuntimeCredentialRecord,
    requested_model: VideoModel,
    expected_model: VideoModel,
    attempt_timeout: Duration,
}

impl BoundVideoTargetSnapshot {
    fn new(
        target: &SchedulerRuntimeTargetRecord,
        credential: &SchedulerRuntimeCredentialRecord,
        requested_model: VideoModel,
        expected_model: VideoModel,
        attempt_timeout: Duration,
    ) -> Self {
        Self {
            channel_type: target.channel_type(),
            protocol: target.protocol(),
            base_url: target.base_url().map(str::to_owned),
            timeout: target.timeout(),
            headers: target.headers().to_vec(),
            auto_ban_rules: target.auto_ban_rules().clone(),
            pool_mode: target.pool_mode(),
            credential: credential.clone(),
            requested_model,
            expected_model,
            attempt_timeout,
        }
    }

    fn resolve_credential<'a>(
        &self,
        target: &'a SchedulerRuntimeTargetRecord,
    ) -> Option<&'a SchedulerRuntimeCredentialRecord> {
        let mapped_model = target
            .mapped_model(self.requested_model.as_str())
            .unwrap_or(self.requested_model.as_str());
        if target.channel_type() != self.channel_type
            || target.protocol() != self.protocol
            || target.base_url() != self.base_url.as_deref()
            || target.timeout() != self.timeout
            || target.headers() != self.headers
            || target.auto_ban_rules() != &self.auto_ban_rules
            || target.pool_mode() != self.pool_mode
            || !target.parameter_overrides().is_empty()
            || mapped_model != self.expected_model.as_str()
        {
            return None;
        }
        target
            .credentials()
            .iter()
            .find(|credential| credential.credential_id() == self.credential.credential_id())
            .filter(|credential| *credential == &self.credential)
    }
}

impl fmt::Debug for BoundVideoTargetSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundVideoTargetSnapshot")
            .field("channel_type", &self.channel_type)
            .field("protocol", &self.protocol)
            .field("has_base_url", &self.base_url.is_some())
            .field("timeout", &self.timeout)
            .field("header_count", &self.headers.len())
            .field("auto_ban_rules", &self.auto_ban_rules)
            .field("pool_mode", &self.pool_mode)
            .field("credential", &self.credential)
            .field("requested_model", &"<已脱敏>")
            .field("expected_model", &"<已脱敏>")
            .field("attempt_timeout", &self.attempt_timeout)
            .finish()
    }
}

#[derive(Clone)]
struct SubmissionBindingTarget {
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_revision: u64,
    snapshot: BoundVideoTargetSnapshot,
}

impl SubmissionBindingTarget {
    fn into_binding(
        self,
        target_group_id: GroupId,
        upstream_task_id: UpstreamTaskId,
    ) -> VideoTaskBinding {
        let binding_fingerprint = self.fingerprint(target_group_id);
        VideoTaskBinding {
            channel_id: self.channel_id,
            credential_id: self.credential_id,
            credential_revision: self.credential_revision,
            upstream_task_id,
            expected_model: self.snapshot.expected_model.clone(),
            binding_fingerprint,
            target_snapshot: self.snapshot,
        }
    }

    fn fingerprint(&self, target_group_id: GroupId) -> AsyncTaskBindingFingerprint {
        binding_fingerprint(
            target_group_id,
            self.channel_id,
            self.credential_id,
            self.credential_revision,
            &self.snapshot,
        )
    }
}

impl VideoTaskRuntime for ScheduledChatService {
    fn submit_video_task<'a>(
        &'a self,
        group_id: GroupId,
        request: CanonicalVideoGenerationRequest,
        request_id: &'a str,
    ) -> VideoTaskSubmissionFuture<'a> {
        Box::pin(async move {
            let route = self.video_route(group_id, request.model().as_str()).await?;
            let credential_health = self
                .load_credential_health(&route, Protocol::XaiVideo, request.model().as_str())
                .await?;
            let account_loads = self
                .load_account_concurrency(&route, Protocol::XaiVideo, request.model().as_str())
                .await?;
            self.execute_video_submission(
                route,
                request,
                request_id,
                account_loads.as_ref(),
                credential_health.as_ref(),
            )
            .await
        })
    }

    fn poll_video_task<'a>(
        &'a self,
        binding: &'a VideoTaskBinding,
        request_id: &'a str,
    ) -> VideoTaskPollFuture<'a> {
        Box::pin(async move { self.execute_video_poll(binding, request_id).await })
    }
}

impl ScheduledChatService {
    pub(super) async fn video_route(
        &self,
        group_id: GroupId,
        model: &str,
    ) -> Result<IndexedRoutePlan, AfError> {
        if self.health.is_some() {
            let (channel_ids, channel_health) = self.load_channel_health(group_id, model).await?;
            return self
                .scheduler
                .route_plan_with_health(group_id, model, &channel_health)
                .map_err(map_scheduler_error)?
                .ok_or_else(|| {
                    AfError::from(if channel_ids.is_empty() {
                        af_domain::UpstreamError::ModelUnsupported
                    } else {
                        af_domain::UpstreamError::overloaded()
                    })
                });
        }
        self.scheduler
            .route_plan(group_id, model)
            .map_err(map_scheduler_error)?
            .ok_or_else(|| af_domain::UpstreamError::ModelUnsupported.into())
    }

    /// 只在当前路由仍命中冻结目标分组时提交视频任务。
    pub(super) async fn submit_video_task_to_target(
        &self,
        group_id: GroupId,
        expected_target_group_id: GroupId,
        request: CanonicalVideoGenerationRequest,
        request_id: &str,
    ) -> Result<BoundVideoTaskSubmission, VideoTaskSubmissionRuntimeError> {
        let route = self.video_route(group_id, request.model().as_str()).await?;
        if route.target_group_id() != expected_target_group_id {
            return Err(AfError::Internal.into());
        }
        let credential_health = self
            .load_credential_health(&route, Protocol::XaiVideo, request.model().as_str())
            .await?;
        let account_loads = self
            .load_account_concurrency(&route, Protocol::XaiVideo, request.model().as_str())
            .await?;
        self.execute_video_submission(
            route,
            request,
            request_id,
            account_loads.as_ref(),
            credential_health.as_ref(),
        )
        .await
    }

    async fn execute_video_submission(
        &self,
        route: IndexedRoutePlan,
        request: CanonicalVideoGenerationRequest,
        request_id: &str,
        account_loads: Option<&std::collections::BTreeMap<i64, CredentialLoad>>,
        credential_health: Option<&std::collections::BTreeMap<i64, RuntimeHealthState>>,
    ) -> Result<BoundVideoTaskSubmission, VideoTaskSubmissionRuntimeError> {
        let target_group_id = route.target_group_id();
        let mut candidates = Vec::with_capacity(
            route
                .candidates()
                .len()
                .min(RelayStateMachine::MAX_CANDIDATES),
        );
        let mut binding_targets = Vec::with_capacity(candidates.capacity());
        let mut attempt_targets = Vec::with_capacity(candidates.capacity());
        let mut auto_ban_targets = Vec::with_capacity(candidates.capacity());
        let mut health_filtered = false;
        let pricing = XaiVideoPricingSnapshot::new(
            request.resolution(),
            PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE),
        );

        'channels: for candidate in route.candidates() {
            let target = candidate.runtime_target();
            if !target_matches_protocol(target, Protocol::XaiVideo)
                || !target.parameter_overrides().is_empty()
            {
                continue;
            }
            let upstream_model = target
                .mapped_model(request.model().as_str())
                .unwrap_or(request.model().as_str());
            let upstream_model =
                VideoModel::new(upstream_model.to_owned()).map_err(|_| AfError::Internal)?;
            if pricing.rate_microusd(upstream_model.as_str()).is_err() {
                continue;
            }
            let mapped_request = CanonicalVideoGenerationRequest::new(
                upstream_model.clone(),
                request.prompt().clone(),
                request.duration(),
                request.aspect_ratio(),
                request.resolution(),
            );
            let adaptor = get_task_adaptor(target.channel_type(), target.protocol())
                .map_err(|_| AfError::Internal)?;
            let headers = header_overrides(target)?;
            let wait_plan = candidate.wait_plan();
            let ordered = ordered_credentials(
                request_id,
                target,
                wait_plan.kind(),
                account_loads,
                credential_health,
            );
            health_filtered |= !target.credentials().is_empty() && ordered.is_empty();

            for runtime_credential in ordered {
                if candidates.len() == RelayStateMachine::MAX_CANDIDATES {
                    break 'channels;
                }
                let credential_id = CredentialId::new(runtime_credential.credential_id())
                    .map_err(|_| AfError::Internal)?;
                let secret_owner_id = runtime_credential.secret_owner_id();
                let client = self.client_for_credential(target, runtime_credential)?;
                let mut context = RelayContext::new(client);
                if let Some(base_url) = target.base_url() {
                    context = context
                        .with_base_url(base_url)
                        .map_err(|_| AfError::Internal)?;
                }
                context = context
                    .with_request_id(request_id.to_owned())
                    .map_err(|_| AfError::Internal)?;
                let decrypted = self
                    .decryptor
                    .decrypt_envelope(
                        target.channel_id(),
                        secret_owner_id.get(),
                        runtime_credential.credential_kind(),
                        runtime_credential.envelope(),
                    )
                    .map_err(|_| AfError::Internal)?;
                let oauth_has_refresh_token = decrypted.oauth_has_refresh_token();
                let credential =
                    build_adaptor_credential(&decrypted).map_err(|_| AfError::Internal)?;
                let mut relay_target = VideoTaskTarget::new(adaptor.clone(), context, credential)
                    .with_header_overrides(headers.clone())
                    .with_channel_group(target.channel_id());
                if let Some(concurrency) = &self.concurrency {
                    relay_target = relay_target.with_attempt_gate(concurrency.account_gate(
                        runtime_credential.concurrency_owner_id(),
                        runtime_credential.concurrency(),
                        wait_plan.timeout(),
                    ));
                }
                candidates.push(VideoTaskSubmissionCandidate::new(
                    relay_target,
                    mapped_request.clone(),
                ));
                binding_targets.push(SubmissionBindingTarget {
                    channel_id: target.channel_id(),
                    credential_id,
                    credential_revision: runtime_credential.credential_revision(),
                    snapshot: BoundVideoTargetSnapshot::new(
                        target,
                        runtime_credential,
                        request.model().clone(),
                        upstream_model.clone(),
                        wait_plan.timeout(),
                    ),
                });
                attempt_targets.push(CredentialAttemptTarget::with_runtime_identity(
                    target.channel_id(),
                    credential_id,
                    secret_owner_id,
                    runtime_credential.shared_health_id(),
                    runtime_credential.credential_kind(),
                    oauth_has_refresh_token,
                    target.pool_mode(),
                )?);
                auto_ban_targets.push(ChannelAutoBanAttemptTarget::new(
                    target.channel_id(),
                    target.auto_ban_rules(),
                    target.pool_mode(),
                )?);
            }
        }
        if candidates.is_empty() {
            return Err(AfError::from(if health_filtered {
                af_domain::UpstreamError::overloaded()
            } else {
                af_domain::UpstreamError::ModelUnsupported
            })
            .into());
        }

        match relay_video_task_submission(&candidates).await {
            Ok(outcome) => {
                let (submission, report) = outcome.into_parts();
                let index = report
                    .successful_candidate_index()
                    .ok_or(AfError::Internal)?;
                let binding_target = binding_targets
                    .get(index)
                    .cloned()
                    .ok_or(AfError::Internal)?;
                let binding =
                    binding_target.into_binding(target_group_id, submission.task_id().clone());
                self.persist_video_feedback(&attempt_targets, &auto_ban_targets, &report)
                    .await;
                Ok(BoundVideoTaskSubmission {
                    target_group_id,
                    submission,
                    binding,
                })
            }
            Err(error) => {
                let (error, report, disposition) = error.into_parts();
                self.persist_video_feedback(&attempt_targets, &auto_ban_targets, &report)
                    .await;
                Err(VideoTaskSubmissionRuntimeError::new(
                    map_relay_error(error),
                    disposition,
                ))
            }
        }
    }

    async fn execute_video_poll(
        &self,
        binding: &VideoTaskBinding,
        request_id: &str,
    ) -> Result<CanonicalTaskPoll, AfError> {
        let target = self
            .scheduler
            .runtime_target(binding.channel_id)
            .map_err(map_scheduler_error)?
            .ok_or(AfError::Internal)?;
        let runtime_credential = binding
            .target_snapshot
            .resolve_credential(&target)
            .ok_or(AfError::Internal)?;
        if runtime_credential.credential_revision() != binding.credential_revision
            || runtime_credential.credential_id() != binding.credential_id.get()
        {
            return Err(AfError::Internal);
        }
        let adaptor = get_task_adaptor(target.channel_type(), target.protocol())
            .map_err(|_| AfError::Internal)?;
        let client = self.client_for_credential(&target, runtime_credential)?;
        let mut context = RelayContext::new(client);
        if let Some(base_url) = target.base_url() {
            context = context
                .with_base_url(base_url)
                .map_err(|_| AfError::Internal)?;
        }
        context = context
            .with_request_id(request_id.to_owned())
            .map_err(|_| AfError::Internal)?;
        let decrypted = self
            .decryptor
            .decrypt_envelope(
                target.channel_id(),
                runtime_credential.secret_owner_id().get(),
                runtime_credential.credential_kind(),
                runtime_credential.envelope(),
            )
            .map_err(|_| AfError::Internal)?;
        let oauth_has_refresh_token = decrypted.oauth_has_refresh_token();
        let credential = build_adaptor_credential(&decrypted).map_err(|_| AfError::Internal)?;
        let mut relay_target = VideoTaskTarget::new(adaptor, context, credential)
            .with_header_overrides(header_overrides(&target)?)
            .with_channel_group(target.channel_id());
        if let Some(concurrency) = &self.concurrency {
            relay_target = relay_target.with_attempt_gate(concurrency.account_gate(
                runtime_credential.concurrency_owner_id(),
                runtime_credential.concurrency(),
                binding.target_snapshot.attempt_timeout,
            ));
        }
        let attempt_targets = [CredentialAttemptTarget::with_runtime_identity(
            target.channel_id(),
            runtime_credential.routing_credential_id(),
            runtime_credential.secret_owner_id(),
            runtime_credential.shared_health_id(),
            runtime_credential.credential_kind(),
            oauth_has_refresh_token,
            target.pool_mode(),
        )?];
        let auto_ban_targets = [ChannelAutoBanAttemptTarget::new(
            target.channel_id(),
            target.auto_ban_rules(),
            target.pool_mode(),
        )?];

        match relay_video_task_poll(
            &relay_target,
            &binding.upstream_task_id,
            &binding.expected_model,
        )
        .await
        {
            Ok(outcome) => {
                let (poll, report) = outcome.into_parts();
                self.persist_video_feedback(&attempt_targets, &auto_ban_targets, &report)
                    .await;
                Ok(poll)
            }
            Err(error) => {
                let (error, report) = error.into_parts();
                self.persist_video_feedback(&attempt_targets, &auto_ban_targets, &report)
                    .await;
                Err(map_relay_error(error))
            }
        }
    }

    /// 从数据库绑定事实恢复当前运行时目标；任一配置漂移都失败关闭。
    pub(super) fn restore_video_task_binding(
        &self,
        task: &AsyncTaskRecord,
        claim: &AsyncTaskSubmissionRecord,
    ) -> Result<VideoTaskBinding, AfError> {
        if !claim.matches_task(task) || task.protocol() != Protocol::XaiVideo {
            return Err(AfError::Internal);
        }
        let target_group_id = claim.target_group_id().ok_or(AfError::Internal)?;
        let attempt_timeout = claim.attempt_timeout().ok_or(AfError::Internal)?;
        let expected_fingerprint = claim.binding_fingerprint().ok_or(AfError::Internal)?;
        let target = self
            .scheduler
            .runtime_target(task.channel_id())
            .map_err(map_scheduler_error)?
            .ok_or(AfError::Internal)?;
        if !target_matches_protocol(&target, Protocol::XaiVideo)
            || !target.parameter_overrides().is_empty()
        {
            return Err(AfError::Internal);
        }
        let requested_model =
            VideoModel::new(task.requested_model().to_owned()).map_err(|_| AfError::Internal)?;
        let expected_model =
            VideoModel::new(task.upstream_model().to_owned()).map_err(|_| AfError::Internal)?;
        let mapped_model = target
            .mapped_model(requested_model.as_str())
            .unwrap_or(requested_model.as_str());
        if mapped_model != expected_model.as_str() {
            return Err(AfError::Internal);
        }
        let runtime_credential = target
            .credentials()
            .iter()
            .find(|credential| credential.credential_id() == task.credential_id().get())
            .filter(|credential| credential.credential_revision() == task.credential_revision())
            .ok_or(AfError::Internal)?;
        let binding_target = SubmissionBindingTarget {
            channel_id: task.channel_id(),
            credential_id: task.credential_id(),
            credential_revision: task.credential_revision(),
            snapshot: BoundVideoTargetSnapshot::new(
                &target,
                runtime_credential,
                requested_model,
                expected_model,
                attempt_timeout,
            ),
        };
        if binding_target.fingerprint(target_group_id) != expected_fingerprint {
            return Err(AfError::Internal);
        }
        Ok(binding_target.into_binding(target_group_id, task.upstream_task_id().clone()))
    }

    async fn persist_video_feedback(
        &self,
        attempt_targets: &[CredentialAttemptTarget],
        auto_ban_targets: &[ChannelAutoBanAttemptTarget],
        report: &RelayAttemptReport,
    ) {
        let ((), (), ()) = tokio::join!(
            persist_credential_feedback(
                &self.credential_states,
                &self.scheduler,
                attempt_targets,
                report,
            ),
            persist_scheduler_health_feedback(self.health.as_ref(), attempt_targets, report),
            persist_channel_auto_ban_feedback(
                self.channel_states.as_ref(),
                &self.scheduler,
                auto_ban_targets,
                report,
            ),
        );
    }
}

fn ordered_credentials<'a>(
    request_id: &str,
    target: &'a SchedulerRuntimeTargetRecord,
    wait_kind: RouteWaitKind,
    account_loads: Option<&std::collections::BTreeMap<i64, CredentialLoad>>,
    credential_health: Option<&std::collections::BTreeMap<i64, RuntimeHealthState>>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    match (wait_kind, account_loads, credential_health) {
        (RouteWaitKind::Fallback, Some(loads), Some(health)) => {
            order_credentials_by_load_and_health(
                request_id,
                target.channel_id(),
                target.credentials(),
                loads,
                health,
            )
        }
        (RouteWaitKind::Fallback, Some(loads), None) => {
            order_credentials_by_load(request_id, target.channel_id(), target.credentials(), loads)
        }
        (_, _, Some(health)) => order_credentials_with_health(
            request_id,
            target.channel_id(),
            target.credentials(),
            health,
        ),
        _ => order_credentials(request_id, target.channel_id(), target.credentials()),
    }
}

const fn map_relay_error(error: RelayError) -> AfError {
    match error {
        RelayError::InvalidModel | RelayError::Request(_) => AfError::InvalidRequest,
        RelayError::ConcurrencyUnavailable => AfError::ConcurrencyLimited,
        RelayError::Upstream(error) => AfError::Upstream(error),
        RelayError::Adaptor(_) | RelayError::AttemptGateFailed => AfError::Internal,
        _ => AfError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener, TcpStream},
        sync::{Arc, RwLock},
        thread,
        time::Duration,
    };

    use af_account::{CredentialDecryptor, SystemSecretCipher, credential_plaintext_aad};
    use af_billing::{DatabaseUsageRecordSink, GroupPricingCache};
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use af_db::{
        AsyncTaskBillingRepository, AsyncTaskRepository, AsyncTaskSubmissionRepository,
        ChannelModelMappings, ChannelParameterOverrides, CredentialProxyScheme,
        CredentialStateRepository, EncryptedCredentialEnvelope, QuotaRepository,
        SchedulerRuntimeProxyRecord, UsageLogRepository,
    };
    use af_domain::{
        AsyncTaskId, AsyncTaskRequestId, ChannelType, CredentialKind, GatewayPrincipal, ProxyId,
        TaskStatus, TokenId, UserId,
    };
    use af_httpclient::{HttpClientConfig, HttpClientProvider};
    use af_protocol::{VideoDuration, VideoPrompt, VideoResolution};
    use af_scheduler::{
        ChannelIndexSource, ChannelIndexSourceFuture, ChannelIndexSourceRecord,
        InMemoryChannelIndex, IndexedWeightedScheduler,
    };
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use chacha20poly1305::{
        XChaCha20Poly1305, XNonce,
        aead::{Aead as _, KeyInit as _, Payload},
    };
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    use serde_json::json;

    use crate::test_database::{SqliteTestDatabase, test_encrypted_envelope_json};

    use super::*;

    const CHANNEL_ID: i64 = 71;
    const FIRST_CREDENTIAL_ID: i64 = 81;
    const SECOND_CREDENTIAL_ID: i64 = 82;
    const GROUP_ID: i64 = 7;
    const USER_ID: i64 = 61;
    const TOKEN_ID: i64 = 62;
    const KEY_ID: &str = "video-task-runtime-test";
    const REQUESTED_MODEL: &str = "video-public";
    const UPSTREAM_MODEL: &str = "grok-imagine-video-1.5";

    #[tokio::test]
    async fn submission_failover_binds_second_credential_for_polling() {
        let steps = vec![
            ProxyStep::new(
                "POST http://upstream.example/v1/videos/generations",
                "Bearer private-key-first",
                "401 Unauthorized",
                br#"{"error":{"type":"authentication_error","message":"expired"}}"#,
            ),
            ProxyStep::new(
                "POST http://upstream.example/v1/videos/generations",
                "Bearer private-key-second",
                "200 OK",
                br#"{"request_id":"video-task-bound"}"#,
            ),
            ProxyStep::new(
                "GET http://upstream.example/v1/videos/video-task-bound",
                "Bearer private-key-second",
                "200 OK",
                br#"{"status":"done","video":{"url":"https://vidgen.x.ai/out.mp4","duration":8,"respect_moderation":true},"model":"grok-imagine-video-1.5"}"#,
            ),
        ];
        let (proxy_address, proxy_server) = spawn_proxy(steps);
        let source = MutableSource::new(vec![runtime_record(proxy_address, 0x31, 0x32)]);
        let (service, database, principal) = service(source).await;
        let tasks =
            AsyncTaskRepository::new(database.pool().clone(), Duration::from_secs(5)).unwrap();
        let submissions =
            AsyncTaskSubmissionRepository::new(database.pool().clone(), Duration::from_secs(5))
                .unwrap();
        let billings =
            AsyncTaskBillingRepository::new(database.pool().clone(), Duration::from_secs(5))
                .unwrap();
        let group_pricing = GroupPricingCache::load_from_database(database.pool().clone())
            .await
            .unwrap();
        let quota = Arc::new(QuotaRepository::new(database.pool().clone()));
        let usage = Arc::new(DatabaseUsageRecordSink::new(UsageLogRepository::new(
            database.pool().clone(),
        )));
        let billing_ports =
            crate::scheduled_chat::video_task_persistence::PersistentVideoTaskBillingPorts::new(
                quota.clone(),
                quota.clone(),
                quota,
                usage,
            );
        let coordinator =
            crate::scheduled_chat::video_task_persistence::PersistentVideoTaskCoordinator::new(
                service,
                tasks.clone(),
                submissions.clone(),
                billings,
                group_pricing,
                billing_ports,
            );
        let request_id = AsyncTaskRequestId::new([0x61; 16]).unwrap();

        let submitted = match coordinator
            .submit(
                AsyncTaskId::new([0x51; 16]).unwrap(),
                request_id,
                principal,
                request(),
                "video-submit-request",
            )
            .await
        {
            Ok(submitted) => submitted,
            Err(error) => {
                let claim_state = submissions
                    .find_by_request(principal.user_id(), request_id)
                    .await
                    .map(|record| record.map(|record| record.state()));
                let task_exists = tasks
                    .find_by_request(principal.user_id(), request_id)
                    .await
                    .map(|record| record.is_some());
                panic!(
                    "视频代理持久化未闭合：error={error:?}, claim_state={claim_state:?}, task_exists={task_exists:?}"
                );
            }
        };

        assert_eq!(submitted.credential_id().get(), SECOND_CREDENTIAL_ID);
        assert_eq!(submitted.upstream_task_id().as_str(), "video-task-bound");
        let poll = coordinator
            .poll(
                principal.user_id(),
                submitted.task_id(),
                "video-poll-request",
            )
            .await
            .unwrap();
        let crate::scheduled_chat::video_task_persistence::PersistentVideoTaskPollOutcome::Updated {
            task,
            poll,
            billing_dimensions,
        } = poll
        else {
            panic!("首次终态轮询必须访问持久化绑定")
        };
        assert_eq!(task.status(), &TaskStatus::Succeeded);
        assert_eq!(poll.status(), &TaskStatus::Succeeded);
        let dimensions = billing_dimensions.expect("成功视频终态必须产生计费审计维度");
        assert_eq!(dimensions.video_duration().unwrap().seconds(), 8);
        assert_eq!(dimensions.video_resolution(), Some(VideoResolution::P720));
        assert_eq!(
            poll.output().unwrap().as_video().unwrap().model().as_str(),
            UPSTREAM_MODEL
        );
        let debug = format!("{task:?}");
        for secret in [
            "private-key-first",
            "private-key-second",
            "video-task-bound",
            UPSTREAM_MODEL,
        ] {
            assert!(!debug.contains(secret));
        }
        proxy_server.join().unwrap();
        database.close().await;
    }

    #[tokio::test]
    async fn polling_fails_closed_after_the_bound_credential_is_replaced() {
        let (proxy_address, proxy_server) = spawn_proxy(vec![ProxyStep::new(
            "POST http://upstream.example/v1/videos/generations",
            "Bearer private-key-second",
            "200 OK",
            br#"{"request_id":"video-task-bound"}"#,
        )]);
        let source = MutableSource::new(vec![single_runtime_record(
            proxy_address,
            SECOND_CREDENTIAL_ID,
            0x32,
            "private-key-second",
        )]);
        let (service, database, _) = service(source.clone()).await;
        let submitted = service
            .submit_video_task(
                GroupId::new(GROUP_ID).unwrap(),
                request(),
                "video-submit-before-replace",
            )
            .await
            .unwrap();
        proxy_server.join().unwrap();

        source.replace(vec![single_runtime_record(
            proxy_address,
            SECOND_CREDENTIAL_ID,
            0x44,
            "private-key-replaced",
        )]);
        service.scheduler.refresh().await.unwrap();

        assert_eq!(
            service
                .poll_video_task(submitted.binding(), "video-poll-after-replace")
                .await,
            Err(AfError::Internal)
        );
        database.close().await;
    }

    async fn service(
        source: MutableSource,
    ) -> (ScheduledChatService, SqliteTestDatabase, GatewayPrincipal) {
        let index = InMemoryChannelIndex::load(Arc::new(source)).await.unwrap();
        let settings = encryption_settings();
        let database = SqliteTestDatabase::new("video-runtime").await;
        seed_persistence_rows(&database).await;
        let service = ScheduledChatService::new(
            IndexedWeightedScheduler::new(index),
            CredentialDecryptor::new(&settings).unwrap(),
            HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap(),
            CredentialStateRepository::new(database.pool().clone()),
        )
        .with_proxy_cipher(SystemSecretCipher::new(&settings).unwrap());
        (
            service,
            database,
            GatewayPrincipal::new(
                TokenId::new(TOKEN_ID).unwrap(),
                UserId::new(USER_ID).unwrap(),
                GroupId::new(GROUP_ID).unwrap(),
            ),
        )
    }

    async fn seed_persistence_rows(database: &SqliteTestDatabase) {
        for sql in [
            format!(
                "INSERT INTO groups (id, name, display_name, flags) VALUES ({GROUP_ID}, 'video-runtime', '视频运行时测试', '{{}}')"
            ),
            format!(
                "INSERT INTO users (id, username, default_group_id, aff_code, quota, settings) VALUES ({USER_ID}, 'video-runtime', {GROUP_ID}, 'video-runtime-aff', 10000000, '{{}}')"
            ),
            format!(
                "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, group_id, remain_quota) VALUES ({TOKEN_ID}, {USER_ID}, '{}', 'sk-video-runtime', '视频运行时令牌', 1, {GROUP_ID}, 10000000)",
                "cd".repeat(32),
            ),
            format!(
                "INSERT INTO channels (id, name, \"type\", protocol, model_mapping, param_override, header_override, settings) VALUES ({CHANNEL_ID}, '视频运行时渠道', 'xai', 'xai_video', '{{}}', '{{}}', '{{}}', '{{}}')"
            ),
        ] {
            database.seed().execute_unprepared(&sql).await.unwrap();
        }
        for (credential_id, marker) in [(FIRST_CREDENTIAL_ID, 0x31), (SECOND_CREDENTIAL_ID, 0x32)] {
            database
                .seed()
                .execute(Statement::from_sql_and_values(
                    DbBackend::Sqlite,
                    "INSERT INTO credentials (id, channel_id, kind, secret) VALUES (?, ?, 'api_key', ?)",
                    [
                        credential_id.into(),
                        CHANNEL_ID.into(),
                        test_encrypted_envelope_json(marker).into(),
                    ],
                ))
                .await
                .unwrap();
        }
    }

    fn runtime_record(
        proxy_address: SocketAddr,
        first_marker: u8,
        second_marker: u8,
    ) -> ChannelIndexSourceRecord {
        let first = runtime_credential(
            proxy_address,
            FIRST_CREDENTIAL_ID,
            first_marker,
            "private-key-first",
            20,
        );
        let second = runtime_credential(
            proxy_address,
            SECOND_CREDENTIAL_ID,
            second_marker,
            "private-key-second",
            10,
        );
        record_for_credentials(vec![first, second])
    }

    fn single_runtime_record(
        proxy_address: SocketAddr,
        credential_id: i64,
        marker: u8,
        api_key: &str,
    ) -> ChannelIndexSourceRecord {
        record_for_credentials(vec![runtime_credential(
            proxy_address,
            credential_id,
            marker,
            api_key,
            10,
        )])
    }

    fn record_for_credentials(
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
    ) -> ChannelIndexSourceRecord {
        let mappings = ChannelModelMappings::parse(&json!({
            "video-public": UPSTREAM_MODEL,
        }))
        .unwrap();
        let target = SchedulerRuntimeTargetRecord::new_pool_with_request_policy(
            ChannelId::new(CHANNEL_ID).unwrap(),
            ChannelType::Xai,
            Protocol::XaiVideo,
            Some("http://upstream.example".to_owned()),
            credentials,
            mappings,
            ChannelParameterOverrides::default(),
            vec![("x-video-runtime".to_owned(), "enabled".to_owned())],
        )
        .unwrap();
        ChannelIndexSourceRecord::with_runtime_target(
            GroupId::new(GROUP_ID).unwrap(),
            REQUESTED_MODEL,
            10,
            0,
            Arc::new(target),
        )
        .unwrap()
    }

    fn runtime_credential(
        proxy_address: SocketAddr,
        credential_id: i64,
        marker: u8,
        api_key: &str,
        priority: i32,
    ) -> SchedulerRuntimeCredentialRecord {
        let proxy = SchedulerRuntimeProxyRecord::new(
            ProxyId::new(credential_id).unwrap(),
            CredentialProxyScheme::Http,
            proxy_address.ip().to_string(),
            proxy_address.port(),
            None,
            None,
            true,
            1,
        )
        .unwrap();
        SchedulerRuntimeCredentialRecord::with_scheduling(
            credential_id,
            CredentialKind::ApiKey,
            encrypted_api_key(credential_id, marker, api_key),
            true,
            priority,
            10,
        )
        .unwrap()
        .with_proxy(proxy)
        .unwrap()
    }

    fn request() -> CanonicalVideoGenerationRequest {
        CanonicalVideoGenerationRequest::new(
            VideoModel::new(REQUESTED_MODEL).unwrap(),
            VideoPrompt::new("private prompt").unwrap(),
            Some(VideoDuration::new(8).unwrap()),
            None,
            Some(VideoResolution::P720),
        )
    }

    fn encryption_settings() -> CredentialEncryptionSettings {
        serde_json::from_value(json!({
            "key_id": KEY_ID,
            "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES]),
        }))
        .unwrap()
    }

    fn encrypted_api_key(
        credential_id: i64,
        marker: u8,
        api_key: &str,
    ) -> EncryptedCredentialEnvelope {
        let plaintext = serde_json::to_vec(&json!({
            "kind": "api_key",
            "api_key": api_key,
        }))
        .unwrap();
        let key = [0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES];
        let nonce = [marker; 24];
        let cipher = XChaCha20Poly1305::new_from_slice(&key).unwrap();
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &plaintext,
                    aad: &credential_plaintext_aad(
                        ChannelId::new(CHANNEL_ID).unwrap(),
                        credential_id,
                        CredentialKind::ApiKey,
                    ),
                },
            )
            .unwrap();
        EncryptedCredentialEnvelope::new(KEY_ID, nonce, ciphertext).unwrap()
    }

    #[derive(Clone)]
    struct MutableSource {
        records: Arc<RwLock<Vec<ChannelIndexSourceRecord>>>,
    }

    impl MutableSource {
        fn new(records: Vec<ChannelIndexSourceRecord>) -> Self {
            Self {
                records: Arc::new(RwLock::new(records)),
            }
        }

        fn replace(&self, records: Vec<ChannelIndexSourceRecord>) {
            *self.records.write().unwrap() = records;
        }
    }

    impl ChannelIndexSource for MutableSource {
        fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a> {
            let records = self.records.read().unwrap().clone();
            Box::pin(async move { Ok(records) })
        }
    }

    struct ProxyStep {
        request_line: &'static str,
        authorization: &'static str,
        status: &'static str,
        body: &'static [u8],
    }

    impl ProxyStep {
        const fn new(
            request_line: &'static str,
            authorization: &'static str,
            status: &'static str,
            body: &'static [u8],
        ) -> Self {
            Self {
                request_line,
                authorization,
                status,
                body,
            }
        }
    }

    fn spawn_proxy(steps: Vec<ProxyStep>) -> (SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for step in steps {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let request = read_request(&mut stream);
                assert!(request.starts_with(step.request_line), "{request}");
                assert!(
                    request.to_ascii_lowercase().contains(
                        &format!("authorization: {}", step.authorization).to_ascii_lowercase()
                    ),
                    "{request}"
                );
                let response = format!(
                    "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    step.status,
                    step.body.len(),
                    String::from_utf8_lossy(step.body),
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });
        (address, server)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1_024];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            assert!(request.len() < 32 * 1_024);
        }
        String::from_utf8(request).unwrap()
    }
}
