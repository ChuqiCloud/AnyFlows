use std::{
    collections::BTreeSet,
    fmt,
    num::NonZeroUsize,
    sync::Arc,
    time::{Duration, Instant},
};

use af_adapter::{
    Adaptor, AdaptorTarget, Bytes, ClientSimulationContext, ClientSimulationMiddleware, Credential,
    HeaderMap, Method, Operation, RelayContext, ResponseMode, TransportDispatcher, UpstreamRequest,
    UpstreamResponse, apply_client_simulation,
};
use af_domain::{
    ChannelId, ClientSimulationProfile, ClientSimulationResult, MAX_MODEL_NAME_BYTES,
    UpstreamError, UpstreamServerStatus,
};
use af_protocol::{StructuredErrorEvidence, extract_structured_error_evidence};
use thiserror::Error;

use crate::{
    RelayAttemptDiagnostic, RelayAttemptGate, RelayAttemptGateError, RelayAttemptPermit,
    RelayBuildError, RelayDiagnosticPolicy, RelayError,
    error::{classify_upstream_status, is_retryable, map_adaptor_error, parse_retry_after},
};

/// 单次转发候选的安全装配结果；调度器只负责按顺序提供候选，不在本类型内读库。
pub struct RelayCandidate {
    adaptor: Arc<dyn Adaptor>,
    context: RelayContext,
    credential: Credential,
    request: Option<RelayCandidateRequest>,
    header_overrides: HeaderMap,
    client_simulation: Option<Arc<dyn ClientSimulationMiddleware>>,
    channel_group: Option<ChannelId>,
    transport: TransportDispatcher,
    attempt_gate: Option<Arc<dyn RelayAttemptGate>>,
}

impl RelayCandidate {
    /// 构造一次尝试所需的适配器、受控上下文和已解密凭据视图。
    #[must_use]
    pub fn new(adaptor: Arc<dyn Adaptor>, context: RelayContext, credential: Credential) -> Self {
        Self {
            adaptor,
            context,
            credential,
            request: None,
            header_overrides: HeaderMap::new(),
            client_simulation: None,
            channel_group: None,
            transport: TransportDispatcher::http(),
            attempt_gate: None,
        }
    }

    /// 附加已由协议构造器验证的候选级模型和正文。
    #[must_use]
    pub fn with_request(mut self, request: RelayCandidateRequest) -> Self {
        self.request = Some(request);
        self
    }

    /// 附加已由持久化边界验证的非认证 Header 覆盖。
    #[must_use]
    pub fn with_header_overrides(mut self, header_overrides: HeaderMap) -> Self {
        self.header_overrides = header_overrides;
        self
    }

    /// 显式装配受控客户端仿真中间件；默认候选始终不启用。
    #[must_use]
    pub fn with_client_simulation(
        mut self,
        client_simulation: Arc<dyn ClientSimulationMiddleware>,
    ) -> Self {
        self.client_simulation = Some(client_simulation);
        self
    }

    /// 标记候选所属渠道；同一渠道的多条凭据必须连续排列。
    #[must_use]
    pub const fn with_channel_group(mut self, channel_id: ChannelId) -> Self {
        self.channel_group = Some(channel_id);
        self
    }

    /// 显式覆盖当前候选使用的受控传输分派器。
    #[must_use]
    pub fn with_transport_dispatcher(mut self, transport: TransportDispatcher) -> Self {
        self.transport = transport;
        self
    }

    /// 绑定发送前的候选并发许可；等待策略由调度装配层提前固定。
    #[must_use]
    pub fn with_attempt_gate(mut self, attempt_gate: Arc<dyn RelayAttemptGate>) -> Self {
        self.attempt_gate = Some(attempt_gate);
        self
    }
}

impl fmt::Debug for RelayCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayCandidate")
            .field("adaptor", &self.adaptor.channel_type())
            .field("context", &self.context)
            .field("credential", &self.credential)
            .field("has_candidate_request", &self.request.is_some())
            .field("header_override_count", &self.header_overrides.len())
            .field(
                "client_simulation_profile",
                &self
                    .client_simulation
                    .as_ref()
                    .map(|middleware| middleware.profile()),
            )
            .field("has_channel_group", &self.channel_group.is_some())
            .field("transport", &self.transport)
            .field("has_attempt_gate", &self.attempt_gate.is_some())
            .finish()
    }
}

