use std::{sync::Arc, time::Duration};

use af_db::{
    DebugTraceAttemptDiagnosticWrite, DebugTraceAttemptWrite, DebugTraceFailureKind,
    DebugTraceOperation, DebugTraceOutcome, DebugTraceProtocol, DebugTraceRepository,
    DebugTraceRequestDiagnosticWrite, DebugTraceSettingsRecord, DebugTraceWrite,
};
use af_domain::{
    ClientSimulationBodyPatchResult, ClientSimulationBodyProfile, GatewayPrincipal, Operation,
    Protocol, UpstreamError,
};
use af_relay::{
    RelayAttemptDiagnostic, RelayAttemptReport, RelayDiagnosticInput, RelayDiagnosticPolicy,
};
use sha2::{Digest as _, Sha256};
use tokio::{
    sync::{Mutex, mpsc, watch},
    time::{Instant, MissedTickBehavior, interval_at},
};

use crate::{ShutdownController, credential_feedback::CredentialAttemptTarget};

const DEBUG_TRACE_QUEUE_CAPACITY: usize = 4_096;
const SETTINGS_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const RETENTION_CLEANUP_INTERVAL: Duration = Duration::from_secs(300);
const SAMPLE_BUCKETS: u64 = 1_000_000;

/// 当前实例使用的不可变调试追踪设置快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DebugTraceSettingsSnapshot {
    enabled: bool,
    sample_per_million: u64,
    retention_hours: i32,
    capture_headers: bool,
    capture_bodies: bool,
    max_body_bytes: i32,
    version: i64,
}

impl DebugTraceSettingsSnapshot {
    fn from_record(record: DebugTraceSettingsRecord) -> Self {
        Self {
            enabled: record.enabled(),
            sample_per_million: u64::try_from(record.sample_per_million())
                .expect("已校验的调试追踪采样率必须非负"),
            retention_hours: record.retention_hours(),
            capture_headers: record.capture_headers(),
            capture_bodies: record.capture_bodies(),
            max_body_bytes: record.max_body_bytes(),
            version: record.version(),
        }
    }
}

/// 请求热路径使用的无阻塞调试追踪入口。
#[derive(Clone)]
pub(crate) struct DebugTraceRuntime {
    settings: Arc<watch::Sender<DebugTraceSettingsSnapshot>>,
    queue: mpsc::Sender<DebugTraceWrite>,
}

impl DebugTraceRuntime {
    /// 使用数据库初始设置创建热路径入口和单消费者 worker。
    pub(crate) fn new(
        repository: DebugTraceRepository,
        settings: DebugTraceSettingsRecord,
    ) -> (Self, DebugTraceWorker) {
        let (settings, _) = watch::channel(DebugTraceSettingsSnapshot::from_record(settings));
        let settings = Arc::new(settings);
        let (queue, receiver) = mpsc::channel(DEBUG_TRACE_QUEUE_CAPACITY);
        (
            Self {
                settings: Arc::clone(&settings),
                queue,
            },
            DebugTraceWorker {
                repository,
                runtime_settings: settings,
                receiver: Arc::new(Mutex::new(receiver)),
            },
        )
    }

    /// 管理员保存后立即替换当前实例快照；旧版本不得覆盖新版本。
    pub(crate) fn apply_settings(&self, record: DebugTraceSettingsRecord) {
        apply_settings(&self.settings, record);
    }

    /// 在请求规划阶段固定采样决定和全部脱敏上下文。
    #[allow(clippy::too_many_arguments, reason = "参数与固定诊断快照字段一一对应")]
    pub(crate) fn capture(
        &self,
        request_id: &str,
        principal: GatewayPrincipal,
        requested_model: &str,
        downstream_protocol: Protocol,
        upstream_protocol: Protocol,
        operation: Operation,
        body_profile: Option<ClientSimulationBodyProfile>,
        body_result: Option<ClientSimulationBodyPatchResult>,
        downstream: RelayDiagnosticInput,
    ) -> Option<DebugTraceCapture> {
        let settings = *self.settings.borrow();
        if !settings.enabled || !sampled(request_id, settings.sample_per_million) {
            return None;
        }
        if body_profile.is_some() != body_result.is_some() {
            return None;
        }
        let policy = diagnostic_policy(
            settings.capture_headers,
            settings.capture_bodies,
            usize::try_from(settings.max_body_bytes).ok()?,
            body_profile,
        )?;
        let downstream = DebugTraceRequestDiagnosticWrite::new(
            downstream.method().to_owned(),
            downstream.path().to_owned(),
            policy
                .capture_headers()
                .then(|| downstream.headers_json().to_owned()),
            policy
                .capture_bodies()
                .then(|| downstream.body_json(policy.max_body_bytes())),
        )
        .ok()?;
        Some(DebugTraceCapture {
            queue: self.queue.clone(),
            request_id: request_id.to_owned(),
            user_id: principal.user_id().get(),
            token_id: principal.token_id().get(),
            group_id: principal.group_id().get(),
            requested_model: requested_model.to_owned(),
            downstream_protocol: map_protocol(downstream_protocol)?,
            upstream_protocol: map_protocol(upstream_protocol)?,
            operation: map_operation(operation)?,
            body_profile,
            body_result,
            policy,
            downstream,
        })
    }
}

