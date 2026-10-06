use std::time::Duration;

use async_trait::async_trait;

use crate::CacheError;

/// Redis 命中结果携带剩余 TTL，避免回填本地后延长生命周期。
#[derive(Clone)]
pub(crate) struct RemoteEntry {
    pub(crate) value: Vec<u8>,
    pub(crate) ttl: Duration,
}

#[async_trait]
pub(crate) trait RemoteCache: Send + Sync {
    async fn get(
        &self,
        key: &str,
        max_value_bytes: usize,
    ) -> Result<Option<RemoteEntry>, CacheError>;

    async fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<(), CacheError>;

    async fn delete(&self, key: &str) -> Result<bool, CacheError>;

    async fn health_check(&self) -> Result<(), CacheError>;
}
