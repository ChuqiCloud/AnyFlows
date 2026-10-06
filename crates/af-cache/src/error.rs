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
    Publish,
    Subscribe,
    Receive,
    ProjectionRead,
    ProjectionWrite,
    LeaseAcquire,
    LeaseRelease,
    StickyRead,
    StickyWrite,
    StickyRefresh,
    StickyDelete,
    ConcurrencyAcquire,
    ConcurrencyRenew,
    ConcurrencyRelease,
    ConcurrencyLoad,
    ConcurrencyCleanup,
    RateLimitCheck,
    HealthRead,
    HealthWrite,
}

impl fmt::Display for CacheOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Connect => "连接",
            Self::HealthCheck => "健康检查",
            Self::Read => "读取",
            Self::Write => "写入",
            Self::Delete => "删除",
            Self::Publish => "发布",
            Self::Subscribe => "订阅",
            Self::Receive => "接收",
            Self::ProjectionRead => "读取投影",
            Self::ProjectionWrite => "写入投影",
            Self::LeaseAcquire => "获取租约",
            Self::LeaseRelease => "释放租约",
            Self::StickyRead => "读取粘性会话",
            Self::StickyWrite => "写入粘性会话",
            Self::StickyRefresh => "续期粘性会话",
            Self::StickyDelete => "删除粘性会话",
            Self::ConcurrencyAcquire => "获取并发槽位",
            Self::ConcurrencyRenew => "续期并发槽位",
            Self::ConcurrencyRelease => "释放并发槽位",
            Self::ConcurrencyLoad => "读取并发负载",
            Self::ConcurrencyCleanup => "清理并发槽位",
            Self::RateLimitCheck => "检查请求限流",
            Self::HealthRead => "读取调度健康状态",
            Self::HealthWrite => "写入调度健康状态",
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
    #[error("Redis 广播通道必须为 1 到 64 个 ASCII 字母、数字、点、短横线或下划线")]
    InvalidBroadcastChannel,
    #[error("Redis 广播消息必须非空且不得超过配置的字节上限")]
    InvalidBroadcastPayload,
    #[error("Redis 投影版本必须大于零")]
    InvalidProjectionVersion,
    #[error("Redis 投影正文必须非空")]
    InvalidProjectionPayload,
    #[error("Redis 广播订阅连接已关闭")]
    BroadcastClosed,
    #[error("缓存 TTL 必须为可表示的整毫秒正数")]
    InvalidTtl,
    #[error("无法取得分布式租约所有者所需的安全随机数")]
    EntropyUnavailable,
    #[error("缓存值超过当前配置允许的大小")]
    ValueTooLarge,
    #[error("粘性会话摘要必须为 64 个十六进制字符")]
    InvalidStickyDigest,
    #[error("粘性会话渠道标识必须为正数")]
    InvalidStickyChannelId,
    #[error("并发槽位标识必须为正数")]
    InvalidConcurrencyIdentifier,
    #[error("并发槽位批次为空间无效、重复或超过容量上限")]
    InvalidConcurrencyBatch,
    #[error("请求限流规则批次包含重复项或超过容量上限")]
    InvalidRateLimitBatch,
    #[error("请求限流固定窗口不得超过七天")]
    InvalidRateLimitWindow,
    #[error("调度健康状态批次为空、超过容量上限或包含重复读取目标")]
    InvalidHealthBatch,
    #[error("调度健康状态策略参数无效")]
    InvalidHealthPolicy,
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
