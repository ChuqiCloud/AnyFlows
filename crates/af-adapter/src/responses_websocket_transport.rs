use std::fmt;

use af_httpclient::{
    WebSocketConnection, WebSocketFrame, WebSocketTransportError as ManagedTransportError,
};
use async_trait::async_trait;

use crate::{
    Bytes, PooledClient, ResponsesWebSocketConnection, ResponsesWebSocketConnector,
    ResponsesWebSocketFrame, ResponsesWebSocketHandshake, ResponsesWebSocketTransportError,
};

/// 使用受控 HTTP Client 建立 Responses WebSocket 的生产拨号器桥接。
///
/// 代理、DNS、SSRF、TLS 与超时均由 [`PooledClient`] 统一执行；本类型只负责收窄协议对象与错误。
#[derive(Clone)]
pub struct PooledResponsesWebSocketConnector {
    client: PooledClient,
}

impl PooledResponsesWebSocketConnector {
    /// 绑定一份已按全局或渠道超时配置取得的受控 Client。
    #[must_use]
    pub const fn new(client: PooledClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ResponsesWebSocketConnector for PooledResponsesWebSocketConnector {
    async fn connect(
        &self,
        handshake: ResponsesWebSocketHandshake,
    ) -> Result<Box<dyn ResponsesWebSocketConnection>, ResponsesWebSocketTransportError> {
        let (target, headers) = handshake.into_parts();
        let connection = self
            .client
            .connect_websocket(&target, headers)
            .await
            .map_err(map_connect_error)?;
        Ok(Box::new(PooledResponsesWebSocketConnection {
            inner: connection,
        }))
    }
}

impl fmt::Debug for PooledResponsesWebSocketConnector {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PooledResponsesWebSocketConnector")
            .field("client", &"<受控>")
            .finish()
    }
}

struct PooledResponsesWebSocketConnection {
    inner: WebSocketConnection,
}

#[async_trait]
impl ResponsesWebSocketConnection for PooledResponsesWebSocketConnection {
    async fn send_text(&mut self, payload: Bytes) -> Result<(), ResponsesWebSocketTransportError> {
        self.inner.send_text(payload).await.map_err(map_send_error)
    }

    async fn receive(
        &mut self,
    ) -> Result<Option<ResponsesWebSocketFrame>, ResponsesWebSocketTransportError> {
        self.inner
            .receive()
            .await
            .map(|frame| frame.map(map_frame))
            .map_err(map_receive_error)
    }

    async fn close(&mut self) {
        let _ = self.inner.close().await;
    }
}

fn map_frame(frame: WebSocketFrame) -> ResponsesWebSocketFrame {
    match frame {
        WebSocketFrame::Text(bytes) => ResponsesWebSocketFrame::Text(bytes),
        WebSocketFrame::Binary(bytes) => ResponsesWebSocketFrame::Binary(bytes),
        WebSocketFrame::Ping => ResponsesWebSocketFrame::Ping,
        WebSocketFrame::Pong => ResponsesWebSocketFrame::Pong,
    }
}

fn map_connect_error(_: ManagedTransportError) -> ResponsesWebSocketTransportError {
    ResponsesWebSocketTransportError::Connect
}

fn map_send_error(error: ManagedTransportError) -> ResponsesWebSocketTransportError {
    if error.is_closed() {
        ResponsesWebSocketTransportError::Closed
    } else {
        ResponsesWebSocketTransportError::Send
    }
}

fn map_receive_error(error: ManagedTransportError) -> ResponsesWebSocketTransportError {
    if error.is_closed() {
        ResponsesWebSocketTransportError::Closed
    } else {
        ResponsesWebSocketTransportError::Receive
    }
}

#[cfg(test)]
mod tests {
    use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig};

    use super::*;

    #[test]
    fn bridge_maps_frames_without_copying_payloads() {
        assert_eq!(
            map_frame(WebSocketFrame::Text(Bytes::from_static(b"text"))),
            ResponsesWebSocketFrame::Text(Bytes::from_static(b"text"))
        );
        assert_eq!(
            map_frame(WebSocketFrame::Binary(Bytes::from_static(b"binary"))),
            ResponsesWebSocketFrame::Binary(Bytes::from_static(b"binary"))
        );
        assert_eq!(
            map_frame(WebSocketFrame::Ping),
            ResponsesWebSocketFrame::Ping
        );
        assert_eq!(
            map_frame(WebSocketFrame::Pong),
            ResponsesWebSocketFrame::Pong
        );
    }

    #[test]
    fn bridge_collapses_errors_and_redacts_client_identity() {
        assert_eq!(
            map_connect_error(ManagedTransportError::TargetAddressBlocked),
            ResponsesWebSocketTransportError::Connect
        );
        assert_eq!(
            map_send_error(ManagedTransportError::Closed),
            ResponsesWebSocketTransportError::Closed
        );
        assert_eq!(
            map_send_error(ManagedTransportError::SendTimeout),
            ResponsesWebSocketTransportError::Send
        );
        assert_eq!(
            map_receive_error(ManagedTransportError::ReceiveTimeout),
            ResponsesWebSocketTransportError::Receive
        );

        let proxy = ProxyConfig::parse("http://user:secret@proxy.example:8080").unwrap();
        let client = HttpClientPool::default()
            .get(&HttpClientConfig::new(proxy, HttpTimeouts::default()))
            .unwrap();
        let debug = format!("{:?}", PooledResponsesWebSocketConnector::new(client));
        assert!(!debug.contains("user"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("proxy.example"));
    }
}
