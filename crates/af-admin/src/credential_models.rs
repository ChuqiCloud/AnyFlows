use crate::{SessionPrincipal, SessionRole, UpstreamModelDiscoverer, UpstreamModelDiscoveryError};
pub use af_db::{
    CredentialModelError, CredentialModelItem, CredentialModelsSnapshot,
    UpstreamModelPolicySettings,
};
use af_domain::{ChannelId, CredentialId};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

pub type CredentialModelFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, CredentialModelError>> + Send + 'a>>;

pub trait CredentialModelService: Send + Sync {
    fn policy(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: Option<CredentialId>,
    ) -> CredentialModelFuture<'_, UpstreamModelPolicySettings>;
    fn write_policy(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: Option<CredentialId>,
        settings: UpstreamModelPolicySettings,
    ) -> CredentialModelFuture<'_, UpstreamModelPolicySettings>;
    fn models(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> CredentialModelFuture<'_, CredentialModelsSnapshot>;
    fn refresh(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> CredentialModelFuture<'_, CredentialModelsSnapshot>;
}

#[derive(Clone)]
pub struct DatabaseCredentialModelService {
    repository: af_db::CredentialModelsRepository,
    discoverer: Arc<dyn UpstreamModelDiscoverer>,
    permits: Arc<tokio::sync::Semaphore>,
}

impl DatabaseCredentialModelService {
    #[must_use]
    pub fn new(
        repository: af_db::CredentialModelsRepository,
        discoverer: Arc<dyn UpstreamModelDiscoverer>,
    ) -> Self {
        Self {
            repository,
            discoverer,
            permits: Arc::new(tokio::sync::Semaphore::new(2)),
        }
    }

    async fn refresh_inner(
        &self,
        channel: ChannelId,
        credential: CredentialId,
        manual: bool,
    ) -> Result<(), CredentialModelError> {
        let _permit = tokio::time::timeout(Duration::from_secs(10), self.permits.acquire())
            .await
            .map_err(|_| CredentialModelError::Conflict)?
            .map_err(|_| CredentialModelError::Internal)?;
        let Some(lease) = self
            .repository
            .claim_refresh(channel, credential, manual)
            .await?
        else {
            return Ok(());
        };
        let discovery = tokio::time::timeout(
            Duration::from_secs(120),
            self.discoverer
                .discover_credential(channel, credential, manual),
        )
        .await;
        let result = match discovery {
            Ok(Ok(discovery)) => Ok(discovery
                .into_models()
                .into_iter()
                .map(|model| model.upstream_model().to_owned())
                .collect()),
            Ok(Err(error)) => Err(match error {
                UpstreamModelDiscoveryError::UnsupportedChannel => "unsupported",
                UpstreamModelDiscoveryError::Timeout => "timeout",
                UpstreamModelDiscoveryError::UpstreamRejected => "upstream_rejected",
                UpstreamModelDiscoveryError::InvalidResponse
                | UpstreamModelDiscoveryError::CandidateLimitExceeded => "invalid_response",
                _ => "unavailable",
            }
            .to_owned()),
            Err(_) => Err("timeout".to_owned()),
        };
        self.repository.complete_refresh(lease, result).await
    }

    /// Every instance may scan; database leases coalesce physical parent identities.
    pub async fn run_periodic_until(&self, shutdown: impl Future<Output = ()>) {
        tokio::pin!(shutdown);
        let mut entropy = [0_u8; 1];
        let _ = getrandom::fill(&mut entropy);
        let first =
            tokio::time::Instant::now() + Duration::from_secs(5 + u64::from(entropy[0] % 25));
        let mut ticks = tokio::time::interval_at(first, Duration::from_secs(30));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = &mut shutdown => return,
                _ = ticks.tick() => {
                    let operation = async {
                        let due = self.repository.due_credentials(8).await?;
                        for (channel, credential) in due {
                            if let Err(error) = self.refresh_inner(channel, credential, false).await {
                                tracing::debug!(error_kind = ?error, "credential model refresh deferred");
                            }
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                        Ok::<(), CredentialModelError>(())
                    };
                    tokio::select! {
                        () = &mut shutdown => return,
                        result = operation => { if result.is_err() { tracing::debug!("credential model refresh storage unavailable"); } }
                    }
                }
            }
        }
    }
}

impl CredentialModelService for DatabaseCredentialModelService {
    fn policy(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: Option<CredentialId>,
    ) -> CredentialModelFuture<'_, UpstreamModelPolicySettings> {
        Box::pin(async move {
            admin(principal)?;
            self.repository.policy(channel, credential).await
        })
    }
    fn write_policy(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: Option<CredentialId>,
        settings: UpstreamModelPolicySettings,
    ) -> CredentialModelFuture<'_, UpstreamModelPolicySettings> {
        Box::pin(async move {
            admin(principal)?;
            self.repository
                .write_policy_audited(principal.user_id(), channel, credential, settings)
                .await
        })
    }
    fn models(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> CredentialModelFuture<'_, CredentialModelsSnapshot> {
        Box::pin(async move {
            admin(principal)?;
            self.repository.models(channel, credential).await
        })
    }
    fn refresh(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> CredentialModelFuture<'_, CredentialModelsSnapshot> {
        Box::pin(async move {
            admin(principal)?;
            self.refresh_inner(channel, credential, true).await?;
            self.repository.models(channel, credential).await
        })
    }
}

fn admin(principal: SessionPrincipal) -> Result<(), CredentialModelError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(CredentialModelError::Forbidden)
    }
}
