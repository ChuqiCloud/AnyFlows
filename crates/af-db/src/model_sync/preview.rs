use std::collections::{BTreeMap, BTreeSet};

use af_domain::{ChannelType, ModelId, Protocol};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::entity::{channel_models, channels, model_sync_items, model_sync_runs, models, users};

use super::{
    DiscoveredModelRecord, MAX_MODEL_SYNC_CANDIDATES, ModelSyncItemRecord, ModelSyncPreviewRecord,
    ModelSyncPreviewWrite, ModelSyncRelationRecord, ModelSyncRepository, ModelSyncRepositoryError,
    record_internal_error, valid_methods, valid_model,
};

/// 固定同步预览有效期为十五分钟。
pub const MODEL_SYNC_PREVIEW_TTL_SECONDS: i64 = 15 * 60;
const INSERT_CHUNK_SIZE: usize = 100;

impl ModelSyncRepository {
    /// 保存脱敏来源证据，并按当前渠道配置与活动元数据生成固定关系快照。
    pub async fn create_preview(
        &self,
        write: ModelSyncPreviewWrite,
    ) -> Result<ModelSyncPreviewRecord, ModelSyncRepositoryError> {
        match timeout(self.operation_timeout, self.create_preview_inner(write)).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ModelSyncRepositoryError::Timeout)),
        }
    }

    async fn create_preview_inner(
        &self,
        write: ModelSyncPreviewWrite,
    ) -> Result<ModelSyncPreviewRecord, ModelSyncRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        let created_at = TimeDateTimeWithTimeZone::now_utc();
        let remaining_ttl = write
            .expires_at
            .checked_sub(created_at.unix_timestamp())
            .ok_or(ModelSyncRepositoryError::Invariant)?;
        if !(1..=MODEL_SYNC_PREVIEW_TTL_SECONDS).contains(&remaining_ttl) {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        validate_actor_and_channel(&transaction, &write).await?;
        let configured_models =
            load_configured_models(&transaction, write.channel_id.get()).await?;
        let discovered = collect_discovered(write.discovered_models)?;
        let mut all_models = configured_models.clone();
        all_models.extend(discovered.keys().cloned());
        if all_models.len() > MAX_MODEL_SYNC_CANDIDATES {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        let existing_models = load_existing_models(&transaction, &all_models).await?;
        let item_inputs = classify_items(configured_models, discovered, &existing_models);
        let candidate_count =
            i32::try_from(item_inputs.len()).map_err(|_| ModelSyncRepositoryError::Invariant)?;
        let expires_at = TimeDateTimeWithTimeZone::from_unix_timestamp(write.expires_at)
            .map_err(|_| ModelSyncRepositoryError::Invariant)?;
        let run = model_sync_runs::ActiveModel {
            preview_id: Set(write.preview_id.clone()),
            channel_id: Set(write.channel_id.get()),
            actor_user_id: Set(write.actor_user_id.get()),
            channel_type: Set(write.channel_type.as_str().to_owned()),
            protocol: Set(write.protocol.as_str().to_owned()),
            state: Set(1),
            candidate_count: Set(candidate_count),
            expires_at: Set(expires_at),
            applied_at: Set(None),
            created_at: Set(created_at),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_preview_write_error)?;

        let active_items = item_inputs
            .into_iter()
            .enumerate()
            .map(|(ordinal, input)| {
                Ok(model_sync_items::ActiveModel {
                    run_id: Set(run.id),
                    ordinal: Set(
                        i32::try_from(ordinal).map_err(|_| ModelSyncRepositoryError::Invariant)?
                    ),
                    canonical_model: Set(input.canonical_model),
                    upstream_model: Set(input.upstream_model),
                    relation: Set(input.relation.database_value()),
                    display_name_hint: Set(input.display_name_hint),
                    description_hint: Set(input.description_hint),
                    context_window_hint: Set(input.context_window_hint),
                    input_token_limit_hint: Set(input.input_token_limit_hint),
                    output_token_limit_hint: Set(input.output_token_limit_hint),
                    supported_methods: Set(serde_json::to_value(input.supported_methods)
                        .map_err(|_| ModelSyncRepositoryError::Invariant)?),
                    applied_model_id: Set(None),
                    ..Default::default()
                })
            })
            .collect::<Result<Vec<_>, ModelSyncRepositoryError>>()?;
        for chunk in active_items.chunks(INSERT_CHUNK_SIZE) {
            model_sync_items::Entity::insert_many(chunk.iter().cloned())
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_preview_write_error)?;
        }
        let item_models = model_sync_items::Entity::find()
            .filter(model_sync_items::Column::RunId.eq(run.id))
            .order_by_asc(model_sync_items::Column::Ordinal)
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        if item_models.len() != active_items.len() {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        let preview = preview_from_models(run, item_models)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        Ok(preview)
    }
}

