//! 面向运维配置上游的 HTTP Client 复用与传输策略。
//!
//! 本 crate 不接受任意用户 URL，也不负责重试或首 token 判定。上游目标在
//! 建连解析边界执行 SSRF 与 DNS rebinding 防护；显式代理始终为强制路由，
//! 任何代理故障都不会回退直连。

mod address;
mod config;
mod error;
#[cfg(test)]
mod http_client_tests;
mod pool;
mod provider;
mod resolver;
mod transport;
mod websocket;

pub use bytes::Bytes;
pub(crate) use config::TargetAddressPolicy;
pub use config::{
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_READ_TIMEOUT, DEFAULT_REQUEST_TIMEOUT, HttpClientConfig,
    HttpTimeouts, ProxyConfig, RemoteDnsPolicy, TargetDnsStrategy,
};
pub use error::{HttpClientError, HttpTransportError, TimeoutPhase};
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use pool::{
    DEFAULT_MAX_CACHED_CLIENTS, DEFAULT_POOL_IDLE_TIMEOUT, DEFAULT_POOL_MAX_IDLE_PER_HOST,
    HttpClientPool,
};
pub use provider::HttpClientProvider;
pub use reqwest::Body;
pub use transport::{HttpBodyStream, HttpResponse, PooledClient, PooledClientIdentity};
pub use websocket::{
    MAX_MANAGED_WEBSOCKET_MESSAGE_BYTES, WebSocketConnection, WebSocketFrame,
    WebSocketTransportError,
};
