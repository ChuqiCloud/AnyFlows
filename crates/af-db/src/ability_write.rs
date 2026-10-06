use af_domain::{ChannelId, GroupId, Status};
use sea_orm::{ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter};
use thiserror::Error;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

mod capacity;
mod mutation;
mod read;
mod sync;
mod validation;

pub(super) use mutation::{
    delete_channel_abilities, delete_group_abilities, set_channel_abilities_enabled,
};
pub(super) use read::load_channel_routing_snapshots;
pub(super) use sync::synchronize_channel_routing;
pub(super) use validation::ValidatedChannelRouting;
pub use validation::{
    MAX_ADMIN_CHANNEL_ABILITIES, MAX_ADMIN_CHANNEL_GROUPS, MAX_ADMIN_CHANNEL_MODEL_BYTES,
    MAX_ADMIN_CHANNEL_MODELS,
};

/// 从非敏感关联表读取并完成边界校验的渠道路由配置。
pub(super) struct ChannelRoutingSnapshot {
    models: Vec<String>,
    group_ids: Vec<GroupId>,
}

impl ChannelRoutingSnapshot {
    pub(super) fn into_parts(self) -> (Vec<String>, Vec<GroupId>) {
        (self.models, self.group_ids)
    }
}

/// 渠道能力行中冗余的调度字段。
pub(super) struct ChannelAbilityMetadata<'a> {
    pub(super) enabled: bool,
    pub(super) priority: i32,
    pub(super) weight: i32,
    pub(super) tag: Option<&'a str>,
}

/// 渠道关联与派生能力维护失败；错误不携带模型、分组或数据库诊断。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub(super) enum AbilityWriteError {
    #[error("渠道能力输入无效")]
    InvalidInput,
    #[error("渠道能力引用无效")]
    InvalidReference,
    #[error("渠道能力容量超限")]
    CapacityExceeded,
    #[error("渠道能力数据库写入失败")]
    Query,
    #[error("渠道能力持久化状态损坏")]
    Invariant,
}

/// 在保留现有分组和调度元数据的前提下，把模型追加到渠道路由。
///
/// 模型发现和渠道写入共用此入口，确保关联表、能力表和调度目录在同一事务内保持一致。
pub(crate) async fn append_channel_models(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    additional_models: &[String],
    now: sea_orm::entity::prelude::TimeDateTimeWithTimeZone,
) -> Result<(), AbilityWriteError> {
    if additional_models.is_empty() {
        return Ok(());
    }
    // 先取得能力目录锁，再读取快照，避免并发导入覆盖另一笔刚追加的模型。
    capacity::lock_ability_catalog(transaction, now).await?;
    let snapshot = load_channel_routing_snapshots(transaction, &[channel_id])
        .await?
        .remove(&channel_id)
        .ok_or(AbilityWriteError::Invariant)?;
    let (mut models, group_ids) = snapshot.into_parts();
    models.extend(additional_models.iter().cloned());
    let routing = ValidatedChannelRouting::new(models, group_ids)?;
    let channel = crate::entity::channels::Entity::find()
        .filter(crate::entity::channels::Column::Id.eq(channel_id.get()))
        .filter(crate::entity::channels::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| AbilityWriteError::Query)?
        .ok_or(AbilityWriteError::Invariant)?;
    let enabled = Status::try_from(channel.status)
        .map_err(|_| AbilityWriteError::Invariant)?
        .is_enabled();
    let metadata = ChannelAbilityMetadata {
        enabled,
        priority: channel.priority,
        weight: channel.weight,
        tag: channel.tag.as_deref(),
    };
    synchronize_channel_routing(transaction, channel_id, &routing, metadata, now).await
}