/// 已由目标协议构造器验证的候选级模型和正文。
///
/// 模型映射或参数覆盖会让不同渠道产生不同请求；本类型只保存最终结果，不保存映射
/// 表、参数配置或原始正文，Debug 也不会输出模型和正文。
#[derive(Clone)]
pub struct RelayCandidateRequest {
    model: String,
    body: Option<Bytes>,
}

impl RelayCandidateRequest {
    /// 创建候选级请求；正文容量仍由统一的 [`UpstreamRequest`] 边界复验。
    pub fn new(model: impl Into<String>, body: Option<Bytes>) -> Result<Self, RelayError> {
        let model = model.into();
        if !is_valid_model(&model) {
            return Err(RelayError::InvalidModel);
        }
        Ok(Self { model, body })
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn body(&self) -> Option<Bytes> {
        self.body.clone()
    }

    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (String, Option<Bytes>) {
        (self.model, self.body)
    }
}

impl fmt::Debug for RelayCandidateRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayCandidateRequest")
            .field("model", &"<已脱敏>")
            .field("body_bytes", &self.body.as_ref().map(Bytes::len))
            .finish()
    }
}

/// 单个候选失败后的脱敏尝试记录。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayAttemptFailure {
    candidate_index: usize,
    error: UpstreamError,
    server_status: Option<UpstreamServerStatus>,
    evidence: Option<StructuredErrorEvidence>,
    elapsed: Duration,
    diagnostic: Option<RelayAttemptDiagnostic>,
    client_simulation: Option<RelayClientSimulationAttempt>,
}

impl RelayAttemptFailure {
    pub(crate) fn new(candidate_index: usize, error: UpstreamError, elapsed: Duration) -> Self {
        Self {
            candidate_index,
            error,
            server_status: None,
            evidence: None,
            elapsed,
            diagnostic: None,
            client_simulation: None,
        }
    }

    pub(crate) fn with_evidence(
        candidate_index: usize,
        error: UpstreamError,
        server_status: Option<UpstreamServerStatus>,
        evidence: Option<StructuredErrorEvidence>,
        elapsed: Duration,
    ) -> Self {
        Self {
            candidate_index,
            error,
            server_status,
            evidence,
            elapsed,
            diagnostic: None,
            client_simulation: None,
        }
    }

    fn with_diagnostic(mut self, diagnostic: Option<RelayAttemptDiagnostic>) -> Self {
        self.diagnostic = diagnostic;
        self
    }

    fn with_client_simulation(
        mut self,
        client_simulation: Option<RelayClientSimulationAttempt>,
    ) -> Self {
        self.client_simulation = client_simulation;
        self
    }

    /// 返回失败候选在固定计划中的零基索引。
    #[must_use]
    pub const fn candidate_index(&self) -> usize {
        self.candidate_index
    }

    /// 返回不含上游正文的闭合错误分类。
    #[must_use]
    pub const fn error(&self) -> UpstreamError {
        self.error
    }

    /// 返回原始 HTTP 响应携带的已校验 5xx 状态；非 HTTP 或非 5xx 故障为空。
    #[must_use]
    pub const fn server_status(&self) -> Option<UpstreamServerStatus> {
        self.server_status
    }

    /// 返回只含标准错误标识字段的内存匹配证据；内容不得记录或持久化。
    #[must_use]
    pub const fn evidence(&self) -> Option<&StructuredErrorEvidence> {
        self.evidence.as_ref()
    }

    /// 返回当前候选从开始判定到取得闭合结果的耗时。
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// 返回已经永久脱敏的请求与响应快照。
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&RelayAttemptDiagnostic> {
        self.diagnostic.as_ref()
    }

    /// 返回当前候选的闭合仿真档案和应用结果；未配置时为空。
    #[must_use]
    pub const fn client_simulation(&self) -> Option<RelayClientSimulationAttempt> {
        self.client_simulation
    }
}

/// 单个候选的脱敏客户端仿真元数据。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayClientSimulationAttempt {
    profile: ClientSimulationProfile,
    result: ClientSimulationResult,
}

impl RelayClientSimulationAttempt {
    const fn new(profile: ClientSimulationProfile, result: ClientSimulationResult) -> Self {
        Self { profile, result }
    }

    /// 返回包含版本的闭合档案 ID。
    #[must_use]
    pub const fn profile(self) -> ClientSimulationProfile {
        self.profile
    }

