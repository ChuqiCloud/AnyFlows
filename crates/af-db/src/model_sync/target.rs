use af_domain::{ChannelTimeout, Status};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use sha2::{Digest, Sha256};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    ChannelModelMappings, EncryptedCredentialEnvelope,
    entity::{channels, credentials},
};

use super::{
    ModelDiscoveryHeaderRecord, ModelDiscoveryMappingRecord, ModelDiscoveryTargetLookup,
    ModelDiscoveryTargetRecord, ModelSyncRepository, ModelSyncRepositoryError,
    record_internal_error,
};

impl ModelSyncRepository {
    /// 读取未删除渠道和最高优先级有效凭据，供受控上游模型发现使用。
    pub async fn load_discovery_target(
        &self,
        channel_id: af_domain::ChannelId,
    ) -> Result<ModelDiscoveryTargetLookup, ModelSyncRepositoryError> {
        self.load_discovery_target_inner(channel_id, None, true)
            .await
    }

    /// Discover exactly this credential; manual discovery may inspect disabled supply.
    pub async fn load_discovery_target_for_credential(
        &self,
        channel_id: af_domain::ChannelId,
        credential_id: af_domain::CredentialId,
        manual: bool,
    ) -> Result<ModelDiscoveryTargetLookup, ModelSyncRepositoryError> {
        self.load_discovery_target_inner(channel_id, Some(credential_id), manual)
            .await
    }

    async fn load_discovery_target_inner(
        &self,
        channel_id: af_domain::ChannelId,
        credential_id: Option<af_domain::CredentialId>,
        manual: bool,
    ) -> Result<ModelDiscoveryTargetLookup, ModelSyncRepositoryError> {
        let operation = async {
            let Some(channel) = channels::Entity::find_by_id(channel_id.get())
                .filter(channels::Column::DeletedAt.is_null())
                .one(self.pool.connection())
                .await
                .map_err(|_| ModelSyncRepositoryError::Query)?
            else {
                return Ok(ModelDiscoveryTargetLookup::NotFound);
            };
            if !manual && channel.status != Status::Enabled.code() {
                return Ok(ModelDiscoveryTargetLookup::Unavailable);
            }
            let channel_type = channel
                .r#type
                .parse()
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let protocol = channel
                .protocol
                .parse()
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let profiles = crate::client_profiles::runtime_profiles(
                self.pool.connection(),
                &[channel_id.get()],
            )
            .await
            .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let client_profile = match profiles.get(&channel_id.get()) {
                Some(None) => return Ok(ModelDiscoveryTargetLookup::Unavailable),
                Some(Some(profile)) => Some(profile.clone()),
                None => None,
            };
            if client_profile
                .as_ref()
                .is_some_and(|profile| !profile.definition.matches_channel(channel_type, protocol))
            {
                return Ok(ModelDiscoveryTargetLookup::Unavailable);
            }
            let mut query = credentials::Entity::find()
                .filter(credentials::Column::ChannelId.eq(channel_id.get()))
                .filter(credentials::Column::OauthTokenPending.eq(false))
                .filter(credentials::Column::DeletedAt.is_null());
            if let Some(id) = credential_id {
                query = query.filter(credentials::Column::Id.eq(id.get()));
            } else {
                query = query
                    .filter(credentials::Column::ParentId.is_null())
                    .filter(crate::credential_probe::compatible_probe_credentials(
                        channel_type,
                        client_profile.as_ref(),
                    ));
            }
            if !manual || credential_id.is_none() {
                query = query
                    .filter(credentials::Column::Status.eq(Status::Enabled.code()))
                    .filter(credentials::Column::Schedulable.eq(true));
            }
            let Some(mut credential) = query
                .order_by_desc(credentials::Column::Priority)
                .order_by_asc(credentials::Column::Id)
                .one(self.pool.connection())
                .await
                .map_err(|_| ModelSyncRepositoryError::Query)?
            else {
                return Ok(if credential_id.is_some() {
                    ModelDiscoveryTargetLookup::NotFound
                } else {
                    ModelDiscoveryTargetLookup::Unavailable
                });
            };
            if let Some(parent_id) = credential.parent_id {
                let Some(parent) = credentials::Entity::find_by_id(parent_id)
                    .filter(credentials::Column::ChannelId.eq(channel_id.get()))
                    .filter(credentials::Column::ParentId.is_null())
                    .filter(credentials::Column::DeletedAt.is_null())
                    .filter(credentials::Column::OauthTokenPending.eq(false))
                    .one(self.pool.connection())
                    .await
                    .map_err(|_| ModelSyncRepositoryError::Query)?
                else {
                    return Ok(ModelDiscoveryTargetLookup::Unavailable);
                };
                if (!manual && parent.status != 1)
                    || credential.kind != "oauth"
                    || parent.kind != "oauth"
                {
                    return Ok(ModelDiscoveryTargetLookup::Unavailable);
                }
                credential = parent;
            }
            if !channel_type.supports_credential_kind(
                credential
                    .kind
                    .parse()
                    .map_err(|_| ModelSyncRepositoryError::Invariant)?,
            ) || client_profile.as_ref().is_some_and(|profile| {
                !profile.definition.matches_credential(
                    credential
                        .kind
                        .parse()
                        .unwrap_or(af_domain::CredentialKind::ApiKey),
                    credential.oauth_provider.as_deref(),
                )
            }) {
                return Ok(ModelDiscoveryTargetLookup::Unavailable);
            }
            let identity_fingerprint =
                target_fingerprint(self.pool.connection(), &channel, &credential).await?;
            if credential.id <= 0 {
                return Err(ModelSyncRepositoryError::Invariant);
            }

            let credential_kind = credential
                .kind
                .parse()
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let opencode_entry =
                crate::channel_opencode_entry(channel_type, &channel.settings.clone().into_inner())
                    .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let channel_timeout = channel
                .timeout_secs
                .map(|value| {
                    u64::try_from(value)
                        .ok()
                        .and_then(|value| ChannelTimeout::new(value).ok())
                        .ok_or(ModelSyncRepositoryError::Invariant)
                })
                .transpose()?;
            let (key_id, nonce, ciphertext) = credential
                .secret
                .envelope_parts()
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let headers = channel
                .header_override
                .pairs()
                .map_err(|_| ModelSyncRepositoryError::Invariant)?
                .into_iter()
                .map(|(name, value)| ModelDiscoveryHeaderRecord { name, value })
                .collect();
            ChannelModelMappings::parse(&channel.model_mapping)
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let mappings = channel
                .model_mapping
                .as_object()
                .ok_or(ModelSyncRepositoryError::Invariant)?
                .iter()
                .map(|(canonical_model, upstream_model)| {
                    upstream_model
                        .as_str()
                        .map(|upstream_model| ModelDiscoveryMappingRecord {
                            canonical_model: canonical_model.clone(),
                            upstream_model: upstream_model.to_owned(),
                        })
                        .ok_or(ModelSyncRepositoryError::Invariant)
                })
                .collect::<Result<Vec<_>, _>>()?;

            Ok(ModelDiscoveryTargetLookup::Found(
                ModelDiscoveryTargetRecord {
                    channel_id,
                    identity_fingerprint,
                    channel_type,
                    protocol,
                    base_url: channel.base_url.map(|value| value.as_str().to_owned()),
                    timeout: channel_timeout,
                    credential_id: credential.id,
                    credential_kind,
                    oauth_provider: credential.oauth_provider,
                    oauth_account_key: credential.oauth_account_key,
                    envelope,
                    headers,
                    mappings,
                    proxy_required: credential.proxy_id.is_some(),
                    proxy_id: credential
                        .proxy_id
                        .map(af_domain::ProxyId::new)
                        .transpose()
                        .map_err(|_| ModelSyncRepositoryError::Invariant)?,
                    client_profile,
                    opencode_entry,
                },
            ))
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ModelSyncRepositoryError::Timeout)),
        }
    }
}

