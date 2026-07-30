use std::{collections::BTreeMap, sync::Arc};

use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol};

use super::{
    SchedulerRuntimeCredentialRecord, SchedulerRuntimeTargetRecord,
    source::{SchedulerRuntimeChannelSource, SchedulerRuntimeCredentialSource},
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
    if channel_type != ChannelType::OpenAi
        || !matches!(protocol, Protocol::OpenAiChat | Protocol::OpenAiResponses)
    {
        return Ok(None);
    }
    let mut records = Vec::with_capacity(credentials.len());
    for credential in credentials {
        let (credential_id, credential_kind, envelope, proxy_required, priority, weight) =
            credential.into_runtime_parts();
        if !matches!(
            credential_kind,
            CredentialKind::ApiKey | CredentialKind::Oauth
        ) {
            continue;
        }
        records.push(
            SchedulerRuntimeCredentialRecord::with_scheduling(
                credential_id,
                credential_kind,
                envelope,
                proxy_required,
                priority,
                weight,
            )
            .map_err(|_| SchedulerRuntimeSelectionError::Invariant)?,
        );
    }
    if records.is_empty() {
        return Ok(None);
    }
    SchedulerRuntimeTargetRecord::new_pool_with_request_policy(
        channel_id,
        channel_type,
        protocol,
        channel.base_url().map(str::to_owned),
        records,
        channel.model_mappings().clone(),
        channel.parameter_overrides().clone(),
        channel.headers().to_vec(),
    )
    .map(Some)
    .map_err(|_| SchedulerRuntimeSelectionError::Invariant)
}
