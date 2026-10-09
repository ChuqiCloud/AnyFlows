//! 面向可重建数据的进程内 LRU 与 Redis 两级缓存。
//!
//! Redis 未配置时显式运行在单实例本地模式；一旦配置，连接与命令故障会返回
//! 类型化错误，不会静默切换为本地权威状态。分布式协调能力应使用独立的 Redis
//! 原子操作实现，不属于本 crate 的 HybridCache 语义。

mod broadcast;
mod concurrency;
mod config;
mod error;
mod health;
mod hybrid;
mod lease;
mod local;
mod projection;
mod rate_limit;
mod redis_backend;
mod remote;
mod sticky;

use std::sync::Arc;

pub use broadcast::{
    DEFAULT_BROADCAST_MAX_MESSAGE_BYTES, RedisBroadcastConfig, RedisBroadcastPublisher,
    RedisBroadcastSubscriber,
};
pub use concurrency::{
    ConcurrencyAccountLoad, ConcurrencyAcquireOutcome, ConcurrencyCleanupReport,
    ConcurrencyLeaseOutcome, ConcurrencyWaitOutcome, DEFAULT_CONCURRENCY_SLOT_TTL,
    DEFAULT_CONCURRENCY_WAIT_TTL, MAX_CONCURRENCY_CLEANUP_BATCH_SIZE,
    MAX_CONCURRENCY_LOAD_BATCH_SIZE, RedisConcurrencyConfig, RedisConcurrencyLease,
    RedisConcurrencyStore, RedisConcurrencyWait,
};
pub use config::{
    DEFAULT_LOCAL_TTL_CAP, DEFAULT_MAX_VALUE_BYTES, DEFAULT_REDIS_CONNECT_TIMEOUT,
    DEFAULT_REDIS_OPERATION_TIMEOUT, HybridCacheConfig, RedisConfig,
};
pub use error::{CacheError, CacheOperation, RedisFailureKind};
pub use health::{
    HEALTH_PENALTY_SCALE, MAX_HEALTH_BATCH_SIZE, RedisHealthConfig, RedisHealthEvent,
    RedisHealthFailure, RedisHealthSnapshot, RedisHealthStore, RedisHealthTarget,
};
pub use hybrid::{CacheMode, HybridCache};
pub use lease::{
    DistributedLease, DistributedLeaseConfig, DistributedLeaseManager, DistributedLeaseMode,
    LeaseAcquireOutcome, LeaseReleaseOutcome,
};
pub use projection::{
    ProjectionWriteOutcome, RedisProjectionConfig, RedisProjectionEntry,
    RedisVersionedProjectionStore,
};
pub use rate_limit::{
    FingerprintRateLimitRule, MAX_REQUEST_RATE_LIMIT_RULES, RedisRequestRateLimitConfig,
    RedisRequestRateLimitStore, RequestRateLimitOutcome, RequestRateLimitRejection,
    RequestRateLimitRule, RequestRateLimitSubject,
};
pub use sticky::{DEFAULT_STICKY_SESSION_TTL, RedisStickySessionConfig, RedisStickySessionStore};

/// 缓存统一存储字节，由调用方负责稳定的编码版本。
pub type CacheValue = Arc<[u8]>;
