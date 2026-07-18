//! 面向运维配置上游的 HTTP Client 复用与传输策略。
//!
//! 本 crate 不接受任意用户 URL，也不负责重试、首 token 判定或 DNS rebinding
//! 防护。显式代理始终为强制路由，任何代理故障都不会回退直连。

mod config;
mod error;
mod pool;
mod transport;

pub use bytes::Bytes;
pub use config::{
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_READ_TIMEOUT, DEFAULT_REQUEST_TIMEOUT, HttpClientConfig,
    HttpTimeouts, ProxyConfig,
};
pub use error::{HttpClientError, HttpTransportError, TimeoutPhase};
pub use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
pub use pool::{
    DEFAULT_MAX_CACHED_CLIENTS, DEFAULT_POOL_IDLE_TIMEOUT, DEFAULT_POOL_MAX_IDLE_PER_HOST,
    HttpClientPool,
};
pub use reqwest::Body;
pub use transport::{HttpBodyStream, HttpResponse, PooledClient};
