use std::fmt;

use af_domain::{CredentialKind, TaskSubmission, UpstreamTaskId};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue, Method};
use af_protocol::{CanonicalTaskPoll, CanonicalVideoGenerationRequest, xai_video};
use async_trait::async_trait;

use crate::credential::clear_authentication_headers;
use crate::{
    AdaptorError, AdaptorResult, Credential, MAX_UPSTREAM_REQUEST_TARGET_BYTES, RelayContext,
    TaskAdaptor, TaskAdaptorSendExt, UpstreamRequest, UpstreamResponse, VideoTaskAdaptor,
};

/// xAI Grok Imagine Video 异步任务适配器。
///
/// 首切片只负责官方文本生成视频提交与按任务标识轮询。请求正文与响应语义全部交给
/// `af-protocol::xai_video`，适配器仅持有 URL、Bearer API Key 和受控传输边界。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct XaiVideoAdaptor;

impl XaiVideoAdaptor {
    /// xAI 官方 API 默认根地址。
    pub const DEFAULT_BASE_URL: &'static str = "https://api.x.ai";

    /// 创建无状态的 xAI 视频任务适配器。
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// 构造一次官方文本生成视频提交请求。
    pub fn build_submission_request(
        &self,
        request: &CanonicalVideoGenerationRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        let body = serde_json::to_vec(
            &xai_video::build_request(request).map_err(|_| AdaptorError::InvalidTaskRequest)?,
        )
        .map_err(|_| AdaptorError::InvalidTaskRequest)?;
        let target = context.append_path(Self::DEFAULT_BASE_URL, self.submission_path(context)?)?;
        UpstreamRequest::new(
            Method::POST,
            target,
            self.headers(credential, true)?,
            Some(body.into()),
        )
        .and_then(|request| request.with_response_body_limit(xai_video::MAX_BODY_BYTES))
    }

    /// 构造与给定任务标识严格绑定的官方轮询请求。
    pub fn build_poll_request(
        &self,
        task_id: &UpstreamTaskId,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        let target = self.poll_target(task_id, context)?;
        UpstreamRequest::new(Method::GET, target, self.headers(credential, false)?, None)
            .and_then(|request| request.with_response_body_limit(xai_video::MAX_BODY_BYTES))
    }

    /// 通过统一 dispatcher 提交任务并归一化官方句柄。
    pub async fn submit_video(
        &self,
        request: &CanonicalVideoGenerationRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<TaskSubmission> {
        let request = self.build_submission_request(request, credential, context)?;
        TaskAdaptorSendExt::submit(self, request, context).await
    }

    /// 通过统一 dispatcher 轮询与任务标识绑定的官方状态。
    pub async fn poll_video(
        &self,
        task_id: &UpstreamTaskId,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<CanonicalTaskPoll> {
        let request = self.build_poll_request(task_id, credential, context)?;
        TaskAdaptorSendExt::poll(self, task_id, request, context).await
    }

    fn submission_path(&self, context: &RelayContext) -> AdaptorResult<&'static str> {
        Ok(if self.has_v1_suffix(context)? {
            "videos/generations"
        } else {
            "v1/videos/generations"
        })
    }

    fn poll_target(
        &self,
        task_id: &UpstreamTaskId,
        context: &RelayContext,
    ) -> AdaptorResult<String> {
        let mut target = context.resolve_base_url(Self::DEFAULT_BASE_URL)?;
        let has_v1_suffix = target
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == "v1");
        {
            // 任务标识作为单一路径段编码，避免斜杠或保留字符注入轮询路径。
            let mut segments = target
                .path_segments_mut()
                .map_err(|_| AdaptorError::InvalidBaseUrl)?;
            segments.pop_if_empty();
            if !has_v1_suffix {
                segments.push("v1");
            }
            segments.push("videos");
            segments.push(task_id.as_str());
        }
        let target = String::from(target);
        if target.len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES {
            return Err(AdaptorError::InvalidRequestTarget);
        }
        Ok(target)
    }

    fn has_v1_suffix(&self, context: &RelayContext) -> AdaptorResult<bool> {
        let base_url = context.resolve_base_url(Self::DEFAULT_BASE_URL)?;
        Ok(base_url
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .is_some_and(|segment| segment == "v1"))
    }

    fn headers(&self, credential: &Credential, has_body: bool) -> AdaptorResult<HeaderMap> {
        if credential.kind() != CredentialKind::ApiKey {
            return Err(AdaptorError::UnsupportedCredential {
                kind: credential.kind(),
            });
        }
        let mut headers = HeaderMap::new();
        clear_authentication_headers(&mut headers);
        let mut authorization =
            HeaderValue::from_str(&format!("Bearer {}", credential.expose_secret()))
                .map_err(|_| AdaptorError::InvalidHeader)?;
        authorization.set_sensitive(true);
        headers.insert(HeaderName::from_static("authorization"), authorization);
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        if has_body {
            headers.insert(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/json"),
            );
        }
        Ok(headers)
    }
}

impl Default for XaiVideoAdaptor {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoTaskAdaptor for XaiVideoAdaptor {
    fn build_video_submission_request(
        &self,
        request: &CanonicalVideoGenerationRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        self.build_submission_request(request, credential, context)
    }

