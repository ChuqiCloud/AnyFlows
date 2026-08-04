use std::{fmt, pin::Pin, time::Duration};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
use reqwest::{Request, Response, Upgraded, Url};
use thiserror::Error;
use tokio::time::{Sleep, timeout};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        Error as TungsteniteError, Message, Utf8Bytes,
        handshake::{client::generate_key, derive_accept_key},
        protocol::{Role, WebSocketConfig},
    },
};

use crate::{HttpTransportError, PooledClient};

/// 受控 WebSocket 单条消息与单帧的统一上限，与适配层响应块预算保持一致。
pub const MAX_MANAGED_WEBSOCKET_MESSAGE_BYTES: usize = 32 * 1_024 * 1_024;

const MAX_MANAGED_WEBSOCKET_TARGET_BYTES: usize = 8_192;
const WEBSOCKET_BUFFER_BYTES: usize = 16 * 1_024;
const MAX_WEBSOCKET_WRITE_BUFFER_BYTES: usize =
    MAX_MANAGED_WEBSOCKET_MESSAGE_BYTES + 2 * WEBSOCKET_BUFFER_BYTES;

/// 已完成协议解析且不携带敏感内容的 WebSocket 帧。
#[derive(Clone, Eq, PartialEq)]
pub enum WebSocketFrame {
    /// UTF-8 文本消息。
    Text(Bytes),
    /// 二进制消息。
    Binary(Bytes),
    /// Ping 控制帧；底层已排队并刷新对应 Pong。
    Ping,
    /// Pong 控制帧。
    Pong,
}

impl fmt::Debug for WebSocketFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(bytes) => formatter
                .debug_struct("Text")
                .field("bytes", &bytes.len())
                .finish(),
            Self::Binary(bytes) => formatter
                .debug_struct("Binary")
                .field("bytes", &bytes.len())
                .finish(),
            Self::Ping => formatter.write_str("Ping"),
            Self::Pong => formatter.write_str("Pong"),
        }
    }
}

/// 不携带 URL、Header、帧内容或底层错误链的 WebSocket 传输错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum WebSocketTransportError {
    #[error("WebSocket 请求目标无效")]
    InvalidRequestTarget,
    #[error("WebSocket 握手 Header 无效")]
    InvalidHandshakeHeader,
    #[error("WebSocket 上游目标地址被安全策略阻断")]
    TargetAddressBlocked,
    #[error("WebSocket 上游目标解析失败")]
    TargetResolution,
    #[error("WebSocket 代理端解析上游域名未获授权")]
    RemoteDnsDenied,
    #[error("WebSocket 建连超时")]
    ConnectTimeout,
    #[error("WebSocket 建连失败")]
    Connect,
    #[error("WebSocket 握手超时")]
    HandshakeTimeout,
    #[error("WebSocket 握手失败")]
    Handshake,
    #[error("WebSocket 发送超时")]
    SendTimeout,
    #[error("WebSocket 发送失败")]
    Send,
    #[error("WebSocket 接收超时")]
    ReceiveTimeout,
    #[error("WebSocket 接收失败")]
    Receive,
    #[error("WebSocket 连接已关闭")]
    Closed,
}

impl WebSocketTransportError {
    /// 返回错误是否由任一受控超时触发。
    #[must_use]
    pub const fn is_timeout(self) -> bool {
        matches!(
            self,
            Self::ConnectTimeout
                | Self::HandshakeTimeout
                | Self::SendTimeout
                | Self::ReceiveTimeout
        )
    }

    /// 返回错误是否属于建连或握手阶段的可重试故障。
    #[must_use]
    pub const fn is_connect(self) -> bool {
        matches!(
            self,
            Self::ConnectTimeout | Self::Connect | Self::HandshakeTimeout | Self::Handshake
        )
    }

    /// 返回底层连接是否已确定不可继续使用。
    #[must_use]
    pub const fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }

    fn from_http(error: HttpTransportError) -> Self {
        match error {
            HttpTransportError::InvalidRequestTarget
            | HttpTransportError::UnsupportedRequestMethod => Self::InvalidRequestTarget,
            HttpTransportError::TargetAddressBlocked => Self::TargetAddressBlocked,
            HttpTransportError::TargetResolution => Self::TargetResolution,
            HttpTransportError::RemoteDnsDenied => Self::RemoteDnsDenied,
            HttpTransportError::ConnectTimeout => Self::ConnectTimeout,
            HttpTransportError::Connect => Self::Connect,
            HttpTransportError::ReadTimeout | HttpTransportError::RequestTimeout => {
                Self::HandshakeTimeout
            }
            HttpTransportError::Request | HttpTransportError::ResponseBody => Self::Handshake,
        }
    }
}