/// 把当前实例调试追踪设置快照接入管理员用例层。
#[derive(Clone)]
pub(crate) struct AdminDebugTraceRuntimeApplier {
    runtime: DebugTraceRuntime,
}

impl AdminDebugTraceRuntimeApplier {
    pub(crate) fn new(runtime: DebugTraceRuntime) -> Self {
        Self { runtime }
    }
}

impl af_admin::AdminDebugTraceSettingsRuntimeApplier for AdminDebugTraceRuntimeApplier {
    fn apply(&self, record: DebugTraceSettingsRecord) {
        self.runtime.apply_settings(record);
    }
}

/// 一次已命中采样的请求上下文；所含诊断材料均已永久脱敏。
pub(crate) struct DebugTraceCapture {
    queue: mpsc::Sender<DebugTraceWrite>,
    request_id: String,
    user_id: i64,
    token_id: i64,
    group_id: i64,
    requested_model: String,
    downstream_protocol: DebugTraceProtocol,
    upstream_protocol: DebugTraceProtocol,
    operation: DebugTraceOperation,
    body_profile: Option<ClientSimulationBodyProfile>,
    body_result: Option<ClientSimulationBodyPatchResult>,
    policy: RelayDiagnosticPolicy,
    downstream: DebugTraceRequestDiagnosticWrite,
}

impl DebugTraceCapture {
    /// 返回本次请求固定的 Relay 采集策略。
    pub(crate) const fn relay_policy(&self) -> RelayDiagnosticPolicy {
        self.policy
    }

    /// 把 Relay 的闭合报告转换为脱敏时间线并尝试无阻塞入队。
    pub(crate) fn submit(
        self,
        report: &RelayAttemptReport,
        targets: &[CredentialAttemptTarget],
        succeeded: bool,
        routing_elapsed: Duration,
    ) {
        let Some(attempts) = trace_attempts(report, targets, self.body_profile, self.body_result)
        else {
            tracing::error!(
                target: "af_server::debug_trace",
                error_kind = "debug_trace_attempt_mapping",
                "调试追踪候选映射不满足运行时不变量"
            );
            return;
        };
        let write = DebugTraceWrite::new(
            self.request_id,
            self.user_id,
            self.token_id,
            self.group_id,
            self.requested_model,
            self.downstream_protocol,
            self.upstream_protocol,
            self.operation,
            if succeeded {
                DebugTraceOutcome::Succeeded
            } else {
                DebugTraceOutcome::Failed
            },
            elapsed_millis(routing_elapsed),
            attempts,
        )
        .map(|write| write.with_downstream_diagnostic(self.downstream));
        let Ok(write) = write else {
            tracing::error!(
                target: "af_server::debug_trace",
                error_kind = "debug_trace_write_invariant",
                "调试追踪写入记录不满足强类型边界"
            );
            return;
        };
        // 队列满或 worker 不可用时直接丢弃追踪，生产请求结果不受影响。
        let _ = self.queue.try_send(write);
    }
}

/// 调试追踪持久化、设置回读与过期清理 worker。
#[derive(Clone)]
pub(crate) struct DebugTraceWorker {
    repository: DebugTraceRepository,
    runtime_settings: Arc<watch::Sender<DebugTraceSettingsSnapshot>>,
    receiver: Arc<Mutex<mpsc::Receiver<DebugTraceWrite>>>,
}

