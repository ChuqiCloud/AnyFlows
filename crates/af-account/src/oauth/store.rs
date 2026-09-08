use std::{
    collections::HashMap,
    fmt,
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};

use super::authorization::{
    OAuthAuthorizationCallback, OAuthAuthorizationError, OAuthAuthorizationGrant,
    OAuthAuthorizationRequest, OAuthAuthorizationStart, PendingAuthorization, SecretText,
    StateDigest, build_authorization_url, code_challenge, generate_random_token,
};
use af_domain::UserId;

/// 默认最多保留的待完成 OAuth 授权会话数。
pub const DEFAULT_MAX_PENDING_OAUTH_AUTHORIZATIONS: usize = 256;
/// 单实例允许配置的待授权会话硬上限。
pub const MAX_PENDING_OAUTH_AUTHORIZATIONS: usize = 4_096;
/// 默认授权会话有效期。
pub const DEFAULT_OAUTH_AUTHORIZATION_SESSION_TTL: Duration = Duration::from_secs(10 * 60);
/// 授权会话允许的最短有效期。
pub const MIN_OAUTH_AUTHORIZATION_SESSION_TTL: Duration = Duration::from_secs(60);
/// 授权会话允许的最长有效期。
pub const MAX_OAUTH_AUTHORIZATION_SESSION_TTL: Duration = Duration::from_secs(15 * 60);
const RANDOM_COLLISION_RETRIES: usize = 4;

/// 进程内待授权会话存储；state 仅以 SHA-256 摘要作为索引，敏感材料随条目释放清零。
pub struct OAuthAuthorizationSessionStore {
    entries: Mutex<HashMap<StateDigest, PendingAuthorization>>,
    max_pending: usize,
    ttl: Duration,
}

impl OAuthAuthorizationSessionStore {
    /// 创建有界会话存储；TTL 必须是 1–15 分钟内的整秒值。
    pub fn new(max_pending: usize, ttl: Duration) -> Result<Self, OAuthAuthorizationError> {
        if max_pending == 0 || max_pending > MAX_PENDING_OAUTH_AUTHORIZATIONS {
            return Err(OAuthAuthorizationError::InvalidSessionCapacity);
        }
        if !(MIN_OAUTH_AUTHORIZATION_SESSION_TTL..=MAX_OAUTH_AUTHORIZATION_SESSION_TTL)
            .contains(&ttl)
            || ttl.subsec_nanos() != 0
        {
            return Err(OAuthAuthorizationError::InvalidSessionTtl);
        }
        Ok(Self {
            entries: Mutex::new(HashMap::with_capacity(max_pending.min(64))),
            max_pending,
            ttl,
        })
    }

    /// 创建默认 256 条、10 分钟有效期的存储。
    #[must_use]
    pub fn with_defaults() -> Self {
        Self {
            entries: Mutex::new(HashMap::with_capacity(64)),
            max_pending: DEFAULT_MAX_PENDING_OAUTH_AUTHORIZATIONS,
            ttl: DEFAULT_OAUTH_AUTHORIZATION_SESSION_TTL,
        }
    }

    /// 生成 state 与 PKCE verifier，原子登记会话并返回浏览器授权地址。
    pub fn begin(
        &self,
        request: &OAuthAuthorizationRequest,
    ) -> Result<OAuthAuthorizationStart, OAuthAuthorizationError> {
        self.begin_at(request, Instant::now())
    }

    /// 一次性消费 loopback 回调；成功返回授权码和 verifier，拒绝回调也会销毁会话。
    pub fn consume(
        &self,
        callback: OAuthAuthorizationCallback,
    ) -> Result<OAuthAuthorizationGrant, OAuthAuthorizationError> {
        self.consume_at(callback, Instant::now())
    }

    /// 仅允许发起该授权的同一用户一次性消费手动回调。
    pub fn consume_for(
        &self,
        callback: OAuthAuthorizationCallback,
        expected_user_id: UserId,
    ) -> Result<OAuthAuthorizationGrant, OAuthAuthorizationError> {
        self.consume_inner(callback, Some(expected_user_id), Instant::now())
    }

    /// 清理所有已过期条目，返回实际删除数。
    pub fn cleanup_expired(&self) -> Result<usize, OAuthAuthorizationError> {
        self.cleanup_expired_at(Instant::now())
    }

    /// 返回当前未过期的待授权数量，供有界指标采集。
    pub fn pending_count(&self) -> Result<usize, OAuthAuthorizationError> {
        self.pending_count_at(Instant::now())
    }

