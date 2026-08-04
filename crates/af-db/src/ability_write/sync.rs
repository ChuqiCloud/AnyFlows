use std::collections::HashSet;

use af_domain::{ChannelId, GroupId};
use sea_orm::{
    ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{
    AbilityWriteError, ChannelAbilityMetadata,
    capacity::{ensure_groups_exist, ensure_projected_capacity, lock_ability_catalog},
    mutation::{
        delete_extra_abilities, delete_extra_groups, delete_extra_models, insert_missing_abilities,
        insert_missing_groups, insert_missing_models, update_channel_ability_metadata,
    },
    validation::{
        MAX_ADMIN_CHANNEL_ABILITIES, MAX_ADMIN_CHANNEL_GROUPS, MAX_ADMIN_CHANNEL_MODELS,
        ValidatedChannelRouting, query_limit, valid_model,
    },
};
use crate::{
    SchedulerCatalogSubject,
    entity::{abilities, channel_groups, channel_models},
    scheduler_outbox::enqueue_scheduler_catalog_change,
};

/// 在现有渠道事务内差量维护关联表和能力笛卡尔积。
pub(crate) async fn synchronize_channel_routing(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    routing: &ValidatedChannelRouting,
    metadata: ChannelAbilityMetadata<'_>,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError> {
    lock_ability_catalog(transaction, now).await?;
    ensure_groups_exist(transaction, routing.group_ids()).await?;

    let current_models = load_channel_models(transaction, channel_id).await?;
    let current_groups = load_channel_groups(transaction, channel_id).await?;
    let current_abilities = load_channel_ability_keys(transaction, channel_id).await?;
    let desired_abilities = desired_ability_keys(routing)?;
    ensure_projected_capacity(
        transaction,
        channel_id,
        routing,
        current_abilities.len(),
        desired_abilities.len(),
    )
    .await?;

    delete_extra_abilities(transaction, channel_id, routing).await?;
    delete_extra_models(transaction, channel_id, routing.models()).await?;
    delete_extra_groups(transaction, channel_id, routing.group_ids()).await?;

    insert_missing_models(
        transaction,
        channel_id,
        routing.models(),
        &current_models,
        now,
    )
    .await?;
    insert_missing_groups(
        transaction,
        channel_id,
        routing.group_ids(),
        &current_groups,
        now,
    )
    .await?;
    insert_missing_abilities(
        transaction,
        channel_id,
        &desired_abilities,
        &current_abilities,
        &metadata,
        now,
    )
    .await?;
    update_channel_ability_metadata(transaction, channel_id, metadata, now).await?;
    enqueue_scheduler_catalog_change(
        transaction,
        SchedulerCatalogSubject::Channel(channel_id),
        now,
    )
    .await
    .map_err(|_| AbilityWriteError::Query)
}

async fn load_channel_models(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
) -> Result<HashSet<String>, AbilityWriteError> {
    let rows = channel_models::Entity::find()
        .filter(channel_models::Column::ChannelId.eq(channel_id.get()))
        .order_by_asc(channel_models::Column::Model)
        .limit(query_limit(MAX_ADMIN_CHANNEL_MODELS)?)
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    if rows.len() > MAX_ADMIN_CHANNEL_MODELS || rows.iter().any(|row| !valid_model(&row.model)) {
        return Err(AbilityWriteError::Invariant);
    }
    Ok(rows.into_iter().map(|row| row.model).collect())
}

async fn load_channel_groups(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
) -> Result<HashSet<GroupId>, AbilityWriteError> {
    let rows = channel_groups::Entity::find()
        .filter(channel_groups::Column::ChannelId.eq(channel_id.get()))
        .order_by_asc(channel_groups::Column::GroupId)
        .limit(query_limit(MAX_ADMIN_CHANNEL_GROUPS)?)
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    if rows.len() > MAX_ADMIN_CHANNEL_GROUPS {
        return Err(AbilityWriteError::Invariant);
    }
    rows.into_iter()
        .map(|row| GroupId::new(row.group_id).map_err(|_| AbilityWriteError::Invariant))
        .collect()
}

async fn load_channel_ability_keys(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
) -> Result<HashSet<(GroupId, String)>, AbilityWriteError> {
    let rows = abilities::Entity::find()
        .filter(abilities::Column::ChannelId.eq(channel_id.get()))
        .order_by_asc(abilities::Column::GroupId)
        .order_by_asc(abilities::Column::Model)
        .limit(query_limit(MAX_ADMIN_CHANNEL_ABILITIES)?)
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    if rows.len() > MAX_ADMIN_CHANNEL_ABILITIES {
        return Err(AbilityWriteError::Invariant);
    }
    rows.into_iter()
        .map(|row| {
            if !valid_model(&row.model) || row.weight < 0 {
                return Err(AbilityWriteError::Invariant);
            }
            Ok((
                GroupId::new(row.group_id).map_err(|_| AbilityWriteError::Invariant)?,
                row.model,
            ))
        })
        .collect()
}

fn desired_ability_keys(
    routing: &ValidatedChannelRouting,
) -> Result<Vec<(GroupId, String)>, AbilityWriteError> {
    let capacity = routing
        .models()
        .len()
        .checked_mul(routing.group_ids().len())
        .filter(|capacity| *capacity <= MAX_ADMIN_CHANNEL_ABILITIES)
        .ok_or(AbilityWriteError::Invariant)?;
    let mut keys = Vec::with_capacity(capacity);
    for group_id in routing.group_ids() {
        for model in routing.models() {
            keys.push((*group_id, model.clone()));
        }
    }
    Ok(keys)
}
