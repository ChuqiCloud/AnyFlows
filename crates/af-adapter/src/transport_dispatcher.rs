use std::fmt;

use af_httpclient::Body;
use futures_util::{StreamExt as _, stream};

use crate::{
    AdaptorResult, RelayContext, ResponseMode, ResponsesWebSocketPool, ResponsesWebSocketPoolError,
    ResponsesWebSocketPoolKey, UpstreamBody, UpstreamRequest, UpstreamResponse,
};

/// Relay 显式选择的受控上游传输。
///
/// 默认值始终使用 HTTP。Responses WebSocket 只有在上层已经验证渠道能力、凭据版本
/// 和稳定下游会话后才可装配；本类型不会从请求正文或供应商名称推断能力。
#[derive(Clone, Default)]
pub struct TransportDispatcher {
    responses_websocket: Option<ResponsesWebSocketDispatch>,
}

#[derive(Clone)]
struct ResponsesWebSocketDispatch {
    pool: ResponsesWebSocketPool,
    key: ResponsesWebSocketPoolKey,
}

/// WebSocket 在任何下游事件可见前转回 HTTP 的固定脱敏原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportFallbackKind {
    /// 会话池键不满足安全边界。
    InvalidSession,
    /// WebSocket 握手目标无效。
    InvalidHandshakeTarget,
    /// WebSocket 握手 Header 无效。
    InvalidHandshakeHeader,
    /// 请求无法构造成 Responses WebSocket 事件。
    InvalidRequest,
    /// 连接池没有可用会话容量。
    PoolSaturated,
    /// 等待同一会话在途请求超时。
    QueueTimeout,
    /// 建立 WebSocket 连接超时。
    ConnectTimeout,
    /// 单轮 Responses 请求超时。
    TurnTimeout,
    /// 上游事件违反 Responses 协议。
    Protocol,
    /// 上游在终态前关闭连接。
    ConnectionClosed,
    /// 连接池内部状态不可用。
    PoolUnavailable,
    /// 受控 WebSocket 传输失败。
    Transport,
    /// 首个流事件返回传输错误。
    FirstEventError,
    /// 首个流事件前连接结束。
    EarlyEof,
    /// 连接池返回了与请求不一致的响应模式。
    UnexpectedResponseMode,
}

impl TransportFallbackKind {
    /// 返回日志与指标使用的稳定低基数字面量。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSession => "invalid_session",
            Self::InvalidHandshakeTarget => "invalid_handshake_target",
            Self::InvalidHandshakeHeader => "invalid_handshake_header",
            Self::InvalidRequest => "invalid_request",
            Self::PoolSaturated => "pool_saturated",
            Self::QueueTimeout => "queue_timeout",
            Self::ConnectTimeout => "connect_timeout",
            Self::TurnTimeout => "turn_timeout",
            Self::Protocol => "protocol",
            Self::ConnectionClosed => "connection_closed",
            Self::PoolUnavailable => "pool_unavailable",
            Self::Transport => "transport",
            Self::FirstEventError => "first_event_error",
            Self::EarlyEof => "early_eof",
            Self::UnexpectedResponseMode => "unexpected_response_mode",
        }
    }
}

/// 一次传输分派的响应及可选内部 HTTP 回退事实。
pub struct TransportDispatchOutcome {
    response: UpstreamResponse,
    fallback: Option<TransportFallbackKind>,
}

impl TransportDispatchOutcome {
    fn direct(response: UpstreamResponse) -> Self {
        Self {
            response,
            fallback: None,
        }
    }

    fn fallback(response: UpstreamResponse, kind: TransportFallbackKind) -> Self {
        Self {
            response,
            fallback: Some(kind),
        }
    }

    /// 返回本次分派是否在首事件前转回 HTTP。
    #[must_use]
    pub const fn fallback_kind(&self) -> Option<TransportFallbackKind> {
        self.fallback
    }

    /// 消费结果并返回传输无关上游响应。
    #[must_use]
    pub fn into_response(self) -> UpstreamResponse {
        self.response
    }

    /// 消费结果并返回响应与固定回退分类。
    #[must_use]
    pub fn into_parts(self) -> (UpstreamResponse, Option<TransportFallbackKind>) {
        (self.response, self.fallback)
    }
}

impl TransportDispatcher {
    /// 创建固定使用受控 HTTP Client 的默认分派器。
    #[must_use]
    pub const fn http() -> Self {
        Self {
            responses_websocket: None,
        }
    }

    /// 创建已经绑定连接池与脱敏会话键的 Responses WebSocket 分派器。
    #[must_use]
    pub const fn responses_websocket(
        pool: ResponsesWebSocketPool,
        key: ResponsesWebSocketPoolKey,
    ) -> Self {
        Self {
            responses_websocket: Some(ResponsesWebSocketDispatch { pool, key }),
        }
    }

