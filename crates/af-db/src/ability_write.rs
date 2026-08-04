use af_domain::GroupId;
use thiserror::Error;

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
