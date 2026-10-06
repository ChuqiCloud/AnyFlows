use std::{fmt, num::NonZeroUsize, sync::Mutex};

use af_adapter::{
    PooledClient, PooledClientIdentity, PooledResponsesWebSocketConnector, ResponsesWebSocketPool,
    ResponsesWebSocketPoolConfig,
};
use lru::LruCache;
use thiserror::Error;

/// 单进程最多保留的受控 Client 对应 Responses WebSocket 连接池数量。
pub(crate) const DEFAULT_MAX_RESPONSES_WEBSOCKET_CLIENT_POOLS: usize = 128;

/// Responses WebSocket 运行时注册表错误；不携带 Client、代理或上游信息。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum ResponsesWebSocketRuntimeError {
    /// 注册表容量或池超时配置无效。
    #[error("Responses WebSocket 运行时配置无效")]
    InvalidConfig,
    /// 注册表互斥锁已经损坏。
    #[error("Responses WebSocket 运行时不可用")]
    Unavailable,
}

/// 按真实受控 Client 身份隔离的有界 Responses WebSocket 连接池注册表。
pub(crate) struct ResponsesWebSocketRuntime {
    pools: Mutex<LruCache<PooledClientIdentity, ResponsesWebSocketPool>>,
}

impl ResponsesWebSocketRuntime {
    /// 创建固定容量注册表；容量只限制 Client 池数量，每个池另有会话上限。
    pub(crate) fn new(max_client_pools: usize) -> Result<Self, ResponsesWebSocketRuntimeError> {
        let capacity = NonZeroUsize::new(max_client_pools)
            .ok_or(ResponsesWebSocketRuntimeError::InvalidConfig)?;
        Ok(Self {
            pools: Mutex::new(LruCache::new(capacity)),
        })
    }

    /// 返回绑定当前 Client 代理、DNS、TLS 与超时身份的连接池。
    pub(crate) fn pool_for(
        &self,
        client: &PooledClient,
    ) -> Result<ResponsesWebSocketPool, ResponsesWebSocketRuntimeError> {
        let identity = client.identity();
        let mut pools = self
            .pools
            .lock()
            .map_err(|_| ResponsesWebSocketRuntimeError::Unavailable)?;
        if let Some(pool) = pools.get(&identity).cloned() {
            return Ok(pool);
        }

        let defaults = ResponsesWebSocketPoolConfig::default();
        let config = ResponsesWebSocketPoolConfig::new(
            defaults.max_sessions(),
            defaults.max_connection_age(),
            defaults.idle_timeout(),
            defaults.queue_timeout(),
            defaults.connect_timeout(),
            client.request_timeout(),
        )
        .map_err(|_| ResponsesWebSocketRuntimeError::InvalidConfig)?;
        let connector = PooledResponsesWebSocketConnector::new(client.clone());
        let pool = ResponsesWebSocketPool::new(config, std::sync::Arc::new(connector));
        pools.put(identity, pool.clone());
        Ok(pool)
    }

    #[cfg(test)]
    fn pool_count(&self) -> Result<usize, ResponsesWebSocketRuntimeError> {
        self.pools
            .lock()
            .map(|pools| pools.len())
            .map_err(|_| ResponsesWebSocketRuntimeError::Unavailable)
    }
}

impl Default for ResponsesWebSocketRuntime {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_RESPONSES_WEBSOCKET_CLIENT_POOLS)
            .expect("默认 Responses WebSocket Client 池容量必须有效")
    }
}

impl fmt::Debug for ResponsesWebSocketRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResponsesWebSocketRuntime")
            .field(
                "client_pool_count",
                &self.pools.lock().ok().map(|pools| pools.len()),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use af_httpclient::{HttpClientConfig, HttpClientPool};

    use super::*;

    #[test]
    fn reuses_cloned_client_identity_and_bounds_registry() {
        let runtime = ResponsesWebSocketRuntime::new(1).unwrap();
        let first = HttpClientPool::default()
            .get(&HttpClientConfig::default())
            .unwrap();
        runtime.pool_for(&first).unwrap();
        runtime.pool_for(&first.clone()).unwrap();
        assert_eq!(runtime.pool_count().unwrap(), 1);

        let second = HttpClientPool::default()
            .get(&HttpClientConfig::default())
            .unwrap();
        runtime.pool_for(&second).unwrap();
        assert_eq!(runtime.pool_count().unwrap(), 1);
        assert_ne!(first.identity(), second.identity());
    }
}
