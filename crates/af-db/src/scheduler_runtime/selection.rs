use std::{collections::BTreeMap, sync::Arc};

use af_domain::{ChannelId, ChannelType, CredentialKind};

use super::{
    SchedulerRuntimeCredentialRecord, SchedulerRuntimeTargetRecord,
    source::{SchedulerRuntimeChannelSource, SchedulerRuntimeCredentialSource},
    target::is_supported_runtime_target,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SchedulerRuntimeSelectionError {
    Invariant,
}

/// 按持久化排序选择每个渠道首个当前可用的运行时目标。
pub(super) fn select_supported_targets(
    channels: &BTreeMap<ChannelId, SchedulerRuntimeChannelSource>,
    credentials: Vec<SchedulerRuntimeCredentialSource>,
) -> Result<BTreeMap<ChannelId, Arc<SchedulerRuntimeTargetRecord>>, SchedulerRuntimeSelectionError>
{
    let mut grouped = BTreeMap::<ChannelId, Vec<SchedulerRuntimeCredentialSource>>::new();
    for credential in credentials {
        grouped
            .entry(credential.channel_id())
            .or_default()
            .push(credential);
    }
    let mut targets = BTreeMap::new();
    for (channel_id, credentials) in grouped {
        let Some(channel) = channels.get(&channel_id) else {
            continue;
        };
        if let Some(target) = try_runtime_target(channel, credentials)? {
            targets.insert(channel_id, Arc::new(target));
        }
    }
    Ok(targets)
}

fn try_runtime_target(
    channel: &SchedulerRuntimeChannelSource,
    credentials: Vec<SchedulerRuntimeCredentialSource>,
) -> Result<Option<SchedulerRuntimeTargetRecord>, SchedulerRuntimeSelectionError> {
    let channel_id = channel.channel_id();
    if credentials
        .iter()
        .any(|credential| credential.channel_id() != channel_id)
    {
        return Err(SchedulerRuntimeSelectionError::Invariant);
    }
    let channel_type = channel.channel_type();
    let protocol = channel.protocol();
    if !is_supported_runtime_target(channel_type, protocol) {
        return Ok(None);
    }
    let mut records = Vec::with_capacity(credentials.len());
    for credential in credentials {
        let (
            routing_credential_id,
            secret_owner_id,
            concurrency_owner_id,
            shared_health_id,
            quota_dimension,
            credential_kind,
            envelope,
            proxy,
            priority,
            weight,
            concurrency,
            oauth_provider,
            oauth_account_key,
        ) = credential.into_runtime_parts();
        if !matches!(
            credential_kind,
            CredentialKind::ApiKey | CredentialKind::Oauth
        ) || (matches!(
            channel_type,
            ChannelType::Jina | ChannelType::Cohere | ChannelType::Xai
        ) && credential_kind != CredentialKind::ApiKey)
            || (channel.client_simulation_profile().is_some()
                && credential_kind != CredentialKind::Oauth)
        {
            continue;
        }
        let proxy_required = proxy.is_some();
        let mut record = SchedulerRuntimeCredentialRecord::with_runtime_identity(
            routing_credential_id,
            secret_owner_id,
            concurrency_owner_id,
            shared_health_id,
            quota_dimension,
            credential_kind,
            envelope,
            proxy_required,
            priority,
            weight,
            concurrency,
        )
        .map_err(|_| SchedulerRuntimeSelectionError::Invariant)?;
        record = record.with_oauth_identity(oauth_provider, oauth_account_key);
        if let Some(proxy) = proxy {
            record = record
                .with_proxy(proxy)
                .map_err(|_| SchedulerRuntimeSelectionError::Invariant)?;
        }
        records.push(record);
    }
    if records.is_empty() {
        return Ok(None);
    }
    SchedulerRuntimeTargetRecord::new_pool_with_request_policy_and_compact_mapping(
        channel_id,
        channel_type,
        protocol,
        channel.base_url().map(str::to_owned),
        records,
        channel.model_mappings().clone(),
        channel.responses_compact_model_mapping().clone(),
        channel.parameter_overrides().clone(),
        channel.headers().to_vec(),
    )
    .map(|target| target.with_timeout(channel.timeout()))
    .map(|target| target.with_auto_ban_rules(channel.auto_ban_rules().clone()))
    .map(|target| target.with_pool_mode(channel.pool_mode()))
    .and_then(|target| target.with_client_simulation_profile(channel.client_simulation_profile()))
    .and_then(|target| {
        target.with_client_simulation_body_profile(channel.client_simulation_body_profile())
    })
    .and_then(|target| {
        target.with_responses_websocket_enabled(channel.responses_websocket_enabled())
    })
    .and_then(|target| target.with_responses_compact_mode(channel.responses_compact_mode()))
    .and_then(|target| {
        target.with_responses_compact_probe_result(channel.responses_compact_probe_result())
    })
    .map(Some)
    .map_err(|_| SchedulerRuntimeSelectionError::Invariant)
}
