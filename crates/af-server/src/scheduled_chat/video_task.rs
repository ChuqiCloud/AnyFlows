use std::{fmt, future::Future, pin::Pin, sync::Arc, time::Duration};

use af_adapter::{RelayContext, get_task_adaptor};
use af_db::{
    SchedulerRuntimeCredentialRecord, SchedulerRuntimeHeader, SchedulerRuntimeTargetRecord,
};
use af_domain::{
    AfError, ChannelAutoBanRules, ChannelId, ChannelTimeout, CredentialId, GroupId, Protocol,
    TaskSubmission, UpstreamTaskId,
};
use af_protocol::{CanonicalTaskPoll, CanonicalVideoGenerationRequest, VideoModel};
use af_relay::{
    RelayAttemptReport, RelayError, RelayStateMachine, VideoTaskSubmissionCandidate,
    VideoTaskTarget, relay_video_task_poll, relay_video_task_submission,
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

/// xAI 视频任务提交计划的异步结果。
pub type VideoTaskSubmissionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<BoundVideoTaskSubmission, AfError>> + Send + 'a>>;

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

/// 提交成功后必须随任务一起保存的原目标绑定。
#[derive(Clone)]
pub struct VideoTaskBinding {
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_revision: u64,
    upstream_task_id: UpstreamTaskId,
    expected_model: VideoModel,
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
    fn into_binding(self, upstream_task_id: UpstreamTaskId) -> VideoTaskBinding {
        VideoTaskBinding {
            channel_id: self.channel_id,
            credential_id: self.credential_id,
            credential_revision: self.credential_revision,
            upstream_task_id,
            expected_model: self.snapshot.expected_model.clone(),
            target_snapshot: self.snapshot,
        }
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
                .load_credential_health(&route, Protocol::XaiVideo)
                .await?;
            let account_loads = self
                .load_account_concurrency(&route, Protocol::XaiVideo)
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
    async fn video_route(
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

    async fn execute_video_submission(
        &self,
        route: IndexedRoutePlan,
        request: CanonicalVideoGenerationRequest,
        request_id: &str,
        account_loads: Option<&std::collections::BTreeMap<i64, CredentialLoad>>,
        credential_health: Option<&std::collections::BTreeMap<i64, RuntimeHealthState>>,
    ) -> Result<BoundVideoTaskSubmission, AfError> {
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
                        runtime_credential.credential_id(),
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
                        credential_id,
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
                attempt_targets.push(CredentialAttemptTarget::new(
                    target.channel_id(),
                    runtime_credential.credential_id(),
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
            return Err(if health_filtered {
                af_domain::UpstreamError::overloaded().into()
            } else {
                af_domain::UpstreamError::ModelUnsupported.into()
            });
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
                let binding = binding_target.into_binding(submission.task_id().clone());
                self.persist_video_feedback(&attempt_targets, &auto_ban_targets, &report)
                    .await;
                Ok(BoundVideoTaskSubmission {
                    target_group_id,
                    submission,
                    binding,
                })
            }
            Err(error) => {
                let (error, report) = error.into_parts();
                self.persist_video_feedback(&attempt_targets, &auto_ban_targets, &report)
                    .await;
                Err(map_relay_error(error))
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
                runtime_credential.credential_id(),
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
                binding.credential_id,
                runtime_credential.concurrency(),
                binding.target_snapshot.attempt_timeout,
            ));
        }
        let attempt_targets = [CredentialAttemptTarget::new(
            target.channel_id(),
            runtime_credential.credential_id(),
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
    use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
    use af_db::{
        ChannelModelMappings, ChannelParameterOverrides, CredentialProxyScheme,
        CredentialStateRepository, DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions,
        SchedulerRuntimeProxyRecord, connect_and_migrate,
    };
    use af_domain::{ChannelType, CredentialKind, ProxyId, TaskStatus};
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
    use serde_json::json;

    use super::*;

    const CHANNEL_ID: i64 = 71;
    const FIRST_CREDENTIAL_ID: i64 = 81;
    const SECOND_CREDENTIAL_ID: i64 = 82;
    const GROUP_ID: i64 = 7;
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
        let service = service(source).await;

        let submitted = service
            .submit_video_task(
                GroupId::new(GROUP_ID).unwrap(),
                request(),
                "video-submit-request",
            )
            .await
            .unwrap();

        assert_eq!(
            submitted.binding().credential_id().get(),
            SECOND_CREDENTIAL_ID
        );
        assert_eq!(
            submitted.binding().upstream_task_id().as_str(),
            "video-task-bound"
        );
        let poll = service
            .poll_video_task(submitted.binding(), "video-poll-request")
            .await
            .unwrap();
        assert_eq!(poll.status(), &TaskStatus::Succeeded);
        assert_eq!(
            poll.output().unwrap().as_video().unwrap().model().as_str(),
            UPSTREAM_MODEL
        );
        let debug = format!("{submitted:?}");
        for secret in [
            "private-key-first",
            "private-key-second",
            "video-task-bound",
            UPSTREAM_MODEL,
        ] {
            assert!(!debug.contains(secret));
        }
        proxy_server.join().unwrap();
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
        let service = service(source.clone()).await;
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
    }

    async fn service(source: MutableSource) -> ScheduledChatService {
        let index = InMemoryChannelIndex::load(Arc::new(source)).await.unwrap();
        let settings = encryption_settings();
        ScheduledChatService::new(
            IndexedWeightedScheduler::new(index),
            CredentialDecryptor::new(&settings).unwrap(),
            HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap(),
            CredentialStateRepository::new(
                connect_and_migrate(
                    &DatabaseOptions::new("sqlite::memory:").unwrap(),
                    MigrationOptions::default(),
                )
                .await
                .unwrap(),
            ),
        )
        .with_proxy_cipher(SystemSecretCipher::new(&settings).unwrap())
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
