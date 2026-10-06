use std::{collections::BTreeSet, fmt, sync::Arc, time::Instant};

use af_adapter::{
    AdaptorError, AdaptorTransportError, Credential, HeaderMap, RelayContext, TransportDispatcher,
    VideoTaskAdaptor,
};
use af_domain::{ChannelId, TaskSubmission, UpstreamError, UpstreamServerStatus, UpstreamTaskId};
use af_protocol::{
    CanonicalTaskPoll, CanonicalVideoGenerationRequest, StructuredErrorEvidence, VideoModel,
    extract_structured_error_evidence,
};

use crate::{
    RelayAttemptFailure, RelayAttemptGate, RelayAttemptGateError, RelayAttemptReport, RelayError,
    RelayExecutionError,
    error::{classify_upstream_status, is_retryable, map_adaptor_error, parse_retry_after},
    state_machine::{is_credential_scoped, release_attempt_permit},
};

/// 一次视频任务提交或轮询所需的固定上游目标。
pub struct VideoTaskTarget {
    adaptor: Arc<dyn VideoTaskAdaptor>,
    context: RelayContext,
    credential: Credential,
    header_overrides: HeaderMap,
    channel_group: Option<ChannelId>,
    attempt_gate: Option<Arc<dyn RelayAttemptGate>>,
}

impl VideoTaskTarget {
    /// 使用已解密凭据和受控 HTTP 上下文创建任务目标。
    #[must_use]
    pub fn new(
        adaptor: Arc<dyn VideoTaskAdaptor>,
        context: RelayContext,
        credential: Credential,
    ) -> Self {
        Self {
            adaptor,
            context,
            credential,
            header_overrides: HeaderMap::new(),
            channel_group: None,
            attempt_gate: None,
        }
    }

    /// 附加已由渠道持久化边界校验的非认证请求头。
    #[must_use]
    pub fn with_header_overrides(mut self, header_overrides: HeaderMap) -> Self {
        self.header_overrides = header_overrides;
        self
    }

    /// 标记候选所属渠道，同一渠道的多条凭据必须连续排列。
    #[must_use]
    pub const fn with_channel_group(mut self, channel_id: ChannelId) -> Self {
        self.channel_group = Some(channel_id);
        self
    }

    /// 绑定发送前必须取得的账号级并发许可。
    #[must_use]
    pub fn with_attempt_gate(mut self, attempt_gate: Arc<dyn RelayAttemptGate>) -> Self {
        self.attempt_gate = Some(attempt_gate);
        self
    }
}

impl fmt::Debug for VideoTaskTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoTaskTarget")
            .field("context", &self.context)
            .field("credential", &self.credential)
            .field("header_override_count", &self.header_overrides.len())
            .field("has_channel_group", &self.channel_group.is_some())
            .field("has_attempt_gate", &self.attempt_gate.is_some())
            .finish()
    }
}

/// 带候选级模型映射的视频任务提交目标。
pub struct VideoTaskSubmissionCandidate {
    target: VideoTaskTarget,
    request: CanonicalVideoGenerationRequest,
}

impl VideoTaskSubmissionCandidate {
    /// 把固定目标与已经完成模型映射的 Canonical 请求绑定。
    #[must_use]
    pub const fn new(target: VideoTaskTarget, request: CanonicalVideoGenerationRequest) -> Self {
        Self { target, request }
    }
}

impl fmt::Debug for VideoTaskSubmissionCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoTaskSubmissionCandidate")
            .field("target", &self.target)
            .field("request", &"<已脱敏>")
            .finish()
    }
}

/// 视频任务提交成功及其候选尝试报告。
pub struct VideoTaskSubmissionOutcome {
    submission: TaskSubmission,
    report: RelayAttemptReport,
}

impl VideoTaskSubmissionOutcome {
    /// 消费结果并返回规范任务句柄和脱敏尝试报告。
    #[must_use]
    pub fn into_parts(self) -> (TaskSubmission, RelayAttemptReport) {
        (self.submission, self.report)
    }
}

impl fmt::Debug for VideoTaskSubmissionOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoTaskSubmissionOutcome")
            .field("submission", &self.submission)
            .field("report", &self.report)
            .finish()
    }
}

