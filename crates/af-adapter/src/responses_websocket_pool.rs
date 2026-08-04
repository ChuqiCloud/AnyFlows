use std::{
    cell::Cell,
    collections::HashMap,
    fmt,
    ops::{Deref, DerefMut},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use af_domain::{ChannelId, CredentialId};
use async_trait::async_trait;
use futures_util::stream;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::{
    sync::{Mutex as AsyncMutex, OwnedMutexGuard, Semaphore},
    time::{timeout, timeout_at},
};

use crate::{
    AdaptorError, AdaptorResult, Bytes, HeaderMap, HeaderName, HeaderValue,
    MAX_UPSTREAM_REQUEST_BODY_BYTES, MAX_UPSTREAM_RESPONSE_CHUNK_BYTES, Method, ResponseMode,
    StatusCode, UpstreamRequest, UpstreamResponse,
};

/// Responses WebSocket 连接最长使用时长；比官方 60 分钟硬上限提前轮换。
pub const MAX_RESPONSES_WEBSOCKET_CONNECTION_AGE: Duration = Duration::from_secs(55 * 60);
/// 单个下游会话标识的最大字节数；只用于计算摘要，不会持久化原文。
pub const MAX_RESPONSES_WEBSOCKET_SESSION_ID_BYTES: usize = 256;
/// 单轮最多接收的事件数，防止异常上游无限发送心跳或垃圾事件。
pub const MAX_RESPONSES_WEBSOCKET_EVENTS_PER_TURN: usize = 16 * 1024;

const MAX_POOL_SESSIONS: usize = 1_024;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
const DEFAULT_QUEUE_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_TURN_TIMEOUT: Duration = Duration::from_secs(900);
const MAX_WEBSOCKET_JSON_DEPTH: usize = 32;
const MAX_WEBSOCKET_JSON_NODES: usize = 100_000;
const MAX_WEBSOCKET_JSON_COLLECTION_ITEMS: usize = 4_096;
const MAX_WEBSOCKET_JSON_OBJECT_ENTRIES: usize = 1_024;
const MAX_WEBSOCKET_JSON_KEY_BYTES: usize = 1_024;

/// WebSocket 连接收到的已分类帧；控制帧由具体拨号器负责保持连接活性。
#[derive(Clone, Eq, PartialEq)]
pub enum ResponsesWebSocketFrame {
    /// UTF-8 JSON 文本帧。
    Text(Bytes),
    /// 不允许承载 Responses JSON 的二进制帧。
    Binary(Bytes),
    /// Ping 控制帧；连接实现可在底层自动回复。
    Ping,
    /// Pong 控制帧。
    Pong,
}

impl fmt::Debug for ResponsesWebSocketFrame {
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

/// 连接实现返回的脱敏传输错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ResponsesWebSocketTransportError {
    /// 建连或握手失败。
    #[error("Responses WebSocket 建连失败")]
    Connect,
    /// 文本帧发送失败。
    #[error("Responses WebSocket 请求发送失败")]
    Send,
    /// 帧读取失败。
    #[error("Responses WebSocket 读取失败")]
    Receive,
    /// 连接已由上游或本地关闭。
    #[error("Responses WebSocket 连接已关闭")]
    Closed,
}

/// 由连接池自身产生的稳定错误分类；不携带 URL、Header、正文或凭据。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum ResponsesWebSocketPoolError {
    /// 会话标识为空、超长或包含控制字符。
    #[error("Responses WebSocket 会话标识无效")]
    InvalidSession,
    /// WebSocket 目标不是安全的 HTTP(S) 上游目标。
    #[error("Responses WebSocket 握手目标无效")]
    InvalidHandshakeTarget,
    /// Header 不在显式允许的握手白名单内。
    #[error("Responses WebSocket 握手 Header 不受支持")]
    InvalidHandshakeHeader,
    /// 请求不是有效的 Responses JSON 对象或携带禁止字段。
    #[error("Responses WebSocket 请求无效")]
    InvalidRequest,
    /// 连接池容量已满且没有可回收的空闲会话。
    #[error("Responses WebSocket 连接池已满")]
    PoolSaturated,
    /// 等待同一会话的在途请求超过队列期限。
    #[error("Responses WebSocket 会话排队超时")]
    QueueTimeout,
    /// 握手超过连接期限。
    #[error("Responses WebSocket 建连超时")]
    ConnectTimeout,
    /// 当前 Responses 轮次超过总期限。
    #[error("Responses WebSocket 轮次超时")]
    TurnTimeout,
    /// 上游帧无法表达为受支持的 Responses 事件。
    #[error("Responses WebSocket 事件无效")]
    Protocol,
    /// 上游在终态前关闭连接。
    #[error("Responses WebSocket 在终态前关闭")]
    ConnectionClosed,
    /// 连接池内部锁已损坏。
    #[error("Responses WebSocket 连接池不可用")]
    PoolUnavailable,
    /// 连接实现报告了脱敏传输错误。
    #[error("Responses WebSocket 传输失败")]
    Transport(#[source] ResponsesWebSocketTransportError),
}

impl ResponsesWebSocketPoolError {
    /// 转换为现有适配层错误，供未来 transport dispatcher 接入 Relay。
    #[must_use]
    pub const fn into_adaptor_error(self) -> AdaptorError {
        let transport = match self {
            Self::ConnectTimeout => crate::AdaptorTransportError::ConnectTimeout,
            Self::TurnTimeout | Self::QueueTimeout => crate::AdaptorTransportError::RequestTimeout,
            Self::Transport(ResponsesWebSocketTransportError::Connect) => {
                crate::AdaptorTransportError::Connect
            }
            Self::Transport(ResponsesWebSocketTransportError::Send) => {
                crate::AdaptorTransportError::Request
            }
            Self::Transport(ResponsesWebSocketTransportError::Receive) | Self::ConnectionClosed => {
                crate::AdaptorTransportError::ResponseBody
            }
            Self::Transport(ResponsesWebSocketTransportError::Closed) => {
                crate::AdaptorTransportError::Connect
            }
            Self::Protocol => crate::AdaptorTransportError::ResponseBody,
            Self::PoolSaturated | Self::PoolUnavailable => crate::AdaptorTransportError::Request,
            Self::InvalidSession
            | Self::InvalidHandshakeTarget
            | Self::InvalidHandshakeHeader
            | Self::InvalidRequest => crate::AdaptorTransportError::InvalidRequestTarget,
        };
        AdaptorError::Transport(transport)
    }
}

/// 连接池容量、生命周期和超时配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponsesWebSocketPoolConfig {
    max_sessions: usize,
    max_connection_age: Duration,
    idle_timeout: Duration,
    queue_timeout: Duration,
    connect_timeout: Duration,
    turn_timeout: Duration,
}