    /// 返回本次候选实际取得的闭合应用结果。
    #[must_use]
    pub const fn result(self) -> ClientSimulationResult {
        self.result
    }
}

/// 一次转发执行产生的脱敏候选报告。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RelayAttemptReport {
    successful_candidate_index: Option<usize>,
    successful_elapsed: Option<Duration>,
    successful_diagnostic: Option<RelayAttemptDiagnostic>,
    successful_client_simulation: Option<RelayClientSimulationAttempt>,
    failures: Vec<RelayAttemptFailure>,
}

impl RelayAttemptReport {
    pub(crate) fn succeeded(
        successful_candidate_index: usize,
        successful_elapsed: Duration,
        failures: Vec<RelayAttemptFailure>,
    ) -> Self {
        Self {
            successful_candidate_index: Some(successful_candidate_index),
            successful_elapsed: Some(successful_elapsed),
            successful_diagnostic: None,
            successful_client_simulation: None,
            failures,
        }
    }

    pub(crate) fn failed(failures: Vec<RelayAttemptFailure>) -> Self {
        Self {
            successful_candidate_index: None,
            successful_elapsed: None,
            successful_diagnostic: None,
            successful_client_simulation: None,
            failures,
        }
    }

    fn with_successful_diagnostic(mut self, diagnostic: Option<RelayAttemptDiagnostic>) -> Self {
        self.successful_diagnostic = diagnostic;
        self
    }

    fn with_successful_client_simulation(
        mut self,
        client_simulation: Option<RelayClientSimulationAttempt>,
    ) -> Self {
        self.successful_client_simulation = client_simulation;
        self
    }

    /// 返回最终成功候选索引；执行失败时为空。
    #[must_use]
    pub const fn successful_candidate_index(&self) -> Option<usize> {
        self.successful_candidate_index
    }

    /// 返回最终成功候选的耗时；执行失败时为空。
    #[must_use]
    pub const fn successful_elapsed(&self) -> Option<Duration> {
        self.successful_elapsed
    }

    /// 返回按尝试顺序排列的失败分类。
    #[must_use]
    pub fn failures(&self) -> &[RelayAttemptFailure] {
        &self.failures
    }

    /// 返回最终成功候选的安全诊断快照。
    #[must_use]
    pub const fn successful_diagnostic(&self) -> Option<&RelayAttemptDiagnostic> {
        self.successful_diagnostic.as_ref()
    }

    /// 返回最终成功候选的闭合仿真元数据；未配置时为空。
    #[must_use]
    pub const fn successful_client_simulation(&self) -> Option<RelayClientSimulationAttempt> {
        self.successful_client_simulation
    }

    /// 把已经取得 HTTP 成功、但未通过协议验证的候选改记为失败。
    pub(crate) fn reject_success(&mut self, error: UpstreamError) {
        if let Some(candidate_index) = self.successful_candidate_index.take() {
            let elapsed = self
                .successful_elapsed
                .take()
                .expect("成功候选索引与耗时必须同时存在");
            self.failures.push(
                RelayAttemptFailure::new(candidate_index, error, elapsed)
                    .with_diagnostic(self.successful_diagnostic.take())
                    .with_client_simulation(self.successful_client_simulation.take()),
            );
        }
    }
}

/// 候选无关的转发请求描述；模型名先校验，最终请求体边界在适配层校验。
pub struct RelayRequest {
    model: String,
    operation: Operation,
    method: Method,
    body: Option<Bytes>,
    response_mode: ResponseMode,
    response_body_limit: usize,
}

impl RelayRequest {
    /// 构造候选无关的上游请求；请求体和 Header 的最终校验仍在适配层完成。
    pub fn new(
        model: impl Into<String>,
        operation: Operation,
        method: Method,
        body: Option<Bytes>,
    ) -> Result<Self, RelayError> {
        let model = model.into();
        if !is_valid_model(&model) {
            return Err(RelayError::InvalidModel);
        }
        Ok(Self {
            model,
            operation,
            method,
            body,
            response_mode: ResponseMode::default(),
            response_body_limit: af_adapter::MAX_UPSTREAM_RESPONSE_BODY_BYTES,
        })
    }

    /// 选择流式响应交付；流式响应返回后由调用方负责消费与取消传播。
    #[must_use]
    pub const fn with_response_mode(mut self, response_mode: ResponseMode) -> Self {
        self.response_mode = response_mode;
        self
    }

