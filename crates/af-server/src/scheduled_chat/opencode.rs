use super::*;
use af_protocol::{Sampling, TokenCount, Usage};
use af_relay::{
    GenerationCompletionFuture, GenerationCompletionHook, ToolConversationPlan,
    ToolConversationReportHandle, ToolConversationState, UsageResolutionError,
    opencode_compatibility_tools, relay_tool_conversation,
};
use std::time::Duration;

pub(super) fn prepare_plan(
    target: &af_db::SchedulerRuntimeTargetRecord,
    request: CanonicalRequestEnvelope,
    model: &str,
) -> Result<ToolConversationPlan, AfError> {
    if target.channel_type() != ChannelType::OpenCode || !target.opencode_full_compatibility() {
        return Err(AfError::InvalidRequest);
    }
    let entry = target.opencode_entry().ok_or(AfError::Internal)?;
    let request = request
        .for_conversation_protocol(target.protocol())
        .ok_or(AfError::InvalidRequest)?;
    let current = &request.canonical().sampling;
    let parameters = target.parameter_overrides();
    let output = parameters
        .max_output_tokens()
        .map(TokenCount::new)
        .transpose()
        .map_err(|_| AfError::InvalidRequest)?;
    let output = match (current.max_output_tokens(), output) {
        (Some(requested), Some(limit)) => Some(requested.min(limit)),
        (requested, None) => requested,
        (None, limit) => limit,
    };
    let sampling = Sampling::new(
        parameters.temperature().or(current.temperature()),
        parameters.top_p().or(current.top_p()),
        output,
        parameters
            .stop_sequences()
            .map_or_else(|| current.stop_sequences().to_vec(), <[String]>::to_vec),
    )
    .map_err(|_| AfError::InvalidRequest)?;
    let request = request.with_routed_parameters(model.to_owned(), sampling);
    let timeout = target
        .timeout()
        .map_or(
            Duration::from_secs(300),
            af_domain::ChannelTimeout::duration,
        )
        .min(Duration::from_secs(600));
    ToolConversationPlan::prepare(
        request,
        target.protocol(),
        opencode_compatibility_tools(entry, model),
        TokenCount::new(4096).expect("固定续答输入余量必须有效"),
        timeout,
    )
}

/// 兼容模式的健康、粘性与诊断均在实际终态后提交。
pub(super) async fn execute(
    execution: ScheduledChatExecution,
) -> Result<crate::RoutedExecution<ChatResponse>, AfError> {
    let ScheduledChatExecution {
        tool_plan,
        machine,
        response_protocol,
        request_id,
        attempt_targets,
        auto_ban_targets,
        route_channel_ids,
        credential_states,
        scheduler,
        sticky_sessions,
        session_scope,
        user_permit,
        health,
        channel_states,
        debug_trace,
        smart_route_feedback,
        ..
    } = execution;
    let plan = tool_plan.ok_or(AfError::Internal)?;
    let machine = match debug_trace.as_ref() {
        Some(capture) => machine.with_diagnostic_policy(capture.relay_policy()),
        None => machine,
    };
    let started = Instant::now();
    let (result, report) =
        relay_tool_conversation(&machine, plan, response_protocol, &request_id).await;
    let channel = report
        .snapshot()
        .report
        .successful_candidate_index()
        .and_then(|index| attempt_targets.get(index))
        .map(CredentialAttemptTarget::channel_id);
    let feedback = Feedback {
        report,
        attempt_targets,
        auto_ban_targets,
        route_channel_ids,
        credential_states,
        scheduler,
        sticky_sessions,
        session_scope,
        user_permit,
        health,
        channel_states,
        debug_trace,
        smart_route_feedback,
        started,
    };
    match result {
        Ok(ChatResponse::Stream { body, usage }) => {
            let Some(channel) = channel else {
                feedback.finish().await;
                return Err(AfError::Internal);
            };
            let body = body.with_completion_hook(Box::new(Completion {
                feedback: Some(feedback),
            }));
            Ok(crate::RoutedExecution::new(
                ChatResponse::Stream { body, usage },
                channel,
            ))
        }
        Ok(response) => {
            feedback.finish().await;
            channel
                .map(|channel| crate::RoutedExecution::new(response, channel))
                .ok_or(AfError::Internal)
        }
        Err(error) => {
            feedback.finish().await;
            Err(error)
        }
    }
}

struct Feedback {
    report: ToolConversationReportHandle,
    attempt_targets: Vec<CredentialAttemptTarget>,
    auto_ban_targets: Vec<ChannelAutoBanAttemptTarget>,
    route_channel_ids: Vec<Option<af_domain::RouteChannelId>>,
    credential_states: CredentialStateRepository,
    scheduler: IndexedWeightedScheduler,
    sticky_sessions: Option<Arc<dyn StickySessionStore>>,
    session_scope: Option<StableSessionScope>,
    user_permit: Option<RuntimeConcurrencyPermit>,
    health: Option<Arc<dyn SchedulerHealthStore>>,
    channel_states: Option<ChannelStateRepository>,
    debug_trace: Option<DebugTraceCapture>,
    smart_route_feedback: Option<SmartRouteRuntimeRepository>,
    started: Instant,
}

