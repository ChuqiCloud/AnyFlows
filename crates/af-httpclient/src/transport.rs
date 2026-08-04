use std::{
    fmt,
    future::{Future, poll_fn},
    hash::{Hash, Hasher},
    net::IpAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderMap, Method, StatusCode, Version};
use reqwest::{Body, Client, Request, Response, Url};
use tokio::time::{Sleep, sleep};
use url::Host;

use crate::{
    HttpTransportError, RemoteDnsPolicy, TargetAddressPolicy, TargetDnsStrategy,
    address::is_target_address_allowed, resolver::normalize_dns_name,
};

type ReqwestBodyStream =
    Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send + 'static>>;

struct PooledClientIdentityMarker {
    _opaque: u8,
}

/// 受控 Client 的不透明传输身份。
///
/// 同一 Client 的克隆共享身份；代理、DNS 或超时配置变化后新建的 Client 使用新身份。
/// 调用方只能用它做相等比较和哈希分桶，不能据此恢复配置或底层连接信息。
#[derive(Clone)]
pub struct PooledClientIdentity(Arc<PooledClientIdentityMarker>);

impl PooledClientIdentity {
    fn new() -> Self {
        Self(Arc::new(PooledClientIdentityMarker { _opaque: 0 }))
    }
}

impl PartialEq for PooledClientIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for PooledClientIdentity {}

impl Hash for PooledClientIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl fmt::Debug for PooledClientIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PooledClientIdentity(<受控>)")
    }
}

/// 只能通过连接池取得的受控 HTTP Client。
#[derive(Clone)]
pub struct PooledClient {
    identity: PooledClientIdentity,
    inner: Client,
    read_timeout: Duration,
    request_timeout: Duration,
    target_address_policy: TargetAddressPolicy,
    target_dns_strategy: TargetDnsStrategy,
    remote_dns_policy: RemoteDnsPolicy,
    proxy_host: Option<String>,
}

impl PooledClient {
    pub(crate) fn new(
        inner: Client,
        read_timeout: Duration,
        request_timeout: Duration,
        target_address_policy: TargetAddressPolicy,
        target_dns_strategy: TargetDnsStrategy,
        remote_dns_policy: RemoteDnsPolicy,
        proxy_host: Option<String>,
    ) -> Self {
        Self {
            identity: PooledClientIdentity::new(),
            inner,
            read_timeout,
            request_timeout,
            target_address_policy,
            target_dns_strategy,
            remote_dns_policy,
            proxy_host,
        }
    }

    /// 返回本 Client 覆盖完整响应体生命周期的总超时。
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// 返回升级后 WebSocket 单次读写停顿所使用的超时。
    #[must_use]
    pub const fn read_timeout(&self) -> Duration {
        self.read_timeout
    }

    /// 返回仅供传输资源分桶使用的不透明 Client 身份。
    #[must_use]
    pub fn identity(&self) -> PooledClientIdentity {
        self.identity.clone()
    }

    /// 按池配置发送上游请求；建连前会执行字面量或域名目标安全策略。
    ///
    /// 原始传输错误会在本 crate 内立即分类并丢弃，避免 URL 或凭据进入错误日志。
    pub async fn execute(
        &self,
        method: Method,
        target: &str,
        headers: HeaderMap,
        body: Option<Body>,
    ) -> Result<HttpResponse, HttpTransportError> {
        if method == Method::CONNECT {
            return Err(HttpTransportError::UnsupportedRequestMethod);
        }
        let target = parse_target(target)?;
        self.validate_target(&target)?;
        let mut request = Request::new(method, target);
        *request.headers_mut() = headers;
        *request.body_mut() = body;

        // 总时限贯穿完整响应体；同时就绪时优先判定硬 deadline。
        let (response, request_timeout) = self.execute_managed_request(request).await?;
        Ok(HttpResponse::new(response, request_timeout))
    }

    /// 使用当前 Client 的强制代理、DNS、SSRF、TLS 与超时策略建立 WebSocket 连接。
    ///
    /// 调用方只提供业务 Header；协议升级与代理 Header 由本层独占生成，避免覆盖受控路由。
    pub async fn connect_websocket(
        &self,
        target: &str,
        headers: HeaderMap,
    ) -> Result<crate::WebSocketConnection, crate::WebSocketTransportError> {
        crate::websocket::connect(self, target, headers).await
    }

    pub(crate) async fn execute_managed_request(
        &self,
        request: Request,
    ) -> Result<(Response, Pin<Box<Sleep>>), HttpTransportError> {
        let mut request_timeout = Box::pin(sleep(self.request_timeout));
        let response = tokio::select! {
            biased;
            () = request_timeout.as_mut() => return Err(HttpTransportError::RequestTimeout),
            result = self.inner.execute(request) => {
                result.map_err(|error| HttpTransportError::from_send(&error))?
            }
        };
        Ok((response, request_timeout))
    }

    pub(crate) fn validate_target(&self, target: &Url) -> Result<(), HttpTransportError> {
        match target.host() {
            Some(Host::Ipv4(address)) => self.validate_literal_address(IpAddr::V4(address)),
            Some(Host::Ipv6(address)) => self.validate_literal_address(IpAddr::V6(address)),
            Some(Host::Domain(host)) => self.validate_domain_target(host),
            None => Err(HttpTransportError::InvalidRequestTarget),
        }
    }

