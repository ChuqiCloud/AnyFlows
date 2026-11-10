use std::{future::Future, pin::Pin};

use af_domain::{
    ChannelId, CredentialId, UpstreamUsageConfig, UpstreamUsageError, UpstreamUsageProfile,
    UpstreamUsageProfileWrite,
};

use crate::SessionPrincipal;

pub type UpstreamUsageFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, UpstreamUsageError>> + Send + 'a>>;

/// Query operations use the same administrator boundary as configuration operations.
pub trait AdminUpstreamUsageService: Send + Sync {
    fn channel_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig>;
    fn save_channel_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        config: UpstreamUsageConfig,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig>;
    fn reset_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> UpstreamUsageFuture<'_, ()>;
    fn config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig>;
    fn save_config(
        &self,
        principal: SessionPrincipal,
        channel: ChannelId,
        credential: CredentialId,
        config: UpstreamUsageConfig,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageConfig>;
    fn profiles(
        &self,
        principal: SessionPrincipal,
    ) -> UpstreamUsageFuture<'_, Vec<UpstreamUsageProfile>>;
    fn save_profile(
        &self,
        principal: SessionPrincipal,
        id: Option<i64>,
        write: UpstreamUsageProfileWrite,
    ) -> UpstreamUsageFuture<'_, UpstreamUsageProfile>;
    fn delete_profile(&self, principal: SessionPrincipal, id: i64) -> UpstreamUsageFuture<'_, ()>;
}
