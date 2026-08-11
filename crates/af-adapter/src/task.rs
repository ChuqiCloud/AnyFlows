use af_domain::{TaskSubmission, UpstreamTaskId};
use af_protocol::{CanonicalTaskPoll, CanonicalVideoGenerationRequest};
use async_trait::async_trait;

use crate::{
    AdaptorError, AdaptorResult, Credential, RelayContext, ResponseMode, TransportDispatcher,
    UpstreamRequest, UpstreamResponse,
};

/// 异步任务适配器的响应归一化契约。
///
/// 具体供应商只负责把已经受控收集的提交/轮询响应映射为领域状态；该 trait 不接收
/// `RelayContext`，因此不能从归一化钩子绕过统一代理、超时、响应预算或错误边界。
#[async_trait]
pub trait TaskAdaptor: Send + Sync {
    /// 在发起网络请求前校验轮询目标与任务标识的关联。
    ///
    /// 无法由响应回显任务标识的供应商必须覆盖该方法，避免调用方把任务 A 的标识与
    /// 任务 B 的轮询 URL 组合后发送。默认实现只供尚无专用 URL 规则的通用适配器使用。
    fn validate_poll_request(
        &self,
        _task_id: &UpstreamTaskId,
        _request: &UpstreamRequest,
        _context: &RelayContext,
    ) -> AdaptorResult<()> {
        Ok(())
    }

    /// 将提交响应归一化为脱敏任务标识和初始状态。
    async fn normalize_submission(
        &self,
        response: UpstreamResponse,
    ) -> AdaptorResult<TaskSubmission>;

    /// 将轮询响应归一化为闭合状态和可选成功输出。
    async fn normalize_poll(
        &self,
        task_id: &UpstreamTaskId,
        response: UpstreamResponse,
    ) -> AdaptorResult<CanonicalTaskPoll>;
}

