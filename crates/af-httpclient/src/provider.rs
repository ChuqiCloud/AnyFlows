use std::{
    fmt,
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::{
    HttpClientConfig, HttpClientError, HttpClientPool, HttpTimeouts, PooledClient, ProxyConfig,
    RemoteDnsPolicy,
};

/// 共享全局安全基线并按渠道超时身份复用受控 Client 的提供器。
#[derive(Clone)]
pub struct HttpClientProvider {
    pool: Arc<HttpClientPool>,
    base_config: Arc<RwLock<HttpClientConfig>>,
}

impl HttpClientProvider {
    /// 使用显式安全基线和严格容量缓存创建 Client 提供器。
    pub fn new(
        base_config: HttpClientConfig,
        max_cached_clients: usize,
    ) -> Result<Self, HttpClientError> {
        Ok(Self {
            pool: Arc::new(HttpClientPool::new(max_cached_clients)?),
            base_config: Arc::new(RwLock::new(base_config)),
        })
    }

    /// 返回继承全局配置或覆盖渠道读取/请求超时的受控 Client。
    pub fn get(&self, timeout: Option<Duration>) -> Result<PooledClient, HttpClientError> {
        let base_config = self
            .base_config
            .read()
            .map_err(|_| HttpClientError::PoolUnavailable)?
            .clone();
        let config = with_timeout(base_config, timeout)?;
        self.pool.get(&config)
    }

    /// 返回强制使用专属代理的受控 Client；代理或构建失败绝不回退全局出口。
    pub fn get_with_proxy(
        &self,
        proxy: ProxyConfig,
        remote_dns_policy: RemoteDnsPolicy,
        timeout: Option<Duration>,
    ) -> Result<PooledClient, HttpClientError> {
        let base_config = self
            .base_config
            .read()
            .map_err(|_| HttpClientError::PoolUnavailable)?
            .clone()
            .with_proxy(proxy)
            .with_remote_dns_policy(remote_dns_policy);
        let config = with_timeout(base_config, timeout)?;
        self.pool.get(&config)
    }

    /// 返回当前共享基线快照，调用方只能据此构造下一份配置。
    pub fn base_config(&self) -> Result<HttpClientConfig, HttpClientError> {
        self.base_config
            .read()
            .map(|config| config.clone())
            .map_err(|_| HttpClientError::PoolUnavailable)
    }

    /// 原子替换后续请求使用的基线，并丢弃旧配置的缓存 Client。
    pub fn replace_base_config(&self, config: HttpClientConfig) -> Result<(), HttpClientError> {
        *self
            .base_config
            .write()
            .map_err(|_| HttpClientError::PoolUnavailable)? = config;
        self.pool.clear()
    }
}

fn with_timeout(
    config: HttpClientConfig,
    timeout: Option<Duration>,
) -> Result<HttpClientConfig, HttpClientError> {
    let Some(timeout) = timeout else {
        return Ok(config);
    };
    let base_timeouts = config.timeouts();
    let timeouts = HttpTimeouts::new(base_timeouts.connect(), timeout, timeout)?;
    Ok(config.with_timeouts(timeouts))
}

impl fmt::Debug for HttpClientProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpClientProvider")
            .field("pool", &self.pool)
            .field("base_config", &self.base_config.read().ok())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProxyConfig, TimeoutPhase};

    #[test]
    fn reuses_equal_timeout_identities_without_affecting_the_global_client() {
        let provider = HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap();

        provider.get(None).unwrap();
        assert_eq!(
            provider
                .get(Some(Duration::from_secs(60)))
                .unwrap()
                .request_timeout(),
            Duration::from_secs(60)
        );
        provider.get(Some(Duration::from_secs(60))).unwrap();

        assert_eq!(provider.pool.len().unwrap(), 2);
    }

    #[test]
    fn rejects_zero_channel_timeout_and_redacts_proxy_debug() {
        let config = HttpClientConfig::new(
            ProxyConfig::parse("http://user:secret@proxy.example:8080").unwrap(),
            HttpTimeouts::default(),
        );
        let provider = HttpClientProvider::new(config, 2).unwrap();
        assert_eq!(
            provider.get(Some(Duration::ZERO)).unwrap_err(),
            HttpClientError::InvalidTimeout {
                phase: TimeoutPhase::Read
            }
        );

        let debug = format!("{provider:?}");
        assert!(!debug.contains("user"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("proxy.example"));
    }

    #[test]
    fn replaces_shared_base_config_without_reusing_old_clients() {
        let provider = HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap();
        provider.get(None).unwrap();
        let proxy = ProxyConfig::parse("http://proxy.example:8080").unwrap();
        provider
            .replace_base_config(HttpClientConfig::default().with_proxy(proxy))
            .unwrap();

        assert_eq!(provider.pool.len().unwrap(), 0);
        assert!(!provider.base_config().unwrap().proxy().is_direct());
    }

    #[test]
    fn dedicated_proxy_overrides_global_route_and_uses_a_distinct_cache_key() {
        let global = ProxyConfig::parse("http://global.example:8080").unwrap();
        let provider =
            HttpClientProvider::new(HttpClientConfig::default().with_proxy(global), 4).unwrap();
        provider.get(None).unwrap();
        let dedicated = ProxyConfig::parse("socks5h://account.example:1080").unwrap();
        provider
            .get_with_proxy(dedicated, RemoteDnsPolicy::TrustProxy, None)
            .unwrap();

        assert_eq!(provider.pool.len().unwrap(), 2);
    }
}