impl ResponsesWebSocketPoolConfig {
    /// 创建经过官方连接上限和本地容量边界校验的配置。
    pub fn new(
        max_sessions: usize,
        max_connection_age: Duration,
        idle_timeout: Duration,
        queue_timeout: Duration,
        connect_timeout: Duration,
        turn_timeout: Duration,
    ) -> Result<Self, ResponsesWebSocketPoolError> {
        if !(1..=MAX_POOL_SESSIONS).contains(&max_sessions)
            || max_connection_age.is_zero()
            || max_connection_age > MAX_RESPONSES_WEBSOCKET_CONNECTION_AGE
            || idle_timeout.is_zero()
            || idle_timeout > max_connection_age
            || queue_timeout.is_zero()
            || queue_timeout > max_connection_age
            || connect_timeout.is_zero()
            || connect_timeout > max_connection_age
            || turn_timeout.is_zero()
            || turn_timeout > max_connection_age
        {
            return Err(ResponsesWebSocketPoolError::InvalidRequest);
        }
        Ok(Self {
            max_sessions,
            max_connection_age,
            idle_timeout,
            queue_timeout,
            connect_timeout,
            turn_timeout,
        })
    }

    /// 返回会话池最大并存会话数。
    #[must_use]
    pub const fn max_sessions(self) -> usize {
        self.max_sessions
    }

    /// 返回主动轮换期限。
    #[must_use]
    pub const fn max_connection_age(self) -> Duration {
        self.max_connection_age
    }

    /// 返回空闲连接回收期限。
    #[must_use]
    pub const fn idle_timeout(self) -> Duration {
        self.idle_timeout
    }

    /// 返回同一会话排队期限。
    #[must_use]
    pub const fn queue_timeout(self) -> Duration {
        self.queue_timeout
    }

    /// 返回握手期限。
    #[must_use]
    pub const fn connect_timeout(self) -> Duration {
        self.connect_timeout
    }

    /// 返回单轮总期限。
    #[must_use]
    pub const fn turn_timeout(self) -> Duration {
        self.turn_timeout
    }
}

impl Default for ResponsesWebSocketPoolConfig {
    fn default() -> Self {
        Self::new(
            64,
            MAX_RESPONSES_WEBSOCKET_CONNECTION_AGE,
            DEFAULT_IDLE_TIMEOUT,
            DEFAULT_QUEUE_TIMEOUT,
            DEFAULT_CONNECT_TIMEOUT,
            DEFAULT_TURN_TIMEOUT,
        )
        .expect("默认 Responses WebSocket 连接池配置必须有效")
    }
}

/// 绑定渠道、凭据版本和下游会话的不可逆池键。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct ResponsesWebSocketPoolKey {
    channel_id: ChannelId,
    credential_id: CredentialId,
    credential_revision: u64,
    session_digest: [u8; 32],
}

impl ResponsesWebSocketPoolKey {
    /// 使用会话原文计算摘要；原文不会进入池状态、Debug 或日志。
    pub fn new(
        channel_id: ChannelId,
        credential_id: CredentialId,
        credential_revision: u64,
        session_id: &str,
    ) -> Result<Self, ResponsesWebSocketPoolError> {
        if session_id.is_empty()
            || session_id.len() > MAX_RESPONSES_WEBSOCKET_SESSION_ID_BYTES
            || session_id.trim() != session_id
            || !session_id.is_ascii()
            || session_id.bytes().any(|byte| byte.is_ascii_control())
        {
            return Err(ResponsesWebSocketPoolError::InvalidSession);
        }
        let mut digest = Sha256::new();
        digest.update(b"AnyFlows:responses-websocket-session:v1\0");
        digest.update(channel_id.get().to_be_bytes());
        digest.update(credential_id.get().to_be_bytes());
        digest.update(credential_revision.to_be_bytes());
        digest.update(
            u32::try_from(session_id.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        digest.update(session_id.as_bytes());
        let digest = digest.finalize();
        let mut session_digest = [0_u8; 32];
        session_digest.copy_from_slice(&digest);
        Ok(Self {
            channel_id,
            credential_id,
            credential_revision,
            session_digest,
        })
    }

    /// 返回绑定的渠道标识。
    #[must_use]
    pub const fn channel_id(self) -> ChannelId {
        self.channel_id
    }

    /// 返回绑定的凭据标识。
    #[must_use]
    pub const fn credential_id(self) -> CredentialId {
        self.credential_id
    }

    /// 返回 OAuth/凭据版本；轮换后旧连接不能继续复用。
    #[must_use]
    pub const fn credential_revision(self) -> u64 {
        self.credential_revision
    }
}

impl fmt::Debug for ResponsesWebSocketPoolKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponsesWebSocketPoolKey")
            .field("channel_id", &self.channel_id)
            .field("credential_id", &self.credential_id)
            .field("credential_revision", &"<已脱敏>")
            .field("session_digest", &"<已脱敏>")
            .finish()
    }
}

/// WebSocket 握手所需的安全目标和白名单 Header。
#[derive(Clone)]
pub struct ResponsesWebSocketHandshake {
    target: String,
    headers: HeaderMap,
    fingerprint: [u8; 32],
}

impl ResponsesWebSocketHandshake {
    /// 从已完成 HTTP 请求边界校验的请求构造握手；逐跳和正文 Header 不会透传。
    pub fn from_request(request: &UpstreamRequest) -> Result<Self, ResponsesWebSocketPoolError> {
        if request.method() != Method::POST {
            return Err(ResponsesWebSocketPoolError::InvalidRequest);
        }
        let mut parsed = url::Url::parse(request.target())
            .map_err(|_| ResponsesWebSocketPoolError::InvalidHandshakeTarget)?;
        if !matches!(parsed.scheme(), "http" | "https")
            || !parsed.has_host()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(ResponsesWebSocketPoolError::InvalidHandshakeTarget);
        }
        let websocket_scheme = if parsed.scheme() == "https" {
            "wss"
        } else {
            "ws"
        };
        parsed
            .set_scheme(websocket_scheme)
            .map_err(|_| ResponsesWebSocketPoolError::InvalidHandshakeTarget)?;
        let target: String = parsed.into();

        let mut headers = HeaderMap::new();
        for (name, value) in request.headers() {
            if is_http_only_handshake_header(name) {
                continue;
            }
            if !is_allowed_handshake_header(name) || headers.contains_key(name) {
                return Err(ResponsesWebSocketPoolError::InvalidHandshakeHeader);
            }
            headers.insert(name.clone(), value.clone());
        }
        let fingerprint = handshake_fingerprint(&target, &headers);
        Ok(Self {
            target,
            headers,
            fingerprint,
        })
    }

    /// 返回握手目标；调用方不得记录该值。
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// 返回白名单 Header；认证值只能交给拨号器短暂使用。
    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub(crate) fn into_parts(self) -> (String, HeaderMap) {
        (self.target, self.headers)
    }

    fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }
}