/// 视频任务提交失败时对上游是否可能已经接受请求的闭合判断。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoTaskSubmissionDisposition {
    /// 可以确定没有收到成功响应，持久化 claim 可安全释放。
    DefinitelyNotAccepted,
    /// 请求可能已到达上游或已经收到 2xx，禁止自动提交第二次。
    AcceptanceUnknown,
}

/// 同时保留候选报告和上游接受确定性的任务提交错误。
pub struct VideoTaskSubmissionError {
    error: RelayError,
    report: RelayAttemptReport,
    disposition: VideoTaskSubmissionDisposition,
}

impl VideoTaskSubmissionError {
    fn new(
        error: RelayError,
        failures: Vec<RelayAttemptFailure>,
        disposition: VideoTaskSubmissionDisposition,
    ) -> Self {
        Self {
            error,
            report: RelayAttemptReport::failed(failures),
            disposition,
        }
    }

    /// 返回脱敏转发错误分类。
    #[must_use]
    pub const fn error(&self) -> RelayError {
        self.error
    }

    /// 返回失败前已经产生的脱敏候选报告。
    #[must_use]
    pub const fn report(&self) -> &RelayAttemptReport {
        &self.report
    }

    /// 返回持久化协调器必须遵守的上游接受确定性。
    #[must_use]
    pub const fn disposition(&self) -> VideoTaskSubmissionDisposition {
        self.disposition
    }

    /// 消费错误并返回分类、报告和接受确定性。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        RelayError,
        RelayAttemptReport,
        VideoTaskSubmissionDisposition,
    ) {
        (self.error, self.report, self.disposition)
    }
}

impl fmt::Debug for VideoTaskSubmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoTaskSubmissionError")
            .field("error", &self.error)
            .field("report", &self.report)
            .field("disposition", &self.disposition)
            .finish()
    }
}

impl std::error::Error for VideoTaskSubmissionError {}

impl fmt::Display for VideoTaskSubmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("视频任务提交失败")
    }
}

/// 视频任务轮询成功及其单候选报告。
pub struct VideoTaskPollOutcome {
    poll: CanonicalTaskPoll,
    report: RelayAttemptReport,
}

impl VideoTaskPollOutcome {
    /// 消费结果并返回规范轮询状态和脱敏尝试报告。
    #[must_use]
    pub fn into_parts(self) -> (CanonicalTaskPoll, RelayAttemptReport) {
        (self.poll, self.report)
    }
}

impl fmt::Debug for VideoTaskPollOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoTaskPollOutcome")
            .field("poll", &self.poll)
            .field("report", &self.report)
            .finish()
    }
}

