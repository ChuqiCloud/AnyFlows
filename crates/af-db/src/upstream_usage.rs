//! Persistent upstream usage configuration, identity-bound snapshots and refresh leases.
use std::{collections::BTreeMap, future::Future, time::Duration};

use af_domain::{
    ChannelId, CredentialId, CredentialKind, ProxyId, UpstreamUsageConfig, UpstreamUsageError,
    UpstreamUsageProfile, UpstreamUsageProvider, UserId,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryResult,
    TransactionTrait,
    sea_query::{Alias, Expr, OnConflict, Query},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{channels, credentials},
};

mod bindings;
mod defaults;
mod snapshots;
use bindings::usage_bindings;
use defaults::channel_config_row;
#[cfg(test)]
mod tests;
pub use snapshots::{UpstreamUsageLease, UpstreamUsageStoredSnapshot};

#[derive(Clone)]
pub struct UpstreamUsageRepository {
    pool: DatabasePool,
}

pub struct UpstreamUsageTarget {
    pub channel_id: ChannelId,
    pub credential_id: CredentialId,
    pub secret_owner_id: CredentialId,
    pub kind: CredentialKind,
    pub quota_dimension: String,
    pub oauth_provider: Option<String>,
    pub oauth_account_key: Option<String>,
    pub base_url: Option<String>,
    pub envelope: EncryptedCredentialEnvelope,
    pub proxy_id: Option<ProxyId>,
    pub enabled: bool,
    pub config: UpstreamUsageConfig,
    pub profile: Option<UpstreamUsageProfile>,
    pub profile_secret: Option<String>,
    pub identity: String,
    pub query_identity: String,
    pub cache_key: String,
    pub operator_user_id: Option<UserId>,
}