impl fmt::Debug for ResponsesWebSocketHandshake {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponsesWebSocketHandshake")
            .field("target", &"<已脱敏>")
            .field("header_count", &self.headers.len())
            .field("fingerprint", &"<已脱敏>")
            .finish()
    }
}

/// Responses WebSocket 连接的最小对象安全契约。
#[async_trait]
pub trait ResponsesWebSocketConnection: Send {
    /// 发送一个 `response.create` JSON 事件。
    async fn send_text(&mut self, payload: Bytes) -> Result<(), ResponsesWebSocketTransportError>;

    /// 接收下一帧；`None` 表示连接关闭。
    async fn receive(
        &mut self,
    ) -> Result<Option<ResponsesWebSocketFrame>, ResponsesWebSocketTransportError>;

    /// 尽力关闭连接；调用方不应依赖关闭错误覆盖业务结论。
    async fn close(&mut self);
}

/// 由网络层实现的受控 Responses WebSocket 拨号器。
#[async_trait]
pub trait ResponsesWebSocketConnector: Send + Sync {
    /// 使用已校验握手建立一条独立连接；不得在实现中记录 Header 或目标。
    async fn connect(
        &self,
        handshake: ResponsesWebSocketHandshake,
    ) -> Result<Box<dyn ResponsesWebSocketConnection>, ResponsesWebSocketTransportError>;
}

struct SessionState {
    connection: Option<Box<dyn ResponsesWebSocketConnection>>,
    handshake_fingerprint: Option<[u8; 32]>,
    created_at: Option<Instant>,
    last_used: Instant,
}

struct SessionEntry {
    state: Arc<AsyncMutex<SessionState>>,
    abandoned: Arc<AtomicBool>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// 持有单轮会话锁；调用方取消时只标记连接，下一轮取得锁后再执行异步关闭。
struct SessionLease {
    guard: OwnedMutexGuard<SessionState>,
    abandoned: Arc<AtomicBool>,
    finished: bool,
}

impl SessionLease {
    fn new(guard: OwnedMutexGuard<SessionState>, abandoned: Arc<AtomicBool>) -> Self {
        Self {
            guard,
            abandoned,
            finished: false,
        }
    }

    fn finish(&mut self) {
        self.finished = true;
        self.abandoned.store(false, Ordering::Release);
    }