    /// 为单次非流式协议请求选择受控的大响应收集预算。
    #[must_use]
    pub const fn with_response_body_limit(mut self, max_bytes: usize) -> Self {
        self.response_body_limit = max_bytes;
        self
    }
}

impl fmt::Debug for RelayRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayRequest")
            .field("operation", &self.operation)
            .field("method", &self.method)
            .field("response_mode", &self.response_mode)
            .field("response_body_limit", &self.response_body_limit)
            .field("body_bytes", &self.body.as_ref().map(Bytes::len))
            .field("model", &"<已脱敏>")
            .finish()
    }
}

/// 转发内部状态；状态只在一次执行中单调前进，重试会回到 Routing。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RelayState {
    /// 正在取下一个未排除候选。
    Routing,
    /// 正在为候选构造认证 Header。
    Credential,
    /// 正在构造并提交上游请求。
    Sending,
    /// 已把流式响应交给上层。
    Streaming,
    /// 正在读取错误响应并归一化故障。
    Classifying,
    /// 当前候选失败，准备尝试下一个候选。
    Retrying,
    /// 已获得成功响应。
    Completed,
    /// 所有候选均失败或请求无法执行。
    Failed,
}

/// 成功转发的响应及审计所需的无敏感尝试次数。
pub struct RelayResponse {
    response: UpstreamResponse,
    attempts: NonZeroUsize,
    state: RelayState,
    report: RelayAttemptReport,
    permit: Option<Box<dyn RelayAttemptPermit>>,
}

impl RelayResponse {
    /// 消费包装并返回适配层响应。
    #[must_use]
    pub fn into_response(self) -> UpstreamResponse {
        self.response
    }

    /// 返回包含路由失败在内的候选尝试次数。
    #[must_use]
    pub const fn attempts(&self) -> NonZeroUsize {
        self.attempts
    }

    /// 返回成功交付时的终态；流式响应为 `Streaming`，完整响应为 `Completed`。
    #[must_use]
    pub const fn state(&self) -> RelayState {
        self.state
    }

    /// 返回本次执行的脱敏候选报告。
    #[must_use]
    pub const fn report(&self) -> &RelayAttemptReport {
        &self.report
    }

    /// 消费响应并同时返回上游响应和脱敏候选报告。
    #[must_use]
    pub fn into_parts(self) -> (UpstreamResponse, RelayAttemptReport) {
        (self.response, self.report)
    }

    /// 消费响应并同时返回成功候选许可，供协议层覆盖完整/流式生命周期。
    #[must_use]
    pub fn into_parts_with_permit(
        self,
    ) -> (
        UpstreamResponse,
        RelayAttemptReport,
        Option<Box<dyn RelayAttemptPermit>>,
    ) {
        (self.response, self.report, self.permit)
    }
}

impl fmt::Debug for RelayResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayResponse")
            .field("attempts", &self.attempts)
            .field("state", &self.state)
            .field("report", &self.report)
            .field("response", &self.response)
            .field("has_attempt_permit", &self.permit.is_some())
            .finish()
    }
}

/// 带候选失败报告的转发执行错误。
#[derive(Debug, Error)]
#[error("转发执行失败")]
pub struct RelayExecutionError {
    error: RelayError,
    report: RelayAttemptReport,
}

impl RelayExecutionError {
    pub(crate) fn new(error: RelayError, failures: Vec<RelayAttemptFailure>) -> Self {
        Self {
            error,
            report: RelayAttemptReport::failed(failures),
        }
    }

    /// 返回兼容入口使用的原始转发错误。
    #[must_use]
    pub const fn error(&self) -> RelayError {
        self.error
    }

    /// 返回失败前已经产生的脱敏候选报告。
    #[must_use]
    pub const fn report(&self) -> &RelayAttemptReport {
        &self.report
    }

    /// 消费包装并返回原始错误。
    #[must_use]
    pub fn into_error(self) -> RelayError {
        self.error
    }

    /// 消费包装并返回原始错误和脱敏候选报告。
    #[must_use]
    pub fn into_parts(self) -> (RelayError, RelayAttemptReport) {
        (self.error, self.report)
    }
}

/// 有界候选列表上的转发状态机。
pub struct RelayStateMachine {
    candidates: Vec<RelayCandidate>,
    diagnostic_policy: Option<RelayDiagnosticPolicy>,
}

