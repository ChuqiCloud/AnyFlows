use super::*;
use af_protocol::Usage;
use af_relay::{
    GenerationCompletionFuture, GenerationCompletionHook, RelayAttemptReport, UsageResolutionError,
};

pub(super) struct Feedback {
    pub(super) report: RelayAttemptReport,
    pub(super) attempt_targets: Vec<CredentialAttemptTarget>,
    pub(super) auto_ban_targets: Vec<ChannelAutoBanAttemptTarget>,
    pub(super) route_channel_ids: Vec<Option<af_domain::RouteChannelId>>,
    pub(super) credential_states: CredentialStateRepository,
    pub(super) scheduler: IndexedWeightedScheduler,
    pub(super) sticky_sessions: Option<Arc<dyn StickySessionStore>>,
    pub(super) session_scope: Option<StableSessionScope>,
    pub(super) health: Option<Arc<dyn SchedulerHealthStore>>,
    pub(super) channel_states: Option<ChannelStateRepository>,
    pub(super) debug_trace: Option<DebugTraceCapture>,
    pub(super) smart_route_feedback: Option<SmartRouteRuntimeRepository>,
    pub(super) started: Instant,
}

impl Feedback {
    pub(super) async fn finish(mut self, succeeded: bool, cancelled: bool) {
        self.report.complete_success(self.started.elapsed());
        if !succeeded {
            if cancelled {
                // Downstream cancellation does not lower the upstream Key's health.
                self.report.discard_pending_success();
            } else {
                self.report
                    .reject_success(af_domain::UpstreamError::network(
                        af_domain::NetworkFailureKind::ResponseBody,
                    ));
            }
        }
        let report = &self.report;
        record_client_simulation_metrics(report);
        if let Some(capture) = self.debug_trace {
            capture.submit(
                report,
                &self.attempt_targets,
                succeeded,
                self.started.elapsed(),
            );
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
        if succeeded
            && let (Some(store), Some(scope), Some(channel)) = (
                self.sticky_sessions.as_ref(),
                self.session_scope.as_ref(),
                report
                    .successful_candidate_index()
                    .and_then(|index| self.attempt_targets.get(index)),
            )
            && let Err(error) = store.bind(scope.as_str(), channel.channel_id().get()).await
        {
            tracing::warn!(target: "af_server::sticky_session", error_kind = sticky_cache_error_kind(&error), "写入粘性会话绑定失败，当前响应继续返回");
        }
    }
}

pub(super) struct Completion(pub(super) Option<Feedback>);

impl GenerationCompletionHook for Completion {
    fn on_delivery(
        mut self: Box<Self>,
        usage: Result<Usage, UsageResolutionError>,
        outcome: af_relay::StreamDeliveryOutcome,
    ) -> GenerationCompletionFuture {
        Box::pin(async move {
            if let Some(feedback) = self.0.take() {
                feedback
                    .finish(
                        outcome == af_relay::StreamDeliveryOutcome::Completed
                            && !matches!(usage, Err(UsageResolutionError::Interrupted)),
                        matches!(
                            outcome,
                            af_relay::StreamDeliveryOutcome::ClientClosed
                                | af_relay::StreamDeliveryOutcome::WriteTimeout
                                | af_relay::StreamDeliveryOutcome::MaxDuration
                                | af_relay::StreamDeliveryOutcome::Internal
                        ),
                    )
                    .await;
            }
        })
    }

    fn on_complete(
        mut self: Box<Self>,
        usage: Result<Usage, UsageResolutionError>,
    ) -> GenerationCompletionFuture {
        Box::pin(async move {
            if let Some(feedback) = self.0.take() {
                // A completed stream can lack reliable usage without an upstream failure.
                feedback
                    .finish(
                        !matches!(usage, Err(UsageResolutionError::Interrupted)),
                        false,
                    )
                    .await;
            }
        })
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        if let Some(feedback) = self.0.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(feedback.finish(false, true));
        }
    }
}