/// Hash all request-affecting configuration without retaining or logging plaintext secrets.
pub(crate) fn target_identity(
    channel: &channels::Model,
    credential: &credentials::Model,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"AnyFlows:model-discovery:v1\0");
    if let Ok((key, nonce, ciphertext)) = credential.secret.envelope_parts() {
        digest.update(key.as_bytes());
        digest.update(nonce);
        digest.update(ciphertext);
    }
    let configuration = serde_json::json!({
        "channel": channel.id, "type":channel.r#type, "protocol":channel.protocol,
        "url":channel.base_url.as_ref().map(|url|url.as_str()), "timeout":channel.timeout_secs,
        "headers":channel.header_override.pairs().ok(), "mapping":channel.model_mapping,
        "settings":channel.settings.clone().into_inner(), "channel_updated":channel.updated_at.to_string(),
        "credential":credential.id, "kind":credential.kind, "proxy":credential.proxy_id,
        "oauth_provider":credential.oauth_provider, "oauth_account":credential.oauth_account_key,
        "oauth_project":credential.oauth_project_id, "oauth_revision":credential.oauth_revision, "credential_updated":credential.updated_at.to_string(),
    });
    digest.update(configuration.to_string().as_bytes());
    format!("{:x}", digest.finalize())
}

pub(crate) async fn target_fingerprint<C: sea_orm::ConnectionTrait>(
    connection: &C,
    channel: &channels::Model,
    credential: &credentials::Model,
) -> Result<String, ModelSyncRepositoryError> {
    let mut digest = Sha256::new();
    digest.update(target_identity(channel, credential).as_bytes());
    let profiles = crate::client_profiles::runtime_profiles(connection, &[channel.id])
        .await
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    digest.update(serde_json::to_vec(&profiles).map_err(|_| ModelSyncRepositoryError::Invariant)?);
    if let Some(id) = credential.proxy_id {
        let proxy = crate::entity::proxies::Entity::find_by_id(id)
            .one(connection)
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?
            .ok_or(ModelSyncRepositoryError::Invariant)?;
        digest.update(proxy.updated_at.to_string().as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}
