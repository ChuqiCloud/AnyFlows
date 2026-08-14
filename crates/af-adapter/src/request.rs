use std::{
    fmt,
    future::poll_fn,
    pin::Pin,
    task::{Context, Poll},
};

use af_httpclient::{
    Bytes, HeaderMap, HeaderName, HttpBodyStream, HttpResponse, Method, StatusCode,
};
use futures_core::Stream;
use url::Url;

use crate::{AdaptorError, AdaptorResult};

/// 普通上游请求目标的最大 UTF-8 字节数。
pub const MAX_UPSTREAM_REQUEST_TARGET_BYTES: usize = 8_192;
/// 上游请求头最大字段数量。
pub const MAX_UPSTREAM_REQUEST_HEADER_COUNT: usize = 64;
/// 单个上游请求头值最大字节数。
pub const MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES: usize = 8_192;
/// 上游请求头累计最大字节数。
pub const MAX_UPSTREAM_REQUEST_HEADERS_BYTES: usize = 128 * 1_024;
/// 非流式上游请求体最大 32 MiB。
pub const MAX_UPSTREAM_REQUEST_BODY_BYTES: usize = 32 * 1_024 * 1_024;
/// 非流式上游响应体最大 32 MiB。
pub const MAX_UPSTREAM_RESPONSE_BODY_BYTES: usize = 32 * 1_024 * 1_024;
/// 单请求可申请的上游响应体绝对上限，供受控大响应协议使用。
pub const MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES: usize = 192 * 1_024 * 1_024;
/// 上游响应头最大字段数量。
pub const MAX_UPSTREAM_RESPONSE_HEADER_COUNT: usize = 128;
/// 单个上游响应头值最大字节数。
pub const MAX_UPSTREAM_RESPONSE_HEADER_VALUE_BYTES: usize = 16 * 1_024;
/// 上游响应头累计最大字节数。
pub const MAX_UPSTREAM_RESPONSE_HEADERS_BYTES: usize = 256 * 1_024;
/// 单个流式响应块最大字节数。
pub const MAX_UPSTREAM_RESPONSE_CHUNK_BYTES: usize = 32 * 1_024 * 1_024;

/// 上游响应的交付模式；默认完整收集，只有协议明确需要时才保留流。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResponseMode {
    /// 由适配层在受限内存预算内完整收集响应。
    #[default]
    Full,
    /// 保留响应流，交由上层协议解析器消费。
    Stream,
}

/// 已完成目标与请求体边界校验的上游 HTTP 请求。
pub struct UpstreamRequest {
    method: Method,
    target: String,
    headers: HeaderMap,
    body: Option<Bytes>,
    response_mode: ResponseMode,
    response_body_limit: usize,
}

impl UpstreamRequest {
    /// 构造普通 HTTP(S) 上游请求。
    pub fn new(
        method: Method,
        target: impl Into<String>,
        headers: HeaderMap,
        body: Option<Bytes>,
    ) -> AdaptorResult<Self> {
        if !is_supported_method(&method) {
            return Err(AdaptorError::UnsupportedRequestMethod);
        }
        let target = validate_request_target(target.into())?;
        validate_headers(&headers)?;
        if body
            .as_ref()
            .is_some_and(|value| value.len() > MAX_UPSTREAM_REQUEST_BODY_BYTES)
        {
            return Err(AdaptorError::RequestBodyTooLarge);
        }
        Ok(Self {
            method,
            target,
            headers,
            body,
            response_mode: ResponseMode::default(),
            response_body_limit: MAX_UPSTREAM_RESPONSE_BODY_BYTES,
        })
    }

    /// 设置响应交付模式；普通 JSON 请求应保持默认的完整收集模式。
    #[must_use]
    pub const fn with_response_mode(mut self, response_mode: ResponseMode) -> Self {
        self.response_mode = response_mode;
        self
    }

    /// 为已知会返回大正文的单次请求设置受控收集上限。
    pub fn with_response_body_limit(mut self, max_bytes: usize) -> AdaptorResult<Self> {
        if max_bytes == 0 || max_bytes > MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES {
            return Err(AdaptorError::ResponseBodyTooLarge);
        }
        self.response_body_limit = max_bytes;
        Ok(self)
    }

