//! 可订阅的代理池；健康检测与模型调用使用相同的受控网络边界。
mod parser;
pub use af_db::{ProxyCleanupMode, ProxyCleanupRequest, ProxyCleanupResult, ProxyCleanupTarget};

use std::{
    future::Future,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};

use af_account::{PlainSystemSecret, SystemSecretCipher, SystemSecretKind};
use af_db::{
    CredentialProxyCreateRecord, CredentialProxyRecord, CredentialProxyScheme, ProxyPoolRecord,
    ProxyPoolRepository, ProxyPoolRepositoryError,
};
use af_domain::{ChannelId, CredentialId, ProxyId};
use af_httpclient::{
    HeaderMap, HttpClientProvider, Method, PooledClient, ProxyConfig, RemoteDnsPolicy,
};
use futures_util::{StreamExt as _, stream};
use serde::{Deserialize, Serialize};

use crate::{SessionPrincipal, SessionRole};

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProxyPoolError {
    #[error("代理池输入无效")]
    InvalidInput,
    #[error("需要管理员权限")]
    Forbidden,
    #[error("代理池不存在")]
    NotFound,
    #[error("代理池冲突、正在操作或仍被引用")]
    Conflict,
    #[error("代理池没有通过检测的可用节点")]
    Unavailable,
    #[error("代理池操作失败")]
    Internal,
}
impl From<ProxyPoolRepositoryError> for ProxyPoolError {
    fn from(value: ProxyPoolRepositoryError) -> Self {
        match value {
            ProxyPoolRepositoryError::NotFound => Self::NotFound,
            ProxyPoolRepositoryError::Conflict => Self::Conflict,
            ProxyPoolRepositoryError::InvalidInput => Self::InvalidInput,
            ProxyPoolRepositoryError::Internal => Self::Internal,
        }
    }
}

/// URL 不实现 Debug，也不通过管理投影回传，防止查询 token 泄露。
#[derive(Serialize, Deserialize)]
struct Settings {
    source_url: Option<String>,
    default_scheme: String,
    trust_proxy_dns: bool,
    probe_url: String,
}
pub struct ProxyPoolWrite {
    pub name: String,
    pub subscribed: bool,
    pub source_url: Option<String>,
    pub interval_secs: i32,
    pub default_scheme: String,
    pub trust_proxy_dns: bool,
    pub probe_url: Option<String>,
    pub content: Option<String>,
    pub revision: Option<i64>,
}
pub struct ProxyPoolView {
    pub id: i64,
    pub name: String,
    pub subscribed: bool,
    pub source_host: Option<String>,
    pub interval_secs: i32,
    pub default_scheme: String,
    pub trust_proxy_dns: bool,
    pub probe_url: String,
    pub revision: i64,
    pub last_refresh_at: Option<i64>,
    pub last_refresh_attempt_at: Option<i64>,
    pub last_check_at: Option<i64>,
    pub status: String,
    pub busy: bool,
    pub nodes: Vec<ProxyPoolNodeView>,
    pub channel_ids: Vec<i64>,
}
pub struct ProxyPoolNodeView {
    pub id: i64,
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub password_configured: bool,
    pub healthy: bool,
    pub latency_ms: Option<i64>,
    pub checked_at: Option<i64>,
    pub remark: String,
    pub enabled: bool,
}
pub struct ProxyPoolImportResult {
    pub pool: ProxyPoolView,
    pub skipped: usize,
    pub duplicates: usize,
}

type RuntimeSnapshot = Option<(Instant, Arc<Vec<ProxyPoolRecord>>)>;