impl UpstreamUsageRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    async fn bounded<T>(
        &self,
        future: impl Future<Output = Result<T, UpstreamUsageError>>,
    ) -> Result<T, UpstreamUsageError> {
        timeout(
            Duration::from_secs(5),
            future.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| UpstreamUsageError::Unavailable)?
    }

    pub async fn target(
        &self,
        channel: ChannelId,
        credential: CredentialId,
    ) -> Result<UpstreamUsageTarget, UpstreamUsageError> {
        self.bounded(load_target(self.pool.connection(), channel, credential))
            .await
    }

    pub async fn config(
        &self,
        channel: ChannelId,
        credential: CredentialId,
    ) -> Result<UpstreamUsageConfig, UpstreamUsageError> {
        Ok(self.target(channel, credential).await?.config)
    }

    pub async fn save_config(
        &self,
        actor: UserId,
        channel: ChannelId,
        credential: CredentialId,
        mut config: UpstreamUsageConfig,
    ) -> Result<UpstreamUsageConfig, UpstreamUsageError> {
        config.validate()?;
        config.inherited = false;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            // Serializes with key rotation, deletion and parallel config creation.
            let lock = Query::update()
                .table(credentials::Entity)
                .value(credentials::Column::Id, credential.get())
                .and_where(Expr::col(credentials::Column::Id).eq(credential.get()))
                .and_where(Expr::col(credentials::Column::ChannelId).eq(channel.get()))
                .and_where(Expr::col(credentials::Column::DeletedAt).is_null())
                .to_owned();
            txn.execute(txn.get_database_backend().build(&lock))
                .await
                .map_err(internal)?;
            let current = load_target(&txn, channel, credential).await?;
            if current.config.version != config.version {
                return Err(UpstreamUsageError::Conflict);
            }
            if let Some(id) = config.profile_id {
                let row = profile_row(&txn, id)
                    .await?
                    .ok_or(UpstreamUsageError::NotFound)?;
                let profile = decode_profile(&row)?;
                if !matches!(config.provider, UpstreamUsageProvider::Auto)
                    && config.provider != profile.provider
                {
                    return Err(UpstreamUsageError::InvalidInput);
                }
            }
            config.version = config
                .version
                .checked_add(1)
                .ok_or(UpstreamUsageError::Conflict)?;
            let encoded = serde_json::to_string(&config).map_err(internal)?;
            let values = [
                credential.get().into(),
                channel.get().into(),
                config.profile_id.into(),
                encoded.into(),
                config.version.into(),
                actor.get().into(),
                0_i64.into(),
            ];
            let mut insert = Query::insert();
            insert
                .into_table(Alias::new("credential_usage_configs"))
                .columns(
                    [
                        "credential_id",
                        "channel_id",
                        "profile_id",
                        "config_json",
                        "version",
                        "operator_user_id",
                        "next_refresh_at",
                    ]
                    .map(Alias::new),
                )
                .values_panic(values)
                .on_conflict(
                    OnConflict::column(Alias::new("credential_id"))
                        .update_columns(
                            [
                                "profile_id",
                                "config_json",
                                "version",
                                "operator_user_id",
                                "next_refresh_at",
                            ]
                            .map(Alias::new),
                        )
                        .to_owned(),
                );
            txn.execute(txn.get_database_backend().build(&insert))
                .await
                .map_err(internal)?;
            let clear = Query::update()
                .table(Alias::new("credential_usage_pauses"))
                .value(Alias::new("next_refresh_at"), 0_i64)
                .value(Alias::new("pause_until"), Option::<i64>::None)
                .value(Alias::new("pause_identity"), Option::<String>::None)
                .and_where(Expr::col(Alias::new("credential_id")).eq(credential.get()))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&clear))
                .await
                .map_err(internal)?;
            crate::scheduler_outbox::enqueue_scheduler_catalog_change(
                &txn,
                crate::SchedulerCatalogSubject::Channel(channel),
                crate::DatabaseTimestamp::now_utc(),
            )
            .await
            .map_err(internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(config)
        })
        .await
    }

    pub async fn profiles(&self) -> Result<Vec<UpstreamUsageProfile>, UpstreamUsageError> {
        self.bounded(async {
            let q = Query::select()
                .columns(["id", "profile_json", "version"].map(Alias::new))
                .from(Alias::new("upstream_usage_profiles"))
                .order_by(Alias::new("id"), sea_orm::Order::Asc)
                .limit(1001)
                .to_owned();
            let rows = self
                .pool
                .connection()
                .query_all(self.pool.connection().get_database_backend().build(&q))
                .await
                .map_err(internal)?;
            if rows.len() > 1000 {
                return Err(UpstreamUsageError::Unavailable);
            }
            rows.iter().map(decode_profile).collect()
        })
        .await
    }

    pub async fn profile(
        &self,
        id: i64,
    ) -> Result<(UpstreamUsageProfile, String), UpstreamUsageError> {
        self.bounded(async {
            let row = profile_row(self.pool.connection(), id)
                .await?
                .ok_or(UpstreamUsageError::NotFound)?;
            Ok((
                decode_profile(&row)?,
                row.try_get("", "secret_json").map_err(internal)?,
            ))
        })
        .await
    }

    pub async fn save_profile(
        &self,
        mut profile: UpstreamUsageProfile,
        secret_json: Option<String>,
    ) -> Result<UpstreamUsageProfile, UpstreamUsageError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let expected = profile.version;
            profile.version = expected
                .checked_add(1)
                .ok_or(UpstreamUsageError::Conflict)?;
            let encoded = serde_json::to_string(&profile).map_err(internal)?;
            if profile.id == 0 {
                if expected != 0 {
                    return Err(UpstreamUsageError::Conflict);
                }
                let mut insert = Query::insert()
                    .into_table(Alias::new("upstream_usage_profiles"))
                    .columns(["profile_json", "secret_json", "version"].map(Alias::new))
                    .values_panic([
                        encoded.into(),
                        secret_json.ok_or(UpstreamUsageError::InvalidInput)?.into(),
                        profile.version.into(),
                    ])
                    .to_owned();
                profile.id = if txn.get_database_backend() == sea_orm::DbBackend::Postgres {
                    insert.returning(Query::returning().column(Alias::new("id")));
                    txn.query_one(txn.get_database_backend().build(&insert))
                        .await
                        .map_err(internal)?
                        .ok_or(UpstreamUsageError::Unavailable)?
                        .try_get("", "id")
                        .map_err(internal)?
                } else {
                    i64::try_from(
                        txn.execute(txn.get_database_backend().build(&insert))
                            .await
                            .map_err(internal)?
                            .last_insert_id(),
                    )
                    .map_err(internal)?
                };
                if profile.id <= 0 {
                    return Err(UpstreamUsageError::Unavailable);
                }
            } else {
                let mut update = Query::update();
                update
                    .table(Alias::new("upstream_usage_profiles"))
                    .value(Alias::new("profile_json"), encoded)
                    .value(Alias::new("version"), profile.version)
                    .and_where(Expr::col(Alias::new("id")).eq(profile.id))
                    .and_where(Expr::col(Alias::new("version")).eq(expected));
                if let Some(secret) = secret_json {
                    update.value(Alias::new("secret_json"), secret);
                }
                let changed = txn
                    .execute(txn.get_database_backend().build(&update))
                    .await
                    .map_err(internal)?;
                if changed.rows_affected() != 1 {
                    return Err(UpstreamUsageError::Conflict);
                }
                let reset = Query::update()
                    .table(Alias::new("credential_usage_configs"))
                    .value(Alias::new("next_refresh_at"), 0_i64)
                    .value(Alias::new("pause_until"), Option::<i64>::None)
                    .value(Alias::new("pause_identity"), Option::<String>::None)
                    .and_where(Expr::col(Alias::new("profile_id")).eq(profile.id))
                    .to_owned();
                txn.execute(txn.get_database_backend().build(&reset))
                    .await
                    .map_err(internal)?;
                let bound = usage_bindings(&txn, Some(profile.id), None).await?;
                let mut affected = std::collections::BTreeSet::new();
                for (channel, _) in bound {
                    affected.insert(channel);
                }
                for channel in affected {
                    defaults::clear_channel_runtime(&txn, channel).await?;
                }
            }
            txn.commit().await.map_err(internal)?;
            Ok(profile)
        })
        .await
    }

    pub async fn delete_profile(&self, id: i64) -> Result<(), UpstreamUsageError> {
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            let lock = Query::update()
                .table(Alias::new("upstream_usage_profiles"))
                .value(Alias::new("id"), id)
                .and_where(Expr::col(Alias::new("id")).eq(id))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&lock))
                .await
                .map_err(internal)?;
            if profile_row(&txn, id).await?.is_none() {
                return Err(UpstreamUsageError::NotFound);
            }
            let refs = Query::select()
                .column(Alias::new("credential_id"))
                .from(Alias::new("credential_usage_configs"))
                .and_where(Expr::col(Alias::new("profile_id")).eq(id))
                .limit(1)
                .to_owned();
            if txn
                .query_one(txn.get_database_backend().build(&refs))
                .await
                .map_err(internal)?
                .is_some()
            {
                return Err(UpstreamUsageError::InUse);
            }
            let refs = Query::select()
                .column(Alias::new("channel_id"))
                .from(Alias::new("channel_usage_configs"))
                .and_where(Expr::col(Alias::new("profile_id")).eq(id))
                .limit(1)
                .to_owned();
            if txn
                .query_one(txn.get_database_backend().build(&refs))
                .await
                .map_err(internal)?
                .is_some()
            {
                return Err(UpstreamUsageError::InUse);
            }
            let delete = Query::delete()
                .from_table(Alias::new("upstream_usage_profiles"))
                .and_where(Expr::col(Alias::new("id")).eq(id))
                .to_owned();
            txn.execute(txn.get_database_backend().build(&delete))
                .await
                .map_err(|_| UpstreamUsageError::InUse)?;
            txn.commit().await.map_err(internal)?;
            Ok(())
        })
        .await
    }
}

