use std::{collections::BTreeSet, fmt, num::NonZeroUsize, sync::Arc};

use af_adapter::{
    Adaptor, AdaptorSendExt, Bytes, Credential, HeaderMap, Method, Operation, RelayContext,
    ResponseMode, UpstreamRequest, UpstreamResponse,
};
use af_domain::{ChannelId, MAX_MODEL_NAME_BYTES, UpstreamError};
use thiserror::Error;

use crate::{
    RelayBuildError, RelayError,
    error::{classify_upstream_status, is_retryable, map_adaptor_error},
};

/// 单次转发候选的安全装配结果；调度器只负责按顺序提供候选，不在本类型内读库。
pub struct RelayCandidate {
    adaptor: Arc<dyn Adaptor>,
    context: RelayContext,
    credential: Credential,
    header_overrides: HeaderMap,
    channel_group: Option<ChannelId>,
}

impl RelayCandidate {
    /// 构造一次尝试所需的适配器、受控上下文和已解密凭据视图。
    #[must_use]
    pub fn new(adaptor: Arc<dyn Adaptor>, context: RelayContext, credential: Credential) -> Self {
        Self {
            adaptor,
            context,
            credential,
            header_overrides: HeaderMap::new(),
            channel_group: None,
        }
    }

    /// 附加已由持久化边界验证的非认证 Header 覆盖。
    #[must_use]
    pub fn with_header_overrides(mut self, header_overrides: HeaderMap) -> Self {
        self.header_overrides = header_overrides;
        self
    }

    /// 标记候选所属渠道；同一渠道的多条凭据必须连续排列。
    #[must_use]
    pub const fn with_channel_group(mut self, channel_id: ChannelId) -> Self {
        self.channel_group = Some(channel_id);
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
            .field("header_override_count", &self.header_overrides.len())
            .field("has_channel_group", &self.channel_group.is_some())
            .finish()
    }
}

/// 单个候选失败后的脱敏尝试记录。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayAttemptFailure {
    candidate_index: usize,
    error: UpstreamError,
}

impl RelayAttemptFailure {
    fn new(candidate_index: usize, error: UpstreamError) -> Self {
        Self {
            candidate_index,
            error,
        }
    }

    /// 返回失败候选在固定计划中的零基索引。
    #[must_use]
    pub const fn candidate_index(self) -> usize {
        self.candidate_index
    }

    /// 返回不含上游正文的闭合错误分类。
    #[must_use]
    pub const fn error(self) -> UpstreamError {
        self.error
    }
}

/// 一次转发执行产生的脱敏候选报告。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RelayAttemptReport {
    successful_candidate_index: Option<usize>,
    failures: Vec<RelayAttemptFailure>,
}

impl RelayAttemptReport {
    fn succeeded(successful_candidate_index: usize, failures: Vec<RelayAttemptFailure>) -> Self {
        Self {
            successful_candidate_index: Some(successful_candidate_index),
            failures,
        }
    }

    fn failed(failures: Vec<RelayAttemptFailure>) -> Self {
        Self {
            successful_candidate_index: None,
            failures,
        }
    }

    /// 返回最终成功候选索引；执行失败时为空。
    #[must_use]
    pub const fn successful_candidate_index(&self) -> Option<usize> {
        self.successful_candidate_index
    }

    /// 返回按尝试顺序排列的失败分类。
    #[must_use]
    pub fn failures(&self) -> &[RelayAttemptFailure] {
        &self.failures
    }

    /// 把已经取得 HTTP 成功、但未通过协议验证的候选改记为失败。
    pub(crate) fn reject_success(&mut self, error: UpstreamError) {
        if let Some(candidate_index) = self.successful_candidate_index.take() {
            self.failures
                .push(RelayAttemptFailure::new(candidate_index, error));
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
        })
    }

    /// 选择流式响应交付；流式响应返回后由调用方负责消费与取消传播。
    #[must_use]
    pub const fn with_response_mode(mut self, response_mode: ResponseMode) -> Self {
        self.response_mode = response_mode;
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
}

impl fmt::Debug for RelayResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayResponse")
            .field("attempts", &self.attempts)
            .field("state", &self.state)
            .field("report", &self.report)
            .field("response", &self.response)
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
    fn new(error: RelayError, failures: Vec<RelayAttemptFailure>) -> Self {
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
        Ok(Self { candidates })
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
            let supported_models = candidate.adaptor.supported_models();
            if !supported_models.is_empty()
                && !supported_models.iter().any(|model| model == &request.model)
            {
                let error = UpstreamError::ModelUnsupported;
                failures.push(RelayAttemptFailure::new(index, error));
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

            let target = match candidate
                .adaptor
                .build_url(&candidate.context, request.operation)
            {
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
                request.body.clone(),
            )
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
            let response = match candidate
                .adaptor
                .send(upstream_request, &candidate.context)
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let mapped = map_adaptor_error(error);
                    failures.push(RelayAttemptFailure::new(index, mapped));
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
                    report: RelayAttemptReport::succeeded(index, failures),
                });
            }

            let status = response.status();
            let body = match response.into_body().into_bytes().await {
                Ok(body) => body,
                Err(error) => {
                    let mapped = map_adaptor_error(error);
                    failures.push(RelayAttemptFailure::new(index, mapped));
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
            let classified = classify_upstream_status(status, &body);
            failures.push(RelayAttemptFailure::new(index, classified));
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

const fn is_credential_scoped(error: UpstreamError) -> bool {
    matches!(
        error,
        UpstreamError::AuthExpired
            | UpstreamError::AuthRevoked
            | UpstreamError::RateLimited
            | UpstreamError::QuotaExhausted
    )
}
