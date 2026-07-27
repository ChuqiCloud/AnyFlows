use std::{fmt, sync::Arc};

use af_db::SchedulerRuntimeTargetRecord;
use af_domain::GroupId;

use crate::RetrySelection;

/// 从同一代不可变快照固定下来的单个转发候选。
pub struct IndexedRouteCandidate {
    selection: RetrySelection,
    runtime_target: Arc<SchedulerRuntimeTargetRecord>,
}

impl IndexedRouteCandidate {
    pub(super) const fn new(
        selection: RetrySelection,
        runtime_target: Arc<SchedulerRuntimeTargetRecord>,
    ) -> Self {
        Self {
            selection,
            runtime_target,
        }
    }

    /// 返回 weighted 重试计划为该候选分配的尝试顺序与优先级层。
    #[must_use]
    pub const fn selection(&self) -> RetrySelection {
        self.selection
    }

    /// 返回与候选同代发布的运行时目标。
    #[must_use]
    pub fn runtime_target(&self) -> &SchedulerRuntimeTargetRecord {
        self.runtime_target.as_ref()
    }
}

impl fmt::Debug for IndexedRouteCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IndexedRouteCandidate")
            .field("selection", &self.selection)
            .field("runtime_target", &self.runtime_target)
            .finish()
    }
}

/// 单请求在计费前固定的候选顺序与实际计费分组。
///
/// 计划持有同一代快照中的运行时目标，后续索引刷新不会改写在途请求，也不会让候选
/// 与 URL、凭据封套或 Header 来自不同代目录。
pub struct IndexedRoutePlan {
    generation: u64,
    target_group_id: GroupId,
    candidates: Vec<IndexedRouteCandidate>,
}

impl IndexedRoutePlan {
    pub(super) const fn new(
        generation: u64,
        target_group_id: GroupId,
        candidates: Vec<IndexedRouteCandidate>,
    ) -> Self {
        Self {
            generation,
            target_group_id,
            candidates,
        }
    }

    /// 返回计划来源的快照代数。
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// 返回该计划中全部候选共同的实际计费分组。
    #[must_use]
    pub const fn target_group_id(&self) -> GroupId {
        self.target_group_id
    }

    /// 返回已固定且不会重复渠道的故障转移顺序。
    #[must_use]
    pub fn candidates(&self) -> &[IndexedRouteCandidate] {
        &self.candidates
    }
}

impl fmt::Debug for IndexedRoutePlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IndexedRoutePlan")
            .field("generation", &self.generation)
            .field("target_group_id", &self.target_group_id)
            .field("candidate_count", &self.candidates.len())
            .finish()
    }
}