    /// 执行一次已最终化请求；WebSocket 首事件前失败时最多回退一次 HTTP。
    pub async fn send(
        &self,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<UpstreamResponse> {
        self.send_with_report(request, context)
            .await
            .map(TransportDispatchOutcome::into_response)
    }

    /// 执行一次最终化请求，并返回首事件前 HTTP 回退的固定脱敏分类。
    pub async fn send_with_report(
        &self,
        request: UpstreamRequest,
        context: &RelayContext,
    ) -> AdaptorResult<TransportDispatchOutcome> {
        let Some(dispatch) = &self.responses_websocket else {
            return send_http(request, context)
                .await
                .map(TransportDispatchOutcome::direct);
        };

        let response = match dispatch.pool.execute(dispatch.key, &request).await {
            Ok(response) => response,
            Err(error) => {
                let kind = pool_error_kind(error);
                return send_http(request, context)
                    .await
                    .map(|response| TransportDispatchOutcome::fallback(response, kind));
            }
        };
        if request.response_mode() != ResponseMode::Stream {
            return Ok(TransportDispatchOutcome::direct(response));
        }

        let status = response.status();
        let headers = response.headers().clone();
        match response.into_body() {
            UpstreamBody::Stream(mut body) => match body.next_chunk().await {
                Some(Ok(first)) => {
                    // 取得首个事件后请求已经可能被下游观察，禁止再通过 HTTP 重放。
                    let body = stream::once(async move { Ok(first) }).chain(body);
                    UpstreamResponse::stream(status, headers, body)
                        .map(TransportDispatchOutcome::direct)
                }
                Some(Err(_)) => send_http(request, context).await.map(|response| {
                    TransportDispatchOutcome::fallback(
                        response,
                        TransportFallbackKind::FirstEventError,
                    )
                }),
                None => send_http(request, context).await.map(|response| {
                    TransportDispatchOutcome::fallback(response, TransportFallbackKind::EarlyEof)
                }),
            },
            UpstreamBody::Full(_) => send_http(request, context).await.map(|response| {
                TransportDispatchOutcome::fallback(
                    response,
                    TransportFallbackKind::UnexpectedResponseMode,
                )
            }),
        }
    }
}

impl fmt::Debug for TransportDispatcher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TransportDispatcher")
            .field(
                "transport",
                &if self.responses_websocket.is_some() {
                    "responses_websocket"
                } else {
                    "http"
                },
            )
            .finish()
    }
}

async fn send_http(
    request: UpstreamRequest,
    context: &RelayContext,
) -> AdaptorResult<UpstreamResponse> {
    let (method, target, headers, body, response_mode, response_body_limit) = request.into_parts();
    let response = context
        .http_client()
        .execute(method, &target, headers, body.map(Body::from))
        .await?;
    match response_mode {
        ResponseMode::Full => UpstreamResponse::from_http_full(response, response_body_limit).await,
        ResponseMode::Stream => UpstreamResponse::from_http_stream(response, response_body_limit),
    }
}

