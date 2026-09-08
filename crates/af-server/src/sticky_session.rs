use std::{fmt, fmt::Write as _, future::Future, pin::Pin};

use af_cache::{CacheError, RedisStickySessionStore};
use af_domain::GatewayPrincipal;
use af_protocol::CanonicalRequest;
use sha2::{Digest, Sha256};

/// 粘性存储异步操作的对象安全返回类型。
pub(crate) type StickySessionStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CacheError>> + Send + 'a>>;

/// 粘性会话存储端口；生产实现使用 Redis，测试可注入脱敏内存替身。
pub(crate) trait StickySessionStore: Send + Sync {
    /// 读取绑定并刷新其 TTL。
    fn get_and_refresh<'a>(&'a self, digest: &'a str) -> StickySessionStoreFuture<'a, Option<i64>>;

    /// 写入成功渠道绑定并覆盖 TTL。
    fn bind<'a>(&'a self, digest: &'a str, channel_id: i64) -> StickySessionStoreFuture<'a, ()>;

    /// 仅在绑定仍指向指定渠道时条件删除。
    fn delete_if_channel<'a>(
        &'a self,
        digest: &'a str,
        channel_id: i64,
    ) -> StickySessionStoreFuture<'a, bool>;
}

impl StickySessionStore for RedisStickySessionStore {
    fn get_and_refresh<'a>(&'a self, digest: &'a str) -> StickySessionStoreFuture<'a, Option<i64>> {
        Box::pin(self.get_and_refresh(digest))
    }

    fn bind<'a>(&'a self, digest: &'a str, channel_id: i64) -> StickySessionStoreFuture<'a, ()> {
        Box::pin(self.bind(digest, channel_id))
    }

    fn delete_if_channel<'a>(
        &'a self,
        digest: &'a str,
        channel_id: i64,
    ) -> StickySessionStoreFuture<'a, bool> {
        Box::pin(self.delete_if_channel(digest, channel_id))
    }
}

/// 只保存 SHA-256 十六进制摘要的稳定会话作用域；原文不得进入日志或 Redis。
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct StableSessionScope(String);

impl StableSessionScope {
    /// 返回用于 Redis 键和 WebSocket 池隔离的脱敏摘要。
    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for StableSessionScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StableSessionScope(<已脱敏>)")
    }
}

/// 按受控元数据选择稳定会话身份并计算不可逆摘要。
pub(crate) fn stable_session_scope(
    principal: GatewayPrincipal,
    request: &CanonicalRequest,
) -> Option<StableSessionScope> {
    let (kind, value) = request
        .metadata
        .session_id()
        .filter(|value| !value.is_empty())
        .map(|value| (b's', value))
        .or_else(|| {
            request
                .continuation
                .conversation_id()
                .filter(|value| !value.is_empty())
                .map(|value| (b'c', value))
        })
        .or_else(|| {
            request
                .continuation
                .prompt_cache_key()
                .filter(|value| !value.is_empty())
                .map(|value| (b'p', value))
        })
        .or_else(|| {
            request
                .metadata
                .user_id()
                .filter(|value| !value.is_empty())
                .map(|value| (b'u', value))
        })?;

    let mut digest = Sha256::new();
    digest.update(b"AnyFlows:stable-session-scope:v1\0");
    digest.update(principal.token_id().get().to_be_bytes());
    digest.update([kind]);
    digest.update(u32::try_from(value.len()).unwrap_or(u32::MAX).to_be_bytes());
    digest.update(value.as_bytes());
    let digest = digest.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("写入 String 不会失败");
    }
    Some(StableSessionScope(encoded))
}

/// 把缓存错误压缩成不含连接信息的日志类别。
pub(crate) const fn sticky_cache_error_kind(error: &CacheError) -> &'static str {
    match error {
        CacheError::InvalidStickyDigest => "invalid_digest",
        CacheError::InvalidStickyChannelId => "invalid_channel_id",
        CacheError::Redis { kind, .. } => match kind {
            af_cache::RedisFailureKind::Configuration => "configuration",
            af_cache::RedisFailureKind::Authentication => "authentication",
            af_cache::RedisFailureKind::Timeout => "timeout",
            af_cache::RedisFailureKind::Unavailable => "unavailable",
            af_cache::RedisFailureKind::Protocol => "protocol",
            af_cache::RedisFailureKind::Rejected => "rejected",
            af_cache::RedisFailureKind::Unknown => "unknown",
            _ => "unknown",
        },
        _ => "invalid",
    }
}