impl RelayStateMachine {
    /// 单次请求最多尝试的候选数量，防止错误配置导致无界串行发送。
    pub const MAX_CANDIDATES: usize = 64;

    /// 构造固定顺序候选列表；调度排序与健康过滤由上层完成。
    pub fn new(candidates: Vec<RelayCandidate>) -> Result<Self, RelayBuildError> {
        if candidates.is_empty() {
            return Err(RelayBuildError::NoCandidates);
        }
        if candidates.len() > Self::MAX_CANDIDATES {
            return Err(RelayBuildError::TooManyCandidates);
        }
        if !channel_groups_are_contiguous(&candidates) {
            return Err(RelayBuildError::NonContiguousChannelGroup);
        }
        Ok(Self {
            candidates,
            diagnostic_policy: None,
        })
    }

    /// 为当前请求启用安全诊断；策略只影响永久脱敏快照，不改变转发语义。
    #[must_use]
    pub const fn with_diagnostic_policy(mut self, policy: RelayDiagnosticPolicy) -> Self {
        self.diagnostic_policy = Some(policy);
        self
    }

    /// 按 Routing → Credential → Sending → Classifying/Streaming → Retry 执行一次请求。
    ///
    /// 每个候选最多发送一次；可重试的上游故障才会切换候选，客户端参数错误不会继续
    /// 扩散。状态机不持有计费会话，调用方应在外层让同一 BillingSession 覆盖整个执行。
    pub async fn execute(&self, request: RelayRequest) -> Result<RelayResponse, RelayError> {
        self.execute_with_report(request)
            .await
            .map_err(RelayExecutionError::into_error)
    }

