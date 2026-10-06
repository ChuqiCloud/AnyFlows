use std::{collections::BTreeMap, fmt, num::NonZeroUsize, sync::Arc};

use af_db::SchedulerRuntimeTargetRecord;
use af_domain::{ChannelId, CredentialId, GroupId, RouteChannelId};

use crate::{RetrySelection, RouteWaitPlan, StickyRouteOutcome, StickyWaitPolicy};

/// 从同一代不可变快照固定下来的单个转发候选。
pub struct IndexedRouteCandidate {
    selection: RetrySelection,
    runtime_target: Arc<SchedulerRuntimeTargetRecord>,
    route_channel_ids: BTreeMap<CredentialId, RouteChannelId>,
    wait_plan: RouteWaitPlan,
}

impl IndexedRouteCandidate {
    pub(super) fn new(
        selection: RetrySelection,
        runtime_target: Arc<SchedulerRuntimeTargetRecord>,
    ) -> Self {
        Self {
            selection,
            runtime_target,
            route_channel_ids: BTreeMap::new(),
            wait_plan: StickyWaitPolicy::default()
                .fallback_plan(selection.candidate().channel_id()),
        }
    }

    pub(super) fn new_bound(
        selection: RetrySelection,
        runtime_target: Arc<SchedulerRuntimeTargetRecord>,
        route_channel_ids: BTreeMap<CredentialId, RouteChannelId>,
    ) -> Self {
        Self {
            selection,
            runtime_target,
            route_channel_ids,
            wait_plan: StickyWaitPolicy::default()
                .fallback_plan(selection.candidate().channel_id()),
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

    /// 返回候选所属渠道；它是粘性等待契约的稳定关联键。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.selection.candidate().channel_id()
    }

    /// 返回供并发槽位层消费的等待契约。
    #[must_use]
    pub const fn wait_plan(&self) -> RouteWaitPlan {
        self.wait_plan
    }

    /// 返回当前凭据在管理员智能路由中的候选记录标识。
    #[must_use]
    pub fn route_channel_id(&self, credential_id: CredentialId) -> Option<RouteChannelId> {
        self.route_channel_ids.get(&credential_id).copied()
    }
}

impl fmt::Debug for IndexedRouteCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IndexedRouteCandidate")
            .field("selection", &self.selection)
            .field("runtime_target", &self.runtime_target)
            .field("route_binding_count", &self.route_channel_ids.len())
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

    /// 为当前快照中的所有候选设置普通回退等待边界。
    pub fn configure_wait_policy(&mut self, policy: StickyWaitPolicy) {
        for candidate in &mut self.candidates {
            candidate.wait_plan = policy.fallback_plan(candidate.channel_id());
        }
    }

    /// 将当前快照中的指定渠道移动到首位，并标记粘性/回退等待阶段。
    pub fn prefer_sticky_channel(
        &mut self,
        channel_id: ChannelId,
        policy: StickyWaitPolicy,
    ) -> StickyRouteOutcome {
        self.configure_wait_policy(policy);
        let Some(index) = self
            .candidates
            .iter()
            .position(|candidate| candidate.channel_id() == channel_id)
        else {
            return StickyRouteOutcome::Unavailable;
        };

        let mut sticky = self.candidates.remove(index);
        sticky.wait_plan = policy.sticky_plan(channel_id);
        self.candidates.insert(0, sticky);
        for (index, candidate) in self.candidates.iter_mut().enumerate() {
            let attempt = NonZeroUsize::new(index + 1).expect("路由候选索引从零开始");
            candidate.selection = candidate.selection.with_attempt(attempt);
        }
        StickyRouteOutcome::Applied
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