impl Feedback {
    async fn finish(self) {
        self.report.cancel();
        let snapshot = self.report.snapshot();
        let report = &snapshot.report;
        record_client_simulation_metrics(report);
        if let Some(capture) = self.debug_trace {
            capture.submit_tools(&snapshot, &self.attempt_targets, self.started.elapsed());
        }
        tokio::join!(
            persist_credential_feedback(
                &self.credential_states,
                &self.scheduler,
                &self.attempt_targets,
                report
            ),
            persist_scheduler_health_feedback(self.health.as_ref(), &self.attempt_targets, report),
            persist_channel_auto_ban_feedback(
                self.channel_states.as_ref(),
                &self.scheduler,
                &self.auto_ban_targets,
                report
            ),
            persist_feedback(
                self.smart_route_feedback.as_ref(),
                &self.route_channel_ids,
                report
            ),
        );
        if snapshot.state == ToolConversationState::Completed
            && let (Some(store), Some(scope), Some(channel)) = (
                self.sticky_sessions.as_ref(),
                self.session_scope.as_ref(),
                report
                    .successful_candidate_index()
                    .and_then(|i| self.attempt_targets.get(i)),
            )
            && let Err(error) = store.bind(scope.as_str(), channel.channel_id().get()).await
        {
            tracing::warn!(target: "af_server::sticky_session", error_kind = sticky_cache_error_kind(&error), "写入粘性会话绑定失败，当前响应继续返回");
        }
        if let Some(permit) = self.user_permit {
            permit.release().await;
        }
    }
}

struct Completion {
    feedback: Option<Feedback>,
}
impl GenerationCompletionHook for Completion {
    fn on_complete(
        mut self: Box<Self>,
        _usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture {
        // Future 开始轮询前仍由 Drop 保底，客户端取消不会漏掉失败反馈。
        Box::pin(async move {
            if let Some(feedback) = self.feedback.take() {
                feedback.finish().await;
            }
        })
    }
}
impl Drop for Completion {
    fn drop(&mut self) {
        if let Some(feedback) = self.feedback.take() {
            feedback.report.cancel();
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(feedback.finish());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_db::{
        ChannelModelMappings, ChannelParameterOverrides, EncryptedCredentialEnvelope,
        SchedulerRuntimeCredentialRecord, SchedulerRuntimeTargetRecord,
    };
    use af_domain::{ChannelId, CredentialKind};
    use af_protocol::openai_chat;

    fn target(enabled: bool, protocol: Protocol) -> SchedulerRuntimeTargetRecord {
        SchedulerRuntimeTargetRecord::new_pool_with_request_policy(
            ChannelId::new(901).unwrap(),
            ChannelType::OpenCode,
            protocol,
            None,
            vec![
                SchedulerRuntimeCredentialRecord::new(
                    902,
                    CredentialKind::ApiKey,
                    EncryptedCredentialEnvelope::new("fixture", [1; 24], vec![2; 16]).unwrap(),
                    false,
                )
                .unwrap(),
            ],
            ChannelModelMappings::parse(&serde_json::json!({"alias":"mimo-v2.6-flash-free"}))
                .unwrap(),
            ChannelParameterOverrides::parse(
                &serde_json::json!({"temperature":0.25,"max_output_tokens":64}),
            )
            .unwrap(),
            Vec::new(),
        )
        .unwrap()
        .with_opencode_full_compatibility(enabled)
        .unwrap()
    }

    #[test]
    fn compatibility_budget_freezes_mapping_sampling_and_tools_before_precharge() {
        for protocol in [
            Protocol::OpenAiChat,
            Protocol::OpenAiResponses,
            Protocol::Anthropic,
        ] {
            let request = openai_chat::parse_request_envelope(af_adapter::Bytes::from_static(
                br#"{"model":"alias","messages":[{"role":"user","content":"hello"}],"max_tokens":128,"stream":true}"#
            )).unwrap();
            let original =
                af_relay::estimate_openai_request_upper_bound(request.canonical()).unwrap();
            let plan =
                prepare_plan(&target(true, protocol), request, "mimo-v2.6-flash-free").unwrap();
            assert_eq!(plan.request().canonical().model, "mimo-v2.6-flash-free");
            assert_eq!(plan.request().requested_model(), "alias");
            assert_eq!(
                plan.request().canonical().sampling.temperature(),
                Some(0.25)
            );
            assert_eq!(
                plan.request()
                    .canonical()
                    .sampling
                    .max_output_tokens()
                    .unwrap()
                    .get(),
                64
            );
            assert!(plan.request().canonical().tools.len() >= 12);
            assert_eq!(plan.upper_bound().output_tokens().get(), 64);
            assert!(plan.upper_bound().input_tokens() > original.input_tokens());
            assert!(plan.request().source_protocol().is_none());
        }
    }

    #[test]
    fn disabled_mode_and_unrepresentable_vendor_state_are_rejected() {
        let request = openai_chat::parse_request_envelope(af_adapter::Bytes::from_static(
            br#"{"model":"alias","messages":[{"role":"user","content":"hello"}],"max_tokens":128}"#,
        ))
        .unwrap();
        assert!(
            prepare_plan(
                &target(false, Protocol::OpenAiChat),
                request,
                "mimo-v2.6-flash-free"
            )
            .is_err()
        );
        let request = af_protocol::openai_responses::parse_request_envelope(af_adapter::Bytes::from_static(
            br#"{"model":"alias","input":"hello","previous_response_id":"resp_private","max_output_tokens":128}"#
        )).unwrap();
        assert!(
            prepare_plan(
                &target(true, Protocol::OpenAiResponses),
                request,
                "mimo-v2.6-flash-free"
            )
            .is_err()
        );
    }
}