    /// 执行一次请求并保留每个已尝试候选的脱敏结果。
    pub async fn execute_with_report(
        &self,
        request: RelayRequest,
    ) -> Result<RelayResponse, RelayExecutionError> {
        let mut last_error = None;
        let mut failures = Vec::new();
        let mut index = 0;
        while let Some(candidate) = self.candidates.get(index) {
            let attempt_started_at = Instant::now();
            let mut client_simulation = candidate.client_simulation.as_ref().map(|middleware| {
                RelayClientSimulationAttempt::new(
                    middleware.profile(),
                    ClientSimulationResult::NotApplied,
                )
            });
            let candidate_model = candidate
                .request
                .as_ref()
                .map_or(request.model.as_str(), RelayCandidateRequest::model);
            let supported_models = candidate.adaptor.supported_models();
            if !supported_models.is_empty()
                && !supported_models
                    .iter()
                    .any(|model| model == candidate_model)
            {
                let error = UpstreamError::ModelUnsupported;
                failures.push(
                    RelayAttemptFailure::new(index, error, attempt_started_at.elapsed())
                        .with_client_simulation(client_simulation),
                );
                last_error = Some(RelayError::Upstream(error));
                index = self.next_candidate_index(index, error);
                continue;
            }

            let mut headers = HeaderMap::new();
            if let Err(error) = candidate.adaptor.setup_headers(
                &mut headers,
                &candidate.credential,
                &candidate.context,
            ) {
                last_error = Some(RelayError::Adaptor(error));
                index = self.next_channel_index(index);
                continue;
            }
            headers.extend(candidate.header_overrides.clone());

            let target = match candidate.adaptor.build_url(
                &candidate.context,
                AdaptorTarget::new(candidate_model, request.operation, request.response_mode),
            ) {
                Ok(target) => target,
                Err(error) => {
                    last_error = Some(RelayError::Adaptor(error));
                    index = self.next_channel_index(index);
                    continue;
                }
            };
            let upstream_request = match UpstreamRequest::new(
                request.method.clone(),
                target,
                headers,
                candidate
                    .request
                    .as_ref()
                    .map_or_else(|| request.body.clone(), RelayCandidateRequest::body),
            )
            .and_then(|upstream_request| {
                upstream_request.with_response_body_limit(request.response_body_limit)
            })
            .map(|upstream_request| upstream_request.with_response_mode(request.response_mode))
            {
                Ok(request) => request,
                Err(error) => {
                    return Err(RelayExecutionError::new(
                        RelayError::Request(error),
                        failures,
                    ));
                }
            };
            let upstream_request = match candidate.client_simulation.as_ref() {
                Some(middleware) => match apply_client_simulation(
                    middleware.as_ref(),
                    ClientSimulationContext::new(
                        candidate.adaptor.channel_type(),
                        candidate.adaptor.default_protocol(),
                        request.operation,
                        candidate.credential.kind(),
                    ),
                    upstream_request,
                ) {
                    Ok(request) => {
                        client_simulation = Some(RelayClientSimulationAttempt::new(
                            middleware.profile(),
                            ClientSimulationResult::Applied,
                        ));
                        request
                    }
                    Err(_error) => {
                        client_simulation = Some(RelayClientSimulationAttempt::new(
                            middleware.profile(),
                            ClientSimulationResult::Failed,
                        ));
                        failures.push(
                            RelayAttemptFailure::new(
                                index,
                                UpstreamError::ProtocolError,
                                attempt_started_at.elapsed(),
                            )
                            .with_client_simulation(client_simulation),
                        );
                        // 仿真档案是显式配置，应用失败不得静默退回未仿真请求或其他候选。
                        return Err(RelayExecutionError::new(
                            RelayError::Upstream(UpstreamError::ProtocolError),
                            failures,
                        ));
                    }
                },
                None => upstream_request,
            };
            let upstream_request = match candidate
                .adaptor
                .finalize_request(upstream_request, &candidate.credential, &candidate.context)
                .await
            {
                Ok(request) => request,
                Err(error) => {
                    if client_simulation.is_some() {
                        failures.push(
                            RelayAttemptFailure::new(
                                index,
                                UpstreamError::ProtocolError,
                                attempt_started_at.elapsed(),
                            )
                            .with_client_simulation(client_simulation),
                        );
                    }
                    last_error = Some(RelayError::Adaptor(error));
                    index = self.next_channel_index(index);
                    continue;
                }
            };
            let mut diagnostic = self
                .diagnostic_policy
                .map(|policy| RelayAttemptDiagnostic::capture_request(&upstream_request, policy));
            let permit = match candidate.attempt_gate.as_ref() {
                Some(gate) => match gate.acquire().await {
                    Ok(permit) => Some(permit),
                    Err(RelayAttemptGateError::Limited) => {
                        last_error = Some(RelayError::ConcurrencyUnavailable);
                        index += 1;
                        continue;
                    }
                    Err(RelayAttemptGateError::Internal) => {
                        return Err(RelayExecutionError::new(
                            RelayError::AttemptGateFailed,
                            failures,
                        ));
                    }
                },
                None => None,
            };
            let outcome = match candidate
                .transport
                .send_with_report(upstream_request, &candidate.context)
                .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    release_attempt_permit(permit).await;
                    let mapped = map_adaptor_error(error);
                    failures.push(
                        RelayAttemptFailure::new(index, mapped, attempt_started_at.elapsed())
                            .with_diagnostic(diagnostic)
                            .with_client_simulation(client_simulation),
                    );
                    last_error = Some(RelayError::Upstream(mapped));
                    if is_retryable(mapped) {
                        index = self.next_candidate_index(index, mapped);
                        continue;
                    }
                    return Err(RelayExecutionError::new(
                        RelayError::Upstream(mapped),
                        failures,
                    ));
                }
            };
            let (response, transport_fallback) = outcome.into_parts();
            if let Some(error_kind) = transport_fallback {
                tracing::warn!(
                    target: "af_relay::transport_dispatcher",
                    transport = "responses_websocket",
                    fallback_transport = "http",
                    error_kind = error_kind.as_str(),
                    candidate_index = index,
                    "Responses WebSocket 在首事件前失败，执行一次 HTTP 回退"
                );
            }

            if let Some(diagnostic) = diagnostic.as_mut() {
                diagnostic.capture_response_head(&response);
            }

            if response.status().is_success() {
                let state = if request.response_mode == ResponseMode::Stream {
                    RelayState::Streaming
                } else {
                    RelayState::Completed
                };
                return Ok(RelayResponse {
                    response,
                    attempts: NonZeroUsize::new(index + 1)
                        .expect("候选索引从零开始时尝试次数必须为正"),
                    state,
                    report: RelayAttemptReport::succeeded(
                        index,
                        attempt_started_at.elapsed(),
                        failures,
                    )
                    .with_successful_diagnostic(diagnostic)
                    .with_successful_client_simulation(client_simulation),
                    permit,
                });
            }