    /// 合并已经由渠道配置边界校验的非认证请求头并重新检查总预算。
    pub fn with_header_overrides(mut self, overrides: HeaderMap) -> AdaptorResult<Self> {
        self.headers.extend(overrides);
        validate_headers(&self.headers)?;
        Ok(self)
    }

    /// 返回请求方法。
    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    /// 返回已校验的请求目标；不得直接写入日志。
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// 返回请求头；不得直接记录其中的值。
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// 返回请求体；不得直接写入日志。
    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        self.body.as_ref()
    }

    /// 返回响应交付模式。
    #[must_use]
    pub const fn response_mode(&self) -> ResponseMode {
        self.response_mode
    }

    /// 返回本次请求允许收集的最大响应正文字节数。
    #[must_use]
    pub const fn response_body_limit(&self) -> usize {
        self.response_body_limit
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        Method,
        String,
        HeaderMap,
        Option<Bytes>,
        ResponseMode,
        usize,
    ) {
        (
            self.method,
            self.target,
            self.headers,
            self.body,
            self.response_mode,
            self.response_body_limit,
        )
    }
}

impl fmt::Debug for UpstreamRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpstreamRequest")
            .field("method", &self.method)
            .field("target", &"<已脱敏>")
            .field("header_count", &self.headers.len())
            .field("body_bytes", &self.body.as_ref().map(Bytes::len))
            .field("response_body_limit", &self.response_body_limit)
            .finish()
    }
}

/// 与具体传输实现无关的上游响应字节流。
pub struct UpstreamBodyStream {
    inner: Pin<Box<dyn Stream<Item = AdaptorResult<Bytes>> + Send + 'static>>,
    terminated: bool,
    max_chunk_bytes: usize,
}

impl UpstreamBodyStream {
    /// 包装特殊传输提供的字节流；每个错误都必须已经完成脱敏。
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = AdaptorResult<Bytes>> + Send + 'static,
    {
        Self::with_chunk_limit(stream, MAX_UPSTREAM_RESPONSE_CHUNK_BYTES)
    }

    fn with_chunk_limit<S>(stream: S, max_chunk_bytes: usize) -> Self
    where
        S: Stream<Item = AdaptorResult<Bytes>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
            terminated: false,
            max_chunk_bytes,
        }
    }

    fn from_http(stream: HttpBodyStream, max_chunk_bytes: usize) -> Self {
        Self::with_chunk_limit(HttpBodyStreamAdapter { inner: stream }, max_chunk_bytes)
    }

    /// 等待下一段响应体字节，流结束时返回 `None`。
    pub async fn next_chunk(&mut self) -> Option<AdaptorResult<Bytes>> {
        poll_fn(|context| Pin::new(&mut *self).poll_next(context)).await
    }
}

impl Stream for UpstreamBodyStream {
    type Item = AdaptorResult<Bytes>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.terminated {
            return Poll::Ready(None);
        }
        match this.inner.as_mut().poll_next(context) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                this.terminated = true;
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(error))) => {
                this.terminated = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(Some(Ok(bytes))) if bytes.len() > this.max_chunk_bytes => {
                this.terminated = true;
                Poll::Ready(Some(Err(AdaptorError::ResponseBodyTooLarge)))
            }
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
        }
    }
}

impl fmt::Debug for UpstreamBodyStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UpstreamBodyStream(<流式>)")
    }
}

struct HttpBodyStreamAdapter {
    inner: HttpBodyStream,
}

impl Stream for HttpBodyStreamAdapter {
    type Item = AdaptorResult<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner)
            .poll_next(context)
            .map(|item| item.map(|result| result.map_err(AdaptorError::from)))
    }
}

/// 上游响应体；流式响应保持背压与取消传播，不在适配层提前缓冲。
pub enum UpstreamBody {
    /// 已完整缓冲的非流式响应体。
    Full(Bytes),
    /// 保持背压与取消语义的传输无关响应字节流。
    Stream(UpstreamBodyStream),
}

impl UpstreamBody {
    /// 返回响应体是否保持流式传输。
    #[must_use]
    pub const fn is_streaming(&self) -> bool {
        matches!(self, Self::Stream(_))
    }

    /// 将响应体收集为字节，并在累计过程中执行 checked 上限校验。
    pub async fn into_bytes(self) -> AdaptorResult<Bytes> {
        self.into_bytes_with_limit(MAX_UPSTREAM_RESPONSE_BODY_BYTES)
            .await
    }