struct ClassifiedItem {
    canonical_model: String,
    upstream_model: Option<String>,
    relation: ModelSyncRelationRecord,
    display_name_hint: Option<String>,
    description_hint: Option<String>,
    context_window_hint: Option<i64>,
    input_token_limit_hint: Option<i64>,
    output_token_limit_hint: Option<i64>,
    supported_methods: Vec<String>,
}

async fn validate_actor_and_channel(
    transaction: &DatabaseTransaction,
    write: &ModelSyncPreviewWrite,
) -> Result<(), ModelSyncRepositoryError> {
    let actor = users::Entity::find_by_id(write.actor_user_id.get())
        .filter(users::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?;
    if actor.is_none_or(|actor| actor.role != 1) {
        return Err(ModelSyncRepositoryError::NotFound);
    }
    let channel = channels::Entity::find_by_id(write.channel_id.get())
        .filter(channels::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?
        .ok_or(ModelSyncRepositoryError::NotFound)?;
    let channel_type = channel
        .r#type
        .parse::<ChannelType>()
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    let protocol = channel
        .protocol
        .parse::<Protocol>()
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    if channel_type != write.channel_type || protocol != write.protocol {
        return Err(ModelSyncRepositoryError::Conflict);
    }
    Ok(())
}

async fn load_configured_models(
    transaction: &DatabaseTransaction,
    channel_id: i64,
) -> Result<BTreeSet<String>, ModelSyncRepositoryError> {
    let rows = channel_models::Entity::find()
        .filter(channel_models::Column::ChannelId.eq(channel_id))
        .order_by_asc(channel_models::Column::Model)
        .limit(
            u64::try_from(MAX_MODEL_SYNC_CANDIDATES + 1)
                .map_err(|_| ModelSyncRepositoryError::Invariant)?,
        )
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?;
    if rows.len() > MAX_MODEL_SYNC_CANDIDATES || rows.iter().any(|row| !valid_model(&row.model)) {
        return Err(ModelSyncRepositoryError::Invariant);
    }
    Ok(rows.into_iter().map(|row| row.model).collect())
}

fn collect_discovered(
    discovered: Vec<DiscoveredModelRecord>,
) -> Result<BTreeMap<String, DiscoveredModelRecord>, ModelSyncRepositoryError> {
    let mut indexed = BTreeMap::new();
    for record in discovered {
        if !record.is_valid()
            || indexed
                .insert(record.canonical_model.clone(), record)
                .is_some()
        {
            return Err(ModelSyncRepositoryError::Invariant);
        }
    }
    Ok(indexed)
}

async fn load_existing_models(
    transaction: &DatabaseTransaction,
    names: &BTreeSet<String>,
) -> Result<BTreeSet<String>, ModelSyncRepositoryError> {
    if names.is_empty() {
        return Ok(BTreeSet::new());
    }
    let rows = models::Entity::find()
        .filter(models::Column::Model.is_in(names.iter().cloned()))
        .filter(models::Column::DeletedAt.is_null())
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?;
    if rows.iter().any(|row| !valid_model(&row.model)) {
        return Err(ModelSyncRepositoryError::Invariant);
    }
    Ok(rows.into_iter().map(|row| row.model).collect())
}

fn classify_items(
    configured: BTreeSet<String>,
    mut discovered: BTreeMap<String, DiscoveredModelRecord>,
    existing: &BTreeSet<String>,
) -> Vec<ClassifiedItem> {
    let mut names = configured.clone();
    names.extend(discovered.keys().cloned());
    names
        .into_iter()
        .map(|canonical_model| {
            let evidence = discovered.remove(&canonical_model);
            let relation = match (
                evidence.is_some(),
                configured.contains(&canonical_model),
                existing.contains(&canonical_model),
            ) {
                (false, true, _) => ModelSyncRelationRecord::NotReported,
                (true, _, true) => ModelSyncRelationRecord::Existing,
                (true, true, false) => ModelSyncRelationRecord::MissingMetadata,
                (true, false, false) => ModelSyncRelationRecord::DiscoveredUnconfigured,
                (false, false, _) => unreachable!("候选名称来自配置和发现集合的并集"),
            };
            let Some(evidence) = evidence else {
                return ClassifiedItem {
                    canonical_model,
                    upstream_model: None,
                    relation,
                    display_name_hint: None,
                    description_hint: None,
                    context_window_hint: None,
                    input_token_limit_hint: None,
                    output_token_limit_hint: None,
                    supported_methods: Vec::new(),
                };
            };
            ClassifiedItem {
                canonical_model,
                upstream_model: Some(evidence.upstream_model),
                relation,
                display_name_hint: evidence.display_name_hint,
                description_hint: evidence.description_hint,
                context_window_hint: evidence.context_window_hint,
                input_token_limit_hint: evidence.input_token_limit_hint,
                output_token_limit_hint: evidence.output_token_limit_hint,
                supported_methods: evidence.supported_methods,
            }
        })
        .collect()
}

pub(super) fn preview_from_models(
    run: model_sync_runs::Model,
    items: Vec<model_sync_items::Model>,
) -> Result<ModelSyncPreviewRecord, ModelSyncRepositoryError> {
    let channel_id = af_domain::ChannelId::new(run.channel_id)
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    let channel_type = run
        .channel_type
        .parse()
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    let protocol = run
        .protocol
        .parse()
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    if !super::valid_preview_id(&run.preview_id)
        || run.candidate_count < 0
        || usize::try_from(run.candidate_count).ok() != Some(items.len())
    {
        return Err(ModelSyncRepositoryError::Invariant);
    }
    let items = items
        .into_iter()
        .map(item_from_model)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ModelSyncPreviewRecord {
        preview_id: run.preview_id,
        channel_id,
        channel_type,
        protocol,
        expires_at: run.expires_at.unix_timestamp(),
        items,
    })
}

fn item_from_model(
    item: model_sync_items::Model,
) -> Result<ModelSyncItemRecord, ModelSyncRepositoryError> {
    let supported_methods = serde_json::from_value::<Vec<String>>(item.supported_methods)
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    let relation = ModelSyncRelationRecord::try_from_database(item.relation)?;
    let applied_model_id = item
        .applied_model_id
        .map(|id| ModelId::new(id).map_err(|_| ModelSyncRepositoryError::Invariant))
        .transpose()?;
    if item.id <= 0
        || !valid_model(&item.canonical_model)
        || item
            .upstream_model
            .as_deref()
            .is_some_and(|value| !valid_model(value))
        || !valid_methods(&supported_methods)
        || (applied_model_id.is_some() && !relation.is_applicable())
    {
        return Err(ModelSyncRepositoryError::Invariant);
    }
    Ok(ModelSyncItemRecord {
        item_id: item.id,
        canonical_model: item.canonical_model,
        upstream_model: item.upstream_model,
        relation,
        display_name_hint: item.display_name_hint,
        description_hint: item.description_hint,
        context_window_hint: item.context_window_hint,
        input_token_limit_hint: item.input_token_limit_hint,
        output_token_limit_hint: item.output_token_limit_hint,
        supported_methods,
        applied_model_id,
    })
}

fn map_preview_write_error(error: sea_orm::DbErr) -> ModelSyncRepositoryError {
    let rendered = error.to_string();
    if rendered.contains("uq_model_sync_runs_preview_id")
        || rendered.contains("model_sync_runs.preview_id")
    {
        ModelSyncRepositoryError::Conflict
    } else {
        ModelSyncRepositoryError::Query
    }
}
