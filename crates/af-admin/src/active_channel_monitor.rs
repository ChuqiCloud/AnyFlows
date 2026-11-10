//! Explicitly opted-in, budgeted active channel monitoring and alert state worker.
use std::{future::Future, pin::Pin, sync::Arc, time::{Duration, SystemTime, UNIX_EPOCH}};

use af_db::{
    ActiveChannelMonitorConfig, ActiveChannelMonitorEvent,
    ActiveChannelMonitorRepository, ActiveChannelMonitorTarget,
    ActiveChannelMonitorTargetInput, ActiveChannelProbeLease, ActiveChannelProbeOutcome,
    MAX_ACTIVE_CHANNEL_MONITOR_BATCH,
};
use af_domain::{ChannelId, CredentialId};
use thiserror::Error;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

const MONITOR_TICK: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActiveChannelProbeResult { Healthy { latency_ms: u64 }, Unhealthy }

pub type ActiveChannelProbeFuture<'a> = Pin<Box<dyn Future<Output = ActiveChannelProbeResult> + Send + 'a>>;

pub trait ActiveChannelProbe: Send + Sync {
    fn probe_target<'a>(&'a self, channel_id: ChannelId, credential_id: CredentialId, model: &'a str) -> ActiveChannelProbeFuture<'a>;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ActiveChannelMonitorServiceError {
    #[error("invalid active channel monitor configuration")]
    InvalidInput,
    #[error("active channel monitor target was not found or became ineligible")]
    NotFound,
    #[error("active channel monitor persistence failed")]
    Unavailable,
}

#[derive(Clone)]
pub struct DatabaseActiveChannelMonitorService { repository: ActiveChannelMonitorRepository }

impl DatabaseActiveChannelMonitorService {
    #[must_use]
    pub const fn new(repository: ActiveChannelMonitorRepository) -> Self { Self { repository } }

    pub async fn config(&self) -> Result<ActiveChannelMonitorConfig, ActiveChannelMonitorServiceError> {
        self.repository.get_config().await.map_err(map_service_error)
    }

    pub async fn set_enabled(&self, enabled: bool) -> Result<ActiveChannelMonitorConfig, ActiveChannelMonitorServiceError> {
        self.repository.set_enabled(enabled).await.map_err(map_service_error)
    }

    pub async fn set_config(&self, config: ActiveChannelMonitorConfig) -> Result<ActiveChannelMonitorConfig, ActiveChannelMonitorServiceError> {
        self.repository.set_config(config).await.map_err(map_service_error)
    }

    pub async fn list_targets(&self) -> Result<Vec<ActiveChannelMonitorTarget>, ActiveChannelMonitorServiceError> {
        self.repository.list_targets(256).await.map_err(map_service_error)
    }

    pub async fn upsert_target(&self, input: ActiveChannelMonitorTargetInput) -> Result<ActiveChannelMonitorTarget, ActiveChannelMonitorServiceError> {
        self.repository.upsert_target(input).await.map_err(map_service_error)
    }

    pub async fn delete_target(&self, id: i64) -> Result<(), ActiveChannelMonitorServiceError> {
        if self.repository.delete_target(id).await.map_err(map_service_error)? { Ok(()) } else { Err(ActiveChannelMonitorServiceError::NotFound) }
    }

    pub async fn events(&self, id: i64) -> Result<Vec<ActiveChannelMonitorEvent>, ActiveChannelMonitorServiceError> {
        self.repository.events(id, 100).await.map_err(map_service_error)
    }
}

impl af_http::ActiveChannelMonitorService for DatabaseActiveChannelMonitorService {
    fn config(&self) -> af_http::ActiveChannelMonitorServiceFuture<'_, ActiveChannelMonitorConfig> {
        Box::pin(async move { DatabaseActiveChannelMonitorService::config(self).await.map_err(http_error) })
    }

    fn set_config(&self, config: ActiveChannelMonitorConfig) -> af_http::ActiveChannelMonitorServiceFuture<'_, ActiveChannelMonitorConfig> {
        Box::pin(async move { DatabaseActiveChannelMonitorService::set_config(self, config).await.map_err(http_error) })
    }

    fn list_targets(&self) -> af_http::ActiveChannelMonitorServiceFuture<'_, Vec<ActiveChannelMonitorTarget>> {
        Box::pin(async move { DatabaseActiveChannelMonitorService::list_targets(self).await.map_err(http_error) })
    }

    fn upsert_target(&self, input: ActiveChannelMonitorTargetInput) -> af_http::ActiveChannelMonitorServiceFuture<'_, ActiveChannelMonitorTarget> {
        Box::pin(async move { DatabaseActiveChannelMonitorService::upsert_target(self, input).await.map_err(http_error) })
    }

    fn delete_target(&self, id: i64) -> af_http::ActiveChannelMonitorServiceFuture<'_, ()> {
        Box::pin(async move { DatabaseActiveChannelMonitorService::delete_target(self, id).await.map_err(http_error) })
    }

    fn events(&self, id: i64) -> af_http::ActiveChannelMonitorServiceFuture<'_, Vec<ActiveChannelMonitorEvent>> {
        Box::pin(async move { DatabaseActiveChannelMonitorService::events(self, id).await.map_err(http_error) })
    }
}