pub(crate) async fn load_usage_pauses(
    connection: &DatabaseConnection,
    credential_ids: &[i64],
) -> Result<BTreeMap<i64, i64>, UpstreamUsageError> {
    if credential_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let now = now_epoch();
    let query = Query::select()
        .columns(
            [
                "credential_id",
                "channel_id",
                "pause_until",
                "pause_identity",
            ]
            .map(Alias::new),
        )
        .from(Alias::new("credential_usage_pauses"))
        .and_where(Expr::col(Alias::new("credential_id")).is_in(credential_ids.iter().copied()))
        .and_where(Expr::col(Alias::new("pause_until")).gt(now))
        .to_owned();
    let rows = connection
        .query_all(connection.get_database_backend().build(&query))
        .await
        .map_err(internal)?;
    let mut result = BTreeMap::new();
    for row in rows {
        let id = row.try_get::<i64>("", "credential_id").map_err(internal)?;
        let channel =
            ChannelId::new(row.try_get("", "channel_id").map_err(internal)?).map_err(internal)?;
        let credential = CredentialId::new(id).map_err(internal)?;
        let identity = row
            .try_get::<Option<String>>("", "pause_identity")
            .map_err(internal)?;
        let target = match load_target(connection, channel, credential).await {
            Ok(target) => target,
            Err(
                UpstreamUsageError::NotFound
                | UpstreamUsageError::InvalidInput
                | UpstreamUsageError::Conflict,
            ) => continue,
            Err(error) => return Err(error),
        };
        if identity.as_deref() == Some(&target.identity) {
            result.insert(id, row.try_get("", "pause_until").map_err(internal)?);
        }
    }
    Ok(result)
}