    async fn reclaim_abandoned(&mut self, timeout_duration: Duration) {
        if self.abandoned.swap(false, Ordering::AcqRel) {
            close_connection(&mut self.guard, timeout_duration).await;
        }
    }
}

impl Deref for SessionLease {
    type Target = SessionState;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl DerefMut for SessionLease {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        if !self.finished {
            self.abandoned.store(true, Ordering::Release);
        }
    }
}

struct PoolInner {
    config: ResponsesWebSocketPoolConfig,
    connector: Arc<dyn ResponsesWebSocketConnector>,
    entries: Mutex<HashMap<ResponsesWebSocketPoolKey, Arc<SessionEntry>>>,
    slots: Arc<Semaphore>,
}

/// 按凭据版本和下游会话隔离的 Responses WebSocket 连接池。
#[derive(Clone)]
pub struct ResponsesWebSocketPool {
    inner: Arc<PoolInner>,
}

impl ResponsesWebSocketPool {
    /// 创建独立连接池；连接建立由注入的受控拨号器负责。
    pub fn new(
        config: ResponsesWebSocketPoolConfig,
        connector: Arc<dyn ResponsesWebSocketConnector>,
    ) -> Self {
        Self {
            inner: Arc::new(PoolInner {
                slots: Arc::new(Semaphore::new(config.max_sessions)),
                config,
                connector,
                entries: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// 发送一轮 Responses 请求；流式模式返回可背压的 SSE 事件流。
    pub async fn execute(
        &self,
        key: ResponsesWebSocketPoolKey,
        request: &UpstreamRequest,
    ) -> Result<UpstreamResponse, ResponsesWebSocketPoolError> {
        let event = build_responses_websocket_event(
            request
                .body()
                .ok_or(ResponsesWebSocketPoolError::InvalidRequest)?,
        )?;
        let response_mode = request.response_mode();
        let handshake = ResponsesWebSocketHandshake::from_request(request)?;
        let entry = self.entry_for(key).await?;
        let guard = timeout(
            self.inner.config.queue_timeout,
            Arc::clone(&entry.state).lock_owned(),
        )
        .await
        .map_err(|_| ResponsesWebSocketPoolError::QueueTimeout)?;
        let mut lease = SessionLease::new(guard, Arc::clone(&entry.abandoned));
        lease
            .reclaim_abandoned(self.inner.config.connect_timeout)
            .await;

        if let Err(error) = self.prepare_connection(&mut lease, &handshake).await {
            close_connection(&mut lease, self.inner.config.connect_timeout).await;
            lease.finish();
            return Err(error);
        }
        let deadline = Instant::now() + self.inner.config.turn_timeout;
        let send_result = match lease.connection.as_mut() {
            Some(connection) => timeout_at(deadline.into(), connection.send_text(event))
                .await
                .map_err(|_| ResponsesWebSocketPoolError::TurnTimeout)
                .and_then(|result| result.map_err(ResponsesWebSocketPoolError::Transport)),
            None => Err(ResponsesWebSocketPoolError::ConnectionClosed),
        };
        if let Err(error) = send_result {
            close_connection(&mut lease, self.inner.config.connect_timeout).await;
            lease.finish();
            return Err(error);
        }

        match response_mode {
            ResponseMode::Full => {
                let result = self.collect_full(&mut lease, deadline).await;
                lease.finish();
                result
            }
            ResponseMode::Stream => self.start_stream(lease, deadline),
        }
    }

    /// 主动关闭指定会话；凭据轮换或管理员停用时应调用。
    pub async fn close_session(&self, key: ResponsesWebSocketPoolKey) {
        let entry = self
            .inner
            .entries
            .lock()
            .ok()
            .and_then(|mut entries| entries.remove(&key));
        if let Some(entry) = entry {
            let mut state = entry.state.lock().await;
            close_connection(&mut state, self.inner.config.connect_timeout).await;
        }
    }

    /// 关闭指定凭据版本下的全部会话；OAuth 轮换完成后用于释放旧连接状态。
    pub async fn close_credential_revision(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
        credential_revision: u64,
    ) {
        let entries = self
            .inner
            .entries
            .lock()
            .map(|mut entries| {
                let keys = entries
                    .keys()
                    .filter(|key| {
                        key.channel_id == channel_id
                            && key.credential_id == credential_id
                            && key.credential_revision == credential_revision
                    })
                    .copied()
                    .collect::<Vec<_>>();
                keys.into_iter()
                    .filter_map(|key| entries.remove(&key))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for entry in entries {
            let mut state = entry.state.lock().await;
            close_connection(&mut state, self.inner.config.connect_timeout).await;
        }
    }

    /// 关闭全部空闲或在途会话；服务关闭阶段应在排空 HTTP 后调用。
    pub async fn close_all(&self) {
        let entries = self
            .inner
            .entries
            .lock()
            .map(|mut entries| entries.drain().map(|(_, entry)| entry).collect::<Vec<_>>())
            .unwrap_or_default();
        for entry in entries {
            let mut state = entry.state.lock().await;
            close_connection(&mut state, self.inner.config.connect_timeout).await;
        }
    }

    /// 回收无在途请求且已超过空闲/最大寿命的连接。
    pub async fn prune_expired(&self) {
        let now = Instant::now();
        let expired = self
            .inner
            .entries
            .lock()
            .ok()
            .map(|mut entries| {
                let keys = entries
                    .iter()
                    .filter_map(|(key, entry)| {
                        if Arc::strong_count(entry) != 1 {
                            return None;
                        }
                        let state = entry.state.try_lock().ok()?;
                        let idle =
                            now.duration_since(state.last_used) >= self.inner.config.idle_timeout;
                        let aged = state.created_at.is_some_and(|created| {
                            now.duration_since(created) >= self.inner.config.max_connection_age
                        });
                        (idle || aged).then_some(*key)
                    })
                    .collect::<Vec<_>>();
                keys.into_iter()
                    .filter_map(|key| entries.remove(&key))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for entry in expired {
            let mut state = entry.state.lock().await;
            close_connection(&mut state, self.inner.config.connect_timeout).await;
        }
    }

    /// 返回当前池中的会话数量；只用于运维指标和测试。
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.inner
            .entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or_default()
    }

    async fn entry_for(
        &self,
        key: ResponsesWebSocketPoolKey,
    ) -> Result<Arc<SessionEntry>, ResponsesWebSocketPoolError> {
        self.prune_expired().await;
        if let Some(entry) = self
            .inner
            .entries
            .lock()
            .map_err(|_| ResponsesWebSocketPoolError::PoolUnavailable)?
            .get(&key)
            .cloned()
        {
            return Ok(entry);
        }

        let permit = timeout(
            self.inner.config.queue_timeout,
            Arc::clone(&self.inner.slots).acquire_owned(),
        )
        .await
        .map_err(|_| ResponsesWebSocketPoolError::PoolSaturated)?
        .map_err(|_| ResponsesWebSocketPoolError::PoolUnavailable)?;
        let mut entries = self
            .inner
            .entries
            .lock()
            .map_err(|_| ResponsesWebSocketPoolError::PoolUnavailable)?;
        if let Some(entry) = entries.get(&key).cloned() {
            drop(permit);
            return Ok(entry);
        }
        let entry = Arc::new(SessionEntry {
            state: Arc::new(AsyncMutex::new(SessionState {
                connection: None,
                handshake_fingerprint: None,
                created_at: None,
                last_used: Instant::now(),
            })),
            abandoned: Arc::new(AtomicBool::new(false)),
            _permit: permit,
        });
        entries.insert(key, Arc::clone(&entry));
        Ok(entry)
    }

    async fn prepare_connection(
        &self,
        state: &mut SessionState,
        handshake: &ResponsesWebSocketHandshake,
    ) -> Result<(), ResponsesWebSocketPoolError> {
        let now = Instant::now();
        let stale = state.connection.is_some()
            && (state.created_at.is_some_and(|created| {
                now.duration_since(created) >= self.inner.config.max_connection_age
            }) || now.duration_since(state.last_used) >= self.inner.config.idle_timeout
                || state.handshake_fingerprint != Some(handshake.fingerprint()));
        if stale {
            close_connection(state, self.inner.config.connect_timeout).await;
        }
        if state.connection.is_none() {
            let connection = timeout(
                self.inner.config.connect_timeout,
                self.inner.connector.connect(handshake.clone()),
            )
            .await
            .map_err(|_| ResponsesWebSocketPoolError::ConnectTimeout)?
            .map_err(ResponsesWebSocketPoolError::Transport)?;
            state.connection = Some(connection);
            state.handshake_fingerprint = Some(handshake.fingerprint());
            // 建连耗时不应计入连接可复用寿命，从握手成功后开始计时。
            state.created_at = Some(Instant::now());
        }
        state.last_used = Instant::now();
        Ok(())
    }

    async fn collect_full(
        &self,
        state: &mut SessionState,
        deadline: Instant,
    ) -> Result<UpstreamResponse, ResponsesWebSocketPoolError> {
        for _ in 0..MAX_RESPONSES_WEBSOCKET_EVENTS_PER_TURN {
            let frame = match receive_event(state, deadline).await {
                Ok(frame) => frame,
                Err(error) => {
                    close_connection(state, self.inner.config.connect_timeout).await;
                    return Err(error);
                }
            };
            if is_control_frame(&frame) {
                continue;
            }
            let (_event, event_type, value) = match normalize_event(frame) {
                Ok(event) => event,
                Err(error) => {
                    close_connection(state, self.inner.config.connect_timeout).await;
                    return Err(error);
                }
            };
            if !is_terminal_event(event_type.as_str()) {
                continue;
            }
            if event_type == "error" {
                close_connection(state, self.inner.config.connect_timeout).await;
                return Err(ResponsesWebSocketPoolError::Protocol);
            }
            let response = match value.get("response").cloned() {
                Some(response) => response,
                None => {
                    close_connection(state, self.inner.config.connect_timeout).await;
                    return Err(ResponsesWebSocketPoolError::Protocol);
                }
            };
            let body = match serde_json::to_vec(&response) {
                Ok(body) => body,
                Err(_) => {
                    close_connection(state, self.inner.config.connect_timeout).await;
                    return Err(ResponsesWebSocketPoolError::Protocol);
                }
            };
            let result =
                UpstreamResponse::full(StatusCode::OK, HeaderMap::new(), Bytes::from(body))
                    .map_err(|_| ResponsesWebSocketPoolError::Protocol);
            if event_type == "response.failed" {
                close_connection(state, self.inner.config.connect_timeout).await;
            } else if result.is_ok() {
                state.last_used = Instant::now();
            } else {
                close_connection(state, self.inner.config.connect_timeout).await;
            }
            return result;
        }
        close_connection(state, self.inner.config.connect_timeout).await;
        Err(ResponsesWebSocketPoolError::Protocol)
    }

    fn start_stream(
        &self,
        lease: SessionLease,
        deadline: Instant,
    ) -> Result<UpstreamResponse, ResponsesWebSocketPoolError> {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        let config = self.inner.config;
        tokio::spawn(async move {
            pump_stream(lease, sender, deadline, config).await;
        });
        let body = stream::unfold(receiver, |mut receiver| async {
            receiver.recv().await.map(|item| (item, receiver))
        });
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(
            HeaderName::from_static("cache-control"),
            HeaderValue::from_static("no-cache"),
        );
        UpstreamResponse::stream(StatusCode::OK, headers, body)
            .map_err(|_| ResponsesWebSocketPoolError::Protocol)
    }
}

impl fmt::Debug for ResponsesWebSocketPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponsesWebSocketPool")
            .field("session_count", &self.session_count())
            .field("max_sessions", &self.inner.config.max_sessions)
            .field("max_connection_age", &self.inner.config.max_connection_age)
            .finish()
    }
}

/// 构造符合官方 WebSocket `response.create` 事件的请求正文。
pub fn build_responses_websocket_event(body: &Bytes) -> Result<Bytes, ResponsesWebSocketPoolError> {
    if body.is_empty() || body.len() > MAX_UPSTREAM_REQUEST_BODY_BYTES {
        return Err(ResponsesWebSocketPoolError::InvalidRequest);
    }
    let mut value =
        parse_strict_json(body).map_err(|_| ResponsesWebSocketPoolError::InvalidRequest)?;
    let object = value
        .as_object_mut()
        .ok_or(ResponsesWebSocketPoolError::InvalidRequest)?;
    if let Some(event_type) = object.get("type")
        && event_type.as_str() != Some("response.create")
    {
        return Err(ResponsesWebSocketPoolError::InvalidRequest);
    }
    for field in ["background", "store", "stream"] {
        if object.get(field).is_some_and(|value| !value.is_boolean()) {
            return Err(ResponsesWebSocketPoolError::InvalidRequest);
        }
    }
    if object.get("background").and_then(Value::as_bool) == Some(true)
        || object.get("store").and_then(Value::as_bool) == Some(true)
    {
        return Err(ResponsesWebSocketPoolError::InvalidRequest);
    }
    object.remove("background");
    object.remove("stream");
    object.insert("store".to_owned(), serde_json::Value::Bool(false));
    object.insert(
        "type".to_owned(),
        serde_json::Value::String("response.create".to_owned()),
    );
    let encoded =
        serde_json::to_vec(&value).map_err(|_| ResponsesWebSocketPoolError::InvalidRequest)?;
    if encoded.len() > MAX_UPSTREAM_REQUEST_BODY_BYTES {
        return Err(ResponsesWebSocketPoolError::InvalidRequest);
    }
    Ok(Bytes::from(encoded))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StrictJsonError {
    Invalid,
}

/// 使用对象访问器保留重复键校验；`serde_json::Value` 默认会覆盖重复键。
fn parse_strict_json(input: &[u8]) -> Result<Value, StrictJsonError> {
    let state = StrictJsonState::default();
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = StrictJsonSeed {
        state: &state,
        depth: 0,
    }
    .deserialize(&mut deserializer)
    .map_err(|_| StrictJsonError::Invalid)?;
    deserializer.end().map_err(|_| StrictJsonError::Invalid)?;
    Ok(value)
}

#[derive(Default)]
struct StrictJsonState {
    nodes: Cell<usize>,
}

impl StrictJsonState {
    fn begin_value<E>(&self, depth: usize) -> Result<(), E>
    where
        E: de::Error,
    {
        let nodes = self
            .nodes
            .get()
            .checked_add(1)
            .ok_or_else(|| E::custom("JSON 结构超过预算"))?;
        if depth > MAX_WEBSOCKET_JSON_DEPTH || nodes > MAX_WEBSOCKET_JSON_NODES {
            return Err(E::custom("JSON 结构超过预算"));
        }
        self.nodes.set(nodes);
        Ok(())
    }
}

struct StrictJsonSeed<'a> {
    state: &'a StrictJsonState,
    depth: usize,
}

impl<'de> DeserializeSeed<'de> for StrictJsonSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        self.state.begin_value(self.depth)?;
        deserializer.deserialize_any(StrictJsonVisitor {
            state: self.state,
            depth: self.depth,
        })
    }
}

struct StrictJsonVisitor<'a> {
    state: &'a StrictJsonState,
    depth: usize,
}

impl<'de> Visitor<'de> for StrictJsonVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("合法 JSON 值")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(Number::from(value)))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| E::custom("JSON 数字无效"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(value)
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(self.child_seed::<A::Error>()?)? {
            if values.len() >= MAX_WEBSOCKET_JSON_COLLECTION_ITEMS {
                return Err(de::Error::custom("JSON 数组超过预算"));
            }
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.len() >= MAX_WEBSOCKET_JSON_OBJECT_ENTRIES
                || key.len() > MAX_WEBSOCKET_JSON_KEY_BYTES
            {
                return Err(de::Error::custom("JSON 对象超过预算"));
            }
            if values.contains_key(&key) {
                return Err(de::Error::custom("JSON 对象包含重复键"));
            }
            let value = object.next_value_seed(self.child_seed::<A::Error>()?)?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

impl StrictJsonVisitor<'_> {
    fn child_seed<E>(&self) -> Result<StrictJsonSeed<'_>, E>
    where
        E: de::Error,
    {
        let depth = self
            .depth
            .checked_add(1)
            .ok_or_else(|| E::custom("JSON 结构超过预算"))?;
        Ok(StrictJsonSeed {
            state: self.state,
            depth,
        })
    }
}

fn is_allowed_handshake_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "authorization"
            | "chatgpt-account-id"
            | "openai-beta"
            | "openai-organization"
            | "openai-project"
            | "user-agent"
            | "originator"
            | "version"
            | "session_id"
            | "session-id"
            | "conversation_id"
            | "conversation-id"
            | "x-codex-beta-features"
            | "x-codex-turn-metadata"
            | "x-codex-turn-state"
            | "x-request-id"
    )
}

fn is_http_only_handshake_header(name: &HeaderName) -> bool {
    matches!(name.as_str(), "accept" | "content-type")
}

fn handshake_fingerprint(target: &str, headers: &HeaderMap) -> [u8; 32] {
    let mut values = headers
        .iter()
        .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
        .collect::<Vec<_>>();
    values.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    let mut digest = Sha256::new();
    digest.update(b"AnyFlows:responses-websocket-handshake:v1\0");
    digest.update(target.as_bytes());
    for (name, value) in values {
        digest.update(u32::try_from(name.len()).unwrap_or(u32::MAX).to_be_bytes());
        digest.update(name.as_bytes());
        digest.update(u32::try_from(value.len()).unwrap_or(u32::MAX).to_be_bytes());
        digest.update(value);
    }
    let output = digest.finalize();
    let mut fingerprint = [0_u8; 32];
    fingerprint.copy_from_slice(&output);
    fingerprint
}

async fn close_connection(state: &mut SessionState, timeout_duration: Duration) {
    if let Some(mut connection) = state.connection.take() {
        let _ = timeout(timeout_duration, connection.close()).await;
    }
    state.handshake_fingerprint = None;
    state.created_at = None;
    state.last_used = Instant::now();
}

async fn receive_event(
    state: &mut SessionState,
    deadline: Instant,
) -> Result<ResponsesWebSocketFrame, ResponsesWebSocketPoolError> {
    let connection = state
        .connection
        .as_mut()
        .ok_or(ResponsesWebSocketPoolError::ConnectionClosed)?;
    let frame = timeout_at(deadline.into(), connection.receive())
        .await
        .map_err(|_| ResponsesWebSocketPoolError::TurnTimeout)?
        .map_err(ResponsesWebSocketPoolError::Transport)?
        .ok_or(ResponsesWebSocketPoolError::ConnectionClosed)?;
    Ok(frame)
}

fn normalize_event(
    frame: ResponsesWebSocketFrame,
) -> Result<(Bytes, String, serde_json::Value), ResponsesWebSocketPoolError> {
    let ResponsesWebSocketFrame::Text(raw) = frame else {
        return Err(ResponsesWebSocketPoolError::Protocol);
    };
    if raw.is_empty() || raw.len() > MAX_UPSTREAM_RESPONSE_CHUNK_BYTES {
        return Err(ResponsesWebSocketPoolError::Protocol);
    }
    let value = parse_strict_json(&raw).map_err(|_| ResponsesWebSocketPoolError::Protocol)?;
    let event_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(ResponsesWebSocketPoolError::Protocol)?
        .to_owned();
    // SSE 的单个 data 行不能直接承载格式化 JSON 中的换行，需压缩为单行后再包装。
    let normalized =
        serde_json::to_vec(&value).map_err(|_| ResponsesWebSocketPoolError::Protocol)?;
    Ok((Bytes::from(normalized), event_type, value))
}

fn is_control_frame(frame: &ResponsesWebSocketFrame) -> bool {
    matches!(
        frame,
        ResponsesWebSocketFrame::Ping | ResponsesWebSocketFrame::Pong
    )
}

fn is_terminal_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "response.completed" | "response.failed" | "response.incomplete" | "error"
    )
}

