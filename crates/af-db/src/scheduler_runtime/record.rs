use std::{fmt, sync::Arc};

use crate::SchedulerAbilityRecord;

use super::SchedulerRuntimeTargetRecord;

/// 已完成能力与运行时目标配对的生产候选记录。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerRuntimeRecord {
    ability: SchedulerAbilityRecord,
    target: Arc<SchedulerRuntimeTargetRecord>,
}

impl SchedulerRuntimeRecord {
    /// 在仓储层配对已验证能力和共享运行时目标。
    #[must_use]
    pub(super) fn new(
        ability: SchedulerAbilityRecord,
        target: Arc<SchedulerRuntimeTargetRecord>,
    ) -> Self {
        Self { ability, target }
    }

    /// 返回调度能力元数据。
    #[must_use]
    pub const fn ability(&self) -> &SchedulerAbilityRecord {
        &self.ability
    }

    /// 返回该能力对应渠道的运行时目标。
    #[must_use]
    pub fn target(&self) -> &SchedulerRuntimeTargetRecord {
        self.target.as_ref()
    }

    /// 消费记录并返回能力与共享运行时目标。
    #[must_use]
    pub fn into_parts(self) -> (SchedulerAbilityRecord, Arc<SchedulerRuntimeTargetRecord>) {
        (self.ability, self.target)
    }
}

impl fmt::Debug for SchedulerRuntimeRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeRecord")
            .field("ability", &self.ability)
            .field("target", &self.target)
            .finish()
    }
}