/// 通过受控 HTTP Client 完成升级的 WebSocket 连接。
pub struct WebSocketConnection {
    inner: Option<WebSocketStream<Upgraded>>,
    io_timeout: Duration,
}

impl WebSocketConnection {
    fn new(inner: WebSocketStream<Upgraded>, io_timeout: Duration) -> Self {
        Self {
            inner: Some(inner),
            io_timeout,
        }
    }

    /// 发送一条 UTF-8 文本消息，并按全局读取超时限制写入停顿。
    pub async fn send_text(&mut self, payload: Bytes) -> Result<(), WebSocketTransportError> {
        if payload.len() > MAX_MANAGED_WEBSOCKET_MESSAGE_BYTES {
            self.inner = None;
            return Err(WebSocketTransportError::Send);
        }
        let payload = Utf8Bytes::try_from(payload).map_err(|_| WebSocketTransportError::Send)?;
        let result = {
            let stream = self.inner.as_mut().ok_or(WebSocketTransportError::Closed)?;
            timeout(self.io_timeout, stream.send(Message::Text(payload))).await
        };
        match result {
            Err(_) => {
                self.inner = None;
                Err(WebSocketTransportError::SendTimeout)
            }
            Ok(Err(error)) => {
                self.inner = None;
                Err(if is_closed_error(&error) {
                    WebSocketTransportError::Closed
                } else {
                    WebSocketTransportError::Send
                })
            }
            Ok(Ok(())) => Ok(()),
        }
    }

    /// 接收下一条消息；正常 Close 或 EOF 返回 `None`，控制帧不会暴露载荷。
    pub async fn receive(&mut self) -> Result<Option<WebSocketFrame>, WebSocketTransportError> {
        let result = {
            let stream = self.inner.as_mut().ok_or(WebSocketTransportError::Closed)?;
            timeout(self.io_timeout, stream.next()).await
        };
        let message = match result {
            Err(_) => {
                self.inner = None;
                return Err(WebSocketTransportError::ReceiveTimeout);
            }
            Ok(None) => {
                self.inner = None;
                return Ok(None);
            }
            Ok(Some(Err(error))) if is_closed_error(&error) => {
                self.inner = None;
                return Ok(None);
            }
            Ok(Some(Err(_))) => {
                self.inner = None;
                return Err(WebSocketTransportError::Receive);
            }
            Ok(Some(Ok(message))) => message,
        };

        match message {
            Message::Text(text) => Ok(Some(WebSocketFrame::Text(text.into()))),
            Message::Binary(bytes) => Ok(Some(WebSocketFrame::Binary(bytes))),
            Message::Ping(_) => {
                // Tungstenite 在读取 Ping 时自动排队 Pong；显式 flush 保证空闲读期间也会及时回写。
                self.flush_control_reply().await?;
                Ok(Some(WebSocketFrame::Ping))
            }
            Message::Pong(_) => Ok(Some(WebSocketFrame::Pong)),
            Message::Close(_) => {
                let _ = self.flush_control_reply().await;
                self.inner = None;
                Ok(None)
            }
            Message::Frame(_) => {
                self.inner = None;
                Err(WebSocketTransportError::Receive)
            }
        }
    }

    /// 尽力发送 Close 帧；无论结果如何都会立即释放底层连接。
    pub async fn close(&mut self) -> Result<(), WebSocketTransportError> {
        let Some(mut stream) = self.inner.take() else {
            return Ok(());
        };
        match timeout(self.io_timeout, stream.close(None)).await {
            Err(_) => Err(WebSocketTransportError::SendTimeout),
            Ok(Err(error)) if is_closed_error(&error) => Ok(()),
            Ok(Err(_)) => Err(WebSocketTransportError::Send),
            Ok(Ok(())) => Ok(()),
        }
    }

    async fn flush_control_reply(&mut self) -> Result<(), WebSocketTransportError> {
        let result = {
            let stream = self.inner.as_mut().ok_or(WebSocketTransportError::Closed)?;
            timeout(self.io_timeout, stream.flush()).await
        };
        match result {
            Err(_) => {
                self.inner = None;
                Err(WebSocketTransportError::SendTimeout)
            }
            Ok(Err(error)) => {
                self.inner = None;
                Err(if is_closed_error(&error) {
                    WebSocketTransportError::Closed
                } else {
                    WebSocketTransportError::Send
                })
            }
            Ok(Ok(())) => Ok(()),
        }
    }
}