pub struct ProxyPoolService {
    repository: ProxyPoolRepository,
    cipher: SystemSecretCipher,
    clients: OnceLock<HttpClientProvider>,
    runtime: tokio::sync::Mutex<RuntimeSnapshot>,
}
impl ProxyPoolService {
    #[must_use]
    pub fn new(repository: ProxyPoolRepository, cipher: SystemSecretCipher) -> Self {
        Self {
            repository,
            cipher,
            clients: OnceLock::new(),
            runtime: tokio::sync::Mutex::new(None),
        }
    }
    pub fn configure_clients(&self, clients: HttpClientProvider) {
        let _ = self.clients.set(clients);
    }
    async fn invalidate(&self) {
        *self.runtime.lock().await = None;
    }
    async fn snapshot(&self) -> Result<Arc<Vec<ProxyPoolRecord>>, ProxyPoolError> {
        let mut cache = self.runtime.lock().await;
        if let Some((loaded, pools)) = cache.as_ref()
            && loaded.elapsed() < Duration::from_secs(5)
        {
            return Ok(Arc::clone(pools));
        }
        let pools = Arc::new(self.repository.list().await?);
        *cache = Some((Instant::now(), Arc::clone(&pools)));
        Ok(pools)
    }
    fn settings(&self, pool: &ProxyPoolRecord) -> Result<Settings, ProxyPoolError> {
        let plaintext = self
            .cipher
            .decrypt(
                SystemSecretKind::ProxyPoolSettings(pool.proxy.proxy_id()),
                &pool.settings_secret,
            )
            .map_err(|_| ProxyPoolError::Internal)?;
        serde_json::from_str(plaintext.expose_secret()).map_err(|_| ProxyPoolError::Internal)
    }
    fn view(&self, pool: ProxyPoolRecord) -> Result<ProxyPoolView, ProxyPoolError> {
        let settings = self.settings(&pool)?;
        Ok(ProxyPoolView {
            id: pool.proxy.proxy_id().get(),
            name: pool.proxy.name().to_owned(),
            subscribed: pool.subscribed,
            source_host: settings
                .source_url
                .and_then(|url| url::Url::parse(&url).ok()?.host_str().map(str::to_owned)),
            interval_secs: pool.interval_secs,
            default_scheme: settings.default_scheme,
            trust_proxy_dns: settings.trust_proxy_dns,
            probe_url: settings.probe_url,
            revision: pool.revision,
            last_refresh_at: pool.last_refresh_at,
            last_refresh_attempt_at: pool.last_refresh_attempt_at,
            last_check_at: pool.last_check_at,
            status: pool.status,
            busy: pool.work_until > now(),
            nodes: pool
                .nodes
                .into_iter()
                .map(|n| ProxyPoolNodeView {
                    id: n.proxy.proxy_id().get(),
                    scheme: n.proxy.scheme().as_str().to_owned(),
                    host: n.proxy.host().to_owned(),
                    port: n.proxy.port(),
                    password_configured: n.proxy.password_configured(),
                    healthy: n.healthy
                        && n.proxy.enabled()
                        && n.checked_at.is_some_and(|at| {
                            now().saturating_sub(at)
                                <= i64::from(pool.interval_secs).saturating_mul(2).max(60)
                        }),
                    latency_ms: n.latency_ms,
                    checked_at: n.checked_at,
                    remark: n.remark,
                    enabled: n.proxy.enabled(),
                })
                .collect(),
            channel_ids: pool.channel_ids,
        })
    }
    pub async fn list(
        &self,
        principal: SessionPrincipal,
    ) -> Result<Vec<ProxyPoolView>, ProxyPoolError> {
        admin(principal)?;
        self.repository
            .list()
            .await?
            .into_iter()
            .map(|p| self.view(p))
            .collect()
    }
    pub async fn save(
        &self,
        principal: SessionPrincipal,
        id: Option<ProxyId>,
        write: ProxyPoolWrite,
    ) -> Result<ProxyPoolImportResult, ProxyPoolError> {
        admin(principal)?;
        if write.name.is_empty()
            || write.name.len() > 128
            || write.name.trim() != write.name
            || write.name.chars().any(char::is_control)
            || !(60..=86400).contains(&write.interval_secs)
        {
            return Err(ProxyPoolError::InvalidInput);
        }
        let scheme = parse_scheme(&write.default_scheme)?;
        let old = match id {
            Some(id) => Some(self.repository.get(id).await?),
            None => None,
        };
        let expected_revision = old.as_ref().map_or(1, |pool| pool.revision + 1);
        let previous = old.as_ref().map(|p| self.settings(p)).transpose()?;
        let source_url = if write.subscribed {
            write
                .source_url
                .filter(|u| !u.is_empty())
                .or_else(|| previous.as_ref().and_then(|s| s.source_url.clone()))
                .ok_or(ProxyPoolError::InvalidInput)
                .map(Some)?
        } else {
            None
        };
        if let Some(url) = &source_url {
            validate_url(url, false)?;
        }
        let probe_url = write
            .probe_url
            .filter(|u| !u.is_empty())
            .or_else(|| previous.as_ref().map(|s| s.probe_url.clone()))
            .unwrap_or_else(|| "https://www.gstatic.com/generate_204".to_owned());
        validate_url(&probe_url, true)?;
        let parsed = write
            .content
            .as_deref()
            .filter(|c| !c.trim().is_empty())
            .map(|text| parser::parse(text, scheme))
            .transpose()?;
        let settings = Settings {
            source_url,
            default_scheme: write.default_scheme,
            trust_proxy_dns: write.trust_proxy_dns,
            probe_url,
        };
        let plain = PlainSystemSecret::new(
            serde_json::to_string(&settings).map_err(|_| ProxyPoolError::Internal)?,
        )
        .map_err(|_| ProxyPoolError::InvalidInput)?;
        let id = if let Some(pool) = old {
            if write.revision != Some(pool.revision) {
                return Err(ProxyPoolError::Conflict);
            }
            let id = pool.proxy.proxy_id();
            let encrypted = self
                .cipher
                .encrypt(SystemSecretKind::ProxyPoolSettings(id), &plain)
                .map_err(|_| ProxyPoolError::Internal)?;
            self.repository
                .update(
                    id,
                    pool.revision,
                    write.name,
                    write.subscribed,
                    write.interval_secs,
                    encrypted,
                )
                .await?;
            id
        } else {
            self.repository
                .create(write.name, write.subscribed, write.interval_secs, |id| {
                    self.cipher
                        .encrypt(SystemSecretKind::ProxyPoolSettings(id), &plain)
                        .map_err(|_| ProxyPoolRepositoryError::Internal)
                })
                .await?
        };
        let (skipped, duplicates) = parsed
            .as_ref()
            .map_or((0, 0), |p| (p.skipped, p.duplicates));
        if let Some(parsed) = parsed {
            let pool = self.repository.get(id).await?;
            if pool.revision != expected_revision {
                return Err(ProxyPoolError::Conflict);
            }
            self.store_nodes(&pool, parsed.nodes, settings.trust_proxy_dns)
                .await?;
        }
        self.invalidate().await;
        Ok(ProxyPoolImportResult {
            pool: self.view(self.repository.get(id).await?)?,
            skipped,
            duplicates,
        })
    }
    async fn store_nodes(
        &self,
        pool: &ProxyPoolRecord,
        candidates: Vec<parser::Candidate>,
        trust_dns: bool,
    ) -> Result<(), ProxyPoolError> {
        let records = candidates
            .iter()
            .map(|node| {
                (
                    node.fingerprint.clone(),
                    node.remark.clone(),
                    CredentialProxyCreateRecord::new(
                        format!(
                            "pool-{}-{}",
                            pool.proxy.proxy_id().get(),
                            &node.fingerprint[..16]
                        ),
                        node.scheme,
                        node.host.clone(),
                        node.port,
                        node.username.clone(),
                        trust_dns,
                        true,
                    ),
                )
            })
            .collect();
        self.repository
            .replace_nodes(
                pool.proxy.proxy_id(),
                pool.revision,
                records,
                |index, id| {
                    candidates[index]
                        .password
                        .as_ref()
                        .map(|password| {
                            let plain = PlainSystemSecret::new(password.clone())
                                .map_err(|_| ProxyPoolRepositoryError::InvalidInput)?;
                            self.cipher
                                .encrypt(SystemSecretKind::CredentialProxyPassword(id), &plain)
                                .map_err(|_| ProxyPoolRepositoryError::Internal)
                        })
                        .transpose()
                },
            )
            .await?;
        Ok(())
    }
    pub async fn action(
        &self,
        principal: SessionPrincipal,
        id: ProxyId,
        refresh: bool,
    ) -> Result<ProxyPoolView, ProxyPoolError> {
        admin(principal)?;
        self.maintain(id, refresh).await?;
        self.view(self.repository.get(id).await?)
    }
    async fn maintain(&self, id: ProxyId, refresh: bool) -> Result<(), ProxyPoolError> {
        let pool = self.repository.get(id).await?;
        if !self
            .repository
            .claim(id, pool.revision, now(), refresh)
            .await?
        {
            return Err(ProxyPoolError::Conflict);
        }
        let result = self.maintain_claimed(&pool, refresh).await;
        match result {
            Ok(measurements) => {
                let status = if measurements.iter().any(|(_, _, latency)| latency.is_some()) {
                    "ready"
                } else {
                    "no_healthy_nodes"
                };
                self.repository
                    .finish(id, pool.revision, status, measurements)
                    .await?;
                self.invalidate().await;
                Ok(())
            }
            Err(error) => {
                self.repository
                    .finish(id, pool.revision, "refresh_failed", Vec::new())
                    .await?;
                self.invalidate().await;
                Err(error)
            }
        }
    }
    async fn maintain_claimed(
        &self,
        pool: &ProxyPoolRecord,
        refresh: bool,
    ) -> Result<Vec<(ProxyId, i64, Option<i64>)>, ProxyPoolError> {
        let settings = self.settings(pool)?;
        if refresh && let Some(source) = &settings.source_url {
            let clients = self.clients.get().ok_or(ProxyPoolError::Internal)?;
            let client = clients
                .get(Some(Duration::from_secs(15)))
                .map_err(|_| ProxyPoolError::Internal)?;
            let response = client
                .execute(Method::GET, source, HeaderMap::new(), None)
                .await
                .map_err(|_| ProxyPoolError::Unavailable)?;
            if !response.status().is_success()
                || response
                    .content_length()
                    .is_some_and(|len| len > parser::MAX_IMPORT_BYTES as u64)
            {
                return Err(ProxyPoolError::Unavailable);
            }
            let mut body = Vec::new();
            let mut chunks = response.into_bytes_stream();
            while let Some(chunk) = chunks.next_chunk().await {
                let chunk = chunk.map_err(|_| ProxyPoolError::Unavailable)?;
                if body.len() + chunk.len() > parser::MAX_IMPORT_BYTES {
                    return Err(ProxyPoolError::Unavailable);
                }
                body.extend_from_slice(&chunk);
            }
            let text = std::str::from_utf8(&body).map_err(|_| ProxyPoolError::InvalidInput)?;
            let parsed = parser::parse(text, parse_scheme(&settings.default_scheme)?)?;
            self.store_nodes(pool, parsed.nodes, settings.trust_proxy_dns)
                .await?;
        }
        let current = self.repository.get(pool.proxy.proxy_id()).await?;
        let measurements = stream::iter(current.nodes.into_iter().map(|node| {
            let settings = &settings;
            async move {
                let start = Instant::now();
                let healthy = if node.proxy.enabled() {
                    self.probe(&node.proxy, &settings.probe_url, settings.trust_proxy_dns)
                        .await
                } else {
                    false
                };
                (
                    node.proxy.proxy_id(),
                    node.proxy.version(),
                    healthy.then(|| i64::try_from(start.elapsed().as_millis()).unwrap_or(i64::MAX)),
                )
            }
        }))
        .buffer_unordered(32)
        .collect::<Vec<_>>()
        .await;
        Ok(measurements)
    }
    fn config(&self, node: &CredentialProxyRecord) -> Result<ProxyConfig, ProxyPoolError> {
        let password = node
            .password_secret()
            .map(|secret| {
                self.cipher.decrypt(
                    SystemSecretKind::CredentialProxyPassword(node.proxy_id()),
                    secret,
                )
            })
            .transpose()
            .map_err(|_| ProxyPoolError::Internal)?;
        ProxyConfig::from_parts(
            node.scheme().as_str(),
            node.host(),
            node.port(),
            node.username(),
            password
                .as_ref()
                .map(af_account::DecryptedSystemSecret::expose_secret),
        )
        .map_err(|_| ProxyPoolError::Internal)
    }
    async fn probe(&self, node: &CredentialProxyRecord, target: &str, trust_dns: bool) -> bool {
        let probe = async {
            let client = self
                .clients
                .get()
                .ok_or(ProxyPoolError::Internal)?
                .get_with_proxy(
                    self.config(node)?,
                    dns(trust_dns),
                    Some(Duration::from_secs(5)),
                )
                .map_err(|_| ProxyPoolError::Internal)?;
            let response = client
                .execute(Method::GET, target, HeaderMap::new(), None)
                .await
                .map_err(|_| ProxyPoolError::Unavailable)?;
            if !response.status().is_success() {
                return Err(ProxyPoolError::Unavailable);
            }
            let mut stream = response.into_bytes_stream();
            let mut size = 0;
            while let Some(chunk) = stream.next_chunk().await {
                size += chunk.map_err(|_| ProxyPoolError::Unavailable)?.len();
                if size > 65536 {
                    return Err(ProxyPoolError::Unavailable);
                }
            }
            Ok::<(), ProxyPoolError>(())
        };
        tokio::time::timeout(Duration::from_secs(5), probe)
            .await
            .is_ok_and(|result| result.is_ok())
    }
    /// 凭据显式绑定优先；只在无显式绑定时继承渠道/全局池。任何池错误均不能降级直连。
    pub async fn resolve_client(
        &self,
        manual: Option<ProxyId>,
        channel: ChannelId,
        credential: CredentialId,
        clients: &HttpClientProvider,
        timeout: Option<Duration>,
    ) -> Result<Option<PooledClient>, ProxyPoolError> {
        let snapshot = self.snapshot().await?;
        let pool = if let Some(id) = manual {
            let Some(pool) = snapshot.iter().find(|pool| pool.proxy.proxy_id() == id) else {
                return Ok(None);
            };
            pool
        } else {
            let channel_pool = snapshot
                .iter()
                .find(|pool| pool.channel_ids.contains(&channel.get()));
            let Some(pool) =
                channel_pool.or_else(|| snapshot.iter().find(|pool| pool.channel_ids.contains(&0)))
            else {
                return Ok(None);
            };
            pool
        };
        let settings = self.settings(pool)?;
        let max_age = i64::from(pool.interval_secs).saturating_mul(2).max(60);
        let healthy = pool
            .nodes
            .iter()
            .filter(|node| {
                node.healthy
                    && node.proxy.enabled()
                    && node
                        .checked_at
                        .is_some_and(|at| now().saturating_sub(at) <= max_age)
            })
            .collect::<Vec<_>>();
        if !pool.proxy.enabled() || healthy.is_empty() {
            return Err(ProxyPoolError::Unavailable);
        }
        let index = usize::try_from(credential.get().unsigned_abs() % healthy.len() as u64)
            .map_err(|_| ProxyPoolError::Internal)?;
        let node = healthy[index];
        let client = clients
            .get_with_proxy(
                self.config(&node.proxy)?,
                dns(settings.trust_proxy_dns),
                timeout,
            )
            .map_err(|_| ProxyPoolError::Internal)?;
        Ok(Some(client))
    }
    /// 供渠道探活和模型发现复用：专属静态节点与池使用完全相同的路由优先级。
    pub async fn route_client(
        &self,
        manual: Option<ProxyId>,
        channel: ChannelId,
        credential: CredentialId,
        clients: &HttpClientProvider,
        timeout: Option<Duration>,
    ) -> Result<PooledClient, ProxyPoolError> {
        if let Some(client) = self
            .resolve_client(manual, channel, credential, clients, timeout)
            .await?
        {
            return Ok(client);
        }
        if let Some(id) = manual {
            let proxy = self.repository.proxy(id).await?;
            return clients
                .get_with_proxy(self.config(&proxy)?, dns(proxy.trust_proxy_dns()), timeout)
                .map_err(|_| ProxyPoolError::Internal);
        }
        clients.get(timeout).map_err(|_| ProxyPoolError::Internal)
    }
    pub async fn bind(
        &self,
        principal: SessionPrincipal,
        channel_id: i64,
        pool_id: Option<ProxyId>,
    ) -> Result<(), ProxyPoolError> {
        admin(principal)?;
        self.repository.bind(channel_id, pool_id).await?;
        self.invalidate().await;
        Ok(())
    }
    pub async fn delete(
        &self,
        principal: SessionPrincipal,
        id: ProxyId,
    ) -> Result<(), ProxyPoolError> {
        admin(principal)?;
        self.repository.delete(id).await?;
        self.invalidate().await;
        Ok(())
    }
    pub async fn cleanup(
        &self,
        principal: SessionPrincipal,
        request: ProxyCleanupRequest,
    ) -> Result<ProxyCleanupResult, ProxyPoolError> {
        admin(principal)?;
        let result = self.repository.cleanup(request).await?;
        self.invalidate().await;
        Ok(result)
    }
    pub async fn contains(&self, id: ProxyId) -> Result<bool, ProxyPoolError> {
        match self.repository.get(id).await {
            Ok(_) => Ok(true),
            Err(ProxyPoolRepositoryError::NotFound) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
    pub async fn run_periodic_until<F>(&self, shutdown: F)
    where
        F: Future<Output = ()>,
    {
        tokio::pin!(shutdown);
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {biased;()=&mut shutdown=>return,_=interval.tick()=>{}}
            let Ok(pools) = self.repository.list().await else {
                continue;
            };
            for pool in pools {
                let (refresh_due, check_due) = maintenance_due(&pool, now());
                if !pool.proxy.enabled() || pool.work_until > now() || (!refresh_due && !check_due)
                {
                    continue;
                }
                tokio::select! {biased;()=&mut shutdown=>return,result=self.maintain(pool.proxy.proxy_id(),refresh_due)=>{if let Err(error)=result {tracing::warn!(error_kind=?error,pool_id=pool.proxy.proxy_id().get(),"代理池维护未完成");}}}
            }
        }
    }
}
fn maintenance_due(pool: &ProxyPoolRecord, at: i64) -> (bool, bool) {
    let due = |last: Option<i64>| {
        last.is_none_or(|last| at.saturating_sub(last) >= i64::from(pool.interval_secs))
    };
    // 更新失败后按配置间隔重试；手动测活成功不能推迟订阅的同步周期。
    let refresh_at = pool.last_refresh_attempt_at;
    (pool.subscribed && due(refresh_at), due(pool.last_check_at))
}
fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
fn admin(principal: SessionPrincipal) -> Result<(), ProxyPoolError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(ProxyPoolError::Forbidden)
    }
}
fn dns(trust: bool) -> RemoteDnsPolicy {
    if trust {
        RemoteDnsPolicy::TrustProxy
    } else {
        RemoteDnsPolicy::Deny
    }
}
fn parse_scheme(value: &str) -> Result<CredentialProxyScheme, ProxyPoolError> {
    match value {
        "http" => Ok(CredentialProxyScheme::Http),
        "https" => Ok(CredentialProxyScheme::Https),
        "socks5" => Ok(CredentialProxyScheme::Socks5),
        "socks5h" => Ok(CredentialProxyScheme::Socks5h),
        _ => Err(ProxyPoolError::InvalidInput),
    }
}
fn validate_url(value: &str, probe: bool) -> Result<(), ProxyPoolError> {
    let url = url::Url::parse(value).map_err(|_| ProxyPoolError::InvalidInput)?;
    if value.len() > 2048
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || (probe && (url.scheme() != "https" || url.query().is_some()))
        || (!probe && !matches!(url.scheme(), "http" | "https"))
    {
        return Err(ProxyPoolError::InvalidInput);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_config::CredentialEncryptionSettings;
    use af_db::{
        AdminChannelRepository, AdminChannelWriteRecord, DatabaseOptions, MigrationOptions,
    };
    use af_domain::{ChannelType, Protocol, Status, UserId};
    use af_httpclient::HttpClientConfig;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    #[tokio::test]
    async fn proxy_pool_enforces_manual_channel_global_precedence_and_never_falls_back() {
        let database = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let repository = ProxyPoolRepository::new(database.clone(), Duration::from_secs(5));
        let key: CredentialEncryptionSettings = serde_json::from_value(
            serde_json::json!({"key_id":"test","key":URL_SAFE_NO_PAD.encode([7;32])}),
        )
        .unwrap();
        let service =
            ProxyPoolService::new(repository.clone(), SystemSecretCipher::new(&key).unwrap());
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin);
        let write = |name: &str, content: Option<&str>| ProxyPoolWrite {
            name: name.to_owned(),
            subscribed: false,
            source_url: None,
            interval_secs: 60,
            default_scheme: "http".to_owned(),
            trust_proxy_dns: false,
            probe_url: None,
            content: content.map(str::to_owned),
            revision: None,
        };
        let healthy = service
            .save(
                principal,
                None,
                write("全局池", Some("http://alice:secret@192.0.2.1:8080")),
            )
            .await
            .unwrap()
            .pool;
        let healthy_id = ProxyId::new(healthy.id).unwrap();
        let mut schedule = repository.get(healthy_id).await.unwrap();
        schedule.subscribed = true;
        schedule.interval_secs = 1800;
        schedule.last_refresh_attempt_at = Some(1000);
        schedule.last_check_at = Some(1000);
        assert_eq!(maintenance_due(&schedule, 2799), (false, false));
        assert_eq!(maintenance_due(&schedule, 2800), (true, true));
        schedule.last_check_at = Some(2800);
        assert_eq!(
            maintenance_due(&schedule, 2800),
            (true, false),
            "manual checks cannot postpone subscription refresh"
        );
        schedule.last_refresh_attempt_at = Some(2800);
        assert_eq!(maintenance_due(&schedule, 4599), (false, false));
        assert_eq!(maintenance_due(&schedule, 4600), (true, true));
        schedule.subscribed = false;
        assert_eq!(maintenance_due(&schedule, 4600), (false, true));
        let node_id = ProxyId::new(healthy.nodes[0].id).unwrap();
        repository
            .finish(healthy_id, 1, "ready", vec![(node_id, 1, Some(10))])
            .await
            .unwrap();
        service.bind(principal, 0, Some(healthy_id)).await.unwrap();
        let clients = HttpClientProvider::new(HttpClientConfig::default(), 4).unwrap();
        let channel = ChannelId::new(123).unwrap();
        let credential = CredentialId::new(1).unwrap();
        let first = service
            .resolve_client(None, channel, credential, &clients, None)
            .await
            .unwrap()
            .unwrap();
        let repeated = service
            .resolve_client(None, channel, credential, &clients, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.identity(), repeated.identity());
        let expected = clients
            .get_with_proxy(
                ProxyConfig::from_parts("http", "192.0.2.1", 8080, Some("alice"), Some("secret"))
                    .unwrap(),
                RemoteDnsPolicy::Deny,
                None,
            )
            .unwrap();
        assert_eq!(
            first.identity(),
            expected.identity(),
            "pool selection must preserve proxy authentication"
        );
        let manual = service
            .route_client(Some(node_id), channel, credential, &clients, None)
            .await
            .unwrap();
        assert_eq!(manual.identity(), expected.identity());
        assert!(
            service
                .resolve_client(Some(node_id), channel, credential, &clients, None)
                .await
                .unwrap()
                .is_none(),
            "static manual proxy takes precedence over the global pool"
        );
        let empty = service
            .save(principal, None, write("空池", None))
            .await
            .unwrap()
            .pool;
        let empty_id = ProxyId::new(empty.id).unwrap();
        assert!(matches!(
            service
                .resolve_client(Some(empty_id), channel, credential, &clients, None)
                .await,
            Err(ProxyPoolError::Unavailable)
        ));
        let channels = AdminChannelRepository::new(database, Duration::from_secs(5)).unwrap();
        let channel = channels
            .create_channel(AdminChannelWriteRecord::new(
                "pool-channel".to_owned(),
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                Some("https://example.com".to_owned()),
                None,
                Status::Enabled,
                10,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            ))
            .await
            .unwrap()
            .channel_id();
        service
            .bind(principal, channel.get(), Some(empty_id))
            .await
            .unwrap();
        assert!(
            matches!(
                service
                    .resolve_client(None, channel, credential, &clients, None)
                    .await,
                Err(ProxyPoolError::Unavailable)
            ),
            "empty channel pool cannot fall back to the healthy global pool"
        );
        service.bind(principal, channel.get(), None).await.unwrap();
        assert!(
            service
                .resolve_client(None, channel, credential, &clients, None)
                .await
                .unwrap()
                .is_some()
        );
        let user = SessionPrincipal::new(UserId::new(2).unwrap(), SessionRole::User);
        assert!(matches!(
            service.list(user).await,
            Err(ProxyPoolError::Forbidden)
        ));
        service
            .bind(principal, channel.get(), Some(empty_id))
            .await
            .unwrap();
        channels.delete_channel(channel).await.unwrap();
        assert!(
            repository
                .get(empty_id)
                .await
                .unwrap()
                .channel_ids
                .is_empty()
        );
    }
}
