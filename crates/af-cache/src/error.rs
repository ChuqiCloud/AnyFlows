use std::fmt;

use thiserror::Error;

/// Redis 操作阶段，用于稳定分类错误而不暴露连接信息。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CacheOperation {
    Connect,
    HealthCheck,
    Read,
    Write,
    Delete,
    LeaseAcquire,
    LeaseRelease,
}

impl fmt::Display for CacheOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Connect => "连接",
            Self::HealthCheck => "健康检查",
            Self::Read => "读取",
            Self::Write => "写入",
            Self::Delete => "删除",
            Self::LeaseAcquire => "获取租约",
            Self::LeaseRelease => "释放租约",
        };
        formatter.write_str(name)
    }
}

/// Redis 故障类别，仅保留调用方可安全处理的信息。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RedisFailureKind {
    Configuration,
    Authentication,
    Timeout,
    Unavailable,
    Protocol,
    Rejected,
    Unknown,
}

impl fmt::Display for RedisFailureKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Configuration => "配置错误",
            Self::Authentication => "认证失败",
            Self::Timeout => "超时",
            Self::Unavailable => "服务不可用",
            Self::Protocol => "协议错误",
            Self::Rejected => "服务端拒绝",
            Self::Unknown => "未知错误",
        };
        formatter.write_str(name)
    }
}

/// 缓存配置与运行期错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum CacheError {
    #[error("缓存命名空间必须为 1 到 64 个 ASCII 字母、数字、点、短横线或下划线")]
    InvalidNamespace,
    #[error("本地缓存容量必须大于零")]
    InvalidCapacity,
    #[error("单个缓存值的字节上限必须在 1 到 i64::MAX 之间")]
    InvalidMaxValueSize,
    #[error("Redis 地址无效")]
    InvalidRedisUrl,
    #[error("Redis 连接与操作超时必须大于零")]
    InvalidTimeout,
    #[error("缓存键必须非空、不得包含控制字符且长度不超过 1024 字节")]
    InvalidKey,
    #[error("缓存 TTL 必须为可表示的整毫秒正数")]
    InvalidTtl,
    #[error("无法取得分布式租约所有者所需的安全随机数")]
    EntropyUnavailable,
    #[error("缓存值超过当前配置允许的大小")]
    ValueTooLarge,
    #[error("本地缓存锁不可用")]
    LocalUnavailable,
    #[error("缓存同实例写入排序等待超时")]
    MutationTimeout,
    #[error("Redis {operation}失败（{kind}）")]
    Redis {
        operation: CacheOperation,
        kind: RedisFailureKind,
    },
}

impl CacheError {
    pub(crate) const fn redis(operation: CacheOperation, kind: RedisFailureKind) -> Self {
        Self::Redis { operation, kind }
    }
}