async fn pump_stream(
    mut lease: SessionLease,
    sender: tokio::sync::mpsc::Sender<AdaptorResult<Bytes>>,
    deadline: Instant,
    config: ResponsesWebSocketPoolConfig,
) {
    for _ in 0..MAX_RESPONSES_WEBSOCKET_EVENTS_PER_TURN {
        let frame = match receive_event(&mut lease, deadline).await {
            Ok(frame) => frame,
            Err(error) => {
                close_connection(&mut lease, config.connect_timeout).await;
                lease.finish();
                let _ = send_stream_item(&sender, Err(error.into_adaptor_error()), deadline).await;
                return;
            }
        };
        if is_control_frame(&frame) {
            continue;
        }
        let (event, event_type, _value) = match normalize_event(frame) {
            Ok(event) => event,
            Err(error) => {
                close_connection(&mut lease, config.connect_timeout).await;
                lease.finish();
                let _ = send_stream_item(&sender, Err(error.into_adaptor_error()), deadline).await;
                return;
            }
        };
        let mut framed = Vec::with_capacity(event.len() + 8);
        framed.extend_from_slice(b"data: ");
        framed.extend_from_slice(&event);
        framed.extend_from_slice(b"\n\n");
        if !send_stream_item(&sender, Ok(Bytes::from(framed)), deadline).await {
            // 下游取消后不能把仍可能属于上一轮的帧留给下一轮。
            close_connection(&mut lease, config.connect_timeout).await;
            lease.finish();
            return;
        }
        if is_terminal_event(&event_type) {
            if matches!(event_type.as_str(), "error" | "response.failed") {
                close_connection(&mut lease, config.connect_timeout).await;
            } else {
                lease.last_used = Instant::now();
            }
            lease.finish();
            return;
        }
    }
    close_connection(&mut lease, config.connect_timeout).await;
    lease.finish();
    let _ = send_stream_item(
        &sender,
        Err(ResponsesWebSocketPoolError::Protocol.into_adaptor_error()),
        deadline,
    )
    .await;
}

