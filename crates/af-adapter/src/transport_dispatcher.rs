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
        let Some(dispatch) = &self.responses_websocket else {
            return send_http(request, context).await;
        };

        let response = match dispatch.pool.execute(dispatch.key, &request).await {
            Ok(response) => response,
            Err(error) => {
                record_http_fallback(pool_error_kind(error));
                return send_http(request, context).await;
            }
        };
        if request.response_mode() != ResponseMode::Stream {
            return Ok(response);
        }

        let status = response.status();
        let headers = response.headers().clone();
        match response.into_body() {
            UpstreamBody::Stream(mut body) => match body.next_chunk().await {
                Some(Ok(first)) => {
                    // 取得首个事件后请求已经可能被下游观察，禁止再通过 HTTP 重放。
                    let body = stream::once(async move { Ok(first) }).chain(body);
                    UpstreamResponse::stream(status, headers, body)
                }
                Some(Err(_)) => {
                    record_http_fallback("first_event_error");
                    send_http(request, context).await
                }
                None => {
                    record_http_fallback("early_eof");
                    send_http(request, context).await
                }
            },
            UpstreamBody::Full(_) => {
                record_http_fallback("unexpected_response_mode");
                send_http(request, context).await
            }
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
    let (method, target, headers, body, response_mode) = request.into_parts();
    let response = context
        .http_client()
        .execute(method, &target, headers, body.map(Body::from))
        .await?;
    match response_mode {
        ResponseMode::Full => UpstreamResponse::from_http_full(response).await,
        ResponseMode::Stream => UpstreamResponse::from_http_stream(response),
    }
}

fn pool_error_kind(error: ResponsesWebSocketPoolError) -> &'static str {
    match error {
        ResponsesWebSocketPoolError::InvalidSession => "invalid_session",
        ResponsesWebSocketPoolError::InvalidHandshakeTarget => "invalid_handshake_target",
        ResponsesWebSocketPoolError::InvalidHandshakeHeader => "invalid_handshake_header",
        ResponsesWebSocketPoolError::InvalidRequest => "invalid_request",
        ResponsesWebSocketPoolError::PoolSaturated => "pool_saturated",
        ResponsesWebSocketPoolError::QueueTimeout => "queue_timeout",
        ResponsesWebSocketPoolError::ConnectTimeout => "connect_timeout",
        ResponsesWebSocketPoolError::TurnTimeout => "turn_timeout",
        ResponsesWebSocketPoolError::Protocol => "protocol",
        ResponsesWebSocketPoolError::ConnectionClosed => "connection_closed",
        ResponsesWebSocketPoolError::PoolUnavailable => "pool_unavailable",
        ResponsesWebSocketPoolError::Transport(_) => "transport",
    }
}

fn record_http_fallback(error_kind: &'static str) {
    tracing::warn!(
        target: "af_adapter::transport_dispatcher",
        transport = "responses_websocket",
        fallback_transport = "http",
        error_kind,
        "Responses WebSocket 在首事件前失败，执行一次 HTTP 回退"
    );
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

    use crate::{
        Bytes, HeaderMap, HeaderName, HeaderValue, Method, ResponsesWebSocketConnection,
        ResponsesWebSocketConnector, ResponsesWebSocketFrame, ResponsesWebSocketHandshake,
        ResponsesWebSocketTransportError,
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
        let response = dispatcher
            .send(request(ResponseMode::Stream), &context)
            .await
            .unwrap();
        assert_eq!(
            response.into_body().into_bytes().await.unwrap(),
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
        let response = dispatcher
            .send(request(ResponseMode::Stream), &context)
            .await
            .unwrap();
        assert!(response.into_body().into_bytes().await.is_err());
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