    pub(crate) fn begin_at(
        &self,
        request: &OAuthAuthorizationRequest,
        now: Instant,
    ) -> Result<OAuthAuthorizationStart, OAuthAuthorizationError> {
        let expires_at = now
            .checked_add(self.ttl)
            .ok_or(OAuthAuthorizationError::InvalidSessionTtl)?;
        for _ in 0..RANDOM_COLLISION_RETRIES {
            let state = SecretText::new(generate_random_token()?);
            let verifier = SecretText::new(generate_random_token()?);
            let digest = StateDigest::from_state(state.expose());
            let authorization_url = build_authorization_url(
                request,
                state.expose(),
                &code_challenge(verifier.expose()),
            )?;
            let mut entries = self.lock_entries()?;
            retain_active(&mut entries, now);
            if entries.len() >= self.max_pending {
                return Err(OAuthAuthorizationError::CapacityExceeded);
            }
            if entries.contains_key(&digest) {
                continue;
            }
            let redirect_uri = request.redirect_uri().clone();
            entries.insert(
                digest,
                PendingAuthorization {
                    provider: request.provider(),
                    context: request.context(),
                    redirect_uri: redirect_uri.clone(),
                    code_verifier: verifier,
                    expires_at,
                },
            );
            return Ok(OAuthAuthorizationStart::new(
                request.provider(),
                request.context(),
                authorization_url,
                redirect_uri,
                expires_at,
            ));
        }
        Err(OAuthAuthorizationError::Entropy)
    }

    pub(crate) fn consume_at(
        &self,
        callback: OAuthAuthorizationCallback,
        now: Instant,
    ) -> Result<OAuthAuthorizationGrant, OAuthAuthorizationError> {
        self.consume_inner(callback, None, now)
    }

    fn consume_inner(
        &self,
        mut callback: OAuthAuthorizationCallback,
        expected_user_id: Option<UserId>,
        now: Instant,
    ) -> Result<OAuthAuthorizationGrant, OAuthAuthorizationError> {
        let digest = StateDigest::from_state(callback.state());
        let mut entries = self.lock_entries()?;
        let pending = entries
            .get(&digest)
            .ok_or(OAuthAuthorizationError::SessionNotFound)?;
        if pending.is_expired(now) {
            entries.remove(&digest);
            return Err(OAuthAuthorizationError::SessionExpired);
        }
        if pending.provider != callback.provider() {
            return Err(OAuthAuthorizationError::ProviderMismatch);
        }
        if &pending.redirect_uri != callback.redirect_uri() {
            return Err(OAuthAuthorizationError::RedirectUriMismatch);
        }
        if expected_user_id.is_some_and(|expected| pending.context.user_id() != expected) {
            return Err(OAuthAuthorizationError::PrincipalMismatch);
        }
        // 所有绑定条件通过后才消费，错误 provider、地址或管理员不能销毁合法会话。
        let pending = entries
            .remove(&digest)
            .ok_or(OAuthAuthorizationError::SessionNotFound)?;
        drop(entries);
        if callback.is_denied() {
            return Err(OAuthAuthorizationError::ProviderDenied);
        }
        let state = callback.take_state();
        let authorization_code = callback
            .take_authorization_code()
            .ok_or(OAuthAuthorizationError::InvalidAuthorizationCode)?;
        Ok(OAuthAuthorizationGrant::new(
            pending.provider,
            pending.context,
            pending.redirect_uri,
            state,
            authorization_code,
            pending.code_verifier.into_inner(),
        ))
    }

    pub(crate) fn cleanup_expired_at(
        &self,
        now: Instant,
    ) -> Result<usize, OAuthAuthorizationError> {
        let mut entries = self.lock_entries()?;
        let before = entries.len();
        retain_active(&mut entries, now);
        Ok(before - entries.len())
    }

    pub(crate) fn pending_count_at(&self, now: Instant) -> Result<usize, OAuthAuthorizationError> {
        let mut entries = self.lock_entries()?;
        retain_active(&mut entries, now);
        Ok(entries.len())
    }

    fn lock_entries(
        &self,
    ) -> Result<MutexGuard<'_, HashMap<StateDigest, PendingAuthorization>>, OAuthAuthorizationError>
    {
        self.entries
            .lock()
            .map_err(|_| OAuthAuthorizationError::StoreUnavailable)
    }
}

impl fmt::Debug for OAuthAuthorizationSessionStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthAuthorizationSessionStore")
            .field("entries", &"<已脱敏>")
            .field("max_pending", &self.max_pending)
            .field("ttl", &self.ttl)
            .finish()
    }
}

fn retain_active(entries: &mut HashMap<StateDigest, PendingAuthorization>, now: Instant) {
    entries.retain(|_, pending| !pending.is_expired(now));
}