/// 对下游背压同样执行单轮截止时间，避免无人消费的流永久占用会话锁。
async fn send_stream_item(
    sender: &tokio::sync::mpsc::Sender<AdaptorResult<Bytes>>,
    item: AdaptorResult<Bytes>,
    deadline: Instant,
) -> bool {
    timeout_at(deadline.into(), sender.send(item))
        .await
        .is_ok_and(|result| result.is_ok())
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    use serde_json::{Value, json};
    use tokio::sync::Notify;

    use super::*;

    #[derive(Clone)]
    struct FakeConnector {
        scripts: Arc<Mutex<VecDeque<Vec<ResponsesWebSocketFrame>>>>,
        connects: Arc<AtomicUsize>,
        sent: Arc<Mutex<Vec<Bytes>>>,
        blocked: Arc<Notify>,
        block_first_receive: Arc<AtomicBool>,
        started: Arc<Notify>,
    }

    struct FakeConnection {
        incoming: VecDeque<ResponsesWebSocketFrame>,
        sent: Arc<Mutex<Vec<Bytes>>>,
        blocked: Arc<Notify>,
        started: Arc<Notify>,
        block_receive: bool,
    }

    #[async_trait]
    impl ResponsesWebSocketConnector for FakeConnector {
        async fn connect(
            &self,
            _handshake: ResponsesWebSocketHandshake,
        ) -> Result<Box<dyn ResponsesWebSocketConnection>, ResponsesWebSocketTransportError>
        {
            self.connects.fetch_add(1, Ordering::SeqCst);
            let script = self.scripts.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::new(FakeConnection {
                incoming: script.into(),
                sent: Arc::clone(&self.sent),
                blocked: Arc::clone(&self.blocked),
                started: Arc::clone(&self.started),
                block_receive: self.block_first_receive.swap(false, Ordering::SeqCst),
            }))
        }
    }

    #[async_trait]
    impl ResponsesWebSocketConnection for FakeConnection {
        async fn send_text(
            &mut self,
            payload: Bytes,
        ) -> Result<(), ResponsesWebSocketTransportError> {
            self.sent.lock().unwrap().push(payload);
            self.started.notify_waiters();
            Ok(())
        }

        async fn receive(
            &mut self,
        ) -> Result<Option<ResponsesWebSocketFrame>, ResponsesWebSocketTransportError> {
            if self.block_receive {
                self.blocked.notified().await;
            }
            Ok(self.incoming.pop_front())
        }

        async fn close(&mut self) {}
    }

    fn key_with_revision(
        session: &str,
        credential: i64,
        revision: u64,
    ) -> ResponsesWebSocketPoolKey {
        ResponsesWebSocketPoolKey::new(
            ChannelId::new(1).unwrap(),
            CredentialId::new(credential).unwrap(),
            revision,
            session,
        )
        .unwrap()
    }

    fn key(session: &str, credential: i64) -> ResponsesWebSocketPoolKey {
        key_with_revision(session, credential, 1)
    }

    fn request(body: Value, mode: ResponseMode) -> UpstreamRequest {
        UpstreamRequest::new(
            Method::POST,
            "https://upstream.example/v1/responses",
            {
                let mut headers = HeaderMap::new();
                headers.insert(
                    HeaderName::from_static("authorization"),
                    HeaderValue::from_static("Bearer secret"),
                );
                headers.insert(
                    HeaderName::from_static("content-type"),
                    HeaderValue::from_static("application/json"),
                );
                headers.insert(
                    HeaderName::from_static("accept"),
                    HeaderValue::from_static("application/json"),
                );
                headers
            },
            Some(Bytes::from(serde_json::to_vec(&body).unwrap())),
        )
        .unwrap()
        .with_response_mode(mode)
    }

    fn completed(id: &str) -> ResponsesWebSocketFrame {
        ResponsesWebSocketFrame::Text(Bytes::from(
            serde_json::to_vec(&json!({
                "type": "response.completed",
                "response": {
                    "id": id,
                    "object": "response",
                    "status": "completed",
                    "model": "gpt-test",
                    "output": [],
                    "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
                }
            }))
            .unwrap(),
        ))
    }

    fn connector(scripts: Vec<Vec<ResponsesWebSocketFrame>>) -> FakeConnector {
        FakeConnector {
            scripts: Arc::new(Mutex::new(scripts.into())),
            connects: Arc::new(AtomicUsize::new(0)),
            sent: Arc::new(Mutex::new(Vec::new())),
            blocked: Arc::new(Notify::new()),
            block_first_receive: Arc::new(AtomicBool::new(false)),
            started: Arc::new(Notify::new()),
        }
    }

    #[test]
    fn event_builder_removes_http_only_fields_and_forces_stateless_mode() {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","stream":true,"background":false,"store":false}"#,
        );
        let value: Value =
            serde_json::from_slice(&build_responses_websocket_event(&body).unwrap()).unwrap();
        assert_eq!(value["type"], "response.create");
        assert_eq!(value["store"], false);
        assert!(value.get("stream").is_none());
        assert!(value.get("background").is_none());
    }

    #[test]
    fn handshake_converts_http_scheme_and_redacts_target() {
        let request = request(json!({"model":"gpt-test"}), ResponseMode::Full);
        let handshake = ResponsesWebSocketHandshake::from_request(&request).unwrap();
        assert_eq!(handshake.target(), "wss://upstream.example/v1/responses");
        assert_eq!(handshake.headers().len(), 1);
        let debug = format!("{handshake:?}");
        assert!(!debug.contains("upstream.example"));
        assert!(!debug.contains("Bearer secret"));
    }

    #[test]
    fn event_builder_rejects_persisted_or_background_requests() {
        for body in [
            json!({"model":"gpt-test","store":true}),
            json!({"model":"gpt-test","background":true}),
            json!({"type":"other"}),
        ] {
            let bytes = Bytes::from(serde_json::to_vec(&body).unwrap());
            assert_eq!(
                build_responses_websocket_event(&bytes),
                Err(ResponsesWebSocketPoolError::InvalidRequest)
            );
        }
    }

    #[test]
    fn json_boundaries_reject_duplicate_keys_and_invalid_control_field_types() {
        for body in [
            br#"{"model":"gpt-test","model":"other"}"#.as_slice(),
            br#"{"model":"gpt-test","input":{"text":"a","text":"b"}}"#.as_slice(),
            br#"{"model":"gpt-test","store":"false"}"#.as_slice(),
            br#"{"model":"gpt-test","type":null}"#.as_slice(),
        ] {
            let bytes = Bytes::copy_from_slice(body);
            assert_eq!(
                build_responses_websocket_event(&bytes),
                Err(ResponsesWebSocketPoolError::InvalidRequest)
            );
        }

        let duplicate_event = ResponsesWebSocketFrame::Text(Bytes::from_static(
            br#"{"type":"response.completed","type":"response.failed"}"#,
        ));
        assert_eq!(
            normalize_event(duplicate_event),
            Err(ResponsesWebSocketPoolError::Protocol)
        );
    }

    #[test]
    fn key_debug_and_hash_do_not_retain_session_text() {
        let key = key("private-session-value", 2);
        let debug = format!("{key:?}");
        assert!(!debug.contains("private-session-value"));
        assert_eq!(key.credential_id().get(), 2);

        let frame = ResponsesWebSocketFrame::Text(Bytes::from_static(b"private-response"));
        let debug = format!("{frame:?}");
        assert!(!debug.contains("private-response"));
        assert!(debug.contains("bytes: 16"));
    }

    #[tokio::test]
    async fn full_turn_reuses_connection_only_for_same_credential_session() {
        let connector = connector(vec![
            vec![completed("resp-1"), completed("resp-2")],
            vec![completed("resp-3")],
        ]);
        let connects = Arc::clone(&connector.connects);
        let sent = Arc::clone(&connector.sent);
        let pool = ResponsesWebSocketPool::new(
            ResponsesWebSocketPoolConfig::default(),
            Arc::new(connector),
        );

        let first = pool
            .execute(
                key("session-a", 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await
            .unwrap();
        let second = pool
            .execute(
                key("session-a", 1),
                &request(
                    json!({"model":"gpt-test","previous_response_id":"resp-1"}),
                    ResponseMode::Full,
                ),
            )
            .await
            .unwrap();
        let third = pool
            .execute(
                key("session-a", 2),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await
            .unwrap();

        assert_eq!(connects.load(Ordering::SeqCst), 2);
        assert_eq!(pool.session_count(), 2);
        let first_body = first.into_body().into_bytes().await.unwrap();
        let second_body = second.into_body().into_bytes().await.unwrap();
        let third_body = third.into_body().into_bytes().await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&first_body).unwrap()["id"],
            "resp-1"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&second_body).unwrap()["id"],
            "resp-2"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&third_body).unwrap()["id"],
            "resp-3"
        );
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 3);
        assert!(sent.iter().all(
            |body| serde_json::from_slice::<Value>(body).unwrap()["type"] == "response.create"
        ));
    }

    #[tokio::test]
    async fn credential_revision_change_does_not_reuse_connection() {
        let connector = connector(vec![
            vec![completed("revision-1")],
            vec![completed("revision-2")],
        ]);
        let connects = Arc::clone(&connector.connects);
        let pool = ResponsesWebSocketPool::new(
            ResponsesWebSocketPoolConfig::default(),
            Arc::new(connector),
        );

        let first = pool
            .execute(
                key_with_revision("revision", 1, 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await
            .unwrap();
        let second = pool
            .execute(
                key_with_revision("revision", 1, 2),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await
            .unwrap();

        assert_eq!(connects.load(Ordering::SeqCst), 2);
        assert_eq!(
            serde_json::from_slice::<Value>(&first.into_body().into_bytes().await.unwrap())
                .unwrap()["id"],
            "revision-1"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&second.into_body().into_bytes().await.unwrap())
                .unwrap()["id"],
            "revision-2"
        );
    }

    #[tokio::test]
    async fn cancelled_turn_poisoned_connection_is_rebuilt_before_reuse() {
        let connector = connector(vec![vec![completed("stale")], vec![completed("fresh")]]);
        connector.block_first_receive.store(true, Ordering::SeqCst);
        let started = Arc::clone(&connector.started);
        let connects = Arc::clone(&connector.connects);
        let pool = ResponsesWebSocketPool::new(
            ResponsesWebSocketPoolConfig::default(),
            Arc::new(connector),
        );
        let wait_started = started.notified();
        let first_pool = pool.clone();
        let first = tokio::spawn(async move {
            first_pool
                .execute(
                    key("cancelled", 1),
                    &request(json!({"model":"gpt-test"}), ResponseMode::Full),
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(1), wait_started)
            .await
            .unwrap();
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());

        let fresh = pool
            .execute(
                key("cancelled", 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await
            .unwrap();
        let body = fresh.into_body().into_bytes().await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["id"],
            "fresh"
        );
        assert_eq!(connects.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn early_eof_invalidates_connection_before_next_turn() {
        let connector = connector(vec![vec![], vec![completed("after-eof")]]);
        let connects = Arc::clone(&connector.connects);
        let pool = ResponsesWebSocketPool::new(
            ResponsesWebSocketPoolConfig::default(),
            Arc::new(connector),
        );
        let first = pool
            .execute(
                key("eof", 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await;
        assert!(matches!(
            first,
            Err(ResponsesWebSocketPoolError::ConnectionClosed)
        ));
        let second = pool
            .execute(
                key("eof", 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&second.into_body().into_bytes().await.unwrap())
                .unwrap()["id"],
            "after-eof"
        );
        assert_eq!(connects.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn stream_frames_are_wrapped_for_existing_sse_decoder() {
        let connector = connector(vec![vec![
            ResponsesWebSocketFrame::Ping,
            ResponsesWebSocketFrame::Text(Bytes::from_static(
                br#"{"type":"response.output_text.delta","delta":"hi"}"#,
            )),
            completed("resp-stream"),
        ]]);
        let pool = ResponsesWebSocketPool::new(
            ResponsesWebSocketPoolConfig::default(),
            Arc::new(connector),
        );
        let response = pool
            .execute(
                key("stream", 1),
                &request(
                    json!({"model":"gpt-test","stream":true}),
                    ResponseMode::Stream,
                ),
            )
            .await
            .unwrap();
        let body = response.into_body().into_bytes().await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("\"type\":\"response.output_text.delta\""));
        assert!(text.contains("\"delta\":\"hi\""));
        assert!(text.contains("\"type\":\"response.completed\""));
    }

    #[tokio::test]
    async fn unconsumed_stream_releases_session_at_turn_deadline() {
        let connector = connector(vec![
            vec![
                ResponsesWebSocketFrame::Text(Bytes::from_static(
                    br#"{"type":"response.output_text.delta","delta":"first"}"#,
                )),
                ResponsesWebSocketFrame::Text(Bytes::from_static(
                    br#"{"type":"response.output_text.delta","delta":"second"}"#,
                )),
                completed("stale"),
            ],
            vec![completed("after-backpressure")],
        ]);
        let connects = Arc::clone(&connector.connects);
        let config = ResponsesWebSocketPoolConfig::new(
            2,
            MAX_RESPONSES_WEBSOCKET_CONNECTION_AGE,
            Duration::from_secs(30),
            Duration::from_millis(200),
            Duration::from_secs(1),
            Duration::from_millis(20),
        )
        .unwrap();
        let pool = ResponsesWebSocketPool::new(config, Arc::new(connector));
        let _held_response = pool
            .execute(
                key("backpressure", 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Stream),
            )
            .await
            .unwrap();

        let second = pool
            .execute(
                key("backpressure", 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&second.into_body().into_bytes().await.unwrap())
                .unwrap()["id"],
            "after-backpressure"
        );
        assert_eq!(connects.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn queue_timeout_keeps_one_in_flight_per_connection() {
        let connector = connector(vec![vec![completed("resp-1")]]);
        connector.block_first_receive.store(true, Ordering::SeqCst);
        let started = Arc::clone(&connector.started);
        let config = ResponsesWebSocketPoolConfig::new(
            2,
            MAX_RESPONSES_WEBSOCKET_CONNECTION_AGE,
            Duration::from_secs(30),
            Duration::from_millis(10),
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let pool = ResponsesWebSocketPool::new(config, Arc::new(connector));
        let wait_started = started.notified();
        let first_pool = pool.clone();
        let first = tokio::spawn(async move {
            first_pool
                .execute(
                    key("queue", 1),
                    &request(json!({"model":"gpt-test"}), ResponseMode::Full),
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(1), wait_started)
            .await
            .unwrap();

        let second = pool
            .execute(
                key("queue", 1),
                &request(json!({"model":"gpt-test"}), ResponseMode::Full),
            )
            .await;
        assert!(matches!(
            second,
            Err(ResponsesWebSocketPoolError::QueueTimeout)
        ));

        first.abort();
        let _ = first.await;
        pool.close_session(key("queue", 1)).await;
        assert_eq!(pool.session_count(), 0);
    }
}