    fn validate_literal_address(&self, address: IpAddr) -> Result<(), HttpTransportError> {
        if !is_target_address_allowed(address, &self.target_address_policy) {
            return Err(HttpTransportError::TargetAddressBlocked);
        }
        Ok(())
    }

    fn validate_domain_target(&self, host: &str) -> Result<(), HttpTransportError> {
        let host = normalize_dns_name(host);
        if self.target_dns_strategy == TargetDnsStrategy::Local {
            // SOCKS5 的代理主机豁免不能被同名上游复用，否则会绕过目标地址校验。
            if self.proxy_host.as_deref() == Some(host.as_str()) {
                return Err(HttpTransportError::TargetAddressBlocked);
            }
            return Ok(());
        }
        if self.remote_dns_policy == RemoteDnsPolicy::Deny {
            return Err(HttpTransportError::RemoteDnsDenied);
        }
        Ok(())
    }
}

impl fmt::Debug for PooledClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PooledClient")
            .field("transport", &"<managed>")
            .field("target_address_policy", &self.target_address_policy)
            .field("target_dns_strategy", &self.target_dns_strategy)
            .field("remote_dns_policy", &self.remote_dns_policy)
            .finish()
    }
}

/// 隐藏最终 URL、Header 值和底层错误的上游 HTTP 响应。
pub struct HttpResponse {
    inner: Response,
    request_timeout: Pin<Box<Sleep>>,
}

impl HttpResponse {
    const fn new(inner: Response, request_timeout: Pin<Box<Sleep>>) -> Self {
        Self {
            inner,
            request_timeout,
        }
    }

    /// 返回上游 HTTP 状态码。
    #[must_use]
    pub fn status(&self) -> StatusCode {
        self.inner.status()
    }

    /// 返回上游 HTTP 协议版本。
    #[must_use]
    pub fn version(&self) -> Version {
        self.inner.version()
    }

    /// 返回响应头；调用方不得将含凭据的 HeaderMap 直接写入日志。
    #[must_use]
    pub fn headers(&self) -> &HeaderMap {
        self.inner.headers()
    }

    /// 返回可变响应头，便于转发前删除逐跳或敏感字段。
    #[must_use]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.inner.headers_mut()
    }

    /// 返回已知的响应体长度；分块响应通常为 `None`。
    #[must_use]
    pub fn content_length(&self) -> Option<u64> {
        self.inner.content_length()
    }

    /// 读取完整响应体，并将读取错误转换为无敏感信息的稳定分类。
    pub async fn bytes(self) -> Result<Bytes, HttpTransportError> {
        let Self {
            inner,
            mut request_timeout,
        } = self;
        tokio::select! {
            biased;
            () = request_timeout.as_mut() => Err(HttpTransportError::RequestTimeout),
            result = inner.bytes() => {
                result.map_err(|error| HttpTransportError::from_response_body(&error))
            }
        }
    }

    /// 将响应体转换为支持背压与取消传播的安全字节流。
    #[must_use]
    pub fn into_bytes_stream(self) -> HttpBodyStream {
        HttpBodyStream {
            inner: Some(Box::pin(self.inner.bytes_stream())),
            request_timeout: self.request_timeout,
        }
    }
}

impl fmt::Debug for HttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpResponse")
            .field("status", &self.status())
            .field("version", &self.version())
            .field("header_count", &self.headers().len())
            .field("content_length", &self.content_length())
            .finish()
    }
}

/// 每一项都已完成底层错误脱敏的响应体字节流。
pub struct HttpBodyStream {
    inner: Option<ReqwestBodyStream>,
    request_timeout: Pin<Box<Sleep>>,
}

impl HttpBodyStream {
    /// 等待下一段响应体字节，流结束时返回 `None`。
    pub async fn next_chunk(&mut self) -> Option<Result<Bytes, HttpTransportError>> {
        poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }
}

impl Stream for HttpBodyStream {
    type Item = Result<Bytes, HttpTransportError>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.inner.is_none() {
            return Poll::Ready(None);
        }
        if this.request_timeout.as_mut().poll(context).is_ready() {
            // 立即释放底层响应体，使总超时能向连接层传播取消。
            this.inner = None;
            return Poll::Ready(Some(Err(HttpTransportError::RequestTimeout)));
        }
        let poll = this
            .inner
            .as_mut()
            .expect("响应体流已经检查存在")
            .as_mut()
            .poll_next(context);
        match poll {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(Some(Err(error))) => {
                this.inner = None;
                Poll::Ready(Some(Err(HttpTransportError::from_response_body(&error))))
            }
            Poll::Ready(None) => {
                this.inner = None;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl fmt::Debug for HttpBodyStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpBodyStream")
            .field("body", &"<streaming>")
            .finish()
    }
}

fn parse_target(target: &str) -> Result<Url, HttpTransportError> {
    let target = Url::parse(target).map_err(|_| HttpTransportError::InvalidRequestTarget)?;
    if !matches!(target.scheme(), "http" | "https")
        || !target.username().is_empty()
        || target.password().is_some()
        || target.port() == Some(0)
    {
        return Err(HttpTransportError::InvalidRequestTarget);
    }
    Ok(target)
}
