use std::{fmt, fmt::Write as _, future::Future, pin::Pin};

use af_cache::{CacheError, RedisStickySessionStore};
use af_domain::{ChannelId, ClientProfileSnapshot, GatewayIdentity};
use af_protocol::CanonicalRequest;
use sha2::{Digest, Sha256};

/// 粘性存储异步操作的对象安全返回类型。
pub(crate) type StickySessionStoreFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CacheError>> + Send + 'a>>;

/// 粘性会话存储端口；生产实现使用 Redis，测试可注入脱敏内存替身。
pub(crate) trait StickySessionStore: Send + Sync {
    fn get_or_insert_epoch<'a>(
        &'a self,
        digest: &'a str,
        proposed: u64,
    ) -> StickySessionStoreFuture<'a, u64>;
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
    fn get_or_insert_epoch<'a>(
        &'a self,
        digest: &'a str,
        proposed: u64,
    ) -> StickySessionStoreFuture<'a, u64> {
        Box::pin(self.get_or_insert_epoch(digest, proposed))
    }
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
pub(crate) struct StableSessionScope {
    digest: String,
    inferred: bool,
}

impl StableSessionScope {
    pub(crate) fn for_client_profile(
        &self,
        channel: ChannelId,
        profile: Option<&ClientProfileSnapshot>,
    ) -> Self {
        let mut digest = Sha256::new();
        digest.update(b"AnyFlows:client-session-scope:v1\0");
        digest.update(self.digest.as_bytes());
        digest.update(channel.get().to_be_bytes());
        if let Some(profile) = profile {
            digest.update(profile.binding.key.as_bytes());
            digest.update(profile.binding.revision.to_be_bytes());
        }
        Self {
            digest: hex_digest(digest),
            inferred: self.inferred,
        }
    }

    pub(crate) const fn is_inferred(&self) -> bool {
        self.inferred
    }

    /// 返回用于 Redis 键和 WebSocket 池隔离的脱敏摘要。
    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.digest
    }
}

impl fmt::Debug for StableSessionScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("StableSessionScope(<已脱敏>)")
    }
}

/// 按受控元数据选择稳定会话身份并计算不可逆摘要。
pub(crate) fn stable_identity_session_scope(
    identity: GatewayIdentity,
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

    Some(hash_identity_scope(identity, kind, value.as_bytes()))
}

pub(crate) fn first_user_message_session_scope(
    identity: GatewayIdentity,
    request: &CanonicalRequest,
) -> Option<StableSessionScope> {
    let first = request
        .messages
        .iter()
        .find(|message| message.role == af_domain::Role::User)?;
    let mut bytes = Vec::new();
    for block in &first.content {
        match block {
            af_protocol::ContentBlock::Text(text) => {
                let length = text.len().min(16_384_usize.saturating_sub(bytes.len()));
                bytes.extend_from_slice(&text.as_bytes()[..length]);
                if bytes.len() == 16_384 {
                    break;
                }
            }
            // 有多模态内容或工具结果时不猜测等价的会话标识。
            _ => return None,
        }
    }
    (!bytes.is_empty()).then(|| hash_identity_scope(identity, b'f', &bytes))
}

fn hash_identity_scope(identity: GatewayIdentity, kind: u8, value: &[u8]) -> StableSessionScope {
    let mut digest = Sha256::new();
    match identity {
        GatewayIdentity::User(principal) => {
            digest.update(b"AnyFlows:stable-session-scope:v1\0");
            digest.update(principal.token_id().get().to_be_bytes());
        }
        GatewayIdentity::ServiceAccount(identity) => {
            digest.update(b"AnyFlows:service-session-scope:v1\0");
            digest.update(identity.locator().organization_id().get().to_be_bytes());
            digest.update(identity.locator().service_account_id().get().to_be_bytes());
            digest.update(identity.account_key().persistence_key().as_bytes());
            digest.update(identity.credential_key().persistence_key().as_bytes());
        }
    }
    digest.update([kind]);
    digest.update(u32::try_from(value.len()).unwrap_or(u32::MAX).to_be_bytes());
    digest.update(value);
    StableSessionScope {
        digest: hex_digest(digest),
        inferred: kind == b'f',
    }
}