impl fmt::Debug for WebSocketConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebSocketConnection")
            .field(
                "state",
                &if self.inner.is_some() {
                    "open"
                } else {
                    "closed"
                },
            )
            .field("io_timeout", &self.io_timeout)
            .finish()
    }
}

struct PreparedHandshake {
    request: Request,
    expected_accept: String,
}

pub(crate) async fn connect(
    client: &PooledClient,
    target: &str,
    headers: HeaderMap,
) -> Result<WebSocketConnection, WebSocketTransportError> {
    let prepared = prepare_handshake(target, headers)?;
    client
        .validate_target(prepared.request.url())
        .map_err(WebSocketTransportError::from_http)?;
    let (response, request_timeout) = client
        .execute_managed_request(prepared.request)
        .await
        .map_err(WebSocketTransportError::from_http)?;
    finish_handshake(
        response,
        request_timeout,
        &prepared.expected_accept,
        client.read_timeout(),
    )
    .await
}

fn prepare_handshake(
    target: &str,
    mut headers: HeaderMap,
) -> Result<PreparedHandshake, WebSocketTransportError> {
    if target.is_empty()
        || target.len() > MAX_MANAGED_WEBSOCKET_TARGET_BYTES
        || target.trim() != target
    {
        return Err(WebSocketTransportError::InvalidRequestTarget);
    }
    validate_application_headers(&headers)?;
    let mut target =
        Url::parse(target).map_err(|_| WebSocketTransportError::InvalidRequestTarget)?;
    if !matches!(target.scheme(), "ws" | "wss")
        || !target.has_host()
        || !target.username().is_empty()
        || target.password().is_some()
        || target.fragment().is_some()
        || target.port() == Some(0)
    {
        return Err(WebSocketTransportError::InvalidRequestTarget);
    }
    let http_scheme = if target.scheme() == "wss" {
        "https"
    } else {
        "http"
    };
    target
        .set_scheme(http_scheme)
        .map_err(|_| WebSocketTransportError::InvalidRequestTarget)?;

    let key = generate_key();
    let expected_accept = derive_accept_key(key.as_bytes());
    headers.insert(
        HeaderName::from_static("connection"),
        HeaderValue::from_static("Upgrade"),
    );
    headers.insert(
        HeaderName::from_static("upgrade"),
        HeaderValue::from_static("websocket"),
    );
    headers.insert(
        HeaderName::from_static("sec-websocket-version"),
        HeaderValue::from_static("13"),
    );
    headers.insert(
        HeaderName::from_static("sec-websocket-key"),
        HeaderValue::from_str(&key).map_err(|_| WebSocketTransportError::InvalidHandshakeHeader)?,
    );

    let mut request = Request::new(Method::GET, target);
    *request.version_mut() = Version::HTTP_11;
    *request.headers_mut() = headers;
    Ok(PreparedHandshake {
        request,
        expected_accept,
    })
}

async fn finish_handshake(
    response: Response,
    mut request_timeout: Pin<Box<Sleep>>,
    expected_accept: &str,
    io_timeout: Duration,
) -> Result<WebSocketConnection, WebSocketTransportError> {
    validate_handshake_response(&response, expected_accept)?;
    let upgraded = tokio::select! {
        biased;
        () = request_timeout.as_mut() => return Err(WebSocketTransportError::HandshakeTimeout),
        result = response.upgrade() => result.map_err(|_| WebSocketTransportError::Handshake)?,
    };
    let websocket = WebSocketStream::from_raw_socket(
        upgraded,
        Role::Client,
        Some(
            WebSocketConfig::default()
                .read_buffer_size(WEBSOCKET_BUFFER_BYTES)
                .write_buffer_size(WEBSOCKET_BUFFER_BYTES)
                .max_write_buffer_size(MAX_WEBSOCKET_WRITE_BUFFER_BYTES)
                .max_message_size(Some(MAX_MANAGED_WEBSOCKET_MESSAGE_BYTES))
                .max_frame_size(Some(MAX_MANAGED_WEBSOCKET_MESSAGE_BYTES)),
        ),
    )
    .await;
    Ok(WebSocketConnection::new(websocket, io_timeout))
}

fn validate_application_headers(headers: &HeaderMap) -> Result<(), WebSocketTransportError> {
    if headers.keys().any(|name| {
        matches!(
            name.as_str(),
            "connection"
                | "content-length"
                | "host"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "proxy-connection"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
        ) || name.as_str().starts_with("sec-websocket-")
    }) {
        return Err(WebSocketTransportError::InvalidHandshakeHeader);
    }
    Ok(())
}

