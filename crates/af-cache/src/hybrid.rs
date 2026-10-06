use std::{
    collections::hash_map::DefaultHasher,
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{
    sync::{Mutex, MutexGuard},
    time,
};

use crate::{
    CacheError, CacheValue, HybridCacheConfig,
    config::{validate_cache_key, validate_ttl},
    local::LocalCache,
    redis_backend::RedisBackend,
    remote::RemoteCache,
};

const REDIS_KEY_PREFIX: &str = "anyflows";
const MUTATION_SHARD_COUNT: usize = 64;

/// 缓存运行模式；本地模式只适用于单实例部署。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CacheMode {
    LocalOnly,
    Hybrid,
}

/// 面向可重建数据的两级缓存。
///
/// 本地命中不会访问 Redis，因此其他实例的更新在当前 TTL 内可能暂时不可见。
/// 分布式锁、限流、并发槽位和余额等权威状态不得通过该类型实现。
#[derive(Clone)]
pub struct HybridCache {
    namespace: Arc<str>,
    local: Arc<LocalCache>,
    local_ttl_cap: Duration,
    max_value_bytes: usize,
    mutation_timeout: Duration,
    mutation_shards: Arc<[Mutex<u64>]>,
    remote: Option<Arc<dyn RemoteCache>>,
}

impl HybridCache {
    /// 构建缓存并在已配置 Redis 时完成连接与健康检查。
    pub async fn new(config: HybridCacheConfig) -> Result<Self, CacheError> {
        config.validate()?;
        let remote = if let Some(redis) = config.redis() {
            let backend = RedisBackend::connect(redis).await?;
            Some(Arc::new(backend) as Arc<dyn RemoteCache>)
        } else {
            None
        };
        Self::from_parts(config, remote)
    }

    /// 按 L1、L2 顺序读取；Redis 命中后使用其剩余 TTL 回填本地。
    pub async fn get(&self, key: &str) -> Result<Option<CacheValue>, CacheError> {
        validate_cache_key(key)?;
        if let Some(value) = self.local.get(key)? {
            return Ok(Some(value));
        }

        let Some(remote) = &self.remote else {
            return Ok(None);
        };
        let generation = self.generation(key).await?;
        let remote_started_at = Instant::now();
        let Some(entry) = remote
            .get(&self.redis_key(key), self.max_value_bytes)
            .await?
        else {
            return Ok(None);
        };
        if entry.value.len() > self.max_value_bytes {
            return Err(CacheError::ValueTooLarge);
        }

        let value = Arc::<[u8]>::from(entry.value);
        let local_ttl = self.remaining_local_ttl(entry.ttl, remote_started_at.elapsed());
        let generation_guard = self.lock_mutation(key).await?;
        if *generation_guard == generation
            && let Some(local_ttl) = local_ttl
        {
            self.local
                .set(key.to_owned(), Arc::clone(&value), local_ttl)?;
        }
        Ok(Some(value))
    }

    /// 串行写入 Redis 与本地；远端失败时清除旧 L1，避免掩盖结果未知状态。
    pub async fn set(
        &self,
        key: &str,
        value: impl AsRef<[u8]>,
        ttl: Duration,
    ) -> Result<(), CacheError> {
        validate_cache_key(key)?;
        let ttl = Duration::from_millis(validate_ttl(ttl)? as u64);
        let value = value.as_ref();
        if value.len() > self.max_value_bytes {
            return Err(CacheError::ValueTooLarge);
        }
        let value = Arc::<[u8]>::from(value);
        let mutation = PendingMutation::new(self.lock_mutation(key).await?, &self.local, key);

        let local_ttl = if let Some(remote) = &self.remote {
            let remote_started_at = Instant::now();
            remote
                .set(&self.redis_key(key), value.as_ref(), ttl)
                .await?;
            self.remaining_local_ttl(ttl, remote_started_at.elapsed())
        } else {
            Some(ttl)
        };
        mutation.complete_set(value, local_ttl)
    }