    /// 按调用链已校验的请求级上限收集响应体，绝不突破适配层绝对预算。
    pub async fn into_bytes_with_limit(self, max_bytes: usize) -> AdaptorResult<Bytes> {
        if max_bytes == 0 || max_bytes > MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES {
            return Err(AdaptorError::ResponseBodyTooLarge);
        }
        match self {
            Self::Full(bytes) => {
                if bytes.len() > max_bytes {
                    return Err(AdaptorError::ResponseBodyTooLarge);
                }
                Ok(bytes)
            }
            Self::Stream(mut stream) => {
                let mut collected = Vec::new();
                while let Some(chunk) = stream.next_chunk().await {
                    let chunk = chunk?;
                    let next_len = collected
                        .len()
                        .checked_add(chunk.len())
                        .ok_or(AdaptorError::ResponseBodyTooLarge)?;
                    if next_len > max_bytes {
                        return Err(AdaptorError::ResponseBodyTooLarge);
                    }
                    collected
                        .try_reserve(chunk.len())
                        .map_err(|_| AdaptorError::ResponseBodyTooLarge)?;
                    collected.extend_from_slice(&chunk);
                }
                Ok(Bytes::from(collected))
            }
        }
    }
}

impl fmt::Debug for UpstreamBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full(bytes) => formatter
                .debug_struct("Full")
                .field("body_bytes", &bytes.len())
                .finish(),
            Self::Stream(_) => formatter.write_str("Stream(<流式>)"),
        }
    }
}

/// 隐藏目标地址、响应头值和响应体内容的上游响应。
pub struct UpstreamResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: UpstreamBody,
}

impl UpstreamResponse {
    /// 构造已完整缓冲的响应，供特殊传输实现使用。
    pub fn full(status: StatusCode, headers: HeaderMap, body: Bytes) -> AdaptorResult<Self> {
        Self::full_with_limit(status, headers, body, MAX_UPSTREAM_RESPONSE_BODY_BYTES)
    }

    fn full_with_limit(
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        max_bytes: usize,
    ) -> AdaptorResult<Self> {
        validate_response_headers(&headers)?;
        if body.len() > max_bytes {
            return Err(AdaptorError::ResponseBodyTooLarge);
        }
        Ok(Self {
            status,
            headers,
            body: UpstreamBody::Full(body),
        })
    }

    /// 将受控 HTTP Client 响应转换为保留流式语义的适配层响应。
    pub(crate) fn from_http_stream(
        response: HttpResponse,
        response_body_limit: usize,
    ) -> AdaptorResult<Self> {
        let status = response.status();
        let headers = response.headers().clone();
        validate_response_headers(&headers)?;
        Ok(Self {
            status,
            headers,
            body: UpstreamBody::Stream(UpstreamBodyStream::from_http(
                response.into_bytes_stream(),
                response_body_limit,
            )),
        })
    }

    /// 收集受控 HTTP Client 响应，并在读取期间执行请求级上限。
    pub(crate) async fn from_http_full(
        response: HttpResponse,
        response_body_limit: usize,
    ) -> AdaptorResult<Self> {
        if response
            .content_length()
            .is_some_and(|length| length > response_body_limit as u64)
        {
            return Err(AdaptorError::ResponseBodyTooLarge);
        }
        let status = response.status();
        let headers = response.headers().clone();
        validate_response_headers(&headers)?;
        let body = UpstreamBody::Stream(UpstreamBodyStream::from_http(
            response.into_bytes_stream(),
            response_body_limit,
        ))
        .into_bytes_with_limit(response_body_limit)
        .await?;
        Self::full_with_limit(status, headers, body, response_body_limit)
    }

    /// 构造特殊传输提供的自定义流式响应。
    pub fn stream<S>(status: StatusCode, headers: HeaderMap, stream: S) -> AdaptorResult<Self>
    where
        S: Stream<Item = AdaptorResult<Bytes>> + Send + 'static,
    {
        validate_response_headers(&headers)?;
        Ok(Self {
            status,
            headers,
            body: UpstreamBody::Stream(UpstreamBodyStream::new(stream)),
        })
    }