fn validate_handshake_response(
    response: &Response,
    expected_accept: &str,
) -> Result<(), WebSocketTransportError> {
    let headers = response.headers();
    if response.status() != StatusCode::SWITCHING_PROTOCOLS
        || response.version() != Version::HTTP_11
        || !header_contains_token(headers, "connection", "upgrade")
        || !header_contains_token(headers, "upgrade", "websocket")
        || !header_has_single_value(headers, "sec-websocket-accept", expected_accept)
        || headers.contains_key("sec-websocket-extensions")
        || headers.contains_key("sec-websocket-protocol")
    {
        return Err(WebSocketTransportError::Handshake);
    }
    Ok(())
}

fn header_contains_token(headers: &HeaderMap, name: &'static str, expected: &str) -> bool {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|value| value.trim().eq_ignore_ascii_case(expected))
}

fn header_has_single_value(headers: &HeaderMap, name: &'static str, expected: &str) -> bool {
    let mut values = headers.get_all(name).iter();
    matches!(values.next(), Some(value) if value.as_bytes() == expected.as_bytes())
        && values.next().is_none()
}

fn is_closed_error(error: &TungsteniteError) -> bool {
    matches!(
        error,
        TungsteniteError::ConnectionClosed | TungsteniteError::AlreadyClosed
    )
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use tokio_tungstenite::accept_async;

    use super::*;
    use crate::{
        HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy,
        TargetAddressPolicy,
    };

    #[tokio::test]
    async fn direct_upgrade_maps_frames_and_flushes_automatic_pong() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            assert_eq!(
                websocket.next().await.unwrap().unwrap(),
                Message::Text(Utf8Bytes::from("request"))
            );
            websocket
                .send(Message::Text(Utf8Bytes::from("text-response")))
                .await
                .unwrap();
            websocket
                .send(Message::Binary(Bytes::from_static(b"binary-response")))
                .await
                .unwrap();
            websocket
                .send(Message::Ping(Bytes::from_static(b"health")))
                .await
                .unwrap();
            assert_eq!(
                websocket.next().await.unwrap().unwrap(),
                Message::Pong(Bytes::from_static(b"health"))
            );
            websocket
                .send(Message::Pong(Bytes::from_static(b"server-pong")))
                .await
                .unwrap();
            websocket.close(None).await.unwrap();
        });

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer test-secret"),
        );
        let mut connection = loopback_client(ProxyConfig::direct())
            .connect_websocket(&format!("ws://{address}/responses"), headers)
            .await
            .unwrap();
        connection
            .send_text(Bytes::from_static(b"request"))
            .await
            .unwrap();
        assert_eq!(
            connection.receive().await.unwrap(),
            Some(WebSocketFrame::Text(Bytes::from_static(b"text-response")))
        );
        assert_eq!(
            connection.receive().await.unwrap(),
            Some(WebSocketFrame::Binary(Bytes::from_static(
                b"binary-response"
            )))
        );
        assert_eq!(
            connection.receive().await.unwrap(),
            Some(WebSocketFrame::Ping)
        );
        assert_eq!(
            connection.receive().await.unwrap(),
            Some(WebSocketFrame::Pong)
        );
        assert_eq!(connection.receive().await.unwrap(), None);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn required_http_proxy_carries_upgrade_without_local_target_dns() {
        let proxy = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let proxy_address = proxy.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = proxy.accept().await.unwrap();
            let request = read_request_head(&mut stream).await;
            assert!(
                request.starts_with("GET http://public.example/responses?mode=proxy HTTP/1.1\r\n")
            );
            assert_eq!(
                request_header(&request, "authorization"),
                Some("Bearer proxy-secret")
            );
            let key = request_header(&request, "sec-websocket-key").unwrap();
            let accept = derive_accept_key(key.as_bytes());
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            let mut websocket = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
            assert_eq!(
                websocket.next().await.unwrap().unwrap(),
                Message::Text(Utf8Bytes::from("through-proxy"))
            );
            websocket
                .send(Message::Text(Utf8Bytes::from("proxy-response")))
                .await
                .unwrap();
            websocket.close(None).await.unwrap();
        });

        let config = HttpClientConfig::new(
            ProxyConfig::parse(format!("http://{proxy_address}")).unwrap(),
            test_timeouts(),
        )
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
        let client = HttpClientPool::default().get(&config).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer proxy-secret"),
        );
        let mut connection = client
            .connect_websocket("ws://public.example/responses?mode=proxy", headers)
            .await
            .unwrap();
        connection
            .send_text(Bytes::from_static(b"through-proxy"))
            .await
            .unwrap();
        assert_eq!(
            connection.receive().await.unwrap(),
            Some(WebSocketFrame::Text(Bytes::from_static(b"proxy-response")))
        );
        assert_eq!(connection.receive().await.unwrap(), None);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn blocked_targets_and_remote_dns_fail_before_network_access() {
        let origin = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let origin_address = origin.local_addr().unwrap();
        let direct = HttpClientPool::default()
            .get(&HttpClientConfig::new(
                ProxyConfig::direct(),
                test_timeouts(),
            ))
            .unwrap();
        for target in [
            format!("ws://{origin_address}/literal"),
            format!("ws://localhost:{}/dns", origin_address.port()),
        ] {
            assert_eq!(
                direct
                    .connect_websocket(&target, HeaderMap::new())
                    .await
                    .unwrap_err(),
                WebSocketTransportError::TargetAddressBlocked
            );
        }
        assert!(
            timeout(Duration::from_millis(100), origin.accept())
                .await
                .is_err()
        );

        let proxy = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let proxy_address = proxy.local_addr().unwrap();
        let denied = HttpClientPool::default()
            .get(&HttpClientConfig::new(
                ProxyConfig::parse(format!("http://{proxy_address}")).unwrap(),
                test_timeouts(),
            ))
            .unwrap();
        assert_eq!(
            denied
                .connect_websocket("ws://public.example/denied", HeaderMap::new())
                .await
                .unwrap_err(),
            WebSocketTransportError::RemoteDnsDenied
        );
        assert!(
            timeout(Duration::from_millis(100), proxy.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn required_proxy_failure_never_falls_back_to_origin() {
        let origin = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let origin_address = origin.local_addr().unwrap();
        let unavailable_proxy = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let proxy_address = unavailable_proxy.local_addr().unwrap();
        drop(unavailable_proxy);

        let client =
            loopback_client(ProxyConfig::parse(format!("http://{proxy_address}")).unwrap());
        let error = client
            .connect_websocket(
                &format!("ws://{origin_address}/must-use-proxy"),
                HeaderMap::new(),
            )
            .await
            .unwrap_err();
        assert!(error.is_connect());
        assert!(
            timeout(Duration::from_millis(100), origin.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn invalid_accept_and_reserved_headers_fail_with_redacted_errors() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let _ = read_request_head(&mut stream).await;
            stream
                .write_all(
                    b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Accept: invalid\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let target = format!("ws://{address}/private?token=secret");
        let error = loopback_client(ProxyConfig::direct())
            .connect_websocket(&target, HeaderMap::new())
            .await
            .unwrap_err();
        assert_eq!(error, WebSocketTransportError::Handshake);
        let rendered = format!("{error:?}\n{error}");
        assert!(!rendered.contains(&target));
        assert!(!rendered.contains("secret"));
        server.await.unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("connection"),
            HeaderValue::from_static("close"),
        );
        assert_eq!(
            loopback_client(ProxyConfig::direct())
                .connect_websocket("ws://public.example/rejected", headers)
                .await
                .unwrap_err(),
            WebSocketTransportError::InvalidHandshakeHeader
        );

        let frame = WebSocketFrame::Text(Bytes::from_static(b"private-frame"));
        assert!(!format!("{frame:?}").contains("private-frame"));
    }

    fn loopback_client(proxy: ProxyConfig) -> PooledClient {
        let policy = TargetAddressPolicy::allow_exact([
            "127.0.0.1".parse().unwrap(),
            "::1".parse().unwrap(),
        ])
        .unwrap();
        let config = HttpClientConfig::new(proxy, test_timeouts())
            .with_target_address_policy(policy)
            .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
        HttpClientPool::default().get(&config).unwrap()
    }

    fn test_timeouts() -> HttpTimeouts {
        HttpTimeouts::new(
            Duration::from_millis(500),
            Duration::from_secs(2),
            Duration::from_secs(4),
        )
        .unwrap()
    }

    async fn read_request_head(stream: &mut TcpStream) -> String {
        const MAX_HEADER_BYTES: usize = 64 * 1_024;
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1_024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut buffer).await.unwrap();
            assert!(read > 0, "握手请求在 Header 结束前关闭");
            request.extend_from_slice(&buffer[..read]);
            assert!(
                request.len() <= MAX_HEADER_BYTES,
                "握手请求 Header 超过测试预算"
            );
        }
        String::from_utf8(request).unwrap()
    }

    fn request_header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
        request.lines().skip(1).find_map(|line| {
            let (header_name, value) = line.split_once(':')?;
            header_name
                .eq_ignore_ascii_case(name)
                .then_some(value.trim())
        })
    }
}
