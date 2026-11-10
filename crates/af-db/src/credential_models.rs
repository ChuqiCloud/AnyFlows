//! Administrative model rules and upstream observations are separate sources of truth.
use std::{collections::BTreeMap, future::Future, time::Duration};

use af_domain::{ChannelId, CredentialId, UpstreamModelPolicy, UpstreamModelPolicyMode};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, IntoActiveModel,
    QueryFilter, QueryOrder, Set, TransactionTrait,
    sea_query::{Expr, OnConflict},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    DatabasePool, ModelDiscoveryTargetLookup, ModelSyncRepository, SchedulerCatalogSubject,
    entity::{
        channel_models, channels, credential_model_observations as observations, credentials,
        upstream_model_policies as policies,
    },
    scheduler_outbox::enqueue_scheduler_catalog_change,
};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum CredentialModelError {
    #[error("invalid model policy")]
    InvalidInput,
    #[error("model target not found")]
    NotFound,
    #[error("model configuration changed")]
    Conflict,
    #[error("model access forbidden")]
    Forbidden,
    #[error("model storage unavailable")]
    Internal,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpstreamModelPolicySettings {
    pub policy: UpstreamModelPolicy,
    pub version: i64,
    pub auto_add_discovered: bool,
    pub refresh_interval_secs: Option<i64>,
}

impl UpstreamModelPolicySettings {
    pub fn validate(&self) -> Result<(), CredentialModelError> {
        self.policy
            .validate()
            .map_err(|_| CredentialModelError::InvalidInput)?;
        if self.version < 0
            || self.version == i64::MAX
            || self
                .refresh_interval_secs
                .is_some_and(|seconds| !(300..=2_592_000).contains(&seconds))
        {
            return Err(CredentialModelError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CredentialModelItem {
    pub upstream_model: String,
    pub canonical_models: Vec<String>,
    pub enabled: bool,
    pub blocked_reason: Option<String>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct CredentialModelsSnapshot {
    pub items: Vec<CredentialModelItem>,
    #[serde(flatten)]
    pub settings: UpstreamModelPolicySettings,
    pub last_success_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    pub status: String,
    pub error_code: Option<String>,
}

/// A lease owns one physical identity. Shadow rules remain local to routing IDs.
pub struct CredentialModelRefreshLease {
    pub channel_id: ChannelId,
    pub credential_id: CredentialId,
    pub source_id: CredentialId,
    fingerprint: String,
    token: String,
}

#[derive(Clone)]
pub struct CredentialModelsRepository {
    pool: DatabasePool,
    timeout: Duration,
}

impl CredentialModelsRepository {
    #[must_use]
    pub fn new(pool: DatabasePool, timeout: Duration) -> Self {
        Self { pool, timeout }
    }

    async fn bounded<T>(
        &self,
        operation: impl Future<Output = Result<T, CredentialModelError>>,
    ) -> Result<T, CredentialModelError> {
        tokio::time::timeout(self.timeout, operation)
            .await
            .map_err(|_| CredentialModelError::Internal)?
    }

    pub async fn policy(
        &self,
        channel: ChannelId,
        credential: Option<CredentialId>,
    ) -> Result<UpstreamModelPolicySettings, CredentialModelError> {
        self.bounded(async {
            scoped(self.pool.connection(), channel, credential).await?;
            read_policy(self.pool.connection(), channel, credential).await
        })
        .await
    }

    pub async fn write_policy(
        &self,
        channel: ChannelId,
        credential: Option<CredentialId>,
        settings: UpstreamModelPolicySettings,
    ) -> Result<UpstreamModelPolicySettings, CredentialModelError> {
        self.write_policy_inner(None, channel, credential, settings)
            .await
    }

    pub async fn write_policy_audited(
        &self,
        actor: af_domain::UserId,
        channel: ChannelId,
        credential: Option<CredentialId>,
        settings: UpstreamModelPolicySettings,
    ) -> Result<UpstreamModelPolicySettings, CredentialModelError> {
        self.write_policy_inner(Some(actor), channel, credential, settings)
            .await
    }

    async fn write_policy_inner(
        &self,
        actor: Option<af_domain::UserId>,
        channel: ChannelId,
        credential: Option<CredentialId>,
        settings: UpstreamModelPolicySettings,
    ) -> Result<UpstreamModelPolicySettings, CredentialModelError> {
        settings.validate()?;
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_channel(&txn, channel).await?;
            scoped(&txn, channel, credential).await?;
            let old = read_policy(&txn, channel, credential).await?;
            if old.version != settings.version { return Err(CredentialModelError::Conflict); }
            let mut updated = settings;
            updated.version += 1;
            store_policy(&txn, channel, credential, &updated).await?;
            if old.refresh_interval_secs != updated.refresh_interval_secs {
                let mut reset = observations::Entity::update_many().filter(observations::Column::ChannelId.eq(channel.get()));
                if let Some(key_id) = credential {
                    let key = credentials::Entity::find_by_id(key_id.get()).one(&txn).await.map_err(internal)?.ok_or(CredentialModelError::NotFound)?;
                    reset = reset.filter(observations::Column::CredentialId.eq(key.parent_id.unwrap_or(key.id)));
                }
                reset.col_expr(observations::Column::NextRefreshAt, Expr::value(0_i64)).exec(&txn).await.map_err(internal)?;
            }
            enqueue_scheduler_catalog_change(&txn, SchedulerCatalogSubject::Channel(channel), now_date()).await.map_err(internal)?;
            if let Some(actor) = actor {
                crate::entity::platform_audit_logs::ActiveModel {
                    operator_user_id: Set(actor.get()), permission_code: Set("channels.manage".to_owned()),
                    route: Set("/api/admin/channels/{channel_id}/model-policy".to_owned()), operation: Set("upstream_model_policy.update".to_owned()),
                    resource: Set("upstream_model_policy".to_owned()), resource_id: Set(Some(credential.map_or_else(|| format!("channel:{}",channel.get()), |id| format!("credential:{}",id.get())))),
                    outcome: Set(1), before_value: Set(None), after_value: Set(Some(serde_json::json!({"version":updated.version,
                        "mode":updated.policy.mode,"model_count":updated.policy.models.len(),"disabled_count":updated.policy.disabled_models.len(),
                        "auto_add_discovered":updated.auto_add_discovered,"refresh_interval_secs":updated.refresh_interval_secs}).to_string())),
                    audit_info: Set(None), request_id: Set(format!("model-policy:{}:{}",channel.get(),now_date().unix_timestamp_nanos())),
                    created_at: Set(now_date()), ..Default::default()
                }.insert(&txn).await.map_err(internal)?;
            }
            txn.commit().await.map_err(internal)?;
            Ok(updated)
        }).await
    }

    pub async fn models(
        &self,
        channel: ChannelId,
        credential: CredentialId,
    ) -> Result<CredentialModelsSnapshot, CredentialModelError> {
        self.bounded(async {
            let (channel_row, key) =
                scoped(self.pool.connection(), channel, Some(credential)).await?;
            let key = key.ok_or(CredentialModelError::NotFound)?;
            let source = key.parent_id.unwrap_or(key.id);
            let settings = read_policy(self.pool.connection(), channel, Some(credential)).await?;
            let channel_policy = read_policy(self.pool.connection(), channel, None)
                .await?
                .policy;
            let parent_policy = if source != key.id {
                read_policy(
                    self.pool.connection(),
                    channel,
                    Some(CredentialId::new(source).map_err(internal)?),
                )
                .await?
                .policy
            } else {
                UpstreamModelPolicy::default()
            };
            let mut observation = observations::Entity::find_by_id(source)
                .one(self.pool.connection())
                .await
                .map_err(internal)?;
            let source_key = if source != key.id {
                credentials::Entity::find_by_id(source)
                    .one(self.pool.connection())
                    .await
                    .map_err(internal)?
                    .ok_or(CredentialModelError::NotFound)?
            } else {
                key.clone()
            };
            let current_fingerprint = crate::model_sync::target_fingerprint(
                self.pool.connection(),
                &channel_row,
                &source_key,
            )
            .await
            .map_err(internal)?;
            let stale_identity = observation
                .as_ref()
                .is_some_and(|row| row.identity_fingerprint != current_fingerprint);
            if stale_identity {
                observation = None;
            }
            let mut indexed = BTreeMap::<String, CredentialModelItem>::new();
            if let Some(record) = &observation {
                let reported: Vec<String> =
                    serde_json::from_value(record.items.clone()).map_err(internal)?;
                for name in reported {
                    indexed.insert(name.clone(), model_item(name, "discovered"));
                }
            }
            for name in settings
                .policy
                .models
                .iter()
                .chain(&settings.policy.disabled_models)
            {
                indexed
                    .entry(name.clone())
                    .or_insert_with(|| model_item(name.clone(), "manual"));
            }
            let mappings =
                crate::ChannelModelMappings::parse(&channel_row.model_mapping).map_err(internal)?;
            let published = channel_models::Entity::find()
                .filter(channel_models::Column::ChannelId.eq(channel.get()))
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            for entry in published {
                let upstream = mappings
                    .resolve(&entry.model)
                    .unwrap_or(&entry.model)
                    .to_owned();
                indexed
                    .entry(upstream.clone())
                    .or_insert_with(|| model_item(upstream.clone(), "channel"))
                    .canonical_models
                    .push(entry.model.clone());
                let compact = crate::channel_settings::responses_compact_model_mapping(
                    &channel_row.settings.clone().into_inner(),
                )
                .map_err(|_| CredentialModelError::Internal)?;
                if let Some(compact_model) = compact.resolve(&upstream) {
                    indexed
                        .entry(compact_model.to_owned())
                        .or_insert_with(|| model_item(compact_model.to_owned(), "channel"))
                        .canonical_models
                        .push(entry.model);
                }
            }
            for item in indexed.values_mut() {
                item.canonical_models.sort();
                item.canonical_models.dedup();
                item.blocked_reason = if channel_row.status != 1 {
                    Some("channel_disabled")
                } else if key.status != 1
                    || !key.schedulable
                    || key.oauth_token_pending
                    || source_key.status != 1
                    || source_key.oauth_token_pending
                    || source_key.deleted_at.is_some()
                {
                    Some("credential_disabled")
                } else if item.canonical_models.is_empty() {
                    Some("channel_not_published")
                } else if !item.canonical_models.iter().any(|model| {
                    af_domain::CredentialQuotaDimension::for_canonical_model(model).as_str()
                        == key.quota_dimension
                }) {
                    Some("quota_dimension")
                } else if !channel_policy.allows(&item.upstream_model) {
                    Some("channel_policy")
                } else if !parent_policy.allows(&item.upstream_model) {
                    Some("parent_policy")
                } else if !settings.policy.allows(&item.upstream_model) {
                    Some("credential_policy")
                } else {
                    None
                }
                .map(str::to_owned);
                item.enabled = item.blocked_reason.is_none();
            }
            let (last_success_at, last_checked_at, status, error_code) = match observation {
                Some(row) => (
                    row.last_success_at,
                    row.last_checked_at,
                    row.status,
                    row.error_code,
                ),
                None => (
                    None,
                    None,
                    if stale_identity {
                        "unavailable"
                    } else {
                        "unknown"
                    }
                    .to_owned(),
                    stale_identity.then(|| "identity_changed".to_owned()),
                ),
            };
            Ok(CredentialModelsSnapshot {
                items: indexed.into_values().collect(),
                settings,
                last_success_at,
                last_checked_at,
                status,
                error_code,
            })
        })
        .await
    }

    pub async fn claim_refresh(
        &self,
        channel: ChannelId,
        credential: CredentialId,
        manual: bool,
    ) -> Result<Option<CredentialModelRefreshLease>, CredentialModelError> {
        let target = match ModelSyncRepository::new(self.pool.clone())
            .load_discovery_target_for_credential(channel, credential, manual)
            .await
            .map_err(internal)?
        {
            ModelDiscoveryTargetLookup::Found(target) => target,
            ModelDiscoveryTargetLookup::NotFound => return Err(CredentialModelError::NotFound),
            ModelDiscoveryTargetLookup::Unavailable => return Err(CredentialModelError::Conflict),
        };
        let source = CredentialId::new(target.credential_id()).map_err(internal)?;
        let fingerprint = target.identity_fingerprint();
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).map_err(internal)?;
        let token = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_channel(&txn, channel).await?;
            scoped(&txn, channel, Some(credential)).await?;
            let now = now_seconds();
            observations::Entity::insert(observations::ActiveModel {
                credential_id: Set(source.get()),
                channel_id: Set(channel.get()),
                identity_fingerprint: Set(fingerprint.clone()),
                items: Set(serde_json::json!([])),
                status: Set("unknown".to_owned()),
                error_code: Set(None),
                last_success_at: Set(None),
                last_checked_at: Set(None),
                lease_token: Set(None),
                lease_until: Set(None),
                next_refresh_at: Set(0),
            })
            .on_conflict(
                OnConflict::column(observations::Column::CredentialId)
                    .do_nothing_on([observations::Column::CredentialId])
                    .to_owned(),
            )
            .exec_without_returning(&txn)
            .await
            .map_err(internal)?;
            let record = observations::Entity::find_by_id(source.get())
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(CredentialModelError::Internal)?;
            if record.identity_fingerprint == fingerprint
                && (record.lease_until.is_some_and(|until| until > now)
                    || (manual
                        && record
                            .last_checked_at
                            .is_some_and(|checked| checked.saturating_add(30) > now))
                    || (!manual && record.next_refresh_at > now))
            {
                return Ok(None);
            }
            let identity_changed = record.identity_fingerprint != fingerprint;
            let mut record = record.into_active_model();
            if identity_changed {
                record.identity_fingerprint = Set(fingerprint.clone());
                record.items = Set(serde_json::json!([]));
                record.status = Set("unknown".to_owned());
                record.last_success_at = Set(None);
                record.error_code = Set(None);
            }
            record.lease_token = Set(Some(token.clone()));
            record.lease_until = Set(Some(now + 180));
            record.update(&txn).await.map_err(internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(Some(CredentialModelRefreshLease {
                channel_id: channel,
                credential_id: credential,
                source_id: source,
                fingerprint,
                token,
            }))
        })
        .await
    }

    /// Full-page success commits atomically. Errors never discard the last good list.
    pub async fn complete_refresh(
        &self,
        lease: CredentialModelRefreshLease,
        result: Result<Vec<String>, String>,
    ) -> Result<(), CredentialModelError> {
        if let Ok(models) = &result {
            UpstreamModelPolicy {
                mode: UpstreamModelPolicyMode::Allowlist,
                models: models.clone(),
                disabled_models: Vec::new(),
            }
            .validate()
            .map_err(|_| CredentialModelError::InvalidInput)?;
        }
        let current = match ModelSyncRepository::new(self.pool.clone())
            .load_discovery_target_for_credential(lease.channel_id, lease.credential_id, true)
            .await
            .map_err(internal)?
        {
            ModelDiscoveryTargetLookup::Found(target) => target.identity_fingerprint(),
            _ => return Err(CredentialModelError::Conflict),
        };
        self.bounded(async {
            let txn = self.pool.connection().begin().await.map_err(internal)?;
            lock_channel(&txn, lease.channel_id).await?;
            let locked_keys =
                lock_model_credentials(&txn, &[lease.credential_id.get(), lease.source_id.get()])
                    .await?;
            let row = observations::Entity::find_by_id(lease.source_id.get())
                .one(&txn)
                .await
                .map_err(internal)?
                .ok_or(CredentialModelError::Conflict)?;
            if row.lease_token.as_deref() != Some(&lease.token) || current != lease.fingerprint {
                return Err(CredentialModelError::Conflict);
            }
            // Recheck under the same channel lock used by credential/config writers.
            let current_rows = scoped(&txn, lease.channel_id, Some(lease.credential_id)).await?;
            let current_key = locked_keys
                .get(&lease.credential_id.get())
                .ok_or(CredentialModelError::NotFound)?;
            if current_key.parent_id.unwrap_or(current_key.id) != lease.source_id.get() {
                return Err(CredentialModelError::Conflict);
            }
            let owner = locked_keys
                .get(&lease.source_id.get())
                .ok_or(CredentialModelError::NotFound)?;
            if crate::model_sync::target_fingerprint(&txn, &current_rows.0, owner)
                .await
                .map_err(internal)?
                != lease.fingerprint
            {
                return Err(CredentialModelError::Conflict);
            }
            let now = now_seconds();
            let settings = read_policy(&txn, lease.channel_id, Some(lease.credential_id)).await?;
            let mut active = row.into_active_model();
            active.last_checked_at = Set(Some(now));
            active.lease_token = Set(None);
            active.lease_until = Set(None);
            active.identity_fingerprint = Set(current);
            let interval = settings.refresh_interval_secs.unwrap_or(21_600);
            match result {
                Ok(mut names) => {
                    names.sort();
                    names.dedup();
                    let related = credentials::Entity::find()
                        .filter(credentials::Column::ChannelId.eq(lease.channel_id.get()))
                        .filter(credentials::Column::DeletedAt.is_null())
                        .filter(
                            Condition::any()
                                .add(credentials::Column::Id.eq(lease.source_id.get()))
                                .add(credentials::Column::ParentId.eq(lease.source_id.get())),
                        )
                        .all(&txn)
                        .await
                        .map_err(internal)?;
                    let mut effective_interval = interval;
                    let mut policy_changed = false;
                    for key in related {
                        let id = CredentialId::new(key.id).map_err(internal)?;
                        let mut own = read_policy(&txn, lease.channel_id, Some(id)).await?;
                        if let Some(seconds) = own.refresh_interval_secs {
                            effective_interval = effective_interval.min(seconds);
                        }
                        if !own.auto_add_discovered
                            || own.policy.mode != UpstreamModelPolicyMode::Allowlist
                        {
                            continue;
                        }
                        let mut changed = false;
                        for name in &names {
                            if own.policy.models.len() >= 1000 {
                                break;
                            }
                            if !own.policy.models.contains(name)
                                && !own.policy.disabled_models.contains(name)
                            {
                                own.policy.models.push(name.clone());
                                changed = true;
                            }
                        }
                        if changed {
                            own.version = own
                                .version
                                .checked_add(1)
                                .ok_or(CredentialModelError::Conflict)?;
                            own.validate()?;
                            store_policy(&txn, lease.channel_id, Some(id), &own).await?;
                            policy_changed = true;
                        }
                    }
                    if policy_changed {
                        enqueue_scheduler_catalog_change(
                            &txn,
                            SchedulerCatalogSubject::Channel(lease.channel_id),
                            now_date(),
                        )
                        .await
                        .map_err(internal)?;
                    }
                    active.status = Set(if names.is_empty() {
                        "empty"
                    } else {
                        "available"
                    }
                    .to_owned());
                    active.items = Set(serde_json::json!(names));
                    active.last_success_at = Set(Some(now));
                    active.error_code = Set(None);
                    active.next_refresh_at = Set(now.saturating_add(effective_interval));
                }
                Err(code) => {
                    active.status = Set("unavailable".to_owned());
                    active.error_code = Set(Some(sanitized_error(&code).to_owned()));
                    active.next_refresh_at = Set(now.saturating_add(interval.clamp(300, 3600)));
                }
            }
            active.update(&txn).await.map_err(internal)?;
            txn.commit().await.map_err(internal)?;
            Ok(())
        })
        .await
    }

    /// Bounded due work; the shared physical-identity lease prevents duplicate probes.
    pub async fn due_credentials(
        &self,
        limit: u64,
    ) -> Result<Vec<(ChannelId, CredentialId)>, CredentialModelError> {
        use sea_orm::QuerySelect;
        self.bounded(async {
            let effective_interval = "CASE WHEN EXISTS (SELECT 1 FROM upstream_model_policies p WHERE p.owner_kind='credential' AND p.owner_id=credentials.id) THEN (SELECT p.refresh_interval_secs FROM upstream_model_policies p WHERE p.owner_kind='credential' AND p.owner_id=credentials.id) ELSE (SELECT p.refresh_interval_secs FROM upstream_model_policies p WHERE p.owner_kind='channel' AND p.owner_id=credentials.channel_id) END";
            let rows = credentials::Entity::find()
                .filter(credentials::Column::Status.eq(1_i16)).filter(credentials::Column::Schedulable.eq(true))
                .filter(credentials::Column::OauthTokenPending.eq(false)).filter(credentials::Column::DeletedAt.is_null())
                .filter(Expr::cust(format!("({effective_interval}) IS NOT NULL")))
                .filter(Expr::cust("EXISTS (SELECT 1 FROM channels h WHERE h.id=credentials.channel_id AND h.status=1 AND h.deleted_at IS NULL)"))
                .filter(Expr::cust("credentials.parent_id IS NULL OR EXISTS (SELECT 1 FROM credentials p WHERE p.id=credentials.parent_id AND p.status=1 AND p.oauth_token_pending=FALSE AND p.deleted_at IS NULL)"))
                .filter(Expr::cust_with_values("NOT EXISTS (SELECT 1 FROM credential_model_observations o WHERE o.credential_id=COALESCE(credentials.parent_id,credentials.id) AND (o.next_refresh_at > ? OR o.lease_until > ?))", [now_seconds(), now_seconds()]))
                .order_by_asc(credentials::Column::Id).limit(limit.min(100)).all(self.pool.connection()).await.map_err(internal)?;
            let mut due = Vec::new();
            for row in rows {
                due.push((ChannelId::new(row.channel_id).map_err(internal)?, CredentialId::new(row.id).map_err(internal)?));
            }
            Ok(due)
        }).await
    }
}

fn model_item(upstream_model: String, source: &str) -> CredentialModelItem {
    CredentialModelItem {
        upstream_model,
        canonical_models: Vec::new(),
        enabled: true,
        blocked_reason: None,
        source: source.to_owned(),
    }
}

pub(crate) async fn load_model_policies<C: ConnectionTrait>(
    connection: &C,
    channel_ids: &[i64],
) -> Result<BTreeMap<(String, i64), UpstreamModelPolicy>, CredentialModelError> {
    let mut result = BTreeMap::new();
    for batch in channel_ids.chunks(400) {
        let rows = policies::Entity::find()
            .filter(policies::Column::ChannelId.is_in(batch.iter().copied()))
            .all(connection)
            .await
            .map_err(internal)?;
        for row in rows {
            if !matches!(row.owner_kind.as_str(), "channel" | "credential")
                || row.owner_id <= 0
                || (row.owner_kind == "channel" && row.owner_id != row.channel_id)
            {
                return Err(CredentialModelError::Internal);
            }
            let settings = settings_from_row(row.clone())?;
            result.insert((row.owner_kind, row.owner_id), settings.policy);
        }
    }
    Ok(result)
}

async fn scoped<C: ConnectionTrait>(
    connection: &C,
    channel: ChannelId,
    credential: Option<CredentialId>,
) -> Result<(channels::Model, Option<credentials::Model>), CredentialModelError> {
    let channel = channels::Entity::find_by_id(channel.get())
        .filter(channels::Column::DeletedAt.is_null())
        .one(connection)
        .await
        .map_err(internal)?
        .ok_or(CredentialModelError::NotFound)?;
    let key = if let Some(id) = credential {
        Some(
            credentials::Entity::find_by_id(id.get())
                .filter(credentials::Column::ChannelId.eq(channel.id))
                .filter(credentials::Column::DeletedAt.is_null())
                .one(connection)
                .await
                .map_err(internal)?
                .ok_or(CredentialModelError::NotFound)?,
        )
    } else {
        None
    };
    Ok((channel, key))
}

async fn lock_channel(
    txn: &sea_orm::DatabaseTransaction,
    channel: ChannelId,
) -> Result<(), CredentialModelError> {
    // An actual write lock on SQLite; row lock on PostgreSQL/MySQL. Existing channel
    // and credential mutations use this same channel before changing their children.
    use sea_orm::{DbBackend, QuerySelect, sea_query::LockType};
    if txn.get_database_backend() == DbBackend::Sqlite {
        let count = channels::Entity::update_many()
            .filter(channels::Column::Id.eq(channel.get()))
            .filter(channels::Column::DeletedAt.is_null())
            .col_expr(
                channels::Column::Status,
                Expr::col(channels::Column::Status).into(),
            )
            .exec(txn)
            .await
            .map_err(internal)?
            .rows_affected;
        if count == 0 {
            return Err(CredentialModelError::NotFound);
        }
    } else if channels::Entity::find_by_id(channel.get())
        .filter(channels::Column::DeletedAt.is_null())
        .lock(LockType::Update)
        .one(txn)
        .await
        .map_err(internal)?
        .is_none()
    {
        return Err(CredentialModelError::NotFound);
    }
    Ok(())
}

async fn lock_model_credentials(
    txn: &sea_orm::DatabaseTransaction,
    ids: &[i64],
) -> Result<BTreeMap<i64, credentials::Model>, CredentialModelError> {
    use sea_orm::{DbBackend, QuerySelect, sea_query::LockType};
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut result = BTreeMap::new();
    for id in ids {
        let mut query =
            credentials::Entity::find_by_id(id).filter(credentials::Column::DeletedAt.is_null());
        if txn.get_database_backend() != DbBackend::Sqlite {
            query = query.lock(LockType::Update);
        }
        let row = query
            .one(txn)
            .await
            .map_err(internal)?
            .ok_or(CredentialModelError::NotFound)?;
        result.insert(id, row);
    }
    Ok(result)
}

async fn read_policy<C: ConnectionTrait>(
    connection: &C,
    channel: ChannelId,
    credential: Option<CredentialId>,
) -> Result<UpstreamModelPolicySettings, CredentialModelError> {
    let (kind, id) = owner(channel, credential);
    let row = policies::Entity::find_by_id((kind.to_owned(), id))
        .one(connection)
        .await
        .map_err(internal)?;
    if row
        .as_ref()
        .is_some_and(|row| row.channel_id != channel.get())
    {
        return Err(CredentialModelError::Internal);
    }
    if let Some(row) = row {
        return settings_from_row(row);
    }
    let mut settings = UpstreamModelPolicySettings::default();
    if credential.is_some()
        && let Some(channel_policy) =
            policies::Entity::find_by_id(("channel".to_owned(), channel.get()))
                .one(connection)
                .await
                .map_err(internal)?
    {
        let inherited = settings_from_row(channel_policy)?;
        settings.refresh_interval_secs = inherited.refresh_interval_secs;
        settings.auto_add_discovered = inherited.auto_add_discovered;
    }
    Ok(settings)
}

fn settings_from_row(
    row: policies::Model,
) -> Result<UpstreamModelPolicySettings, CredentialModelError> {
    let settings = UpstreamModelPolicySettings {
        policy: serde_json::from_value(row.policy).map_err(internal)?,
        version: row.version,
        auto_add_discovered: row.auto_add_discovered,
        refresh_interval_secs: row.refresh_interval_secs,
    };
    settings
        .validate()
        .map_err(|_| CredentialModelError::Internal)?;
    Ok(settings)
}

async fn store_policy<C: ConnectionTrait>(
    connection: &C,
    channel: ChannelId,
    credential: Option<CredentialId>,
    settings: &UpstreamModelPolicySettings,
) -> Result<(), CredentialModelError> {
    let (kind, id) = owner(channel, credential);
    policies::Entity::insert(policies::ActiveModel {
        owner_kind: Set(kind.to_owned()),
        owner_id: Set(id),
        channel_id: Set(channel.get()),
        policy: Set(serde_json::to_value(&settings.policy).map_err(internal)?),
        version: Set(settings.version),
        auto_add_discovered: Set(settings.auto_add_discovered),
        refresh_interval_secs: Set(settings.refresh_interval_secs),
        updated_at: Set(now_seconds()),
    })
    .on_conflict(
        OnConflict::columns([policies::Column::OwnerKind, policies::Column::OwnerId])
            .update_columns([
                policies::Column::Policy,
                policies::Column::Version,
                policies::Column::AutoAddDiscovered,
                policies::Column::RefreshIntervalSecs,
                policies::Column::UpdatedAt,
            ])
            .to_owned(),
    )
    .exec_without_returning(connection)
    .await
    .map_err(internal)?;
    Ok(())
}

fn owner(channel: ChannelId, credential: Option<CredentialId>) -> (&'static str, i64) {
    credential.map_or(("channel", channel.get()), |id| ("credential", id.get()))
}
fn internal(_: impl std::fmt::Display) -> CredentialModelError {
    CredentialModelError::Internal
}
fn now_seconds() -> i64 {
    now_date().unix_timestamp()
}
fn now_date() -> sea_orm::entity::prelude::TimeDateTimeWithTimeZone {
    sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()
}
fn sanitized_error(code: &str) -> &str {
    match code {
        "unsupported" | "timeout" | "upstream_rejected" | "invalid_response" | "unavailable" => {
            code
        }
        _ => "unavailable",
    }
}

#[cfg(test)]
mod tests;