    /// 返回上游 HTTP 状态码。
    #[must_use]
    pub const fn status(&self) -> StatusCode {
        self.status
    }

    /// 返回响应头；调用方不得直接记录其中的值。
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// 返回可变响应头，供转发前移除逐跳或敏感字段。
    #[must_use]
    pub const fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// 返回已完整缓冲的响应正文；流式响应必须保持背压，因而返回空。
    #[must_use]
    pub const fn full_body(&self) -> Option<&Bytes> {
        match &self.body {
            UpstreamBody::Full(body) => Some(body),
            UpstreamBody::Stream(_) => None,
        }
    }

    /// 消费响应并返回响应体。
    #[must_use]
    pub fn into_body(self) -> UpstreamBody {
        self.body
    }
}

impl fmt::Debug for UpstreamResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpstreamResponse")
            .field("status", &self.status)
            .field("header_count", &self.headers.len())
            .field("body", &self.body)
            .finish()
    }
}

fn validate_request_target(target: String) -> AdaptorResult<String> {
    if target.is_empty()
        || target.len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES
        || target.trim() != target
    {
        return Err(AdaptorError::InvalidRequestTarget);
    }
    let parsed = Url::parse(&target).map_err(|_| AdaptorError::InvalidRequestTarget)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.has_host()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AdaptorError::InvalidRequestTarget);
    }
    if parsed.as_str().len() > MAX_UPSTREAM_REQUEST_TARGET_BYTES {
        return Err(AdaptorError::InvalidRequestTarget);
    }
    Ok(parsed.into())
}

fn is_supported_method(method: &Method) -> bool {
    method == Method::GET
        || method == Method::POST
        || method == Method::PUT
        || method == Method::PATCH
        || method == Method::DELETE
        || method == Method::HEAD
        || method == Method::OPTIONS
}

fn validate_headers(headers: &HeaderMap) -> AdaptorResult<()> {
    if headers.len() > MAX_UPSTREAM_REQUEST_HEADER_COUNT {
        return Err(AdaptorError::InvalidHeader);
    }
    let mut total_bytes = 0_usize;
    for (name, value) in headers {
        if is_forbidden_request_header(name)
            || value.len() > MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES
        {
            return Err(AdaptorError::InvalidHeader);
        }
        total_bytes = total_bytes
            .checked_add(name.as_str().len())
            .and_then(|size| size.checked_add(value.len()))
            .ok_or(AdaptorError::InvalidHeader)?;
        if total_bytes > MAX_UPSTREAM_REQUEST_HEADERS_BYTES {
            return Err(AdaptorError::InvalidHeader);
        }
    }
    Ok(())
}

/// 判断 Header 是否会改变目标、消息分帧、代理认证或 Cookie 边界。
pub(crate) fn is_forbidden_request_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host"
            | "content-length"
            | "connection"
            | "proxy-connection"
            | "keep-alive"
            | "transfer-encoding"
            | "upgrade"
            | "te"
            | "trailer"
            | "proxy-authorization"
            | "cookie"
    )
}