            let status = response.status();
            let retry_after = parse_retry_after(response.headers());
            let body = match response.into_body().into_bytes().await {
                Ok(body) => body,
                Err(error) => {
                    release_attempt_permit(permit).await;
                    let mapped = map_adaptor_error(error);
                    failures.push(
                        RelayAttemptFailure::new(index, mapped, attempt_started_at.elapsed())
                            .with_diagnostic(diagnostic)
                            .with_client_simulation(client_simulation),
                    );
                    last_error = Some(RelayError::Upstream(mapped));
                    if is_retryable(mapped) {
                        index = self.next_candidate_index(index, mapped);
                        continue;
                    }
                    return Err(RelayExecutionError::new(
                        RelayError::Upstream(mapped),
                        failures,
                    ));
                }
            };
            let classified = classify_upstream_status(status, retry_after, &body);
            let server_status = UpstreamServerStatus::new(status.as_u16());
            let evidence = extract_structured_error_evidence(&body);
            if let Some(diagnostic) = diagnostic.as_mut() {
                diagnostic.capture_buffered_response_body(&body);
            }
            release_attempt_permit(permit).await;
            failures.push(
                RelayAttemptFailure::with_evidence(
                    index,
                    classified,
                    server_status,
                    evidence,
                    attempt_started_at.elapsed(),
                )
                .with_diagnostic(diagnostic)
                .with_client_simulation(client_simulation),
            );
            last_error = Some(RelayError::Upstream(classified));
            if is_retryable(classified) {
                index = self.next_candidate_index(index, classified);
                continue;
            }
            return Err(RelayExecutionError::new(
                RelayError::Upstream(classified),
                failures,
            ));
        }

        Err(RelayExecutionError::new(
            last_error.unwrap_or(RelayError::Upstream(UpstreamError::ModelUnsupported)),
            failures,
        ))
    }

    fn next_candidate_index(&self, index: usize, error: UpstreamError) -> usize {
        if is_credential_scoped(error) {
            index + 1
        } else {
            self.next_channel_index(index)
        }
    }

    fn next_channel_index(&self, index: usize) -> usize {
        let Some(channel_group) = self.candidates[index].channel_group else {
            return index + 1;
        };
        let mut next = index + 1;
        while self
            .candidates
            .get(next)
            .is_some_and(|candidate| candidate.channel_group == Some(channel_group))
        {
            next += 1;
        }
        next
    }
}

pub(crate) async fn release_attempt_permit(permit: Option<Box<dyn RelayAttemptPermit>>) {
    if let Some(permit) = permit {
        permit.release().await;
    }
}

impl fmt::Debug for RelayStateMachine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayStateMachine")
            .field("candidate_count", &self.candidates.len())
            .finish()
    }
}

fn is_valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

fn channel_groups_are_contiguous(candidates: &[RelayCandidate]) -> bool {
    let mut closed = BTreeSet::new();
    let mut current = None;
    for candidate in candidates {
        if candidate.channel_group == current {
            continue;
        }
        if let Some(previous) = current {
            closed.insert(previous);
        }
        if candidate
            .channel_group
            .is_some_and(|channel_group| closed.contains(&channel_group))
        {
            return false;
        }
        current = candidate.channel_group;
    }
    true
}

pub(crate) const fn is_credential_scoped(error: UpstreamError) -> bool {
    matches!(
        error,
        UpstreamError::AuthExpired
            | UpstreamError::AuthRevoked
            | UpstreamError::AccountDisabled
            | UpstreamError::RateLimited {
                scope: af_domain::RateLimitScope::Credential
                    | af_domain::RateLimitScope::Window
                    | af_domain::RateLimitScope::Unknown,
                ..
            }
            | UpstreamError::QuotaExhausted
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejecting_protocol_success_retains_client_simulation_metadata() {
        let simulation = Some(RelayClientSimulationAttempt::new(
            ClientSimulationProfile::AnthropicCliHeadersV1,
            ClientSimulationResult::Applied,
        ));
        let mut report = RelayAttemptReport::succeeded(2, Duration::from_millis(7), Vec::new())
            .with_successful_client_simulation(simulation);

        report.reject_success(UpstreamError::ProtocolError);

        assert_eq!(report.successful_client_simulation(), None);
        assert_eq!(report.failures().len(), 1);
        assert_eq!(report.failures()[0].client_simulation(), simulation);
        assert_eq!(report.failures()[0].error(), UpstreamError::ProtocolError);
    }
}
