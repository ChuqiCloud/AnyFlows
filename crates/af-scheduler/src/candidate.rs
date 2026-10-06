use std::fmt;

use af_domain::{ChannelId, GroupId};

/// 已通过持久化边界校验、可交给调度策略的渠道候选。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct SchedulerCandidate {
    group_id: GroupId,
    channel_id: ChannelId,
    priority: i32,
    weight: u32,
}

impl SchedulerCandidate {
    /// 使用强类型标识和非负权重构造候选。
    #[must_use]
    pub const fn new(group_id: GroupId, channel_id: ChannelId, priority: i32, weight: u32) -> Self {
        Self {
            group_id,
            channel_id,
            priority,
            weight,
        }
    }

    /// 返回该候选实际计费与可见性所属分组。
    #[must_use]
    pub const fn group_id(self) -> GroupId {
        self.group_id
    }

    /// 返回候选渠道标识。
    #[must_use]
    pub const fn channel_id(self) -> ChannelId {
        self.channel_id
    }

    /// 返回优先级；数值越大越先参与选择。
    #[must_use]
    pub const fn priority(self) -> i32 {
        self.priority
    }

    /// 返回管理员配置的非负权重，不包含基础概率。
    #[must_use]
    pub const fn weight(self) -> u32 {
        self.weight
    }
}

impl fmt::Debug for SchedulerCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerCandidate")
            .field("group_id", &self.group_id)
            .field("channel_id", &self.channel_id)
            .field("priority", &self.priority)
            .field("weight", &self.weight)
            .finish()
    }
}
