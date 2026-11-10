//! Read-only upstream usage probes, with persistent leases and last-good observations.
use std::{future::Future, sync::Arc, time::Duration};

use af_account::{CredentialDecryptor, PlainSystemSecret, SystemSecretCipher};
use af_admin::{AdminUpstreamUsageService, SessionPrincipal, SessionRole, UpstreamUsageFuture};
use af_db::{DatabasePool, UpstreamUsageRepository, UpstreamUsageTarget, upstream_usage_now_epoch};
use af_domain::{
    ChannelId, CredentialId, UpstreamUsageConfig, UpstreamUsageError, UpstreamUsageProfile,
    UpstreamUsageProfileWrite, UpstreamUsageProvider, UserId,
};
use af_http::{
    AdminCredentialUsageFuture, AdminCredentialUsageProbe, CredentialUsageSnapshot,
    CredentialUsageStatus,
};
use af_httpclient::HttpClientProvider;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::{StreamExt, stream};
use rust_decimal::Decimal;
use tokio::sync::Semaphore;

#[cfg(test)]
mod tests;
mod transport;

const PROFILE_AAD: &[u8] = b"anyflows:upstream-usage-profile:v1";

pub(crate) struct UpstreamCredentialUsageProbe {
    repository: UpstreamUsageRepository,
    decryptor: CredentialDecryptor,
    cipher: SystemSecretCipher,
    clients: HttpClientProvider,
    proxy_pools: Option<Arc<af_admin::ProxyPoolService>>,
    concurrency: Semaphore,
}

impl UpstreamCredentialUsageProbe {
    pub(crate) fn new(
        pool: DatabasePool,
        decryptor: CredentialDecryptor,
        cipher: SystemSecretCipher,
        clients: HttpClientProvider,
        proxy_pools: Option<Arc<af_admin::ProxyPoolService>>,
    ) -> Self {
        Self {
            repository: UpstreamUsageRepository::new(pool),
            decryptor,
            cipher,
            clients,
            proxy_pools,
            concurrency: Semaphore::new(2),
        }
    }

    async fn query(
        &self,
        channel: ChannelId,
        credential: CredentialId,
        refresh: bool,
    ) -> Result<CredentialUsageSnapshot, UpstreamUsageError> {
        let target = self.repository.target(channel, credential).await?;
        let previous = self.repository.snapshot(&target).await?;
        let now = upstream_usage_now_epoch();
        let cached = previous
            .as_ref()
            .and_then(|s| s.value.clone())
            .and_then(|v| serde_json::from_value::<CredentialUsageSnapshot>(v).ok());
        if !refresh
            && previous
                .as_ref()
                .is_some_and(|s| now.saturating_sub(s.last_checked_at) < 30)
            && let Some(mut value) = cached.clone()
        {
            value.stale |= previous
                .as_ref()
                .and_then(|s| s.last_success_at)
                .is_none_or(|at| {
                    now.saturating_sub(at) > target.config.refresh_interval_secs.unwrap_or(900)
                });
            return Ok(derive_snapshot(&target, value, now));
        }
        let Some(_permit) = self.concurrency.try_acquire().ok() else {
            return Ok(derive_snapshot(
                &target,
                pending_snapshot(cached, "refresh_in_progress"),
                now,
            ));
        };
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(|_| UpstreamUsageError::Unavailable)?;
        let Some(lease) = self
            .repository
            .claim(&target, URL_SAFE_NO_PAD.encode(random), now)
            .await?
        else {
            let latest = self
                .repository
                .snapshot(&target)
                .await?
                .and_then(|s| s.value)
                .and_then(|v| serde_json::from_value(v).ok())
                .or(cached);
            return Ok(derive_snapshot(
                &target,
                pending_snapshot(latest, "refresh_in_progress"),
                now,
            ));
        };
        let result = tokio::time::timeout(Duration::from_secs(25), self.fetch(&target))
            .await
            .unwrap_or(Err(transport::UsageFetchError::Unavailable));
        let success = result.is_ok();
        let mut snapshot = match result {
            Ok(mut value) => {
                value.last_success_at = Some(now);
                value.fetched_at = Some(now);
                value.stale = false;
                value
            }
            Err(error) => {
                let mut value = cached.unwrap_or_else(CredentialUsageSnapshot::unavailable);
                value.status = if error.unsupported() {
                    CredentialUsageStatus::Unsupported
                } else {
                    CredentialUsageStatus::Unavailable
                };
                value.error_code = Some(error.code().to_owned());
                value.stale = value.last_success_at.is_some();
                value
            }
        };
        snapshot.last_checked_at = Some(now);
        if snapshot.provider.is_none() {
            snapshot.provider = Some(transport::effective_provider(&target).as_str().to_owned());
        }
        snapshot.account_ref = target.profile.as_ref().map(|p| format!("profile:{}", p.id));
        let exhausted = success && is_exhausted(&snapshot, now);
        let alert = if exhausted {
            Some("exhausted")
        } else if success && below_threshold(&target, &snapshot) {
            Some("low_balance")
        } else if success
            && snapshot
                .expires_at
                .is_some_and(|at| at > now && at - now < 86_400)
        {
            Some("expiring")
        } else if !success && previous.as_ref().is_some_and(|s| s.failures >= 2) {
            Some("query_failed")
        } else {
            None
        };
        // Shared cache stores source data only; local thresholds and pause are derived per binding.
        snapshot.alert = None;
        snapshot.pause_until = None;
        let encoded =
            serde_json::to_value(&snapshot).map_err(|_| UpstreamUsageError::Unavailable)?;
        let failures = previous
            .as_ref()
            .map_or(1, |s| s.failures.saturating_add(1));
        let notify = self
            .repository
            .complete(
                &target, lease, encoded, success, failures, now, exhausted, alert,
            )
            .await?;
        if notify && let (Some(actor), Some(kind)) = (target.operator_user_id, alert) {
            self.audit(actor, kind, &target.cache_key, Some(&snapshot))
                .await?;
        }
        if self.repository.target(channel, credential).await?.identity != target.identity {
            return Ok(pending_snapshot(None, "configuration_changed"));
        }
        Ok(derive_snapshot(&target, snapshot, now))
    }

