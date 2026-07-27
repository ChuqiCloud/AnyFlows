use std::collections::HashMap;

use af_domain::{ChannelId, GroupId};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{
    AbilityWriteError, ChannelRoutingSnapshot,
    validation::{
        MAX_ADMIN_CHANNEL_GROUPS, MAX_ADMIN_CHANNEL_MODELS, relation_page_limit, valid_model,
    },
};
use crate::entity::{channel_groups, channel_models};

/// 批量读取一页渠道的模型和分组集合；空关联也会返回显式空快照。
pub(crate) async fn load_channel_routing_snapshots<C>(
    connection: &C,
    channel_ids: &[ChannelId],
) -> Result<HashMap<ChannelId, ChannelRoutingSnapshot>, AbilityWriteError>
where
    C: ConnectionTrait,
{
    if channel_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let raw_ids = channel_ids
        .iter()
        .map(|channel_id| channel_id.get())
        .collect::<Vec<_>>();
    let model_limit = relation_page_limit(channel_ids.len(), MAX_ADMIN_CHANNEL_MODELS)?;
    let group_limit = relation_page_limit(channel_ids.len(), MAX_ADMIN_CHANNEL_GROUPS)?;
    let models = channel_models::Entity::find()
        .filter(channel_models::Column::ChannelId.is_in(raw_ids.clone()))
        .order_by_asc(channel_models::Column::ChannelId)
        .order_by_asc(channel_models::Column::Model)
        .limit(model_limit)
        .all(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    let groups = channel_groups::Entity::find()
        .filter(channel_groups::Column::ChannelId.is_in(raw_ids))
        .order_by_asc(channel_groups::Column::ChannelId)
        .order_by_asc(channel_groups::Column::GroupId)
        .limit(group_limit)
        .all(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?;
    if u64::try_from(models.len()).map_err(|_| AbilityWriteError::Invariant)? >= model_limit
        || u64::try_from(groups.len()).map_err(|_| AbilityWriteError::Invariant)? >= group_limit
    {
        return Err(AbilityWriteError::Invariant);
    }

    let mut by_channel = channel_ids
        .iter()
        .copied()
        .map(|channel_id| {
            (
                channel_id,
                ChannelRoutingSnapshot {
                    models: Vec::new(),
                    group_ids: Vec::new(),
                },
            )
        })
        .collect::<HashMap<_, _>>();
    for row in models {
        let channel_id =
            ChannelId::new(row.channel_id).map_err(|_| AbilityWriteError::Invariant)?;
        let snapshot = by_channel
            .get_mut(&channel_id)
            .ok_or(AbilityWriteError::Invariant)?;
        if !valid_model(&row.model)
            || snapshot.models.len() >= MAX_ADMIN_CHANNEL_MODELS
            || snapshot.models.last() == Some(&row.model)
        {
            return Err(AbilityWriteError::Invariant);
        }
        snapshot.models.push(row.model);
    }
    for row in groups {
        let channel_id =
            ChannelId::new(row.channel_id).map_err(|_| AbilityWriteError::Invariant)?;
        let group_id = GroupId::new(row.group_id).map_err(|_| AbilityWriteError::Invariant)?;
        let snapshot = by_channel
            .get_mut(&channel_id)
            .ok_or(AbilityWriteError::Invariant)?;
        if snapshot.group_ids.len() >= MAX_ADMIN_CHANNEL_GROUPS
            || snapshot.group_ids.last() == Some(&group_id)
        {
            return Err(AbilityWriteError::Invariant);
        }
        snapshot.group_ids.push(group_id);
    }
    Ok(by_channel)
}