const fn pool_error_kind(error: ResponsesWebSocketPoolError) -> TransportFallbackKind {
    match error {
        ResponsesWebSocketPoolError::InvalidSession => TransportFallbackKind::InvalidSession,
        ResponsesWebSocketPoolError::InvalidHandshakeTarget => {
            TransportFallbackKind::InvalidHandshakeTarget
        }
        ResponsesWebSocketPoolError::InvalidHandshakeHeader => {
            TransportFallbackKind::InvalidHandshakeHeader
        }
        ResponsesWebSocketPoolError::InvalidRequest => TransportFallbackKind::InvalidRequest,
        ResponsesWebSocketPoolError::PoolSaturated => TransportFallbackKind::PoolSaturated,
        ResponsesWebSocketPoolError::QueueTimeout => TransportFallbackKind::QueueTimeout,
        ResponsesWebSocketPoolError::ConnectTimeout => TransportFallbackKind::ConnectTimeout,
        ResponsesWebSocketPoolError::TurnTimeout => TransportFallbackKind::TurnTimeout,
        ResponsesWebSocketPoolError::Protocol => TransportFallbackKind::Protocol,
        ResponsesWebSocketPoolError::ConnectionClosed => TransportFallbackKind::ConnectionClosed,
        ResponsesWebSocketPoolError::PoolUnavailable => TransportFallbackKind::PoolUnavailable,
        ResponsesWebSocketPoolError::Transport(_) => TransportFallbackKind::Transport,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    use af_domain::{ChannelId, CredentialId};
    use af_httpclient::{
        HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use tokio::{
        io::{AsyncReadExt as _, AsyncWriteExt as _},
        net::TcpListener,
    };

    use super::*;

    use crate::{
        Bytes, HeaderMap, HeaderName, HeaderValue, Method, ResponsesWebSocketConnection,
        ResponsesWebSocketConnector, ResponsesWebSocketFrame, ResponsesWebSocketHandshake,
        ResponsesWebSocketPoolConfig, ResponsesWebSocketTransportError,
    };

    #[derive(Clone)]
    struct ScriptedConnector {
        frames: Arc<Mutex<VecDeque<ResponsesWebSocketFrame>>>,
    }

    struct ScriptedConnection {
        frames: Arc<Mutex<VecDeque<ResponsesWebSocketFrame>>>,
    }

    #[async_trait]
    impl ResponsesWebSocketConnector for ScriptedConnector {
        async fn connect(
            &self,
            _handshake: ResponsesWebSocketHandshake,
        ) -> Result<Box<dyn ResponsesWebSocketConnection>, ResponsesWebSocketTransportError>
        {
            Ok(Box::new(ScriptedConnection {
                frames: Arc::clone(&self.frames),
            }))
        }
    }

    #[async_trait]
    impl ResponsesWebSocketConnection for ScriptedConnection {
        async fn send_text(
            &mut self,
            _payload: Bytes,
        ) -> Result<(), ResponsesWebSocketTransportError> {
            Ok(())
        }

        async fn receive(
            &mut self,
        ) -> Result<Option<ResponsesWebSocketFrame>, ResponsesWebSocketTransportError> {
            Ok(self.frames.lock().unwrap().pop_front())
        }

        async fn close(&mut self) {}
    }

    #[tokio::test]
    async fn stream_early_eof_falls_back_to_http_once() {
        let (context, server) = fallback_http_context().await;
        let dispatcher = websocket_dispatcher(VecDeque::new());
        let outcome = dispatcher
            .send_with_report(request(ResponseMode::Stream), &context)
            .await
            .unwrap();
        assert_eq!(
            outcome.fallback_kind(),
            Some(TransportFallbackKind::FirstEventError)
        );
        assert_eq!(
            outcome
                .into_response()
                .into_body()
                .into_bytes()
                .await
                .unwrap(),
            Bytes::from_static(b"fallback-ok")
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn first_websocket_event_permanently_disables_http_replay() {
        let unavailable_proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = unavailable_proxy.local_addr().unwrap();
        drop(unavailable_proxy);
        let context = RelayContext::new(
            HttpClientPool::default()
                .get(&loopback_proxy_config(address))
                .unwrap(),
        );
        let event = ResponsesWebSocketFrame::Text(Bytes::from(
            serde_json::to_vec(&json!({
                "type": "response.created",
                "response": {"id": "resp_1"}
            }))
            .unwrap(),
        ));
        let dispatcher = websocket_dispatcher(VecDeque::from([event]));
        let outcome = dispatcher
            .send_with_report(request(ResponseMode::Stream), &context)
            .await
            .unwrap();
        assert_eq!(outcome.fallback_kind(), None);
        assert!(
            outcome
                .into_response()
                .into_body()
                .into_bytes()
                .await
                .is_err()
        );
    }

    fn websocket_dispatcher(frames: VecDeque<ResponsesWebSocketFrame>) -> TransportDispatcher {
        let connector = ScriptedConnector {
            frames: Arc::new(Mutex::new(frames)),
        };
        let pool = ResponsesWebSocketPool::new(
            ResponsesWebSocketPoolConfig::default(),
            Arc::new(connector),
        );
        let key = ResponsesWebSocketPoolKey::new(
            ChannelId::new(1).unwrap(),
            CredentialId::new(1).unwrap(),
            1,
            "session-scope",
        )
        .unwrap();
        TransportDispatcher::responses_websocket(pool, key)
    }

    fn request(mode: ResponseMode) -> UpstreamRequest {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer secret"),
        );
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/json"),
        );
        UpstreamRequest::new(
            Method::POST,
            "http://upstream.example/v1/responses",
            headers,
            Some(Bytes::from_static(
                br#"{"model":"gpt-test","stream":true,"store":false}"#,
            )),
        )
        .unwrap()
        .with_response_mode(mode)
    }

    async fn fallback_http_context() -> (RelayContext, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 4_096];
            let read = socket.read(&mut request).await.unwrap();
            assert!(read > 0);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 11\r\nConnection: close\r\n\r\nfallback-ok",
                )
                .await
                .unwrap();
        });
        let client = HttpClientPool::default()
            .get(&loopback_proxy_config(address))
            .unwrap();
        (RelayContext::new(client), server)
    }

    fn loopback_proxy_config(address: std::net::SocketAddr) -> HttpClientConfig {
        HttpClientConfig::new(
            ProxyConfig::parse(format!("http://{address}")).unwrap(),
            HttpTimeouts::default(),
        )
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy)
    }
}