async fn load_target<C: ConnectionTrait>(
    connection: &C,
    channel: ChannelId,
    credential: CredentialId,
) -> Result<UpstreamUsageTarget, UpstreamUsageError> {
    let channel_row = channels::Entity::find_by_id(channel.get())
        .filter(channels::Column::DeletedAt.is_null())
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(UpstreamUsageError::NotFound)?;
    let row = credentials::Entity::find_by_id(credential.get())
        .filter(credentials::Column::ChannelId.eq(channel.get()))
        .filter(credentials::Column::DeletedAt.is_null())
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(UpstreamUsageError::NotFound)?;
    let secret_owner = if let Some(parent) = row.parent_id {
        credentials::Entity::find_by_id(parent)
            .filter(credentials::Column::ChannelId.eq(channel.get()))
            .filter(credentials::Column::DeletedAt.is_null())
            .filter(credentials::Column::ParentId.is_null())
            .one(connection)
            .await
            .map_err(internal)?
            .ok_or(UpstreamUsageError::NotFound)?
    } else {
        row.clone()
    };
    let query = Query::select()
        .columns(["config_json", "operator_user_id"].map(Alias::new))
        .from(Alias::new("credential_usage_configs"))
        .and_where(Expr::col(Alias::new("credential_id")).eq(credential.get()))
        .to_owned();
    let explicit = connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(internal)?;
    let is_inherited = explicit.is_none();
    let stored = match explicit {
        Some(row) => Some(row),
        None => channel_config_row(connection, channel).await?,
    };
    let mut config: UpstreamUsageConfig = match &stored {
        Some(row) => {
            serde_json::from_str(&row.try_get::<String>("", "config_json").map_err(internal)?)
                .map_err(|_| UpstreamUsageError::InvalidInput)?
        }
        None => UpstreamUsageConfig::default(),
    };
    let source_version = config.version;
    if is_inherited {
        config.version = 0;
        config.inherited = true;
    }
    let (profile, profile_secret) = match config.profile_id {
        Some(id) => {
            let row = profile_row(connection, id)
                .await?
                .ok_or(UpstreamUsageError::NotFound)?;
            (
                Some(decode_profile(&row)?),
                Some(row.try_get("", "secret_json").map_err(internal)?),
            )
        }
        None => (None, None),
    };
    let (key, nonce, ciphertext) = secret_owner.secret.envelope_parts().map_err(internal)?;
    let envelope = EncryptedCredentialEnvelope::new(key, nonce, ciphertext).map_err(internal)?;
    let mut hash = Sha256::new();
    // Include target and secret revisions, so rotations never reuse an older observation.
    for value in [
        channel.get().to_string(),
        credential.get().to_string(),
        secret_owner.id.to_string(),
        channel_row.updated_at.unix_timestamp_nanos().to_string(),
        row.updated_at.unix_timestamp_nanos().to_string(),
        secret_owner.updated_at.unix_timestamp_nanos().to_string(),
        serde_json::to_string(&config).map_err(internal)?,
        profile
            .as_ref()
            .map(|p| format!("{}:{}", p.id, p.version))
            .unwrap_or_default(),
        envelope.key_id().to_owned(),
        source_version.to_string(),
    ] {
        hash.update(value.len().to_le_bytes());
        hash.update(value.as_bytes());
    }
    hash.update(envelope.nonce());
    hash.update(envelope.ciphertext());
    let identity = format!("{:x}", hash.finalize());
    let query_identity = profile.as_ref().map_or_else(
        || identity.clone(),
        |p| {
            format!(
                "{:x}",
                Sha256::digest(format!("account:{}:{}", p.id, p.version).as_bytes())
            )
        },
    );
    let cache_key = profile.as_ref().map_or_else(
        || format!("key:{}:{}", channel.get(), credential.get()),
        |p| format!("account:{}:{}", p.id, p.version),
    );
    let enabled = channel_row.status == 1
        && row.status == 1
        && row.schedulable
        && !row.oauth_token_pending
        && secret_owner.status == 1
        && !secret_owner.oauth_token_pending;
    Ok(UpstreamUsageTarget {
        channel_id: channel,
        credential_id: credential,
        secret_owner_id: CredentialId::new(secret_owner.id).map_err(internal)?,
        kind: secret_owner.kind.parse().map_err(internal)?,
        quota_dimension: row.quota_dimension,
        oauth_provider: secret_owner.oauth_provider,
        oauth_account_key: secret_owner.oauth_account_key,
        base_url: channel_row.base_url.map(|v| v.as_str().to_owned()),
        envelope,
        proxy_id: secret_owner
            .proxy_id
            .map(ProxyId::new)
            .transpose()
            .map_err(internal)?,
        enabled,
        operator_user_id: stored
            .as_ref()
            .map(|r| r.try_get::<i64>("", "operator_user_id").map_err(internal))
            .transpose()?
            .map(UserId::new)
            .transpose()
            .map_err(internal)?,
        config,
        profile,
        profile_secret,
        identity,
        query_identity,
        cache_key,
    })
}

async fn profile_row<C: ConnectionTrait>(
    connection: &C,
    id: i64,
) -> Result<Option<QueryResult>, UpstreamUsageError> {
    let query = Query::select()
        .columns(["id", "profile_json", "secret_json", "version"].map(Alias::new))
        .from(Alias::new("upstream_usage_profiles"))
        .and_where(Expr::col(Alias::new("id")).eq(id))
        .to_owned();
    connection
        .query_one(connection.get_database_backend().build(&query))
        .await
        .map_err(internal)
}

fn decode_profile(row: &QueryResult) -> Result<UpstreamUsageProfile, UpstreamUsageError> {
    let mut profile: UpstreamUsageProfile = serde_json::from_str(
        &row.try_get::<String>("", "profile_json")
            .map_err(internal)?,
    )
    .map_err(internal)?;
    profile.id = row.try_get("", "id").map_err(internal)?;
    profile.version = row.try_get("", "version").map_err(internal)?;
    Ok(profile)
}

fn internal<E>(_: E) -> UpstreamUsageError {
    UpstreamUsageError::Unavailable
}

pub fn upstream_usage_now_epoch() -> i64 {
    now_epoch()
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}