fn http_error(error: ActiveChannelMonitorServiceError) -> af_http::ActiveChannelMonitorServiceError {
    match error {
        ActiveChannelMonitorServiceError::InvalidInput => af_http::ActiveChannelMonitorServiceError::InvalidInput,
        ActiveChannelMonitorServiceError::NotFound => af_http::ActiveChannelMonitorServiceError::NotFound,
        ActiveChannelMonitorServiceError::Unavailable => af_http::ActiveChannelMonitorServiceError::Unavailable,
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ActiveChannelMonitorWorkerError {
    #[error("active channel monitor clock is invalid")]
    Clock,
    #[error("active channel monitor database operation failed")]
    Store,
}

#[derive(Clone)]
pub struct ActiveChannelMonitorWorker<P: ?Sized> {
    repository: ActiveChannelMonitorRepository,
    probe: Arc<P>,
}

impl<P: ActiveChannelProbe + ?Sized + 'static> ActiveChannelMonitorWorker<P> {
    #[must_use]
    pub fn new(repository: ActiveChannelMonitorRepository, probe: Arc<P>) -> Self { Self { repository, probe } }

    pub async fn run_once(&self) -> Result<usize, ActiveChannelMonitorWorkerError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| ActiveChannelMonitorWorkerError::Clock)?;
        let now = i64::try_from(now.as_secs()).map_err(|_| ActiveChannelMonitorWorkerError::Clock)?;
        let token = Uuid::new_v4().simple().to_string();
        let leases = self.repository.claim_due_targets(&token, MAX_ACTIVE_CHANNEL_MONITOR_BATCH, now).await.map_err(|_| ActiveChannelMonitorWorkerError::Store)?;
        let count=leases.len();
        for lease in leases {
            let target_timeout=Duration::from_secs(u64::from(lease.target.timeout_secs));
            let result = timeout(target_timeout, self.probe.probe_target(lease.target.channel_id, lease.target.credential_id, &lease.target.model)).await;
            let outcome=match result { Ok(ActiveChannelProbeResult::Healthy{latency_ms})=>ActiveChannelProbeOutcome::Healthy{latency_ms},Ok(ActiveChannelProbeResult::Unhealthy)=>ActiveChannelProbeOutcome::Unhealthy,Err(_)=>ActiveChannelProbeOutcome::TimedOut };
            let completed_at=SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_|ActiveChannelMonitorWorkerError::Clock)?;
            let completed_at=i64::try_from(completed_at.as_secs()).map_err(|_|ActiveChannelMonitorWorkerError::Clock)?;
            self.repository.complete_probe(lease,outcome,completed_at).await.map_err(|_|ActiveChannelMonitorWorkerError::Store)?;
        }
        Ok(count)
    }

    pub async fn run_periodic_until<F>(&self, shutdown:F) where F:Future<Output=()>+Send {
        tokio::pin!(shutdown);
        loop {
            tokio::select! { ()=&mut shutdown=>return, result=self.run_once()=>{if result.is_err(){tracing::warn!(error_kind="active_channel_monitor_cycle","Active channel monitor cycle failed");}} }
            tokio::select! { ()=&mut shutdown=>return, ()=sleep(MONITOR_TICK)=>{} }
        }
    }
}

fn map_service_error(error: ActiveChannelMonitorError) -> ActiveChannelMonitorServiceError {
    match error { af_db::ActiveChannelMonitorError::InvalidConfiguration=>ActiveChannelMonitorServiceError::InvalidInput,af_db::ActiveChannelMonitorError::NotEligible=>ActiveChannelMonitorServiceError::NotFound,af_db::ActiveChannelMonitorError::Invariant=>ActiveChannelMonitorServiceError::Unavailable,af_db::ActiveChannelMonitorError::Query|af_db::ActiveChannelMonitorError::Timeout=>ActiveChannelMonitorServiceError::Unavailable }
}