impl DebugTraceWorker {
    /// 持续消费有界队列，并在关机信号到达后排空当前积压。
    pub(crate) async fn run(self, shutdown: ShutdownController) {
        let mut refresh = interval_at(
            Instant::now() + SETTINGS_REFRESH_INTERVAL,
            SETTINGS_REFRESH_INTERVAL,
        );
        refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut cleanup = interval_at(
            Instant::now() + RETENTION_CLEANUP_INTERVAL,
            RETENTION_CLEANUP_INTERVAL,
        );
        cleanup.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                () = shutdown.cancelled() => {
                    self.drain().await;
                    return;
                }
                write = self.recv() => {
                    let Some(write) = write else {
                        return;
                    };
                    self.persist(write).await;
                }
                _ = refresh.tick() => self.refresh_settings().await,
                _ = cleanup.tick() => self.cleanup_expired().await,
            }
        }
    }

    async fn recv(&self) -> Option<DebugTraceWrite> {
        self.receiver.lock().await.recv().await
    }

    async fn drain(&self) {
        loop {
            let write = self.receiver.lock().await.try_recv().ok();
            let Some(write) = write else {
                return;
            };
            self.persist(write).await;
        }
    }

    async fn persist(&self, write: DebugTraceWrite) {
        if self.repository.insert(write).await.is_err() {
            tracing::error!(
                target: "af_server::debug_trace",
                error_kind = "debug_trace_persist",
                "调试追踪持久化失败"
            );
        }
    }

    async fn refresh_settings(&self) {
        match self.repository.settings().await {
            Ok(record) => apply_settings(&self.runtime_settings, record),
            Err(_) => tracing::error!(
                target: "af_server::debug_trace",
                error_kind = "debug_trace_settings_refresh",
                "调试追踪设置回读失败，保留当前实例快照"
            ),
        }
    }

    async fn cleanup_expired(&self) {
        let retention_hours = self.runtime_settings.borrow().retention_hours;
        if self.repository.prune(retention_hours).await.is_err() {
            tracing::error!(
                target: "af_server::debug_trace",
                error_kind = "debug_trace_cleanup",
                "调试追踪过期清理失败"
            );
        }
    }
}

fn apply_settings(
    runtime_settings: &watch::Sender<DebugTraceSettingsSnapshot>,
    record: DebugTraceSettingsRecord,
) {
    let next = DebugTraceSettingsSnapshot::from_record(record);
    if next.version > runtime_settings.borrow().version {
        runtime_settings.send_replace(next);
    }
}

fn trace_attempts(
    report: &RelayAttemptReport,
    targets: &[CredentialAttemptTarget],
    body_profile: Option<ClientSimulationBodyProfile>,
    body_result: Option<ClientSimulationBodyPatchResult>,
) -> Option<Vec<DebugTraceAttemptWrite>> {
    let mut attempts = Vec::with_capacity(
        report.failures().len() + usize::from(report.successful_candidate_index().is_some()),
    );
    for (position, failure) in report.failures().iter().enumerate() {
        let target = targets.get(failure.candidate_index())?;
        let retried =
            position + 1 < report.failures().len() || report.successful_candidate_index().is_some();
        let mut attempt = DebugTraceAttemptWrite::failed(
            failure.candidate_index(),
            target.channel_id().get(),
            target.credential_id().get(),
            map_failure(failure.error()),
            failure.server_status().map(|status| status.get()),
            retried,
            elapsed_millis(failure.elapsed()),
        )
        .ok()?;
        if let Some(client_simulation) = failure.client_simulation() {
            attempt = attempt
                .with_client_simulation(client_simulation.profile(), client_simulation.result());
        }
        if let (Some(profile), Some(result)) = (body_profile, body_result) {
            attempt = attempt.with_client_simulation_body(profile, result);
        }
        if let Some(diagnostic) = failure.diagnostic() {
            attempt = attempt.with_diagnostic(map_attempt_diagnostic(diagnostic)?);
        }
        attempts.push(attempt);
    }
    if let Some(index) = report.successful_candidate_index() {
        let target = targets.get(index)?;
        let mut attempt = DebugTraceAttemptWrite::succeeded(
            index,
            target.channel_id().get(),
            target.credential_id().get(),
            elapsed_millis(report.successful_elapsed()?),
        )
        .ok()?;
        if let Some(client_simulation) = report.successful_client_simulation() {
            attempt = attempt
                .with_client_simulation(client_simulation.profile(), client_simulation.result());
        }
        if let (Some(profile), Some(result)) = (body_profile, body_result) {
            attempt = attempt.with_client_simulation_body(profile, result);
        }
        if let Some(diagnostic) = report.successful_diagnostic() {
            attempt = attempt.with_diagnostic(map_attempt_diagnostic(diagnostic)?);
        }
        attempts.push(attempt);
    }
    Some(attempts)
}