    async fn audit(
        &self,
        actor: UserId,
        operation: &str,
        resource: &str,
        snapshot: Option<&CredentialUsageSnapshot>,
    ) -> Result<(), UpstreamUsageError> {
        let info = serde_json::json!({"scope":snapshot.and_then(|s|s.scope.as_deref()),"remaining":snapshot.and_then(|s|s.remaining.as_deref()),
            "unit":snapshot.and_then(|s|s.unit.as_deref()),"error_code":snapshot.and_then(|s|s.error_code.as_deref())}).to_string();
        let write = af_db::PlatformAuditWrite::new(
            actor,
            "upstream_usage.manage".to_owned(),
            "/api/admin/upstream-usage-profiles".to_owned(),
            format!("upstream_usage.{operation}"),
            "upstream_usage".to_owned(),
            Some(resource.to_owned()),
            af_db::PlatformAuditOutcome::Succeeded,
            None,
            None,
            Some(info),
            format!("usage-{}", upstream_usage_now_epoch()),
        )
        .map_err(|_| UpstreamUsageError::Unavailable)?;
        af_db::PlatformAuditRepository::new(self.repository.pool().clone())
            .record(&write)
            .await
            .map_err(|_| UpstreamUsageError::Unavailable)?;
        Ok(())
    }

    pub(crate) async fn run_periodic_until(
        self: Arc<Self>,
        shutdown: impl Future<Output = ()> + Send,
    ) {
        tokio::pin!(shutdown);
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = &mut shutdown => return,
                _ = ticker.tick() => {
                    let due = self.repository.due(upstream_usage_now_epoch(), 16).await.unwrap_or_default();
                    let probe=&self;
                    let run = stream::iter(due).for_each_concurrent(2, |(channel, credential)| async move {
                        if probe.query(channel, credential, true).await.is_err() {
                            tracing::debug!(channel_id=channel.get(),credential_id=credential.get(),"上游额度后台刷新失败");
                        }
                    });
                    tokio::select! { _ = &mut shutdown => return, _ = run => {} }
                }
            }
        }
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<UserId, UpstreamUsageError> {
    if principal.role() != SessionRole::Admin {
        return Err(UpstreamUsageError::Forbidden);
    }
    Ok(principal.user_id())
}

