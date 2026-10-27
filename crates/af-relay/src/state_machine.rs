use std::{
    collections::BTreeSet,
    fmt,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use af_adapter::{
    Adaptor, AdaptorError, AdaptorTarget, Bytes, ClientSimulationContext,
    ClientSimulationMiddleware, Credential, HeaderMap, HeaderName, HeaderValue, Method, Operation,
    RelayContext, ResponseMode, TransportDispatcher, UpstreamRequest, UpstreamResponse,
    apply_client_simulation,
};
use af_domain::{
    ChannelId, ClientSimulationProfile, ClientSimulationResult, MAX_MODEL_NAME_BYTES, Protocol,
    UpstreamError, UpstreamServerStatus,
};
use af_protocol::{StructuredErrorEvidence, extract_structured_error_evidence};
use thiserror::Error;
use tokio::time::sleep;

use crate::{
    RelayAttemptDiagnostic, RelayAttemptGate, RelayAttemptGateError, RelayAttemptPermit,
    RelayBuildError, RelayDiagnosticPolicy, RelayError,
    error::{
        classify_upstream_status, is_retryable, is_same_candidate_retryable, map_adaptor_error,
        parse_retry_after,
    },
};

/// 单次转发候选的安全装配结果；调度器只负责按顺序提供候选，不在本类型内读库。
#[derive(Clone)]
pub struct RelayCandidate {
    adaptor: Arc<dyn Adaptor>,
    context: RelayContext,
    credential: Credential,
    request: Option<RelayCandidateRequest>,
    header_overrides: HeaderMap,
    client_simulation: Option<Arc<dyn ClientSimulationMiddleware>>,
    client_profile: Option<af_domain::ClientProfileSnapshot>,
    channel_group: Option<ChannelId>,
    transport: TransportDispatcher,
    attempt_gate: Option<Arc<dyn RelayAttemptGate>>,
    retry_limit: Option<u8>,
    retry_delay_ms: Option<u64>,
    race_width: Option<usize>,
    response_validation: Option<af_domain::Protocol>,
    first_output_timeout: Duration,
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
            client_profile: None,
            channel_group: None,
            transport: TransportDispatcher::http(),
            attempt_gate: None,
            retry_limit: None,
            retry_delay_ms: None,
            race_width: None,
            response_validation: None,
            first_output_timeout: Duration::from_secs(300),
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

    /// 绑定请求开始时冻结的档案；发送阶段只读取该快照。
    #[must_use]
    pub fn with_client_profile(mut self, profile: af_domain::ClientProfileSnapshot) -> Self {
        self.client_profile = Some(profile);
        self
    }

    /// 标记候选所属渠道；同一渠道的多条凭据必须连续排列。
    #[must_use]
    pub const fn with_channel_group(mut self, channel_id: ChannelId) -> Self {
        self.channel_group = Some(channel_id);
        self
    }

    /// 校验成功响应后再选定候选；流式请求等待首个有效输出。
    #[must_use]
    pub const fn with_response_validation(mut self, protocol: af_domain::Protocol) -> Self {
        self.response_validation = Some(protocol);
        self
    }

    /// 继承渠道或全局客户端的首输出等待上限。
    #[must_use]
    pub const fn with_first_output_timeout(mut self, timeout: Duration) -> Self {
        self.first_output_timeout = timeout;
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

    #[must_use]
    pub const fn with_retry_limit(mut self, retry_limit: Option<u8>) -> Self {
        self.retry_limit = retry_limit;
        self
    }

    #[must_use]
    pub const fn with_retry_delay_ms(mut self, retry_delay_ms: Option<u64>) -> Self {
        self.retry_delay_ms = retry_delay_ms;
        self
    }

    #[must_use]
    pub const fn with_race_width(mut self, race_width: Option<usize>) -> Self {
        self.race_width = race_width;
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
            .field("retry_limit", &self.retry_limit)
            .field("retry_delay_ms", &self.retry_delay_ms)
            .field("race_width", &self.race_width)
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
    body_factory: Option<Arc<dyn Fn() -> Result<Bytes, RelayError> + Send + Sync>>,
}

impl RelayCandidateRequest {
    /// 创建候选级请求；正文容量仍由统一的 [`UpstreamRequest`] 边界复验。
    pub fn new(model: impl Into<String>, body: Option<Bytes>) -> Result<Self, RelayError> {
        let model = model.into();
        if !is_valid_model(&model) {
            return Err(RelayError::InvalidModel);
        }
        Ok(Self {
            model,
            body,
            body_factory: None,
        })
    }

    /// 创建候选级延迟正文；内容仅在该候选实际执行时构造。
    pub fn with_body_factory<F>(
        model: impl Into<String>,
        body_factory: F,
    ) -> Result<Self, RelayError>
    where
        F: Fn() -> Result<Bytes, RelayError> + Send + Sync + 'static,
    {
        let model = model.into();
        if !is_valid_model(&model) {
            return Err(RelayError::InvalidModel);
        }
        Ok(Self {
            model,
            body: None,
            body_factory: Some(Arc::new(body_factory)),
        })
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn body(&self) -> Result<Option<Bytes>, RelayError> {
        if let Some(body) = &self.body {
            return Ok(Some(body.clone()));
        }
        self.body_factory
            .as_ref()
            .map(|factory| factory())
            .transpose()
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
            .field("has_body_factory", &self.body_factory.is_some())
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

    /// 流到达逻辑终点后补全最终耗时。
    pub fn complete_success(&mut self, elapsed: Duration) {
        if self.successful_candidate_index.is_some() {
            self.successful_elapsed = Some(elapsed);
        }
    }

    /// 下游放弃读取时撤销尚未完成的成功反馈，不给上游附加失败惩罚。
    pub fn discard_pending_success(&mut self) {
        self.successful_candidate_index = None;
        self.successful_elapsed = None;
        self.successful_diagnostic = None;
        self.successful_client_simulation = None;
    }

    /// 把已经取得 HTTP 成功、但未通过协议验证的候选改记为失败。
    pub fn reject_success(&mut self, error: UpstreamError) {
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
#[derive(Clone)]
pub struct RelayRequest {
    model: String,
    operation: Operation,
    method: Method,
    body: Option<Bytes>,
    response_mode: ResponseMode,
    response_body_limit: usize,
    anthropic_beta: Option<String>,
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
            anthropic_beta: None,
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

    /// 附加唯一允许跨协议入口传递的客户端 Anthropic beta 列表。
    pub fn with_anthropic_beta(mut self, value: Option<&str>) -> Result<Self, RelayError> {
        self.anthropic_beta = value
            .map(af_protocol::normalize_anthropic_beta)
            .transpose()
            .map_err(|_| RelayError::Request(AdaptorError::InvalidHeader))?
            .filter(|value| !value.is_empty());
        Ok(self)
    }

    fn apply_anthropic_beta(
        &self,
        protocol: Protocol,
        headers: &mut HeaderMap,
    ) -> Result<(), AdaptorError> {
        if protocol != Protocol::Anthropic {
            return Ok(());
        }
        let Some(client_beta) = self.anthropic_beta.as_deref() else {
            return Ok(());
        };
        let mut values = headers
            .get_all("anthropic-beta")
            .iter()
            .map(|value| value.to_str())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AdaptorError::InvalidHeader)?;
        values.push(client_beta);
        let beta = af_protocol::normalize_anthropic_beta_values(values)
            .map_err(|_| AdaptorError::InvalidHeader)?;
        let value =
            HeaderValue::from_bytes(beta.as_bytes()).map_err(|_| AdaptorError::InvalidHeader)?;
        headers.insert(HeaderName::from_static("anthropic-beta"), value);
        Ok(())
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
            .field("has_anthropic_beta", &self.anthropic_beta.is_some())
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
    continuation: Option<crate::frozen_candidate::FrozenCandidate>,
}

struct RaceResponse(Option<RelayResponse>);

impl RaceResponse {
    fn take(mut self) -> RelayResponse {
        self.0.take().expect("竞速任务响应只能消费一次")
    }
}

impl Drop for RaceResponse {
    fn drop(&mut self) {
        let Some(response) = self.0.take() else {
            return;
        };
        let (_, _, permit) = response.into_parts_with_permit();
        drop(permit);
    }
}

impl RelayResponse {
    pub(crate) fn into_tool_parts(
        self,
    ) -> (
        UpstreamResponse,
        RelayAttemptReport,
        Option<Box<dyn RelayAttemptPermit>>,
        Option<crate::frozen_candidate::FrozenCandidate>,
    ) {
        (self.response, self.report, self.permit, self.continuation)
    }

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
    execution_policy: RelayExecutionPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayExecutionPolicy {
    race_width: usize,
    retry_limit: u8,
    retry_delay: Duration,
}

impl RelayExecutionPolicy {
    #[must_use]
    pub const fn new(race_width: usize, retry_limit: u8, retry_delay: Duration) -> Self {
        Self {
            race_width: if race_width == 0 { 1 } else { race_width },
            retry_limit,
            retry_delay,
        }
    }
    #[must_use]
    pub const fn race_width(self) -> usize {
        self.race_width
    }
    #[must_use]
    pub const fn retry_limit(self) -> u8 {
        self.retry_limit
    }
    #[must_use]
    pub const fn retry_delay(self) -> Duration {
        self.retry_delay
    }
    pub fn with_channel_settings(
        self,
        race_width: Option<usize>,
        retry_limit: Option<u8>,
        retry_delay_ms: Option<u64>,
    ) -> Self {
        Self {
            race_width: race_width.unwrap_or(self.race_width).max(1),
            retry_limit: retry_limit.unwrap_or(self.retry_limit),
            retry_delay: Duration::from_millis(
                retry_delay_ms.unwrap_or(self.retry_delay.as_millis() as u64),
            ),
        }
    }
}

impl Default for RelayExecutionPolicy {
    fn default() -> Self {
        Self::new(1, 0, Duration::ZERO)
    }
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
            execution_policy: RelayExecutionPolicy::default(),
        })
    }

    pub fn requires_conversation_decoder(&self) -> bool {
        self.candidates
            .iter()
            .any(|candidate| candidate.adaptor.channel_type() == af_domain::ChannelType::OpenCode)
    }

    /// 为当前请求启用安全诊断；策略只影响永久脱敏快照，不改变转发语义。
    #[must_use]
    pub const fn with_diagnostic_policy(mut self, policy: RelayDiagnosticPolicy) -> Self {
        self.diagnostic_policy = Some(policy);
        self
    }

    #[must_use]
    pub const fn with_execution_policy(mut self, policy: RelayExecutionPolicy) -> Self {
        self.execution_policy = policy;
        self
    }

    /// 按 Routing → Credential → Sending → Classifying/Streaming → Retry 执行一次请求。
    ///
    /// 按配置重试可恢复的上游故障；客户端参数错误不会继续扩散。
    /// 状态机不持有计费会话，调用方应在外层让同一 BillingSession 覆盖整个执行。
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
        self.execute_inner(request, false).await
    }

    /// 工具补充只允许 OpenCode；候选正文须已纳入同一份预扣计划。
    /// 暂不接受另行覆盖的候选正文，避免工具映射和实际发送内容不一致。
    pub(crate) async fn execute_tool_request(
        &self,
        request: RelayRequest,
        protocol: af_domain::Protocol,
    ) -> Result<RelayResponse, RelayExecutionError> {
        if request.method != Method::POST
            || !matches!(request.operation, Operation::Chat | Operation::Responses)
            || self.candidates.iter().any(|candidate| {
                candidate.adaptor.channel_type() != af_domain::ChannelType::OpenCode
                    || candidate.adaptor.default_protocol() != protocol
                    || candidate.request.is_some()
            })
        {
            return Err(RelayExecutionError::new(
                RelayError::Upstream(UpstreamError::ProtocolError),
                Vec::new(),
            ));
        }
        self.execute_inner(request, true).await
    }

    async fn execute_inner(
        &self,
        request: RelayRequest,
        freeze_continuation: bool,
    ) -> Result<RelayResponse, RelayExecutionError> {
        let attempt_count = Arc::new(AtomicUsize::new(0));
        let race_width = self
            .candidates
            .iter()
            .map(|candidate| {
                candidate
                    .race_width
                    .unwrap_or(self.execution_policy.race_width)
            })
            .max()
            .unwrap_or(1);
        if race_width > 1 {
            return self
                .execute_race(request, freeze_continuation, attempt_count)
                .await;
        }
        self.execute_serial(request, freeze_continuation, attempt_count)
            .await
    }

    async fn execute_race(
        &self,
        request: RelayRequest,
        freeze_continuation: bool,
        attempt_count: Arc<AtomicUsize>,
    ) -> Result<RelayResponse, RelayExecutionError> {
        let mut index = 0;
        let mut last_error = RelayError::Upstream(UpstreamError::ModelUnsupported);
        let mut failures = Vec::new();
        while index < self.candidates.len() {
            let channel = self.candidates[index].channel_group;
            let mut end = index + 1;
            while end < self.candidates.len() && self.candidates[end].channel_group == channel {
                end += 1;
            }
            let race_width = self.candidates[index]
                .race_width
                .unwrap_or(self.execution_policy.race_width);
            if race_width <= 1 {
                let child = RelayStateMachine {
                    candidates: vec![self.candidates[index].clone()],
                    diagnostic_policy: self.diagnostic_policy,
                    execution_policy: self.execution_policy,
                };
                match child
                    .execute_serial(request.clone(), freeze_continuation, attempt_count.clone())
                    .await
                {
                    Ok(mut response) => {
                        response.report.successful_candidate_index = response
                            .report
                            .successful_candidate_index
                            .map(|value| value + index);
                        for failure in &mut response.report.failures {
                            failure.candidate_index += index;
                        }
                        failures.append(&mut response.report.failures);
                        response.report.failures = failures;
                        response.attempts =
                            NonZeroUsize::new(attempt_count.load(Ordering::Relaxed).max(1))
                                .expect("成功响应的上游尝试次数必须为正");
                        return Ok(response);
                    }
                    Err(error) => {
                        let (last_error, report) = error.into_parts();
                        failures.extend(report.failures.into_iter().map(|mut failure| {
                            failure.candidate_index += index;
                            failure
                        }));
                        index = match last_error {
                            RelayError::Upstream(error) => self.next_candidate_index(index, error),
                            _ => index + 1,
                        };
                        if index >= self.candidates.len() {
                            return Err(RelayExecutionError::new(last_error, failures));
                        }
                        continue;
                    }
                }
            }
            let mut tasks = tokio::task::JoinSet::new();
            let spawn_candidate = |tasks: &mut tokio::task::JoinSet<_>, candidate_index: usize| {
                let candidate = self.candidates[candidate_index].clone();
                let policy = self.execution_policy;
                let diagnostic_policy = self.diagnostic_policy;
                let request = request.clone();
                let attempt_count = attempt_count.clone();
                tasks.spawn(async move {
                    let child = RelayStateMachine {
                        candidates: vec![candidate],
                        diagnostic_policy,
                        execution_policy: policy,
                    };
                    (
                        candidate_index,
                        child
                            .execute_serial(request, freeze_continuation, attempt_count)
                            .await
                            .map(|response| RaceResponse(Some(response))),
                    )
                });
            };
            let mut next = index;
            while next < end && tasks.len() < race_width {
                spawn_candidate(&mut tasks, next);
                next += 1;
            }
            let mut fallback_index = end;
            while let Some(joined) = tasks.join_next().await {
                let (candidate_index, result) = match joined {
                    Ok(result) => result,
                    Err(_) => {
                        last_error = RelayError::AttemptGateFailed;
                        if next < end {
                            spawn_candidate(&mut tasks, next);
                            next += 1;
                        }
                        continue;
                    }
                };
                match result {
                    Ok(race_response) => {
                        let mut response = race_response.take();
                        response.report.successful_candidate_index = response
                            .report
                            .successful_candidate_index
                            .map(|value| value + candidate_index);
                        for failure in &mut response.report.failures {
                            failure.candidate_index += candidate_index;
                        }
                        failures.append(&mut response.report.failures);
                        response.report.failures = failures;
                        response.attempts =
                            NonZeroUsize::new(attempt_count.load(Ordering::Relaxed).max(1))
                                .expect("成功响应的上游尝试次数必须为正");
                        tasks.abort_all();
                        while tasks.join_next().await.is_some() {}
                        return Ok(response);
                    }
                    Err(error) => {
                        let (error, report) = error.into_parts();
                        failures.extend(report.failures.into_iter().map(|mut failure| {
                            failure.candidate_index += candidate_index;
                            failure
                        }));
                        if let RelayError::Upstream(upstream) = error {
                            let following = self.next_candidate_index(candidate_index, upstream);
                            if following > candidate_index + 1 {
                                // Channel-wide failures stop refilling this channel, while
                                // already running candidates may still produce a valid winner.
                                next = end;
                                fallback_index = fallback_index.max(following);
                            }
                        }
                        last_error = error;
                        if next < end {
                            spawn_candidate(&mut tasks, next);
                            next += 1;
                        }
                    }
                }
            }
            if fallback_index >= self.candidates.len() {
                return Err(RelayExecutionError::new(last_error, failures));
            }
            index = fallback_index;
        }
        Err(RelayExecutionError::new(last_error, failures))
    }

    async fn execute_serial(
        &self,
        request: RelayRequest,
        freeze_continuation: bool,
        attempt_count: Arc<AtomicUsize>,
    ) -> Result<RelayResponse, RelayExecutionError> {
        let mut last_error = None;
        let mut failures = Vec::new();
        let mut retry_counts = vec![0; self.candidates.len()];
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
            let target =
                AdaptorTarget::new(candidate_model, request.operation, request.response_mode);
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
            if let Err(error) = candidate.adaptor.setup_headers_for_target(
                &mut headers,
                &candidate.credential,
                &candidate.context,
                target,
            ) {
                last_error = Some(RelayError::Adaptor(error));
                index = self.next_channel_index(index);
                continue;
            }
            headers.extend(candidate.header_overrides.clone());
            if let Err(error) =
                request.apply_anthropic_beta(candidate.adaptor.default_protocol(), &mut headers)
            {
                last_error = Some(RelayError::Request(error));
                index = self.next_channel_index(index);
                continue;
            }

            let target = match candidate.adaptor.build_url(&candidate.context, target) {
                Ok(target) => target,
                Err(error) => {
                    last_error = Some(RelayError::Adaptor(error));
                    index = self.next_channel_index(index);
                    continue;
                }
            };
            let outbound_body = if let Some(candidate_request) = &candidate.request {
                match candidate_request.body() {
                    Ok(body) => body,
                    Err(error) => {
                        last_error = Some(error);
                        index = self.next_channel_index(index);
                        continue;
                    }
                }
            } else {
                request.body.clone()
            };
            let upstream_request =
                match UpstreamRequest::new(request.method.clone(), target, headers, outbound_body)
                    .and_then(|upstream_request| {
                        upstream_request.with_response_body_limit(request.response_body_limit)
                    })
                    .map(|upstream_request| {
                        upstream_request.with_response_mode(request.response_mode)
                    }) {
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
            let upstream_request = match candidate.client_profile.as_ref() {
                Some(profile) => match af_adapter::apply_client_profile_identity(
                    profile,
                    ClientSimulationContext::new(
                        candidate.adaptor.channel_type(),
                        candidate.adaptor.default_protocol(),
                        request.operation,
                        candidate.credential.kind(),
                    ),
                    candidate.context.oauth_provider(),
                    upstream_request,
                ) {
                    Ok(request) => request,
                    Err(_) => {
                        failures.push(RelayAttemptFailure::new(
                            index,
                            UpstreamError::ProtocolError,
                            attempt_started_at.elapsed(),
                        ));
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
            attempt_count.fetch_add(1, Ordering::Relaxed);
            let continuation = freeze_continuation.then(|| {
                crate::frozen_candidate::FrozenCandidate::new(
                    candidate.adaptor.clone(),
                    candidate.context.clone(),
                    candidate.credential.clone(),
                    candidate.transport.clone(),
                    &upstream_request,
                    self.diagnostic_policy,
                )
            });
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
                        let limit = candidate
                            .retry_limit
                            .unwrap_or(self.execution_policy.retry_limit);
                        if is_same_candidate_retryable(mapped)
                            && reserve_candidate_retry(&mut retry_counts, index, limit)
                        {
                            let delay = candidate
                                .retry_delay_ms
                                .map_or(self.execution_policy.retry_delay, Duration::from_millis);
                            if !delay.is_zero() {
                                sleep(delay).await;
                            }
                            continue;
                        }
                        index = self.next_candidate_index(index, mapped);
                        continue;
                    }
                    return Err(RelayExecutionError::new(
                        RelayError::Upstream(mapped),
                        failures,
                    ));
                }
            };
            let (mut response, transport_fallback) = outcome.into_parts();
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
                if let Some(protocol) = candidate.response_validation {
                    match crate::response_readiness::validate(
                        response,
                        protocol,
                        request.response_mode,
                        candidate.first_output_timeout,
                    )
                    .await
                    {
                        Ok(ready) => response = ready,
                        Err(error) => {
                            release_attempt_permit(permit).await;
                            failures.push(
                                RelayAttemptFailure::new(
                                    index,
                                    error,
                                    attempt_started_at.elapsed(),
                                )
                                .with_diagnostic(diagnostic)
                                .with_client_simulation(client_simulation),
                            );
                            last_error = Some(RelayError::Upstream(error));
                            let limit = candidate
                                .retry_limit
                                .unwrap_or(self.execution_policy.retry_limit);
                            if (is_same_candidate_retryable(error)
                                || error == UpstreamError::ProtocolError)
                                && reserve_candidate_retry(&mut retry_counts, index, limit)
                            {
                                let delay = candidate.retry_delay_ms.map_or(
                                    self.execution_policy.retry_delay,
                                    Duration::from_millis,
                                );
                                if !delay.is_zero() {
                                    sleep(delay).await;
                                }
                                continue;
                            }
                            if is_retryable(error) || error == UpstreamError::ProtocolError {
                                index = self.next_candidate_index(index, error);
                                continue;
                            }
                            return Err(RelayExecutionError::new(
                                RelayError::Upstream(error),
                                failures,
                            ));
                        }
                    }
                }
                let state = if request.response_mode == ResponseMode::Stream {
                    RelayState::Streaming
                } else {
                    RelayState::Completed
                };
                return Ok(RelayResponse {
                    response,
                    // 保持候选遍历次数语义，并把同一候选的额外重试计入总次数。
                    attempts: NonZeroUsize::new(
                        index
                            + 1
                            + retry_counts
                                .iter()
                                .map(|&retries| usize::from(retries))
                                .sum::<usize>(),
                    )
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
                    continuation,
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
                        let limit = candidate
                            .retry_limit
                            .unwrap_or(self.execution_policy.retry_limit);
                        if is_same_candidate_retryable(mapped)
                            && reserve_candidate_retry(&mut retry_counts, index, limit)
                        {
                            let delay = candidate
                                .retry_delay_ms
                                .map_or(self.execution_policy.retry_delay, Duration::from_millis);
                            if !delay.is_zero() {
                                sleep(delay).await;
                            }
                            continue;
                        }
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
                let limit = candidate
                    .retry_limit
                    .unwrap_or(self.execution_policy.retry_limit);
                if is_same_candidate_retryable(classified)
                    && reserve_candidate_retry(&mut retry_counts, index, limit)
                {
                    let delay = candidate
                        .retry_delay_ms
                        .map_or(self.execution_policy.retry_delay, Duration::from_millis);
                    if !delay.is_zero() {
                        sleep(delay).await;
                    }
                    continue;
                }
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
        if is_credential_scoped(error)
            || matches!(
                error,
                UpstreamError::Overloaded { .. }
                    | UpstreamError::ServerError { .. }
                    | UpstreamError::ProtocolError
            )
            || matches!(error, UpstreamError::Network { kind } if kind != af_domain::NetworkFailureKind::Policy)
        {
            if self.candidates[index].adaptor.channel_type() == af_domain::ChannelType::OpenCode
                && !self.candidates.get(index + 1).is_some_and(|next| {
                    next.channel_group.is_some()
                        && next.channel_group == self.candidates[index].channel_group
                })
            {
                return self.candidates.len();
            }
            index + 1
        } else {
            self.next_channel_index(index)
        }
    }

    fn next_channel_index(&self, index: usize) -> usize {
        if self.candidates[index].adaptor.channel_type() == af_domain::ChannelType::OpenCode {
            return self.candidates.len();
        }
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

fn reserve_candidate_retry(retry_counts: &mut [u8], candidate_index: usize, limit: u8) -> bool {
    let Some(retries) = retry_counts.get_mut(candidate_index) else {
        return false;
    };
    if *retries >= limit {
        return false;
    }
    *retries += 1;
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
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn candidate_body_factory_runs_only_when_body_is_requested() {
        let calls = Arc::new(Mutex::new(0));
        let factory_calls = Arc::clone(&calls);
        let request = RelayCandidateRequest::with_body_factory("mapped-model", move || {
            *factory_calls.lock().unwrap() += 1;
            Ok(Bytes::from_static(b"mapped body"))
        })
        .unwrap();

        assert_eq!(*calls.lock().unwrap(), 0);
        assert_eq!(
            request.body().unwrap(),
            Some(Bytes::from_static(b"mapped body"))
        );
        assert_eq!(*calls.lock().unwrap(), 1);
    }

    struct CaptureHeadersAdaptor {
        protocol: Protocol,
        captured: Arc<Mutex<Vec<HeaderMap>>>,
    }

    #[async_trait::async_trait]
    impl Adaptor for CaptureHeadersAdaptor {
        fn channel_type(&self) -> af_domain::ChannelType {
            af_domain::ChannelType::Custom
        }

        fn default_protocol(&self) -> Protocol {
            self.protocol
        }

        fn default_base_url(&self) -> &str {
            "https://upstream.example"
        }

        fn supported_models(&self) -> Vec<String> {
            Vec::new()
        }

        fn build_url(
            &self,
            _context: &RelayContext,
            _target: AdaptorTarget<'_>,
        ) -> af_adapter::AdaptorResult<String> {
            Ok("https://upstream.example/v1/messages".to_owned())
        }

        fn setup_headers(
            &self,
            headers: &mut HeaderMap,
            _credential: &Credential,
            _context: &RelayContext,
        ) -> af_adapter::AdaptorResult<()> {
            headers.insert("x-api-key", HeaderValue::from_static("upstream-api-key"));
            Ok(())
        }

        async fn finalize_request(
            &self,
            request: UpstreamRequest,
            _credential: &Credential,
            _context: &RelayContext,
        ) -> af_adapter::AdaptorResult<UpstreamRequest> {
            self.captured
                .lock()
                .unwrap()
                .push(request.headers().clone());
            // Capture the fully prepared request without starting a transport.
            Err(AdaptorError::InvalidTaskRequest)
        }
    }

    #[tokio::test]
    async fn client_beta_survives_serial_and_racing_candidates_only_for_anthropic_targets() {
        for protocol in [Protocol::Anthropic, Protocol::OpenAiChat] {
            for race_width in [1, 2] {
                let captured = Arc::new(Mutex::new(Vec::new()));
                let client = af_adapter::HttpClientPool::default()
                    .get(&af_adapter::HttpClientConfig::default())
                    .unwrap();
                let mut overrides = HeaderMap::new();
                overrides.append(
                    "anthropic-beta",
                    HeaderValue::from_static("channel-beta,shared-beta"),
                );
                overrides.append(
                    "anthropic-beta",
                    HeaderValue::from_static("channel-other-beta"),
                );
                let candidates = (0..2)
                    .map(|_| {
                        RelayCandidate::new(
                            Arc::new(CaptureHeadersAdaptor {
                                protocol,
                                captured: captured.clone(),
                            }),
                            RelayContext::new(client.clone()),
                            Credential::api_key("upstream-api-key").unwrap(),
                        )
                        .with_channel_group(ChannelId::new(1).unwrap())
                        .with_header_overrides(overrides.clone())
                    })
                    .collect();
                let machine = RelayStateMachine::new(candidates)
                    .unwrap()
                    .with_execution_policy(RelayExecutionPolicy::new(
                        race_width,
                        0,
                        Duration::ZERO,
                    ));
                let request = RelayRequest::new(
                    "claude-test",
                    Operation::Chat,
                    Method::POST,
                    Some(Bytes::from_static(b"{}")),
                )
                .unwrap()
                .with_anthropic_beta(Some("client-beta,shared-beta"))
                .unwrap();
                assert!(!format!("{request:?}").contains("client-beta"));
                assert!(machine.execute(request).await.is_err());
                let captured = captured.lock().unwrap();
                assert_eq!(captured.len(), if race_width == 1 { 1 } else { 2 });
                for headers in captured.iter() {
                    assert_eq!(headers["x-api-key"], "upstream-api-key");
                    if protocol == Protocol::Anthropic {
                        assert_eq!(
                            headers["anthropic-beta"],
                            "channel-beta,shared-beta,channel-other-beta,client-beta"
                        );
                        assert_eq!(headers.get_all("anthropic-beta").iter().count(), 1);
                    } else {
                        assert_eq!(headers.get_all("anthropic-beta").iter().count(), 2);
                        assert!(!headers.get_all("anthropic-beta").iter().any(|value| {
                            value
                                .as_bytes()
                                .windows(b"client-beta".len())
                                .any(|part| part == b"client-beta")
                        }));
                    }
                }
            }
        }
    }

    #[test]
    fn opencode_failure_never_falls_through_to_another_channel() {
        let client = af_adapter::HttpClientPool::default()
            .get(&af_adapter::HttpClientConfig::default())
            .unwrap();
        let candidate = |channel, opencode| {
            let adaptor: Arc<dyn Adaptor> = if opencode {
                Arc::new(
                    af_adapter::OpenCodeAdaptor::new(
                        af_domain::OpenCodeEntry::InferenceV1,
                        af_domain::Protocol::OpenAiChat,
                        vec![],
                    )
                    .unwrap(),
                )
            } else {
                Arc::new(af_adapter::OpenAiAdaptor::new())
            };
            RelayCandidate::new(
                adaptor,
                RelayContext::new(client.clone()),
                Credential::api_key("fixture").unwrap(),
            )
            .with_channel_group(ChannelId::new(channel).unwrap())
        };
        let machine = RelayStateMachine::new(vec![
            candidate(1, true),
            candidate(1, true),
            candidate(2, false),
        ])
        .unwrap();
        assert_eq!(
            machine.next_candidate_index(0, UpstreamError::AuthRevoked),
            1
        );
        assert_eq!(
            machine.next_candidate_index(1, UpstreamError::AuthRevoked),
            3
        );
        assert_eq!(machine.next_channel_index(0), 3);
        assert!(machine.requires_conversation_decoder());
        let standard =
            RelayStateMachine::new(vec![candidate(2, false), candidate(3, false)]).unwrap();
        assert_eq!(standard.next_channel_index(0), 1);
    }

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
