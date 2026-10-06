use af_domain::{ChannelTimeout, Status};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
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
        let operation = async {
            let Some(channel) = channels::Entity::find_by_id(channel_id.get())
                .filter(channels::Column::DeletedAt.is_null())
                .one(self.pool.connection())
                .await
                .map_err(|_| ModelSyncRepositoryError::Query)?
            else {
                return Ok(ModelDiscoveryTargetLookup::NotFound);
            };
            let Some(credential) = credentials::Entity::find()
                .filter(credentials::Column::ChannelId.eq(channel_id.get()))
                .filter(credentials::Column::Status.eq(Status::Enabled.code()))
                .filter(credentials::Column::Schedulable.eq(true))
                .filter(credentials::Column::OauthTokenPending.eq(false))
                .filter(credentials::Column::DeletedAt.is_null())
                .order_by_desc(credentials::Column::Priority)
                .order_by_asc(credentials::Column::Id)
                .one(self.pool.connection())
                .await
                .map_err(|_| ModelSyncRepositoryError::Query)?
            else {
                return Ok(ModelDiscoveryTargetLookup::Unavailable);
            };
            if credential.id <= 0 {
                return Err(ModelSyncRepositoryError::Invariant);
            }

            let channel_type = channel
                .r#type
                .parse()
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let protocol = channel
                .protocol
                .parse()
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            let credential_kind = credential
                .kind
                .parse()
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