/// 在有界候选上提交一次视频任务；只有尚未收到 2xx 的可恢复故障允许切换候选。
pub async fn relay_video_task_submission(
    candidates: &[VideoTaskSubmissionCandidate],
) -> Result<VideoTaskSubmissionOutcome, VideoTaskSubmissionError> {
    validate_submission_candidates(candidates).map_err(|error| {
        VideoTaskSubmissionError::new(
            RelayError::Request(error),
            Vec::new(),
            VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
        )
    })?;
    let mut failures = Vec::new();
    let mut last_error = None;
    let mut index = 0;
    while let Some(candidate) = candidates.get(index) {
        let started_at = Instant::now();
        let request = match candidate.target.adaptor.build_video_submission_request(
            &candidate.request,
            &candidate.target.credential,
            &candidate.target.context,
        ) {
            Ok(request) => request,
            Err(error) => {
                last_error = Some(RelayError::Adaptor(error));
                index = next_channel_index(candidates, index);
                continue;
            }
        };
        let request = match request.with_header_overrides(candidate.target.header_overrides.clone())
        {
            Ok(request) => request,
            Err(error) => {
                last_error = Some(RelayError::Adaptor(error));
                index = next_channel_index(candidates, index);
                continue;
            }
        };
        let permit = match acquire_attempt_permit(&candidate.target).await {
            Ok(permit) => permit,
            Err(RelayError::ConcurrencyUnavailable) => {
                last_error = Some(RelayError::ConcurrencyUnavailable);
                index += 1;
                continue;
            }
            Err(error) => {
                return Err(VideoTaskSubmissionError::new(
                    error,
                    failures,
                    VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
                ));
            }
        };
        let response = match TransportDispatcher::http()
            .send(request, &candidate.target.context)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                release_attempt_permit(permit).await;
                let disposition = submission_error_disposition(error);
                let mapped = map_adaptor_error(error);
                failures.push(RelayAttemptFailure::new(
                    index,
                    mapped,
                    started_at.elapsed(),
                ));
                last_error = Some(RelayError::Upstream(mapped));
                if disposition == VideoTaskSubmissionDisposition::AcceptanceUnknown {
                    return Err(VideoTaskSubmissionError::new(
                        RelayError::Upstream(mapped),
                        failures,
                        disposition,
                    ));
                }
                if is_retryable(mapped) {
                    index = next_candidate_index(candidates, index, mapped);
                    continue;
                }
                return Err(VideoTaskSubmissionError::new(
                    RelayError::Upstream(mapped),
                    failures,
                    disposition,
                ));
            }
        };
        if response.status().is_success() {
            let normalized = candidate
                .target
                .adaptor
                .normalize_submission(response)
                .await;
            release_attempt_permit(permit).await;
            return match normalized {
                Ok(submission) => Ok(VideoTaskSubmissionOutcome {
                    submission,
                    report: RelayAttemptReport::succeeded(index, started_at.elapsed(), failures),
                }),
                Err(_) => {
                    // 上游已经确认接收，禁止因响应异常切换候选并重复创建付费任务。
                    let error = UpstreamError::ProtocolError;
                    failures.push(RelayAttemptFailure::new(index, error, started_at.elapsed()));
                    Err(VideoTaskSubmissionError::new(
                        RelayError::Upstream(error),
                        failures,
                        VideoTaskSubmissionDisposition::AcceptanceUnknown,
                    ))
                }
            };
        }
        let status = response.status();
        let retry_after = parse_retry_after(response.headers());
        let body = match response.into_body().into_bytes().await {
            Ok(body) => body,
            Err(error) => {
                release_attempt_permit(permit).await;
                let mapped = map_adaptor_error(error);
                failures.push(RelayAttemptFailure::new(
                    index,
                    mapped,
                    started_at.elapsed(),
                ));
                last_error = Some(RelayError::Upstream(mapped));
                if is_retryable(mapped) {
                    index = next_candidate_index(candidates, index, mapped);
                    continue;
                }
                return Err(VideoTaskSubmissionError::new(
                    RelayError::Upstream(mapped),
                    failures,
                    VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
                ));
            }
        };
        let classified = classify_upstream_status(status, retry_after, &body);
        let server_status = UpstreamServerStatus::new(status.as_u16());
        let evidence = extract_structured_error_evidence(&body);
        release_attempt_permit(permit).await;
        failures.push(RelayAttemptFailure::with_evidence(
            index,
            classified,
            server_status,
            evidence,
            started_at.elapsed(),
        ));
        last_error = Some(RelayError::Upstream(classified));
        if is_retryable(classified) {
            index = next_candidate_index(candidates, index, classified);
            continue;
        }
        return Err(VideoTaskSubmissionError::new(
            RelayError::Upstream(classified),
            failures,
            VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
        ));
    }
    Err(VideoTaskSubmissionError::new(
        last_error.unwrap_or(RelayError::Upstream(UpstreamError::ModelUnsupported)),
        failures,
        VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
    ))
}

fn submission_error_disposition(error: AdaptorError) -> VideoTaskSubmissionDisposition {
    match error {
        AdaptorError::Transport(
            AdaptorTransportError::ReadTimeout
            | AdaptorTransportError::RequestTimeout
            | AdaptorTransportError::Request
            | AdaptorTransportError::ResponseBody,
        ) => VideoTaskSubmissionDisposition::AcceptanceUnknown,
        _ => VideoTaskSubmissionDisposition::DefinitelyNotAccepted,
    }
}