    fn build_video_poll_request(
        &self,
        task_id: &UpstreamTaskId,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest> {
        self.build_poll_request(task_id, credential, context)
    }
}

#[async_trait]
impl TaskAdaptor for XaiVideoAdaptor {
    fn validate_poll_request(
        &self,
        task_id: &UpstreamTaskId,
        request: &UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<()> {
        let expected = self.poll_target(task_id, context)?;
        if request.method() != Method::GET
            || request.target() != expected.as_str()
            || request.body().is_some()
        {
            return Err(AdaptorError::InvalidTaskRequest);
        }
        Ok(())
    }

    async fn normalize_submission(
        &self,
        response: UpstreamResponse,
    ) -> AdaptorResult<TaskSubmission> {
        if response.status() != af_httpclient::StatusCode::OK {
            return Err(AdaptorError::InvalidTaskResponse);
        }
        let body = response
            .into_body()
            .into_bytes_with_limit(xai_video::MAX_BODY_BYTES)
            .await?;
        xai_video::parse_submission(&body).map_err(|_| AdaptorError::InvalidTaskResponse)
    }

    async fn normalize_poll(
        &self,
        _task_id: &UpstreamTaskId,
        response: UpstreamResponse,
    ) -> AdaptorResult<CanonicalTaskPoll> {
        if response.status() != af_httpclient::StatusCode::OK {
            return Err(AdaptorError::InvalidTaskResponse);
        }
        let body = response
            .into_body()
            .into_bytes_with_limit(xai_video::MAX_BODY_BYTES)
            .await?;
        xai_video::parse_poll(&body).map_err(|_| AdaptorError::InvalidTaskResponse)
    }
}

impl fmt::Debug for XaiVideoAdaptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("XaiVideoAdaptor")
    }
}

#[cfg(test)]
mod tests {
    use af_httpclient::{Bytes, HttpClientConfig, HttpClientPool, StatusCode};
    use af_protocol::{VideoDuration, VideoModel, VideoPrompt, VideoResolution};

    use super::*;

    fn context() -> RelayContext {
        RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        )
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

    #[test]
    fn builds_official_submission_and_poll_requests_with_bearer_auth() {
        let adaptor = XaiVideoAdaptor::new();
        let credential = Credential::api_key("private-xai-key").unwrap();
        let submission = adaptor
            .build_submission_request(&request(), &credential, &context())
            .unwrap();
        assert_eq!(submission.method(), Method::POST);
        assert_eq!(
            submission.target(),
            "https://api.x.ai/v1/videos/generations"
        );
        assert_eq!(
            submission.headers()["authorization"],
            "Bearer private-xai-key"
        );
        assert_eq!(submission.headers()["content-type"], "application/json");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(submission.body().unwrap()).unwrap(),
            serde_json::json!({
                "model": "grok-imagine-video-1.5",
                "prompt": "private prompt",
                "duration": 8,
                "resolution": "720p"
            })
        );

        let task_id = UpstreamTaskId::new("task/with-path").unwrap();
        let poll = adaptor
            .build_poll_request(&task_id, &credential, &context())
            .unwrap();
        assert_eq!(poll.method(), Method::GET);
        assert_eq!(poll.target(), "https://api.x.ai/v1/videos/task%2Fwith-path");
        assert!(poll.body().is_none());
        assert!(poll.headers().get("content-type").is_none());

        let debug = format!("{submission:?}\n{poll:?}\n{adaptor:?}");
        for secret in [
            "private-xai-key",
            "private prompt",
            "grok-imagine-video",
            "task/with-path",
            "api.x.ai",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn supports_v1_proxy_prefix_and_rejects_oauth() {
        let adaptor = XaiVideoAdaptor::new();
        let proxied = context()
            .with_base_url("https://gateway.example/xai/v1/")
            .unwrap();
        let built = adaptor
            .build_submission_request(
                &request(),
                &Credential::api_key("private-key").unwrap(),
                &proxied,
            )
            .unwrap();
        assert_eq!(
            built.target(),
            "https://gateway.example/xai/v1/videos/generations"
        );
        assert_eq!(
            adaptor
                .build_submission_request(
                    &request(),
                    &Credential::oauth("private-oauth").unwrap(),
                    &context(),
                )
                .unwrap_err(),
            AdaptorError::UnsupportedCredential {
                kind: CredentialKind::Oauth
            }
        );
    }

    #[test]
    fn poll_validation_rejects_a_request_for_another_task() {
        let adaptor = XaiVideoAdaptor::new();
        let expected_id = UpstreamTaskId::new("expected-task").unwrap();
        let wrong_request = adaptor
            .build_poll_request(
                &UpstreamTaskId::new("other-task").unwrap(),
                &Credential::api_key("private-key").unwrap(),
                &context(),
            )
            .unwrap();
        assert_eq!(
            adaptor
                .validate_poll_request(&expected_id, &wrong_request, &context())
                .unwrap_err(),
            AdaptorError::InvalidTaskRequest
        );
    }

    #[tokio::test]
    async fn normalizes_official_submission_and_terminal_poll() {
        let adaptor = XaiVideoAdaptor::new();
        let submission = adaptor
            .normalize_submission(
                UpstreamResponse::full(
                    StatusCode::OK,
                    HeaderMap::new(),
                    Bytes::from_static(br#"{"request_id":"video-task-1"}"#),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let poll = adaptor
            .normalize_poll(
                submission.task_id(),
                UpstreamResponse::full(
                    StatusCode::OK,
                    HeaderMap::new(),
                    Bytes::from_static(
                        br#"{"status":"done","video":{"url":"https://vidgen.x.ai/out.mp4","duration":8,"respect_moderation":true},"model":"grok-imagine-video-1.5"}"#,
                    ),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            poll.output()
                .unwrap()
                .as_video()
                .unwrap()
                .duration()
                .seconds(),
            8
        );
    }
}