impl AdminCredentialUsageProbe for UpstreamCredentialUsageProbe {
    fn probe<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> AdminCredentialUsageFuture<'a> {
        Box::pin(async move {
            if require_admin(principal).is_err() {
                return CredentialUsageSnapshot::unavailable();
            }
            self.query(channel, credential, false)
                .await
                .unwrap_or_else(|_| CredentialUsageSnapshot::unavailable())
        })
    }
    fn refresh<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> AdminCredentialUsageFuture<'a> {
        Box::pin(async move {
            if require_admin(principal).is_err() {
                return CredentialUsageSnapshot::unavailable();
            }
            self.query(channel, credential, true)
                .await
                .unwrap_or_else(|_| CredentialUsageSnapshot::unavailable())
        })
    }
}

impl AdminUpstreamUsageService for UpstreamCredentialUsageProbe {
    fn channel_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository.channel_config(channel).await
        })
    }
    fn save_channel_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        config: UpstreamUsageConfig,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig> {
        Box::pin(async move {
            let actor = require_admin(principal)?;
            let saved = self
                .repository
                .save_channel_config(actor, channel, config)
                .await?;
            self.audit(
                actor,
                "channel_configure",
                &format!("channel:{}", channel.get()),
                None,
            )
            .await?;
            Ok(saved)
        })
    }
    fn reset_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> UpstreamUsageFuture<'_, ()> {
        Box::pin(async move {
            let actor = require_admin(principal)?;
            self.repository.reset_config(channel, credential).await?;
            self.audit(
                actor,
                "inherit",
                &format!("key:{}:{}", channel.get(), credential.get()),
                None,
            )
            .await
        })
    }
    fn config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository.config(channel, credential).await
        })
    }
    fn save_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
        config: UpstreamUsageConfig,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig> {
        Box::pin(async move {
            let actor = require_admin(principal)?;
            let saved = self
                .repository
                .save_config(actor, channel, credential, config)
                .await?;
            self.audit(
                actor,
                "configure",
                &format!("key:{}:{}", channel.get(), credential.get()),
                None,
            )
            .await?;
            Ok(saved)
        })
    }
    fn profiles(
        &self,
        principal: SessionPrincipal,
    ) -> UpstreamUsageFuture<'_, Vec<UpstreamUsageProfile>> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository.profiles().await
        })
    }
    fn save_profile(
        &self,
        principal: SessionPrincipal,
        id: Option<i64>,
        mut write: UpstreamUsageProfileWrite,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageProfile> {
        Box::pin(async move {
            let actor = require_admin(principal)?;
            validate_profile(&write)?;
            if let Some(id) = id {
                let (profile, _) = self.repository.profile(id).await?;
                if profile.version != write.version {
                    return Err(UpstreamUsageError::Conflict);
                }
            }
            let secret = write.secret.take().filter(|v| !v.trim().is_empty());
            if id.is_none() && secret.is_none() {
                return Err(UpstreamUsageError::InvalidInput);
            }
            let encrypted = secret.map(|value| {
                let plain = PlainSystemSecret::new(value).map_err(|_|UpstreamUsageError::InvalidInput)?;
                let envelope = self.cipher.encrypt_with_aad(PROFILE_AAD, &plain).map_err(|_| UpstreamUsageError::Unavailable)?;
                Ok::<_,UpstreamUsageError>(serde_json::json!({"key_id":envelope.key_id(),"nonce":URL_SAFE_NO_PAD.encode(envelope.nonce()),"ciphertext":URL_SAFE_NO_PAD.encode(envelope.ciphertext())}).to_string())
            }).transpose()?;
            let profile = UpstreamUsageProfile {
                id: id.unwrap_or(0),
                name: write.name,
                provider: write.provider,
                base_url: write.base_url.trim_end_matches('/').to_owned(),
                platform_user_id: write.platform_user_id,
                quota_per_unit: write.quota_per_unit,
                currency: write.currency,
                version: write.version,
                has_secret: true,
            };
            let saved = self.repository.save_profile(profile, encrypted).await?;
            self.audit(
                actor,
                "profile_save",
                &format!("profile:{}", saved.id),
                None,
            )
            .await?;
            Ok(saved)
        })
    }
    fn delete_profile(&self, principal: SessionPrincipal, id: i64) -> UpstreamUsageFuture<'_, ()> {
        Box::pin(async move {
            let actor = require_admin(principal)?;
            self.repository.delete_profile(id).await?;
            self.audit(actor, "profile_delete", &format!("profile:{id}"), None)
                .await
        })
    }
}