/// 只在已经固化的单一目标上轮询视频任务，不执行任何跨渠道故障转移。
pub async fn relay_video_task_poll(
    target: &VideoTaskTarget,
    task_id: &UpstreamTaskId,
    expected_model: &VideoModel,
) -> Result<VideoTaskPollOutcome, RelayExecutionError> {
    let started_at = Instant::now();
    let request = target
        .adaptor
        .build_video_poll_request(task_id, &target.credential, &target.context)
        .and_then(|request| request.with_header_overrides(target.header_overrides.clone()))
        .map_err(|error| RelayExecutionError::new(RelayError::Adaptor(error), Vec::new()))?;
    let permit = acquire_attempt_permit(target)
        .await
        .map_err(|error| RelayExecutionError::new(error, Vec::new()))?;
    let response = match TransportDispatcher::http()
        .send(request, &target.context)
        .await
    {
        Ok(response) => response,
        Err(error) => {
            release_attempt_permit(permit).await;
            let mapped = map_adaptor_error(error);
            return Err(RelayExecutionError::new(
                RelayError::Upstream(mapped),
                vec![RelayAttemptFailure::new(0, mapped, started_at.elapsed())],
            ));
        }
    };
    if response.status().is_success() {
        let normalized = target.adaptor.normalize_poll(task_id, response).await;
        release_attempt_permit(permit).await;
        return normalized
            .and_then(|poll| {
                validate_poll_model(&poll, expected_model)?;
                Ok(poll)
            })
            .map(|poll| VideoTaskPollOutcome {
                poll,
                report: RelayAttemptReport::succeeded(0, started_at.elapsed(), Vec::new()),
            })
            .map_err(|_| {
                let error = UpstreamError::ProtocolError;
                RelayExecutionError::new(
                    RelayError::Upstream(error),
                    vec![RelayAttemptFailure::new(0, error, started_at.elapsed())],
                )
            });
    }
    let status = response.status();
    let retry_after = parse_retry_after(response.headers());
    let body = response.into_body().into_bytes().await.map_err(|error| {
        let mapped = map_adaptor_error(error);
        RelayExecutionError::new(
            RelayError::Upstream(mapped),
            vec![RelayAttemptFailure::new(0, mapped, started_at.elapsed())],
        )
    });
    release_attempt_permit(permit).await;
    let body = body?;
    let classified = classify_upstream_status(status, retry_after, &body);
    let failure = RelayAttemptFailure::with_evidence(
        0,
        classified,
        UpstreamServerStatus::new(status.as_u16()),
        structured_evidence(&body),
        started_at.elapsed(),
    );
    Err(RelayExecutionError::new(
        RelayError::Upstream(classified),
        vec![failure],
    ))
}

fn validate_poll_model(
    poll: &CanonicalTaskPoll,
    expected_model: &VideoModel,
) -> Result<(), AdaptorError> {
    let Some(output) = poll.output() else {
        return Ok(());
    };
    if output
        .as_video()
        .is_none_or(|video| video.model() != expected_model)
    {
        return Err(AdaptorError::InvalidTaskResponse);
    }
    Ok(())
}

async fn acquire_attempt_permit(
    target: &VideoTaskTarget,
) -> Result<Option<Box<dyn crate::RelayAttemptPermit>>, RelayError> {
    match target.attempt_gate.as_ref() {
        Some(gate) => match gate.acquire().await {
            Ok(permit) => Ok(Some(permit)),
            Err(RelayAttemptGateError::Limited) => Err(RelayError::ConcurrencyUnavailable),
            Err(RelayAttemptGateError::Internal) => Err(RelayError::AttemptGateFailed),
        },
        None => Ok(None),
    }
}

fn validate_submission_candidates(
    candidates: &[VideoTaskSubmissionCandidate],
) -> Result<(), AdaptorError> {
    if candidates.is_empty() {
        return Err(AdaptorError::InvalidTaskRequest);
    }
    if candidates.len() > crate::RelayStateMachine::MAX_CANDIDATES {
        return Err(AdaptorError::InvalidTaskRequest);
    }
    if !channel_groups_are_contiguous(candidates) {
        return Err(AdaptorError::InvalidTaskRequest);
    }
    Ok(())
}

fn channel_groups_are_contiguous(candidates: &[VideoTaskSubmissionCandidate]) -> bool {
    let mut closed = BTreeSet::new();
    let mut current = None;
    for candidate in candidates {
        if candidate.target.channel_group == current {
            continue;
        }
        if let Some(previous) = current {
            closed.insert(previous);
        }
        if candidate
            .target
            .channel_group
            .is_some_and(|channel_group| closed.contains(&channel_group))
        {
            return false;
        }
        current = candidate.target.channel_group;
    }
    true
}

fn next_candidate_index(
    candidates: &[VideoTaskSubmissionCandidate],
    index: usize,
    error: UpstreamError,
) -> usize {
    if is_credential_scoped(error) {
        index + 1
    } else {
        next_channel_index(candidates, index)
    }
}

