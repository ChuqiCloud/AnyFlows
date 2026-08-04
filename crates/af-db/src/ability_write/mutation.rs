use std::collections::HashSet;

use af_domain::{ChannelId, GroupId};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Condition, Expr},
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{AbilityWriteError, ChannelAbilityMetadata, validation::ValidatedChannelRouting};
use crate::{
    SchedulerCatalogSubject,
    entity::{abilities, channel_groups, channel_models},
    scheduler_outbox::enqueue_scheduler_catalog_change,
};

const INSERT_BATCH_SIZE: usize = 100;

/// 仅修改指定渠道能力的启用状态，供自动禁用与探活恢复事务复用。
pub(crate) async fn set_channel_abilities_enabled<C>(
    connection: &C,
    channel_id: ChannelId,
    enabled: bool,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError>
where
    C: ConnectionTrait,
{
    let result = abilities::Entity::update_many()
        .filter(abilities::Column::ChannelId.eq(channel_id.get()))
        .filter(abilities::Column::Enabled.ne(enabled))
        .col_expr(abilities::Column::Enabled, Expr::value(enabled))
        .col_expr(abilities::Column::UpdatedAt, Expr::value(now))
        .exec(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    if result.rows_affected > 0 {
        enqueue_scheduler_catalog_change(
            connection,
            SchedulerCatalogSubject::Channel(channel_id),
            now,
        )
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    }
    Ok(())
}

/// 删除指定渠道的全部派生能力，并在同一事务内追加渠道失效事件。
pub(crate) async fn delete_channel_abilities<C>(
    connection: &C,
    channel_id: ChannelId,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError>
where
    C: ConnectionTrait,
{
    abilities::Entity::delete_many()
        .filter(abilities::Column::ChannelId.eq(channel_id.get()))
        .exec(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    // 删除事件同时承担失效职责；即使 DB 已无能力行，也要清理可能滞后的共享投影。
    enqueue_scheduler_catalog_change(
        connection,
        SchedulerCatalogSubject::Channel(channel_id),
        now,
    )
    .await
    .map_err(|_| AbilityWriteError::Query)?;
    Ok(())
}

/// 显式删除分组对应的派生能力，避免依赖不同方言的级联时序。
pub(crate) async fn delete_group_abilities<C>(
    connection: &C,
    group_id: GroupId,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError>
where
    C: ConnectionTrait,
{
    abilities::Entity::delete_many()
        .filter(abilities::Column::GroupId.eq(group_id.get()))
        .exec(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    // 分组墓碑必须广播，即使派生能力已提前清空，也不能把旧分组键留在共享投影中。
    enqueue_scheduler_catalog_change(connection, SchedulerCatalogSubject::Group(group_id), now)
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    Ok(())
}

pub(super) async fn delete_extra_abilities(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    routing: &ValidatedChannelRouting,
) -> Result<(), AbilityWriteError> {
    let mut delete =
        abilities::Entity::delete_many().filter(abilities::Column::ChannelId.eq(channel_id.get()));
    if routing.models().is_empty() || routing.group_ids().is_empty() {
        return delete
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map(|_| ())
            .map_err(|_| AbilityWriteError::Query);
    }
    let model_names = routing.models().to_vec();
    let group_ids = routing
        .group_ids()
        .iter()
        .map(|group_id| group_id.get())
        .collect::<Vec<_>>();
    delete = delete.filter(
        Condition::any()
            .add(abilities::Column::Model.is_not_in(model_names))
            .add(abilities::Column::GroupId.is_not_in(group_ids)),
    );
    delete
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|_| ())
        .map_err(|_| AbilityWriteError::Query)
}

pub(super) async fn delete_extra_models(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    models: &[String],
) -> Result<(), AbilityWriteError> {
    let mut delete = channel_models::Entity::delete_many()
        .filter(channel_models::Column::ChannelId.eq(channel_id.get()));
    if !models.is_empty() {
        delete = delete.filter(channel_models::Column::Model.is_not_in(models.to_vec()));
    }
    delete
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|_| ())
        .map_err(|_| AbilityWriteError::Query)
}

pub(super) async fn delete_extra_groups(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    group_ids: &[GroupId],
) -> Result<(), AbilityWriteError> {
    let mut delete = channel_groups::Entity::delete_many()
        .filter(channel_groups::Column::ChannelId.eq(channel_id.get()));
    if !group_ids.is_empty() {
        delete = delete.filter(
            channel_groups::Column::GroupId.is_not_in(
                group_ids
                    .iter()
                    .map(|group_id| group_id.get())
                    .collect::<Vec<_>>(),
            ),
        );
    }
    delete
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|_| ())
        .map_err(|_| AbilityWriteError::Query)
}

pub(super) async fn insert_missing_models(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    models: &[String],
    current: &HashSet<String>,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError> {
    let rows = models
        .iter()
        .filter(|model| !current.contains(*model))
        .map(|model| channel_models::ActiveModel {
            channel_id: Set(channel_id.get()),
            model: Set(model.clone()),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .collect::<Vec<_>>();
    insert_model_batches(transaction, rows).await
}

pub(super) async fn insert_missing_groups(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    group_ids: &[GroupId],
    current: &HashSet<GroupId>,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError> {
    let rows = group_ids
        .iter()
        .filter(|group_id| !current.contains(group_id))
        .map(|group_id| channel_groups::ActiveModel {
            channel_id: Set(channel_id.get()),
            group_id: Set(group_id.get()),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .collect::<Vec<_>>();
    insert_group_batches(transaction, rows).await
}

pub(super) async fn insert_missing_abilities(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    desired: &[(GroupId, String)],
    current: &HashSet<(GroupId, String)>,
    metadata: &ChannelAbilityMetadata<'_>,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError> {
    let rows = desired
        .iter()
        .filter(|key| !current.contains(*key))
        .map(|(group_id, model)| abilities::ActiveModel {
            group_id: Set(group_id.get()),
            model: Set(model.clone()),
            channel_id: Set(channel_id.get()),
            enabled: Set(metadata.enabled),
            priority: Set(metadata.priority),
            weight: Set(metadata.weight),
            tag: Set(metadata.tag.map(str::to_owned)),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .collect::<Vec<_>>();
    insert_ability_batches(transaction, rows).await
}

pub(super) async fn update_channel_ability_metadata(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    metadata: ChannelAbilityMetadata<'_>,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError> {
    let tag_changed = match metadata.tag {
        Some(tag) => Condition::any()
            .add(abilities::Column::Tag.is_null())
            .add(abilities::Column::Tag.ne(tag)),
        None => Condition::all().add(abilities::Column::Tag.is_not_null()),
    };
    abilities::Entity::update_many()
        .filter(abilities::Column::ChannelId.eq(channel_id.get()))
        .filter(
            Condition::any()
                .add(abilities::Column::Enabled.ne(metadata.enabled))
                .add(abilities::Column::Priority.ne(metadata.priority))
                .add(abilities::Column::Weight.ne(metadata.weight))
                .add(tag_changed),
        )
        .col_expr(abilities::Column::Enabled, Expr::value(metadata.enabled))
        .col_expr(abilities::Column::Priority, Expr::value(metadata.priority))
        .col_expr(abilities::Column::Weight, Expr::value(metadata.weight))
        .col_expr(
            abilities::Column::Tag,
            Expr::value(metadata.tag.map(str::to_owned)),
        )
        .col_expr(abilities::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|_| ())
        .map_err(|_| AbilityWriteError::Query)
}

async fn insert_model_batches(
    transaction: &DatabaseTransaction,
    rows: Vec<channel_models::ActiveModel>,
) -> Result<(), AbilityWriteError> {
    for chunk in rows.chunks(INSERT_BATCH_SIZE) {
        channel_models::Entity::insert_many(chunk.iter().cloned())
            .exec_without_returning(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AbilityWriteError::Query)?;
    }
    Ok(())
}

async fn insert_group_batches(
    transaction: &DatabaseTransaction,
    rows: Vec<channel_groups::ActiveModel>,
) -> Result<(), AbilityWriteError> {
    for chunk in rows.chunks(INSERT_BATCH_SIZE) {
        channel_groups::Entity::insert_many(chunk.iter().cloned())
            .exec_without_returning(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AbilityWriteError::Query)?;
    }
    Ok(())
}

async fn insert_ability_batches(
    transaction: &DatabaseTransaction,
    rows: Vec<abilities::ActiveModel>,
) -> Result<(), AbilityWriteError> {
    for chunk in rows.chunks(INSERT_BATCH_SIZE) {
        abilities::Entity::insert_many(chunk.iter().cloned())
            .exec_without_returning(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| AbilityWriteError::Query)?;
    }
    Ok(())
}