fn validate_response_headers(headers: &HeaderMap) -> AdaptorResult<()> {
    if headers.len() > MAX_UPSTREAM_RESPONSE_HEADER_COUNT {
        return Err(AdaptorError::InvalidResponseHeader);
    }
    let mut total_bytes = 0_usize;
    for (name, value) in headers {
        if value.len() > MAX_UPSTREAM_RESPONSE_HEADER_VALUE_BYTES {
            return Err(AdaptorError::InvalidResponseHeader);
        }
        total_bytes = total_bytes
            .checked_add(name.as_str().len())
            .and_then(|size| size.checked_add(value.len()))
            .ok_or(AdaptorError::InvalidResponseHeader)?;
        if total_bytes > MAX_UPSTREAM_RESPONSE_HEADERS_BYTES {
            return Err(AdaptorError::InvalidResponseHeader);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, pin::Pin, task::Poll};

    use af_httpclient::HeaderValue;

    use super::*;

    #[test]
    fn request_rejects_unsafe_targets_and_large_bodies() {
        for target in [
            "ftp://upstream.example",
            "https://user:secret@upstream.example",
            "https://upstream.example/path#secret",
        ] {
            assert_eq!(
                UpstreamRequest::new(Method::POST, target, HeaderMap::new(), None).unwrap_err(),
                AdaptorError::InvalidRequestTarget
            );
        }
        assert_eq!(
            UpstreamRequest::new(
                Method::POST,
                "https://upstream.example",
                HeaderMap::new(),
                Some(Bytes::from(vec![0; MAX_UPSTREAM_REQUEST_BODY_BYTES + 1])),
            )
            .unwrap_err(),
            AdaptorError::RequestBodyTooLarge
        );

        let mut forbidden = HeaderMap::new();
        forbidden.insert(
            af_httpclient::HeaderName::from_static("host"),
            HeaderValue::from_static("attacker.invalid"),
        );
        assert_eq!(
            UpstreamRequest::new(Method::POST, "https://upstream.example", forbidden, None,)
                .unwrap_err(),
            AdaptorError::InvalidHeader
        );
    }

    #[test]
    fn request_and_response_debug_never_expose_payloads_or_header_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            af_httpclient::HeaderName::from_static("authorization"),
            HeaderValue::from_static("Bearer header-secret"),
        );
        let request = UpstreamRequest::new(
            Method::POST,
            "https://upstream.example/path?key=query-secret",
            headers.clone(),
            Some(Bytes::from_static(b"body-secret")),
        )
        .unwrap();
        let response = UpstreamResponse::full(
            StatusCode::OK,
            headers,
            Bytes::from_static(b"response-secret"),
        )
        .unwrap();

        let debug = format!("{request:?}\n{response:?}");
        for secret in [
            "upstream.example",
            "query-secret",
            "header-secret",
            "body-secret",
            "response-secret",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[tokio::test]
    async fn directly_constructed_full_body_still_obeys_collection_limit() {
        let body = UpstreamBody::Full(Bytes::from(vec![0; MAX_UPSTREAM_RESPONSE_BODY_BYTES + 1]));
        assert_eq!(
            body.into_bytes().await,
            Err(AdaptorError::ResponseBodyTooLarge)
        );
    }

    #[tokio::test]
    async fn explicit_large_response_budget_is_bounded_and_preserves_the_default() {
        let default = UpstreamRequest::new(
            Method::POST,
            "https://upstream.example",
            HeaderMap::new(),
            None,
        )
        .unwrap();
        assert_eq!(
            default.response_body_limit(),
            MAX_UPSTREAM_RESPONSE_BODY_BYTES
        );

        for invalid_limit in [0, MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES + 1] {
            assert_eq!(
                UpstreamRequest::new(
                    Method::POST,
                    "https://upstream.example",
                    HeaderMap::new(),
                    None,
                )
                .unwrap()
                .with_response_body_limit(invalid_limit)
                .unwrap_err(),
                AdaptorError::ResponseBodyTooLarge
            );
        }

        let request = UpstreamRequest::new(
            Method::POST,
            "https://upstream.example",
            HeaderMap::new(),
            None,
        )
        .unwrap()
        .with_response_body_limit(MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES)
        .unwrap();
        assert_eq!(
            request.response_body_limit(),
            MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES
        );

        let bytes = UpstreamBody::Full(Bytes::from(vec![0; MAX_UPSTREAM_RESPONSE_BODY_BYTES + 1]))
            .into_bytes_with_limit(MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES)
            .await
            .unwrap();
        assert_eq!(bytes.len(), MAX_UPSTREAM_RESPONSE_BODY_BYTES + 1);
    }

    #[test]
    fn request_defaults_to_full_and_allows_only_standard_methods() {
        for method in [
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::HEAD,
            Method::OPTIONS,
        ] {
            let request =
                UpstreamRequest::new(method, "https://upstream.example", HeaderMap::new(), None)
                    .unwrap();
            assert_eq!(request.response_mode(), ResponseMode::Full);
        }
        for method in [
            Method::CONNECT,
            Method::TRACE,
            Method::from_bytes(b"PURGE").unwrap(),
        ] {
            assert_eq!(
                UpstreamRequest::new(method, "https://upstream.example", HeaderMap::new(), None,)
                    .unwrap_err(),
                AdaptorError::UnsupportedRequestMethod
            );
        }
    }

    #[test]
    fn normalized_target_length_is_checked_after_url_encoding() {
        let target = format!("https://upstream.example/{}", "测".repeat(2_500));
        assert!(target.len() < MAX_UPSTREAM_REQUEST_TARGET_BYTES);
        assert_eq!(
            UpstreamRequest::new(Method::GET, target, HeaderMap::new(), None).unwrap_err(),
            AdaptorError::InvalidRequestTarget
        );
    }

    #[tokio::test]
    async fn custom_stream_preserves_errors_and_enforces_collection_limit() {
        let response = UpstreamResponse::stream(
            StatusCode::OK,
            HeaderMap::new(),
            TestStream::new(vec![
                Ok(Bytes::from_static(b"first")),
                Err(AdaptorError::InvalidHeader),
            ]),
        )
        .unwrap();
        assert_eq!(
            response.into_body().into_bytes().await,
            Err(AdaptorError::InvalidHeader)
        );

        let oversized = UpstreamBody::Stream(UpstreamBodyStream::new(TestStream::new(vec![
            Ok(Bytes::from(vec![0; MAX_UPSTREAM_RESPONSE_BODY_BYTES])),
            Ok(Bytes::from_static(b"overflow")),
        ])));
        assert_eq!(
            oversized.into_bytes().await,
            Err(AdaptorError::ResponseBodyTooLarge)
        );
    }

    #[tokio::test]
    async fn custom_stream_terminates_after_error_or_oversized_chunk() {
        let mut error_stream = UpstreamBodyStream::new(TestStream::new(vec![
            Err(AdaptorError::InvalidHeader),
            Ok(Bytes::from_static(b"must-not-be-read")),
        ]));
        assert_eq!(
            error_stream.next_chunk().await,
            Some(Err(AdaptorError::InvalidHeader))
        );
        assert_eq!(error_stream.next_chunk().await, None);

        let mut oversized_stream = UpstreamBodyStream::new(TestStream::new(vec![
            Ok(Bytes::from(vec![0; MAX_UPSTREAM_RESPONSE_CHUNK_BYTES + 1])),
            Ok(Bytes::from_static(b"must-not-be-read")),
        ]));
        assert_eq!(
            oversized_stream.next_chunk().await,
            Some(Err(AdaptorError::ResponseBodyTooLarge))
        );
        assert_eq!(oversized_stream.next_chunk().await, None);
    }

    #[test]
    fn response_headers_obey_count_value_and_total_budgets() {
        let mut too_many = HeaderMap::new();
        for index in 0..=MAX_UPSTREAM_RESPONSE_HEADER_COUNT {
            let name = af_httpclient::HeaderName::from_bytes(format!("x-test-{index}").as_bytes())
                .unwrap();
            too_many.insert(name, HeaderValue::from_static("value"));
        }
        assert_eq!(
            UpstreamResponse::full(StatusCode::OK, too_many, Bytes::new()).unwrap_err(),
            AdaptorError::InvalidResponseHeader
        );

        let mut too_large_value = HeaderMap::new();
        too_large_value.insert(
            af_httpclient::HeaderName::from_static("x-test"),
            HeaderValue::from_bytes(&vec![b'x'; MAX_UPSTREAM_RESPONSE_HEADER_VALUE_BYTES + 1])
                .unwrap(),
        );
        assert_eq!(
            UpstreamResponse::full(StatusCode::OK, too_large_value, Bytes::new()).unwrap_err(),
            AdaptorError::InvalidResponseHeader
        );

        let mut too_large_total = HeaderMap::new();
        for index in 0..17 {
            let name = af_httpclient::HeaderName::from_bytes(format!("x-total-{index}").as_bytes())
                .unwrap();
            too_large_total.insert(
                name,
                HeaderValue::from_bytes(&vec![b'x'; MAX_UPSTREAM_RESPONSE_HEADER_VALUE_BYTES])
                    .unwrap(),
            );
        }
        assert_eq!(
            UpstreamResponse::full(StatusCode::OK, too_large_total, Bytes::new()).unwrap_err(),
            AdaptorError::InvalidResponseHeader
        );
    }

    struct TestStream {
        items: VecDeque<AdaptorResult<Bytes>>,
    }

    impl TestStream {
        fn new(items: Vec<AdaptorResult<Bytes>>) -> Self {
            Self {
                items: items.into(),
            }
        }
    }

    impl Stream for TestStream {
        type Item = AdaptorResult<Bytes>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut std::task::Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.items.pop_front())
        }
    }
}