fn next_channel_index(candidates: &[VideoTaskSubmissionCandidate], index: usize) -> usize {
    let Some(channel_group) = candidates[index].target.channel_group else {
        return index + 1;
    };
    let mut next = index + 1;
    while candidates
        .get(next)
        .is_some_and(|candidate| candidate.target.channel_group == Some(channel_group))
    {
        next += 1;
    }
    next
}

fn structured_evidence(body: &[u8]) -> Option<StructuredErrorEvidence> {
    extract_structured_error_evidence(body)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener, TcpStream},
        thread,
        time::{Duration, Instant},
    };

    use af_adapter::{
        Credential, HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy,
        XaiVideoAdaptor,
    };
    use af_protocol::{VideoDuration, VideoModel, VideoPrompt, VideoResolution};

    use super::*;

    #[tokio::test]
    async fn submission_retries_pre_acceptance_rate_limit_and_binds_successful_candidate() {
        let (address, server) = spawn_sequence(vec![
            (
                "429 Too Many Requests",
                br#"{"error":{"code":"rate_limit_exceeded","message":"private-canary"}}"#.to_vec(),
            ),
            ("200 OK", br#"{"request_id":"video-task-bound"}"#.to_vec()),
        ]);
        let context = proxy_context(address);
        let candidates = vec![
            submission_candidate(context.clone(), "private-key-a", 1, request()),
            submission_candidate(context, "private-key-b", 1, request()),
        ];

        let outcome = relay_video_task_submission(&candidates).await.unwrap();
        let (submission, report) = outcome.into_parts();
        assert_eq!(submission.task_id().as_str(), "video-task-bound");
        assert_eq!(report.successful_candidate_index(), Some(1));
        assert_eq!(report.failures().len(), 1);
        assert_eq!(report.failures()[0].candidate_index(), 0);
        assert!(!format!("{report:?}").contains("private-canary"));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn accepted_but_invalid_submission_never_creates_a_second_task() {
        let (first_address, first_server) = spawn_sequence(vec![("200 OK", b"{}".to_vec())]);
        let second_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let second_address = second_listener.local_addr().unwrap();
        second_listener.set_nonblocking(true).unwrap();
        let second_server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                match second_listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = read_request(&mut stream);
                        return true;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("监听第二候选失败: {error}"),
                }
            }
            false
        });
        let candidates = vec![
            submission_candidate(proxy_context(first_address), "private-key-a", 1, request()),
            submission_candidate(proxy_context(second_address), "private-key-b", 2, request()),
        ];

        let error = relay_video_task_submission(&candidates).await.unwrap_err();
        assert_eq!(
            error.error(),
            RelayError::Upstream(UpstreamError::ProtocolError)
        );
        assert_eq!(error.report().failures().len(), 1);
        assert_eq!(error.report().failures()[0].candidate_index(), 0);
        assert_eq!(
            error.disposition(),
            VideoTaskSubmissionDisposition::AcceptanceUnknown
        );
        first_server.join().unwrap();
        assert!(!second_server.join().unwrap());
    }

    #[tokio::test]
    async fn response_timeout_is_acceptance_unknown_and_never_tries_second_candidate() {
        let first_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let first_address = first_listener.local_addr().unwrap();
        let first_server = thread::spawn(move || {
            let (mut stream, _) = first_listener.accept().unwrap();
            let _ = read_request(&mut stream);
            thread::sleep(Duration::from_millis(400));
        });
        let second_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let second_address = second_listener.local_addr().unwrap();
        second_listener.set_nonblocking(true).unwrap();
        let second_server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                match second_listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = read_request(&mut stream);
                        return true;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("监听第二候选失败: {error}"),
                }
            }
            false
        });
        let timeouts = HttpTimeouts::new(
            Duration::from_secs(1),
            Duration::from_millis(100),
            Duration::from_millis(250),
        )
        .unwrap();
        let candidates = vec![
            submission_candidate(
                proxy_context_with_timeouts(first_address, timeouts),
                "private-key-a",
                1,
                request(),
            ),
            submission_candidate(
                proxy_context_with_timeouts(second_address, timeouts),
                "private-key-b",
                2,
                request(),
            ),
        ];

        let error = relay_video_task_submission(&candidates).await.unwrap_err();
        assert_eq!(
            error.disposition(),
            VideoTaskSubmissionDisposition::AcceptanceUnknown
        );
        assert_eq!(error.report().failures()[0].candidate_index(), 0);
        first_server.join().unwrap();
        assert!(!second_server.join().unwrap());
    }

    #[tokio::test]
    async fn polling_uses_only_the_bound_target_and_returns_terminal_video() {
        let (address, server) = spawn_sequence(vec![(
            "200 OK",
            br#"{"status":"done","video":{"url":"https://vidgen.x.ai/out.mp4","duration":8,"respect_moderation":true},"model":"grok-imagine-video-1.5"}"#
                .to_vec(),
        )]);
        let target = target(proxy_context(address), "private-key", 7);
        let task_id = UpstreamTaskId::new("bound/task").unwrap();

        let expected_model = VideoModel::new("grok-imagine-video-1.5").unwrap();
        let outcome = relay_video_task_poll(&target, &task_id, &expected_model)
            .await
            .unwrap();
        let (poll, report) = outcome.into_parts();
        assert_eq!(report.successful_candidate_index(), Some(0));
        assert_eq!(
            poll.output()
                .unwrap()
                .as_video()
                .unwrap()
                .duration()
                .seconds(),
            8
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn polling_rejects_a_terminal_result_from_another_model() {
        let (address, server) = spawn_sequence(vec![(
            "200 OK",
            br#"{"status":"done","video":{"url":"https://vidgen.x.ai/out.mp4","duration":8,"respect_moderation":true},"model":"private-wrong-model"}"#
                .to_vec(),
        )]);
        let target = target(proxy_context(address), "private-key", 7);
        let task_id = UpstreamTaskId::new("bound-task").unwrap();
        let expected_model = VideoModel::new("grok-imagine-video-1.5").unwrap();

        let error = relay_video_task_poll(&target, &task_id, &expected_model)
            .await
            .unwrap_err();

        assert_eq!(
            error.error(),
            RelayError::Upstream(UpstreamError::ProtocolError)
        );
        assert_eq!(error.report().failures()[0].candidate_index(), 0);
        assert!(!format!("{error:?}").contains("private-wrong-model"));
        server.join().unwrap();
    }

    fn submission_candidate(
        context: RelayContext,
        key: &str,
        channel_id: i64,
        request: CanonicalVideoGenerationRequest,
    ) -> VideoTaskSubmissionCandidate {
        VideoTaskSubmissionCandidate::new(target(context, key, channel_id), request)
    }

    fn target(context: RelayContext, key: &str, channel_id: i64) -> VideoTaskTarget {
        VideoTaskTarget::new(
            Arc::new(XaiVideoAdaptor::new()),
            context,
            Credential::api_key(key).unwrap(),
        )
        .with_channel_group(ChannelId::new(channel_id).unwrap())
    }

    fn request() -> CanonicalVideoGenerationRequest {
        CanonicalVideoGenerationRequest::new(
            VideoModel::new("grok-imagine-video-1.5").unwrap(),
            VideoPrompt::new("private prompt").unwrap(),
            Some(VideoDuration::new(8).unwrap()),
            None,
            Some(VideoResolution::P720),
        )
    }

    fn proxy_context(address: SocketAddr) -> RelayContext {
        proxy_context_with_timeouts(address, HttpTimeouts::default())
    }

    fn proxy_context_with_timeouts(address: SocketAddr, timeouts: HttpTimeouts) -> RelayContext {
        RelayContext::new(
            HttpClientPool::default()
                .get(
                    &HttpClientConfig::new(
                        ProxyConfig::parse(format!("http://{address}")).unwrap(),
                        timeouts,
                    )
                    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy),
                )
                .unwrap(),
        )
        .with_base_url("http://upstream.example")
        .unwrap()
    }

    fn spawn_sequence(
        responses: Vec<(&'static str, Vec<u8>)>,
    ) -> (SocketAddr, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let request = read_request(&mut stream);
                assert!(request.contains(" http://upstream.example/v1/videos/"));
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    String::from_utf8(body).unwrap()
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });
        (address, server)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 512];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            assert!(read > 0);
            request.extend_from_slice(&buffer[..read]);
            if request.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
            assert!(request.len() < 16 * 1_024);
        }
        String::from_utf8(request).unwrap()
    }
}