fn hex_digest(digest: Sha256) -> String {
    let digest = digest.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("写入 String 不会失败");
    }
    encoded
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

#[cfg(test)]
mod service_tests {
    use super::*;
    use af_domain::{
        OrganizationId, OrganizationServiceAccountId, OrganizationServiceAccountKey,
        OrganizationServiceAccountLocator, OrganizationServiceAccountRuntimeIdentity,
    };

    #[test]
    fn first_message_scope_isolates_subject_channel_and_revision() {
        let identity = |credential| {
            GatewayIdentity::ServiceAccount(OrganizationServiceAccountRuntimeIdentity::new(
                OrganizationServiceAccountLocator::new(
                    OrganizationId::new(1).unwrap(),
                    OrganizationServiceAccountId::new(2).unwrap(),
                ),
                OrganizationServiceAccountKey::new([3; 16]).unwrap(),
                OrganizationServiceAccountKey::new([credential; 16]).unwrap(),
            ))
        };
        let first =
            af_protocol::openai_chat::parse_request_envelope(af_adapter::Bytes::from_static(
                br#"{"model":"model","messages":[{"role":"user","content":"remember"}]}"#,
            ))
            .unwrap();
        let next = af_protocol::openai_chat::parse_request_envelope(af_adapter::Bytes::from_static(br#"{"model":"model","messages":[{"role":"user","content":"remember"},{"role":"assistant","content":"ok"},{"role":"user","content":"what?"}]}"#)).unwrap();
        let scope = first_user_message_session_scope(identity(4), first.canonical()).unwrap();
        assert!(scope.is_inferred());
        assert_eq!(
            scope,
            first_user_message_session_scope(identity(4), next.canonical()).unwrap()
        );
        assert_ne!(
            scope,
            first_user_message_session_scope(identity(5), first.canonical()).unwrap()
        );
        let mut profile = af_domain::built_in_client_profiles().remove(0);
        let isolated = scope.for_client_profile(ChannelId::new(1).unwrap(), Some(&profile));
        assert_ne!(
            isolated,
            scope.for_client_profile(ChannelId::new(2).unwrap(), Some(&profile))
        );
        profile.binding.revision += 1;
        assert_ne!(
            isolated,
            scope.for_client_profile(ChannelId::new(1).unwrap(), Some(&profile))
        );
        assert!(!format!("{scope:?}").contains("remember"));
    }

    #[test]
    fn service_session_isolates_organizations_accounts_and_credentials() {
        let request = af_protocol::openai_chat::parse_request_envelope(af_adapter::Bytes::from_static(br#"{"model":"gpt-5","messages":[{"role":"user","content":"hello"}],"user":"shared-session"}"#)).unwrap();
        let scope = |organization, account, credential| {
            stable_identity_session_scope(
                GatewayIdentity::ServiceAccount(OrganizationServiceAccountRuntimeIdentity::new(
                    OrganizationServiceAccountLocator::new(
                        OrganizationId::new(organization).unwrap(),
                        OrganizationServiceAccountId::new(account).unwrap(),
                    ),
                    OrganizationServiceAccountKey::new([3; 16]).unwrap(),
                    OrganizationServiceAccountKey::new([credential; 16]).unwrap(),
                )),
                request.canonical(),
            )
            .unwrap()
            .as_str()
            .to_owned()
        };
        let first = scope(1, 2, 4);
        assert_eq!(first, scope(1, 2, 4));
        assert_ne!(first, scope(2, 2, 4));
        assert_ne!(first, scope(1, 3, 4));
        assert_ne!(first, scope(1, 2, 5));
    }
}
