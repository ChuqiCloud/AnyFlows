use af_domain::{ChannelId, GroupId};
use sea_orm::{
    ColumnTrait, DatabaseTransaction, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, ExprTrait, OnConflict},
};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{AbilityWriteError, validation::ValidatedChannelRouting};
use crate::{
    MAX_SCHEDULER_ABILITY_ENTRIES, MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES,
    entity::{SensitiveString, abilities, groups, options},
};

const ABILITY_CATALOG_LOCK_KEY: &str = "__anyflows_internal_ability_catalog_lock_v1";

/// 在事务内串行化全部能力容量变更，防止并发请求同时通过硬上限检查。
pub(super) async fn lock_ability_catalog(
    transaction: &DatabaseTransaction,
    now: TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError> {
    options::Entity::insert(options::ActiveModel {
        key: Set(ABILITY_CATALOG_LOCK_KEY.to_owned()),
        value: Set(SensitiveString::from("1")),
        created_at: Set(now),
        updated_at: Set(now),
    })
    .on_conflict(
        OnConflict::column(options::Column::Key)
            .do_nothing_on([options::Column::Key])
            .to_owned(),
    )
    .exec_without_returning(transaction)
    .with_subscriber(NoSubscriber::default())
    .await
    .map_err(|_| AbilityWriteError::Query)?;

    // 各方言都会对同一主键行取得写锁；更新时间同时留下最近一次容量校验审计点。
    options::Entity::update_many()
        .filter(options::Column::Key.eq(ABILITY_CATALOG_LOCK_KEY))
        .col_expr(options::Column::UpdatedAt, Expr::value(now))
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|_| ())
        .map_err(|_| AbilityWriteError::Query)
}

/// 确认全部关联分组仍然有效，避免物化悬空能力。
pub(super) async fn ensure_groups_exist(
    transaction: &DatabaseTransaction,
    group_ids: &[GroupId],
) -> Result<(), AbilityWriteError> {
    if group_ids.is_empty() {
        return Ok(());
    }
    let raw_ids = group_ids
        .iter()
        .map(|group_id| group_id.get())
        .collect::<Vec<_>>();
    let rows = groups::Entity::find()
        .select_only()
        .column(groups::Column::Id)
        .filter(groups::Column::Id.is_in(raw_ids))
        .filter(groups::Column::DeletedAt.is_null())
        .into_tuple::<i64>()
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    if rows.len() == group_ids.len() {
        Ok(())
    } else {
        Err(AbilityWriteError::InvalidReference)
    }
}

/// 校验更新后的全目录容量和每个调度键的候选渠道上限。
pub(super) async fn ensure_projected_capacity(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    routing: &ValidatedChannelRouting,
    current_channel_count: usize,
    desired_channel_count: usize,
) -> Result<(), AbilityWriteError> {
    let total = abilities::Entity::find()
        .count(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    let current_channel_count =
        u64::try_from(current_channel_count).map_err(|_| AbilityWriteError::Invariant)?;
    let desired_channel_count =
        u64::try_from(desired_channel_count).map_err(|_| AbilityWriteError::Invariant)?;
    let maximum = u64::try_from(MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES)
        .map_err(|_| AbilityWriteError::Invariant)?;
    if total > maximum {
        return Err(AbilityWriteError::Invariant);
    }
    if total
        .checked_sub(current_channel_count)
        .and_then(|count| count.checked_add(desired_channel_count))
        .is_none_or(|count| count > maximum)
    {
        return Err(AbilityWriteError::CapacityExceeded);
    }
    if routing.models().is_empty() || routing.group_ids().is_empty() {
        return Ok(());
    }

    let group_ids = routing
        .group_ids()
        .iter()
        .map(|group_id| group_id.get())
        .collect::<Vec<_>>();
    let model_names = routing.models().to_vec();
    let maximum_candidates =
        i64::try_from(MAX_SCHEDULER_ABILITY_ENTRIES).map_err(|_| AbilityWriteError::Invariant)?;
    let overflow = abilities::Entity::find()
        .select_only()
        .column(abilities::Column::GroupId)
        .column(abilities::Column::Model)
        .column_as(abilities::Column::ChannelId.count(), "candidate_count")
        .filter(abilities::Column::ChannelId.ne(channel_id.get()))
        .filter(abilities::Column::GroupId.is_in(group_ids))
        .filter(abilities::Column::Model.is_in(model_names))
        .group_by(abilities::Column::GroupId)
        .group_by(abilities::Column::Model)
        .having(abilities::Column::ChannelId.count().gte(maximum_candidates))
        .limit(1)
        .into_tuple::<(i64, String, i64)>()
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    if overflow.is_empty() {
        Ok(())
    } else {
        Err(AbilityWriteError::CapacityExceeded)
    }
}