/// 视频任务适配器的请求构造契约。
///
/// 供应商实现只负责把已校验 Canonical 请求和绑定任务标识编码为官方请求；发送、
/// 故障转移和提交后目标固化仍由上层任务状态机统一负责。
pub trait VideoTaskAdaptor: TaskAdaptor {
    /// 构造一次视频任务提交请求。
    fn build_video_submission_request(
        &self,
        request: &CanonicalVideoGenerationRequest,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest>;

    /// 构造与给定上游任务标识绑定的轮询请求。
    fn build_video_poll_request(
        &self,
        task_id: &UpstreamTaskId,
        credential: &Credential,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamRequest>;
}

/// 任务提交与轮询的统一受控发送扩展。
///
/// 该扩展没有给实现方留下可覆盖的发送方法：所有请求均经 HTTP dispatcher，且任务
/// 响应必须使用完整收集模式，避免轮询状态被流式缓冲或取消语义截断。
#[async_trait]
pub trait TaskAdaptorSendExt: TaskAdaptor {
    /// 发送一次任务提交请求并归一化响应。
    async fn submit(
        &self,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<TaskSubmission>;

    /// 发送一次任务轮询请求并归一化响应。
    async fn poll(
        &self,
        task_id: &UpstreamTaskId,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<CanonicalTaskPoll>;
}

#[async_trait]
impl<T> TaskAdaptorSendExt for T
where
    T: TaskAdaptor + ?Sized,
{
    async fn submit(
        &self,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<TaskSubmission> {
        ensure_full_response(&request)?;
        let response = TransportDispatcher::http().send(request, context).await?;
        self.normalize_submission(response).await
    }

    async fn poll(
        &self,
        task_id: &UpstreamTaskId,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<CanonicalTaskPoll> {
        self.validate_poll_request(task_id, &request, context)?;
        ensure_full_response(&request)?;
        let response = TransportDispatcher::http().send(request, context).await?;
        self.normalize_poll(task_id, response).await
    }
}

fn ensure_full_response(request: &UpstreamRequest) -> AdaptorResult<()> {
    if request.response_mode() == ResponseMode::Full {
        Ok(())
    } else {
        Err(AdaptorError::UnsupportedResponseMode)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::{SocketAddr, TcpListener, TcpStream},
        thread,
        time::Duration,
    };

    use af_domain::{TaskProgress, TaskState, TaskStatus, TaskSubmission, UpstreamTaskId};
    use af_httpclient::{
        Bytes, HeaderMap, HttpClientConfig, HttpClientPool, Method, ProxyConfig, RemoteDnsPolicy,
    };
    use af_protocol::CanonicalTaskPoll;
    use async_trait::async_trait;

    use super::*;

    struct TestAdaptor;

    #[async_trait]
    impl TaskAdaptor for TestAdaptor {
        async fn normalize_submission(
            &self,
            response: UpstreamResponse,
        ) -> AdaptorResult<TaskSubmission> {
            assert_eq!(response.status(), af_httpclient::StatusCode::OK);
            assert_eq!(
                response.into_body().into_bytes().await?,
                Bytes::from_static(br#"{"task":"accepted"}"#)
            );
            Ok(TaskSubmission::new(
                UpstreamTaskId::new("task-test-1").unwrap(),
                TaskStatus::Queued {
                    progress: TaskProgress::ZERO,
                },
            ))
        }

        async fn normalize_poll(
            &self,
            task_id: &UpstreamTaskId,
            response: UpstreamResponse,
        ) -> AdaptorResult<CanonicalTaskPoll> {
            assert_eq!(task_id.as_str(), "task-test-1");
            assert_eq!(response.status(), af_httpclient::StatusCode::OK);
            assert_eq!(
                response.into_body().into_bytes().await?,
                Bytes::from_static(br#"{"status":"running"}"#)
            );
            CanonicalTaskPoll::new(
                TaskStatus::Running {
                    progress: TaskProgress::new(2_500).unwrap(),
                },
                None,
            )
            .map_err(|_| AdaptorError::InvalidTaskResponse)
        }
    }

    #[tokio::test]
    async fn task_send_extension_uses_managed_transport_for_submit_and_poll() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for index in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let request = read_request(&mut stream);
                assert!(request.starts_with("POST http://upstream.example/task HTTP/1.1"));
                let body = if index == 0 {
                    br#"{"task":"accepted"}"#.as_slice()
                } else {
                    br#"{"status":"running"}"#.as_slice()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    String::from_utf8(body.to_vec()).unwrap()
                );
                stream.write_all(response.as_bytes()).unwrap();
                stream.flush().unwrap();
            }
        });

        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&loopback_proxy_config(address))
                .unwrap(),
        );
        let request = || {
            UpstreamRequest::new(
                Method::POST,
                "http://upstream.example/task",
                HeaderMap::new(),
                None,
            )
            .unwrap()
        };
        let adaptor = TestAdaptor;
        let submission = adaptor.submit(request(), &context).await.unwrap();
        assert_eq!(submission.status().state(), TaskState::Queued);
        let task_id = submission.task_id().clone();
        let status = adaptor.poll(&task_id, request(), &context).await.unwrap();
        assert_eq!(status.status().state(), TaskState::Running);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn task_send_extension_rejects_streaming_requests_before_network_io() {
        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&HttpClientConfig::default())
                .unwrap(),
        );
        let request = UpstreamRequest::new(
            Method::GET,
            "https://upstream.example/task",
            HeaderMap::new(),
            None,
        )
        .unwrap()
        .with_response_mode(ResponseMode::Stream);
        assert_eq!(
            TestAdaptor.submit(request, &context).await.unwrap_err(),
            AdaptorError::UnsupportedResponseMode
        );
    }

    fn loopback_proxy_config(address: SocketAddr) -> HttpClientConfig {
        HttpClientConfig::new(
            ProxyConfig::parse(format!("http://{address}")).unwrap(),
            af_httpclient::HttpTimeouts::default(),
        )
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy)
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