    /// 串行删除 Redis 与本地；远端失败时也清除 L1，迫使后续读取确认真实状态。
    pub async fn delete(&self, key: &str) -> Result<bool, CacheError> {
        validate_cache_key(key)?;
        let mutation = PendingMutation::new(self.lock_mutation(key).await?, &self.local, key);
        let remote_deleted = if let Some(remote) = &self.remote {
            remote.delete(&self.redis_key(key)).await?
        } else {
            false
        };
        let local_deleted = mutation.complete_delete()?;
        Ok(remote_deleted || local_deleted)
    }

    /// 检查当前 Redis 连接；纯本地模式始终返回就绪。
    pub async fn health_check(&self) -> Result<(), CacheError> {
        if let Some(remote) = &self.remote {
            remote.health_check().await?;
        }
        Ok(())
    }

    /// 返回当前缓存模式，供启动日志与就绪检查显式展示降级状态。
    #[must_use]
    pub const fn mode(&self) -> CacheMode {
        if self.remote.is_some() {
            CacheMode::Hybrid
        } else {
            CacheMode::LocalOnly
        }
    }

    /// 返回缓存命名空间。
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    fn from_parts(
        config: HybridCacheConfig,
        remote: Option<Arc<dyn RemoteCache>>,
    ) -> Result<Self, CacheError> {
        config.validate()?;
        Ok(Self {
            namespace: Arc::from(config.namespace()),
            local: Arc::new(LocalCache::new(config.local_capacity())),
            local_ttl_cap: config.local_ttl_cap(),
            max_value_bytes: config.max_value_bytes().get(),
            mutation_timeout: config.mutation_timeout(),
            mutation_shards: (0..MUTATION_SHARD_COUNT)
                .map(|_| Mutex::new(0))
                .collect::<Vec<_>>()
                .into(),
            remote,
        })
    }

    async fn generation(&self, key: &str) -> Result<u64, CacheError> {
        Ok(*self.lock_mutation(key).await?)
    }

    async fn lock_mutation(&self, key: &str) -> Result<MutexGuard<'_, u64>, CacheError> {
        let shard = self.mutation_shard_index(key);
        time::timeout(self.mutation_timeout, self.mutation_shards[shard].lock())
            .await
            .map_err(|_| CacheError::MutationTimeout)
    }

    fn mutation_shard_index(&self, key: &str) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish() as usize % self.mutation_shards.len()
    }

    fn local_ttl(&self, ttl: Duration) -> Duration {
        if self.remote.is_some() {
            ttl.min(self.local_ttl_cap)
        } else {
            ttl
        }
    }

    fn remaining_local_ttl(&self, ttl: Duration, elapsed: Duration) -> Option<Duration> {
        let remaining = ttl.checked_sub(elapsed)?;
        (remaining.as_millis() > 0).then(|| self.local_ttl(remaining))
    }

    fn redis_key(&self, key: &str) -> String {
        format!("{REDIS_KEY_PREFIX}:{}:{key}", self.namespace)
    }
}

/// 写删 future 被取消或 panic 时，仅清理本地投影，不在 Drop 中执行异步 IO。
struct PendingMutation<'a> {
    generation: MutexGuard<'a, u64>,
    local: &'a LocalCache,
    key: &'a str,
    finalized: bool,
}

impl<'a> PendingMutation<'a> {
    fn new(generation: MutexGuard<'a, u64>, local: &'a LocalCache, key: &'a str) -> Self {
        Self {
            generation,
            local,
            key,
            finalized: false,
        }
    }

    fn complete_set(mut self, value: CacheValue, ttl: Option<Duration>) -> Result<(), CacheError> {
        self.advance_generation();
        let result = if let Some(ttl) = ttl {
            self.local.set(self.key.to_owned(), value, ttl)
        } else {
            self.local.delete(self.key)?;
            Ok(())
        };
        self.finalized = true;
        result
    }

    fn complete_delete(mut self) -> Result<bool, CacheError> {
        self.advance_generation();
        let result = self.local.delete(self.key);
        self.finalized = true;
        result
    }

