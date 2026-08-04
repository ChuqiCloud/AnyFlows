//! 面向可重建数据的进程内 LRU 与 Redis 两级缓存。
//!
//! Redis 未配置时显式运行在单实例本地模式；一旦配置，连接与命令故障会返回
//! 类型化错误，不会静默切换为本地权威状态。分布式协调能力应使用独立的 Redis
//! 原子操作实现，不属于本 crate 的 HybridCache 语义。

mod broadcast;
mod config;
mod error;
mod hybrid;
mod lease;
mod local;
mod redis_backend;
mod remote;

use std::sync::Arc;

pub use broadcast::{
    DEFAULT_BROADCAST_MAX_MESSAGE_BYTES, RedisBroadcastConfig, RedisBroadcastPublisher,
    RedisBroadcastSubscriber,
};
pub use config::{
    DEFAULT_LOCAL_TTL_CAP, DEFAULT_MAX_VALUE_BYTES, DEFAULT_REDIS_CONNECT_TIMEOUT,
    DEFAULT_REDIS_OPERATION_TIMEOUT, HybridCacheConfig, RedisConfig,
};
pub use error::{CacheError, CacheOperation, RedisFailureKind};
pub use hybrid::{CacheMode, HybridCache};
pub use lease::{
    DistributedLease, DistributedLeaseConfig, DistributedLeaseManager, DistributedLeaseMode,
    LeaseAcquireOutcome, LeaseReleaseOutcome,
};

/// 缓存统一存储字节，由调用方负责稳定的编码版本。
pub type CacheValue = Arc<[u8]>;
