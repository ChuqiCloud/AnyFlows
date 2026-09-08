use std::{
    fmt,
    num::NonZeroUsize,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use lru::LruCache;
use reqwest::{Client as ReqwestClient, Proxy, redirect::Policy, retry};

use crate::{
    HttpClientConfig, HttpClientError, PooledClient, TargetDnsStrategy,
    resolver::{ValidatedResolver, normalize_dns_name},
};

/// 默认最多缓存 128 种代理与超时组合。
pub const DEFAULT_MAX_CACHED_CLIENTS: usize = 128;
/// 空闲连接默认保留 90 秒，低于常见上游负载均衡空闲超时。
pub const DEFAULT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);
/// 每个上游主机默认最多保留 10 条空闲连接。
pub const DEFAULT_POOL_MAX_IDLE_PER_HOST: usize = 10;
/// 受控客户端统一声明真实产品与版本，兼容会拒绝空 User-Agent 的上游网关。
pub(crate) const DEFAULT_USER_AGENT: &str = concat!("AnyFlows/", env!("CARGO_PKG_VERSION"));

/// 按代理、超时与目标安全策略复用 reqwest Client 的严格容量 LRU。
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
    build_client_with_resolver(config, None)
}

fn build_client_with_resolver(
    config: &HttpClientConfig,
    resolver_override: Option<ValidatedResolver>,
) -> Result<PooledClient, HttpClientError> {
    ensure_tls_provider()?;
    let timeouts = config.timeouts();
    let target_dns_strategy = config.proxy().target_dns_strategy();
    let proxy_host = config.proxy().endpoint_host().map(normalize_dns_name);
    let mut builder = ReqwestClient::builder()
        .no_proxy()
        .user_agent(DEFAULT_USER_AGENT)
        .redirect(Policy::none())
        .referer(false)
        // 重试由 af-relay 统一编排，传输层不得隐式重放请求。
        .retry(retry::never())
        .connect_timeout(timeouts.connect())
        .read_timeout(timeouts.read())
        .pool_idle_timeout(DEFAULT_POOL_IDLE_TIMEOUT)
        .pool_max_idle_per_host(DEFAULT_POOL_MAX_IDLE_PER_HOST);

    if target_dns_strategy == TargetDnsStrategy::Local {
        let resolver = resolver_override.unwrap_or_else(|| {
            ValidatedResolver::system(
                config.target_address_policy().clone(),
                proxy_host.as_deref(),
            )
        });
        builder = builder.dns_resolver(resolver);
    }

    if let Some(endpoint) = config.proxy().required_endpoint() {
        let proxy = Proxy::all(endpoint.clone()).map_err(|_| HttpClientError::ClientBuild)?;
        builder = builder.proxy(proxy);
    }

    let client = builder.build().map_err(|_| HttpClientError::ClientBuild)?;
    Ok(PooledClient::new(
        client,
        timeouts.read(),
        timeouts.request(),
        config.target_address_policy().clone(),
        target_dns_strategy,
        config.remote_dns_policy(),
        proxy_host,
    ))
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
    use std::{
        collections::VecDeque,
        io::{Read, Write},
        net::{SocketAddr, TcpListener},
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use reqwest::dns::{Addrs, Name, Resolve, Resolving};

    use super::*;
    use crate::{HeaderMap, HttpTimeouts, HttpTransportError, Method, ProxyConfig};

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

    #[test]
    fn separates_clients_by_target_security_policy() {
        let pool = HttpClientPool::new(4).unwrap();
        let base = config(10);
        let trusted = base
            .clone()
            .with_remote_dns_policy(crate::RemoteDnsPolicy::TrustProxy);
        let exception = base.clone().with_target_address_policy(
            crate::TargetAddressPolicy::allow_exact(["127.0.0.1".parse().unwrap()]).unwrap(),
        );

        pool.get(&base).unwrap();
        pool.get(&trusted).unwrap();
        pool.get(&exception).unwrap();
        assert_eq!(pool.len().unwrap(), 3);
    }

    #[tokio::test]
    async fn closed_connection_rechecks_and_blocks_rebinding_answer() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 512];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
            }
            stream
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });

        let answers = SequenceResolver::new(vec![
            vec![SocketAddr::from(([127, 0, 0, 1], 0))],
            vec![SocketAddr::from(([127, 0, 0, 2], 0))],
        ]);
        let policy =
            crate::TargetAddressPolicy::allow_exact(["127.0.0.1".parse().unwrap()]).unwrap();
        let config = HttpClientConfig::new(
            ProxyConfig::direct(),
            HttpTimeouts::new(
                Duration::from_secs(1),
                Duration::from_secs(1),
                Duration::from_secs(2),
            )
            .unwrap(),
        )
        .with_target_address_policy(policy.clone());
        let resolver = ValidatedResolver::new(Arc::new(answers), policy, None);
        let client = build_client_with_resolver(&config, Some(resolver)).unwrap();
        let target = format!("http://rebind.example:{port}/resource");

        client
            .execute(Method::GET, &target, HeaderMap::new(), None)
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(
            client
                .execute(Method::GET, &target, HeaderMap::new(), None)
                .await
                .unwrap_err(),
            HttpTransportError::TargetAddressBlocked
        );
    }

    #[derive(Clone)]
    struct SequenceResolver {
        answers: Arc<Mutex<VecDeque<Vec<SocketAddr>>>>,
    }

    impl SequenceResolver {
        fn new(answers: Vec<Vec<SocketAddr>>) -> Self {
            Self {
                answers: Arc::new(Mutex::new(answers.into())),
            }
        }
    }

    impl Resolve for SequenceResolver {
        fn resolve(&self, _name: Name) -> Resolving {
            let answer = self.answers.lock().unwrap().pop_front().unwrap();
            Box::pin(async move { Ok(Box::new(answer.into_iter()) as Addrs) })
        }
    }
}
