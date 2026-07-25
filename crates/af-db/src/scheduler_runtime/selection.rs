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
    let mut targets = BTreeMap::new();
    for credential in credentials {
        let channel_id = credential.channel_id();
        if targets.contains_key(&channel_id) {
            continue;
        }
        let Some(channel) = channels.get(&channel_id) else {
            continue;
        };
        if let Some(target) = try_runtime_target(channel, credential)? {
            targets.insert(channel_id, Arc::new(target));
        }
    }
    Ok(targets)
}

fn try_runtime_target(
    channel: &SchedulerRuntimeChannelSource,
    credential: SchedulerRuntimeCredentialSource,
) -> Result<Option<SchedulerRuntimeTargetRecord>, SchedulerRuntimeSelectionError> {
    let channel_id = channel.channel_id();
    if credential.channel_id() != channel_id {
        return Err(SchedulerRuntimeSelectionError::Invariant);
    }
    let channel_type = channel.channel_type();
    let protocol = channel.protocol();
    if channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiChat {
        return Ok(None);
    }
    let (credential_id, credential_kind, envelope, proxy_required) =
        credential.into_runtime_parts();
    if !matches!(
        credential_kind,
        CredentialKind::ApiKey | CredentialKind::Oauth
    ) {
        return Ok(None);
    }
    let credential = SchedulerRuntimeCredentialRecord::new(
        credential_id,
        credential_kind,
        envelope,
        proxy_required,
    )
    .map_err(|_| SchedulerRuntimeSelectionError::Invariant)?;
    SchedulerRuntimeTargetRecord::new(
        channel_id,
        channel_type,
        protocol,
        channel.base_url().map(str::to_owned),
        credential,
        channel.headers().to_vec(),
    )
    .map(Some)
    .map_err(|_| SchedulerRuntimeSelectionError::Invariant)
}