fn map_attempt_diagnostic(
    diagnostic: &RelayAttemptDiagnostic,
) -> Option<DebugTraceAttemptDiagnosticWrite> {
    DebugTraceAttemptDiagnosticWrite::new(
        diagnostic.request_method().to_owned(),
        diagnostic.request_url().to_owned(),
        diagnostic.request_headers_json().map(str::to_owned),
        diagnostic.request_body_json().map(str::to_owned),
        diagnostic.response_status(),
        diagnostic.response_headers_json().map(str::to_owned),
        diagnostic.response_body_json().map(str::to_owned),
        diagnostic.response_streamed(),
    )
    .ok()
}

fn sampled(request_id: &str, sample_per_million: u64) -> bool {
    if sample_per_million == 0 {
        return false;
    }
    if sample_per_million >= SAMPLE_BUCKETS {
        return true;
    }
    let digest = Sha256::digest(request_id.as_bytes());
    let bucket = u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("SHA-256 摘要前八字节长度固定"),
    ) % SAMPLE_BUCKETS;
    bucket < sample_per_million
}

const fn map_protocol(protocol: Protocol) -> Option<DebugTraceProtocol> {
    match protocol {
        Protocol::OpenAiChat => Some(DebugTraceProtocol::OpenAiChat),
        Protocol::OpenAiResponses => Some(DebugTraceProtocol::OpenAiResponses),
        // Embeddings 追踪需要先扩展三方言闭合约束，本切片不伪装成 Chat 记录。
        Protocol::OpenAiEmbeddings
        | Protocol::OpenAiImages
        | Protocol::OpenAiAudio
        | Protocol::OpenAiSpeech
        | Protocol::JinaRerank
        | Protocol::CohereRerank
        | Protocol::XaiVideo => None,
        Protocol::Anthropic => Some(DebugTraceProtocol::Anthropic),
        Protocol::Gemini => Some(DebugTraceProtocol::Gemini),
    }
}

const fn map_operation(operation: Operation) -> Option<DebugTraceOperation> {
    match operation {
        Operation::Chat => Some(DebugTraceOperation::Chat),
        Operation::Responses => Some(DebugTraceOperation::Responses),
        _ => None,
    }
}

const fn map_failure(error: UpstreamError) -> DebugTraceFailureKind {
    match error {
        UpstreamError::AuthExpired => DebugTraceFailureKind::AuthExpired,
        UpstreamError::AuthRevoked => DebugTraceFailureKind::AuthRevoked,
        UpstreamError::AccountDisabled => DebugTraceFailureKind::AccountDisabled,
        UpstreamError::RateLimited { .. } => DebugTraceFailureKind::RateLimited,
        UpstreamError::Overloaded { .. } => DebugTraceFailureKind::Overloaded,
        UpstreamError::QuotaExhausted => DebugTraceFailureKind::QuotaExhausted,
        UpstreamError::ModelUnsupported => DebugTraceFailureKind::ModelUnsupported,
        UpstreamError::ProtocolError => DebugTraceFailureKind::ProtocolError,
        UpstreamError::ServerError { .. } => DebugTraceFailureKind::ServerError,
        UpstreamError::BadRequest => DebugTraceFailureKind::BadRequest,
        UpstreamError::Network { .. } => DebugTraceFailureKind::Network,
    }
}

fn elapsed_millis(elapsed: Duration) -> i64 {
    i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
}

/// 正文仿真启用时无条件关闭诊断正文采集，避免保留原始 system 指令。
fn diagnostic_policy(
    capture_headers: bool,
    capture_bodies: bool,
    max_body_bytes: usize,
    body_profile: Option<ClientSimulationBodyProfile>,
) -> Option<RelayDiagnosticPolicy> {
    RelayDiagnosticPolicy::new(
        capture_headers,
        capture_bodies && body_profile.is_none(),
        max_body_bytes,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_sampling_obeys_closed_boundaries() {
        assert!(!sampled("request-a", 0));
        assert!(sampled("request-a", SAMPLE_BUCKETS));
        assert_eq!(
            sampled("request-stable", 500_000),
            sampled("request-stable", 500_000)
        );
    }

    #[test]
    fn duration_conversion_saturates_instead_of_wrapping() {
        assert_eq!(elapsed_millis(Duration::from_millis(15)), 15);
        assert_eq!(elapsed_millis(Duration::MAX), i64::MAX);
    }

    #[test]
    fn body_simulation_trace_policy_never_captures_bodies() {
        let policy = diagnostic_policy(
            true,
            true,
            1_024,
            Some(ClientSimulationBodyProfile::AnthropicCliSystemDateV1),
        )
        .unwrap();
        assert!(policy.capture_headers());
        assert!(!policy.capture_bodies());
    }
}
