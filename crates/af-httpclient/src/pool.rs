use std::{
    fmt,
    num::NonZeroUsize,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use lru::LruCache;
use reqwest::{Client as ReqwestClient, Proxy, redirect::Policy, retry};

use crate::{HttpClientConfig, HttpClientError, PooledClient};

/// 默认最多缓存 128 种代理与超时组合。
pub const DEFAULT_MAX_CACHED_CLIENTS: usize = 128;
/// 空闲连接默认保留 90 秒，低于常见上游负载均衡空闲超时。
pub const DEFAULT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// 每个上游主机默认最多保留 10 条空闲连接。
pub const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 10;

/// 按代理身份与超时配置复用 reqwest Client 的严格容量 LRU。
pub struct HttpClientPool {
    clients: Mutex<LruCache<HttpClientConfig, PooledClient>>,
    capacity: NonZeroUsize,
}

impl HttpClientPool {
    /// 创建有界 Client 缓存；每个 Client 内部继续复用 HTTP 连接池。
    pub fn new(max_cached_clients: usize) -> Result<Self, HttpClientError> {
        let capacity =
            NonZeroUsize::new(max_cached_clients).ok_or(HttpClientError::InvalidPoolCapacity)?;
        Ok(Self {
            clients: Mutex::new(LruCache::new(capacity)),
            capacity,
        })
    }

    /// 返回与配置完全匹配的受控 Client；构建失败不会写入缓存。
    pub fn get(&self, config: &HttpClientConfig) -> Result<PooledClient, HttpClientError> {
        if let Some(client) = self.lock()?.get(config).cloned() {
            return Ok(client);
        }

        // Client 构建不跨锁执行；并发重复构建会在二次检查时收敛到同一缓存项。
        let client = build_client(config)?;
        let mut clients = self.lock()?;
        if let Some(existing) = clients.get(config).cloned() {
            return Ok(existing);
        }
        clients.put(config.clone(), client.clone());
        Ok(client)
    }

    /// 返回最多缓存的 Client 配置数量。
    #[must_use]
    pub const fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// 返回当前缓存项数量。
    pub fn len(&self) -> Result<usize, HttpClientError> {
        Ok(self.lock()?.len())
    }

    /// 返回缓存是否为空。
    pub fn is_empty(&self) -> Result<bool, HttpClientError> {
        Ok(self.lock()?.is_empty())
    }

    /// 清空缓存引用；正在使用的 Client clone 不受影响。
    pub fn clear(&self) -> Result<(), HttpClientError> {
        self.lock()?.clear();
        Ok(())
    }

    fn lock(
        &self,
    ) -> Result<MutexGuard<'_, LruCache<HttpClientConfig, PooledClient>>, HttpClientError> {
        self.clients
            .lock()
            .map_err(|_| HttpClientError::PoolUnavailable)
    }
}

impl Default for HttpClientPool {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CACHED_CLIENTS).expect("默认 HTTP Client 缓存容量必须大于零")
    }
}

impl fmt::Debug for HttpClientPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cached_clients = self.clients.lock().ok().map(|clients| clients.len());
        formatter
            .debug_struct("HttpClientPool")
            .field("capacity", &self.capacity)
            .field("cached_clients", &cached_clients)
            .field("pool_idle_timeout", &DEFAULT_POOL_IDLE_TIMEOUT)
            .field("pool_max_idle_per_host", &DEFAULT_POOL_MAX_IDLE_PER_HOST)
            .finish()
    }
}

fn build_client(config: &HttpClientConfig) -> Result<PooledClient, HttpClientError> {
    ensure_tls_provider()?;
    let timeouts = config.timeouts();
    let mut builder = ReqwestClient::builder()
        .no_proxy()
        .redirect(Policy::none())
        .referer(false)
        // 重试由 af-relay 统一编排，传输层不得隐式重放请求。
        .retry(retry::never())
        .connect_timeout(timeouts.connect())
        .read_timeout(timeouts.read())
        .pool_idle_timeout(DEFAULT_POOL_IDLE_TIMEOUT)
        .pool_max_idle_per_host(DEFAULT_POOL_MAX_IDLE_PER_HOST);

    if let Some(endpoint) = config.proxy().required_endpoint() {
        let proxy = Proxy::all(endpoint.clone()).map_err(|_| HttpClientError::ClientBuild)?;
        builder = builder.proxy(proxy);
    }

    builder
        .build()
        .map(|client| PooledClient::new(client, timeouts.request()))
        .map_err(|_| HttpClientError::ClientBuild)
}

fn ensure_tls_provider() -> Result<(), HttpClientError> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        return Err(HttpClientError::TlsProviderUnavailable);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{HttpTimeouts, ProxyConfig};

    fn config(connect_millis: u64) -> HttpClientConfig {
        HttpClientConfig::new(
            ProxyConfig::direct(),
            HttpTimeouts::new(
                Duration::from_millis(connect_millis),
                Duration::from_secs(1),
                Duration::from_secs(2),
            )
            .unwrap(),
        )
    }

    #[test]
    fn rejects_zero_capacity() {
        assert_eq!(
            HttpClientPool::new(0).unwrap_err(),
            HttpClientError::InvalidPoolCapacity
        );
    }

    #[test]
    fn reuses_exact_configuration_and_separates_timeout_keys() {
        let pool = HttpClientPool::new(2).unwrap();
        pool.get(&config(10)).unwrap();
        pool.get(&config(10)).unwrap();
        assert_eq!(pool.len().unwrap(), 1);

        pool.get(&config(11)).unwrap();
        assert_eq!(pool.len().unwrap(), 2);
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn evicts_the_least_recently_used_configuration() {
        let pool = HttpClientPool::new(2).unwrap();
        let first = config(10);
        let second = config(20);
        let third = config(30);
        pool.get(&first).unwrap();
        pool.get(&second).unwrap();
        pool.get(&first).unwrap();
        pool.get(&third).unwrap();

        let clients = pool.lock().unwrap();
        assert!(clients.peek(&first).is_some());
        assert!(clients.peek(&second).is_none());
        assert!(clients.peek(&third).is_some());
    }

    #[test]
    fn clear_drops_cached_references_without_exposing_keys() {
        let pool = HttpClientPool::default();
        let proxy = ProxyConfig::parse("http://user:secret@proxy.example:8080").unwrap();
        pool.get(&HttpClientConfig::new(proxy, HttpTimeouts::default()))
            .unwrap();
        assert_eq!(pool.len().unwrap(), 1);
        let debug = format!("{pool:?}");
        assert!(!debug.contains("user"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("proxy.example"));

        pool.clear().unwrap();
        assert!(pool.is_empty().unwrap());
    }
}
