use std::{fmt, sync::Arc, time::Duration};

use crate::{HttpClientConfig, HttpClientError, HttpClientPool, HttpTimeouts, PooledClient};

/// 共享全局安全基线并按渠道超时身份复用受控 Client 的提供器。
#[derive(Clone)]
pub struct HttpClientProvider {
    pool: Arc<HttpClientPool>,
    base_config: HttpClientConfig,
}

impl HttpClientProvider {
    /// 使用显式安全基线和严格容量缓存创建 Client 提供器。
    pub fn new(
        base_config: HttpClientConfig,
        max_cached_clients: usize,
    ) -> Result<Self, HttpClientError> {
        Ok(Self {
            pool: Arc::new(HttpClientPool::new(max_cached_clients)?),
            base_config,
        })
    }

    /// 返回继承全局配置或覆盖渠道读取/请求超时的受控 Client。
    pub fn get(&self, timeout: Option<Duration>) -> Result<PooledClient, HttpClientError> {
        let config = match timeout {
            None => self.base_config.clone(),
            Some(timeout) => {
                let base_timeouts = self.base_config.timeouts();
                let timeouts = HttpTimeouts::new(base_timeouts.connect(), timeout, timeout)?;
                self.base_config.clone().with_timeouts(timeouts)
            }
        };
        self.pool.get(&config)
    }
}

impl fmt::Debug for HttpClientProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpClientProvider")
            .field("pool", &self.pool)
            .field("base_config", &self.base_config)
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
        provider.get(Some(Duration::from_secs(60))).unwrap();
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
}