fn validate_profile(write: &UpstreamUsageProfileWrite) -> Result<(), UpstreamUsageError> {
    if write.name.trim().is_empty()
        || write.name.len() > 128
        || write.name.chars().any(char::is_control)
        || write.version < 0
        || !matches!(
            write.provider,
            UpstreamUsageProvider::NewApi
                | UpstreamUsageProvider::OneApi
                | UpstreamUsageProvider::Sub2api
        )
        || write
            .platform_user_id
            .as_ref()
            .is_some_and(|v| v.len() > 64 || v.chars().any(char::is_control))
        || write
            .secret
            .as_ref()
            .is_some_and(|v| v.len() > 8192 || v.chars().any(char::is_control))
    {
        return Err(UpstreamUsageError::InvalidInput);
    }
    transport::validate_base_url(&write.base_url).map_err(|_| UpstreamUsageError::InvalidInput)?;
    af_domain::validate_upstream_usage_amount(write.quota_per_unit.as_deref(), true)?;
    af_domain::validate_upstream_usage_currency(write.currency.as_deref())
}

fn pending_snapshot(
    cached: Option<CredentialUsageSnapshot>,
    code: &str,
) -> CredentialUsageSnapshot {
    cached.unwrap_or_else(|| CredentialUsageSnapshot {
        error_code: Some(code.to_owned()),
        ..CredentialUsageSnapshot::unavailable()
    })
}

fn below_threshold(target: &UpstreamUsageTarget, snapshot: &CredentialUsageSnapshot) -> bool {
    let threshold = target
        .config
        .low_balance_threshold
        .as_deref()
        .and_then(|v| v.parse::<Decimal>().ok());
    let remaining = snapshot
        .remaining
        .as_deref()
        .and_then(|v| v.parse::<Decimal>().ok());
    !snapshot.unlimited && threshold.zip(remaining).is_some_and(|(t, r)| r <= t)
}

fn is_exhausted(snapshot: &CredentialUsageSnapshot, now: i64) -> bool {
    !snapshot.unlimited
        && (snapshot
            .remaining
            .as_deref()
            .and_then(|v| v.parse::<Decimal>().ok())
            .is_some_and(|v| v <= Decimal::ZERO)
            || snapshot
                .windows
                .iter()
                .any(|w| w.used_percent >= 100.0 && w.reset_at.is_some_and(|at| at > now)))
}

fn derive_snapshot(
    target: &UpstreamUsageTarget,
    mut snapshot: CredentialUsageSnapshot,
    now: i64,
) -> CredentialUsageSnapshot {
    snapshot.alert = None;
    snapshot.pause_until = None;
    snapshot.stale |= snapshot.last_success_at.is_some_and(|at| {
        now.saturating_sub(at) > target.config.refresh_interval_secs.unwrap_or(900)
    });
    let fresh = matches!(snapshot.status, CredentialUsageStatus::Available)
        && !snapshot.stale
        && snapshot.last_success_at.is_some_and(|at| {
            now.saturating_sub(at) <= target.config.refresh_interval_secs.unwrap_or(900)
        });
    if fresh {
        let exhausted = is_exhausted(&snapshot, now);
        snapshot.alert = if exhausted {
            Some("exhausted")
        } else if below_threshold(target, &snapshot) {
            Some("low_balance")
        } else {
            None
        }
        .map(str::to_owned);
        if exhausted && target.config.pause_when_exhausted && target.enabled {
            snapshot.pause_until = snapshot
                .last_success_at
                .map(|at| {
                    at + target
                        .config
                        .refresh_interval_secs
                        .unwrap_or(900)
                        .clamp(60, 1800)
                })
                .filter(|at| *at > now);
        }
    }
    snapshot
}