    fn advance_generation(&mut self) {
        *self.generation = self.generation.wrapping_add(1);
    }
}

impl Drop for PendingMutation<'_> {
    fn drop(&mut self) {
        if !self.finalized {
            self.advance_generation();
            let _ = self.local.delete(self.key);
        }
    }
}

impl fmt::Debug for HybridCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HybridCache")
            .field("namespace", &self.namespace)
            .field("local_capacity", &self.local.capacity())
            .field("local_ttl_cap", &self.local_ttl_cap)
            .field("max_value_bytes", &self.max_value_bytes)
            .field("mutation_timeout", &self.mutation_timeout)
            .field("mutation_shards", &self.mutation_shards.len())
            .field("mode", &self.mode())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use async_trait::async_trait;
    use tokio::sync::Notify;

    use super::*;
    use crate::{CacheOperation, RedisConfig, RedisFailureKind, remote::RemoteEntry};

    #[derive(Default)]
    struct FakeRemote {
        entries: Mutex<HashMap<String, RemoteEntry>>,
        failure: Mutex<Option<CacheOperation>>,
        block_reads: AtomicBool,
        read_started: Notify,
        read_release: Notify,
        read_count: AtomicUsize,
        block_writes: AtomicBool,
        write_started: Notify,
        write_release: Notify,
        write_count: AtomicUsize,
        delete_count: AtomicUsize,
        write_delay_millis: AtomicU64,
    }

    impl FakeRemote {
        fn fail(&self, operation: Option<CacheOperation>) {
            *self.failure.lock().unwrap() = operation;
        }

        fn maybe_fail(&self, operation: CacheOperation) -> Result<(), CacheError> {
            if *self.failure.lock().unwrap() == Some(operation) {
                return Err(CacheError::redis(operation, RedisFailureKind::Unavailable));
            }
            Ok(())
        }

        fn block_reads(&self) {
            self.block_reads.store(true, Ordering::Release);
        }

        async fn wait_for_read(&self) {
            self.read_started.notified().await;
        }

        fn release_read(&self) {
            self.block_reads.store(false, Ordering::Release);
            self.read_release.notify_one();
        }

        fn delay_writes(&self, delay: Duration) {
            self.write_delay_millis
                .store(delay.as_millis() as u64, Ordering::Release);
        }

        fn block_writes(&self) {
            self.block_writes.store(true, Ordering::Release);
        }

        async fn wait_for_write(&self) {
            self.write_started.notified().await;
        }

        fn release_write(&self) {
            self.write_release.notify_one();
        }
    }

    #[async_trait]
    impl RemoteCache for FakeRemote {
        async fn get(
            &self,
            key: &str,
            max_value_bytes: usize,
        ) -> Result<Option<RemoteEntry>, CacheError> {
            self.read_count.fetch_add(1, Ordering::Relaxed);
            self.maybe_fail(CacheOperation::Read)?;
            let entry = {
                let entries = self.entries.lock().unwrap();
                let entry = entries.get(key);
                if entry.is_some_and(|entry| entry.value.len() > max_value_bytes) {
                    return Err(CacheError::ValueTooLarge);
                }
                entry.cloned()
            };
            if self.block_reads.load(Ordering::Acquire) {
                self.read_started.notify_one();
                self.read_release.notified().await;
            }
            Ok(entry)
        }

        async fn set(&self, key: &str, value: &[u8], ttl: Duration) -> Result<(), CacheError> {
            self.write_count.fetch_add(1, Ordering::Relaxed);
            self.maybe_fail(CacheOperation::Write)?;
            let delay = self.write_delay_millis.load(Ordering::Acquire);
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            self.entries.lock().unwrap().insert(
                key.to_owned(),
                RemoteEntry {
                    value: value.to_vec(),
                    ttl,
                },
            );
            if self
                .block_writes
                .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.write_started.notify_one();
                self.write_release.notified().await;
            }
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<bool, CacheError> {
            self.delete_count.fetch_add(1, Ordering::Relaxed);
            self.maybe_fail(CacheOperation::Delete)?;
            Ok(self.entries.lock().unwrap().remove(key).is_some())
        }

        async fn health_check(&self) -> Result<(), CacheError> {
            self.maybe_fail(CacheOperation::HealthCheck)
        }
    }

    fn hybrid_cache(remote: Arc<FakeRemote>) -> HybridCache {
        let config = HybridCacheConfig::new("test.v1", 2).unwrap();
        HybridCache::from_parts(config, Some(remote)).unwrap()
    }

    #[tokio::test]
    async fn local_mode_supports_ttl_reads_and_deletes() {
        let config = HybridCacheConfig::new("local.v1", 2).unwrap();
        let cache = HybridCache::new(config).await.unwrap();
        assert_eq!(cache.mode(), CacheMode::LocalOnly);

        cache
            .set("key", b"value", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(cache.get("key").await.unwrap().unwrap().as_ref(), b"value");
        assert!(cache.delete("key").await.unwrap());
        assert!(!cache.delete("key").await.unwrap());
        assert!(cache.get("key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn configured_invalid_redis_never_falls_back_to_local_mode() {
        let config = HybridCacheConfig::new("redis.v1", 2)
            .unwrap()
            .with_redis(RedisConfig::new("not-a-redis-url"));
        assert_eq!(
            HybridCache::new(config).await.unwrap_err(),
            CacheError::InvalidRedisUrl
        );
    }

    #[tokio::test]
    async fn remote_hit_fills_l1_with_remaining_ttl() {
        let remote = Arc::new(FakeRemote::default());
        remote
            .set("anyflows:test.v1:key", b"remote", Duration::from_secs(1))
            .await
            .unwrap();
        let cache = hybrid_cache(Arc::clone(&remote));

        assert_eq!(cache.get("key").await.unwrap().unwrap().as_ref(), b"remote");
        assert_eq!(remote.read_count.load(Ordering::Relaxed), 1);
        remote.fail(Some(CacheOperation::Read));
        assert_eq!(cache.get("key").await.unwrap().unwrap().as_ref(), b"remote");
        assert_eq!(remote.read_count.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn remote_miss_failure_is_not_downgraded_to_local_miss() {
        let remote = Arc::new(FakeRemote::default());
        remote.fail(Some(CacheOperation::Read));
        let cache = hybrid_cache(remote);

        assert_eq!(
            cache.get("missing").await.unwrap_err(),
            CacheError::Redis {
                operation: CacheOperation::Read,
                kind: RedisFailureKind::Unavailable,
            }
        );
    }

    #[tokio::test]
    async fn failed_remote_write_invalidates_existing_l1() {
        let remote = Arc::new(FakeRemote::default());
        let cache = hybrid_cache(Arc::clone(&remote));
        cache
            .set("key", b"old", Duration::from_secs(1))
            .await
            .unwrap();

        remote.fail(Some(CacheOperation::Write));
        assert!(
            cache
                .set("key", b"new", Duration::from_secs(1))
                .await
                .is_err()
        );
        remote.fail(Some(CacheOperation::Read));
        assert!(matches!(
            cache.get("key").await,
            Err(CacheError::Redis {
                operation: CacheOperation::Read,
                kind: RedisFailureKind::Unavailable,
            })
        ));
    }

    #[tokio::test]
    async fn failed_remote_delete_invalidates_existing_l1() {
        let remote = Arc::new(FakeRemote::default());
        let cache = hybrid_cache(Arc::clone(&remote));
        cache
            .set("key", b"value", Duration::from_secs(1))
            .await
            .unwrap();

        remote.fail(Some(CacheOperation::Delete));
        assert!(cache.delete("key").await.is_err());
        remote.fail(Some(CacheOperation::Read));
        assert!(matches!(
            cache.get("key").await,
            Err(CacheError::Redis {
                operation: CacheOperation::Read,
                kind: RedisFailureKind::Unavailable,
            })
        ));
    }

    #[tokio::test]
    async fn older_remote_read_cannot_overwrite_a_later_write() {
        let remote = Arc::new(FakeRemote::default());
        remote
            .set("anyflows:test.v1:key", b"old", Duration::from_secs(1))
            .await
            .unwrap();
        let cache = hybrid_cache(Arc::clone(&remote));
        remote.block_reads();

        let pending_cache = cache.clone();
        let pending_read = tokio::spawn(async move { pending_cache.get("key").await });
        remote.wait_for_read().await;
        cache
            .set("key", b"new", Duration::from_secs(1))
            .await
            .unwrap();
        remote.release_read();

        assert_eq!(
            pending_read.await.unwrap().unwrap().unwrap().as_ref(),
            b"old"
        );
        assert_eq!(cache.get("key").await.unwrap().unwrap().as_ref(), b"new");
    }

    #[tokio::test]
    async fn older_remote_read_cannot_restore_a_deleted_l1_entry() {
        let remote = Arc::new(FakeRemote::default());
        remote
            .set("anyflows:test.v1:key", b"old", Duration::from_secs(1))
            .await
            .unwrap();
        let cache = hybrid_cache(Arc::clone(&remote));
        remote.block_reads();

        let pending_cache = cache.clone();
        let pending_read = tokio::spawn(async move { pending_cache.get("key").await });
        remote.wait_for_read().await;
        assert!(cache.delete("key").await.unwrap());
        remote.release_read();

        assert_eq!(
            pending_read.await.unwrap().unwrap().unwrap().as_ref(),
            b"old"
        );
        assert!(cache.get("key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn hybrid_l1_ttl_is_capped_without_shortening_l2() {
        let remote = Arc::new(FakeRemote::default());
        remote
            .set("anyflows:test.v1:key", b"value", Duration::from_secs(1))
            .await
            .unwrap();
        let config = HybridCacheConfig::new("test.v1", 2)
            .unwrap()
            .with_local_ttl_cap(Duration::from_millis(1));
        let cache = HybridCache::from_parts(config, Some(remote.clone())).unwrap();

        assert!(cache.get("key").await.unwrap().is_some());
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(cache.get("key").await.unwrap().is_some());
        assert_eq!(remote.read_count.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn slow_remote_write_does_not_extend_l1_past_l2() {
        let remote = Arc::new(FakeRemote::default());
        let cache = hybrid_cache(Arc::clone(&remote));
        cache
            .set("key", b"old", Duration::from_secs(1))
            .await
            .unwrap();

        remote.delay_writes(Duration::from_millis(20));
        cache
            .set("key", b"new", Duration::from_millis(1))
            .await
            .unwrap();
        remote.fail(Some(CacheOperation::Read));

        assert!(matches!(
            cache.get("key").await,
            Err(CacheError::Redis {
                operation: CacheOperation::Read,
                kind: RedisFailureKind::Unavailable,
            })
        ));
    }

    #[tokio::test]
    async fn concurrent_mutations_keep_redis_and_l1_in_the_same_order() {
        let remote = Arc::new(FakeRemote::default());
        let cache = hybrid_cache(Arc::clone(&remote));
        remote.block_writes();

        let first_cache = cache.clone();
        let first = tokio::spawn(async move {
            first_cache
                .set("key", b"first", Duration::from_secs(1))
                .await
        });
        remote.wait_for_write().await;
        let second_started = Arc::new(Notify::new());
        let second_started_in_task = Arc::clone(&second_started);
        let second_cache = cache.clone();
        let second = tokio::spawn(async move {
            second_started_in_task.notify_one();
            second_cache
                .set("key", b"second", Duration::from_secs(1))
                .await
        });
        second_started.notified().await;
        tokio::task::yield_now().await;
        assert!(!second.is_finished());
        assert_eq!(remote.write_count.load(Ordering::Relaxed), 1);
        remote.release_write();

        first.await.unwrap().unwrap();
        second.await.unwrap().unwrap();
        assert_eq!(cache.get("key").await.unwrap().unwrap().as_ref(), b"second");
        assert_eq!(
            remote
                .entries
                .lock()
                .unwrap()
                .get("anyflows:test.v1:key")
                .unwrap()
                .value,
            b"second"
        );
    }

    #[tokio::test]
    async fn delete_waits_for_an_earlier_write_before_removing_both_levels() {
        let remote = Arc::new(FakeRemote::default());
        let cache = hybrid_cache(Arc::clone(&remote));
        remote.block_writes();

        let write_cache = cache.clone();
        let write = tokio::spawn(async move {
            write_cache
                .set("key", b"value", Duration::from_secs(1))
                .await
        });
        remote.wait_for_write().await;
        let delete_started = Arc::new(Notify::new());
        let delete_started_in_task = Arc::clone(&delete_started);
        let delete_cache = cache.clone();
        let delete = tokio::spawn(async move {
            delete_started_in_task.notify_one();
            delete_cache.delete("key").await
        });
        delete_started.notified().await;
        tokio::task::yield_now().await;
        assert!(!delete.is_finished());
        assert_eq!(remote.delete_count.load(Ordering::Relaxed), 0);
        remote.release_write();

        write.await.unwrap().unwrap();
        assert!(delete.await.unwrap().unwrap());
        assert!(cache.get("key").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn mutation_wait_has_a_hard_timeout() {
        let remote = Arc::new(FakeRemote::default());
        let mut cache = hybrid_cache(Arc::clone(&remote));
        cache.mutation_timeout = Duration::from_millis(1);
        remote.block_writes();

        let first_cache = cache.clone();
        let first = tokio::spawn(async move {
            first_cache
                .set("key", b"first", Duration::from_secs(1))
                .await
        });
        remote.wait_for_write().await;
        assert_eq!(
            cache
                .set("key", b"second", Duration::from_secs(1))
                .await
                .unwrap_err(),
            CacheError::MutationTimeout
        );
        remote.release_write();
        first.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn different_keys_do_not_share_the_same_mutation_bottleneck() {
        let remote = Arc::new(FakeRemote::default());
        let cache = hybrid_cache(Arc::clone(&remote));
        assert_ne!(
            cache.mutation_shard_index("blocked-key"),
            cache.mutation_shard_index("independent-key")
        );
        remote.block_writes();

        let first_cache = cache.clone();
        let first = tokio::spawn(async move {
            first_cache
                .set("blocked-key", b"first", Duration::from_secs(1))
                .await
        });
        remote.wait_for_write().await;
        time::timeout(
            Duration::from_millis(250),
            cache.set("independent-key", b"second", Duration::from_secs(1)),
        )
        .await
        .expect("不同键的写入不应被同一分片阻塞")
        .unwrap();

        remote.release_write();
        first.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn rejects_oversized_values_before_copying_or_filling_l1() {
        let remote = Arc::new(FakeRemote::default());
        let config = HybridCacheConfig::new("test.v1", 2)
            .unwrap()
            .with_max_value_bytes(3)
            .unwrap();
        let cache = HybridCache::from_parts(config, Some(remote.clone())).unwrap();

        assert_eq!(
            cache
                .set("key", b"four", Duration::from_secs(1))
                .await
                .unwrap_err(),
            CacheError::ValueTooLarge
        );
        remote
            .set("anyflows:test.v1:key", b"four", Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(
            cache.get("key").await.unwrap_err(),
            CacheError::ValueTooLarge
        );
    }

    #[tokio::test]
    async fn cancelled_remote_write_invalidates_the_old_l1_value() {
        let remote = Arc::new(FakeRemote::default());
        let cache = hybrid_cache(Arc::clone(&remote));
        cache
            .set("key", b"old", Duration::from_secs(1))
            .await
            .unwrap();
        remote.block_writes();

        let pending_cache = cache.clone();
        let pending = tokio::spawn(async move {
            pending_cache
                .set("key", b"new", Duration::from_secs(1))
                .await
        });
        remote.wait_for_write().await;
        pending.abort();
        assert!(pending.await.unwrap_err().is_cancelled());
        remote.release_write();

        assert_eq!(cache.get("key").await.unwrap().unwrap().as_ref(), b"new");
    }
}
