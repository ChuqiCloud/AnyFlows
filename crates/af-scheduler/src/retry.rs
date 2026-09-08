use std::{collections::BTreeSet, fmt, num::NonZeroUsize};

use af_domain::ChannelId;
use thiserror::Error;

use crate::{
    ChannelRoutingHealth, MAX_SCHEDULER_CANDIDATES, SchedulerCandidate, WeightedSelectionError,
    WeightedStrategy, health::healthy_channel,
};

/// 失败重试计划返回的一次候选选择。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrySelection {
    candidate: SchedulerCandidate,
    attempt: NonZeroUsize,
    priority_rank: usize,
}

impl RetrySelection {
    /// 从已经完成策略排序的候选构造固定尝试位置。
    pub(crate) fn from_ordered_candidate(
        candidate: SchedulerCandidate,
        attempt: NonZeroUsize,
        priority_rank: usize,
    ) -> Self {
        Self {
            candidate,
            attempt,
            priority_rank,
        }
    }

    /// 返回本次应尝试的候选渠道。
    #[must_use]
    pub const fn candidate(self) -> SchedulerCandidate {
        self.candidate
    }

    /// 返回从一开始计数的尝试次数。
    #[must_use]
    pub const fn attempt(self) -> NonZeroUsize {
        self.attempt
    }

    /// 返回本次使用的去重优先级层序号，零表示最高层。
    #[must_use]
    pub const fn priority_rank(self) -> usize {
        self.priority_rank
    }

    /// 按路由重排后的实际候选位置更新尝试序号；优先级层仍保留 weighted 基线。
    pub(crate) const fn with_attempt(self, attempt: NonZeroUsize) -> Self {
        Self { attempt, ..self }
    }
}

/// 构造或推进 weighted 失败重试计划时的安全错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum WeightedRetryPlanError {
    /// 候选数量超过单次 relay 可接受的硬上限。
    #[error("调度重试候选数量超过上限")]
    TooManyCandidates,
    /// 同一重试计划混入了不同分组的候选。
    #[error("调度重试候选分组不一致")]
    MixedGroups,
    /// 同一渠道在候选集合中重复出现。
    #[error("调度重试候选渠道重复")]
    DuplicateChannel,
    /// weighted 选择无法安全完成。
    #[error("调度重试选择失败")]
    Selection(#[from] WeightedSelectionError),
    /// 内部游标、排除集合或候选选择违反状态机不变量。
    #[error("调度重试计划状态损坏")]
    Invariant,
}

/// 单请求内的 weighted 失败重试游标。
///
/// 首次调用 [`Self::select_next`] 返回最高优先级层候选；候选会在返回前加入排除集合，
/// 调用方只有在该次尝试失败后才应再次调用并降到下一优先级层。到达最低层后继续在
/// 最低层重选，直到该层耗尽；计划绝不会回到更高层，也不会重复返回同一渠道。
pub struct WeightedRetryPlan {
    candidates: Vec<SchedulerCandidate>,
    priorities: Vec<i32>,
    excluded_channels: BTreeSet<ChannelId>,
    attempts_started: usize,
    strategy: WeightedStrategy,
    health: std::collections::BTreeMap<ChannelId, ChannelRoutingHealth>,
}

impl WeightedRetryPlan {
    /// 校验候选容量、分组一致性和渠道唯一性后创建重试计划。
    pub fn new(candidates: Vec<SchedulerCandidate>) -> Result<Self, WeightedRetryPlanError> {
        Self::new_with_health(candidates, &std::collections::BTreeMap::new())
    }

    /// 创建应用共享健康惩罚的重试计划；冷却渠道在优先级分层前移除。
    pub fn new_with_health(
        mut candidates: Vec<SchedulerCandidate>,
        health: &std::collections::BTreeMap<ChannelId, ChannelRoutingHealth>,
    ) -> Result<Self, WeightedRetryPlanError> {
        if candidates.len() > MAX_SCHEDULER_CANDIDATES {
            return Err(WeightedRetryPlanError::TooManyCandidates);
        }
        candidates.retain(|candidate| healthy_channel(candidate.channel_id(), health));
        validate_single_group(&candidates)?;
        validate_unique_channels(&candidates)?;
        let bounded_health = candidates
            .iter()
            .filter_map(|candidate| {
                health
                    .get(&candidate.channel_id())
                    .copied()
                    .map(|state| (candidate.channel_id(), state))
            })
            .collect();

        let mut priorities = candidates
            .iter()
            .map(|candidate| candidate.priority())
            .collect::<Vec<_>>();
        priorities.sort_unstable_by(|left, right| right.cmp(left));
        priorities.dedup();

        Ok(Self {
            candidates,
            priorities,
            excluded_channels: BTreeSet::new(),
            attempts_started: 0,
            strategy: WeightedStrategy,
            health: bounded_health,
        })
    }

    /// 返回并排除下一次候选；后续再次调用即表示上一次候选已经失败。
    pub fn select_next(&mut self) -> Result<Option<RetrySelection>, WeightedRetryPlanError> {
        let Some(last_priority_rank) = self.priorities.len().checked_sub(1) else {
            return Ok(None);
        };
        let priority_rank = self.attempts_started.min(last_priority_rank);
        let priority = self.priorities[priority_rank];
        let eligible = self
            .candidates
            .iter()
            .copied()
            .filter(|candidate| {
                candidate.priority() == priority
                    && !self.excluded_channels.contains(&candidate.channel_id())
            })
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            return Ok(None);
        }

        let next_attempt = self
            .attempts_started
            .checked_add(1)
            .ok_or(WeightedRetryPlanError::Invariant)?;
        let selected = self
            .strategy
            .select_with_health(&eligible, &self.health)?
            .ok_or(WeightedRetryPlanError::Invariant)?
            .candidate();
        if !self.excluded_channels.insert(selected.channel_id()) {
            return Err(WeightedRetryPlanError::Invariant);
        }
        self.attempts_started = next_attempt;

        Ok(Some(RetrySelection {
            candidate: selected,
            attempt: NonZeroUsize::new(next_attempt).ok_or(WeightedRetryPlanError::Invariant)?,
            priority_rank,
        }))
    }

    /// 消费计划并生成可直接交给 relay 装配层的固定候选顺序。
    ///
    /// 该顺序等价于每次候选都失败后继续推进，只做装配规划，不会提前执行上游请求。
    pub fn into_retry_order(mut self) -> Result<Vec<RetrySelection>, WeightedRetryPlanError> {
        let mut order = Vec::with_capacity(self.candidates.len());
        while let Some(selection) = self.select_next()? {
            order.push(selection);
        }
        Ok(order)
    }

    /// 返回已经开始的尝试次数，不包含尚未选择的候选。
    #[must_use]
    pub const fn attempts_started(&self) -> usize {
        self.attempts_started
    }
}

impl fmt::Debug for WeightedRetryPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WeightedRetryPlan")
            .field("candidate_count", &self.candidates.len())
            .field("priority_count", &self.priorities.len())
            .field("excluded_count", &self.excluded_channels.len())
            .field("attempts_started", &self.attempts_started)
            .field("health_count", &self.health.len())
            .finish()
    }
}

fn validate_single_group(candidates: &[SchedulerCandidate]) -> Result<(), WeightedRetryPlanError> {
    let Some(group_id) = candidates.first().map(|candidate| candidate.group_id()) else {
        return Ok(());
    };
    if candidates
        .iter()
        .any(|candidate| candidate.group_id() != group_id)
    {
        return Err(WeightedRetryPlanError::MixedGroups);
    }
    Ok(())
}

fn validate_unique_channels(
    candidates: &[SchedulerCandidate],
) -> Result<(), WeightedRetryPlanError> {
    let mut channel_ids = BTreeSet::new();
    if candidates
        .iter()
        .any(|candidate| !channel_ids.insert(candidate.channel_id()))
    {
        return Err(WeightedRetryPlanError::DuplicateChannel);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use af_domain::{GroupId, UpstreamError, UpstreamServerStatus};

    use super::*;
    use crate::{FailureAction, FailureContext, FailurePolicy};

    #[test]
    fn retries_descend_priorities_and_then_exhaust_the_lowest_tier() {
        let order = WeightedRetryPlan::new(vec![
            candidate(1, 1, 30),
            candidate(1, 2, 20),
            candidate(1, 3, 10),
            candidate(1, 4, 10),
        ])
        .unwrap()
        .into_retry_order()
        .unwrap();

        assert_eq!(
            order
                .iter()
                .map(|selection| selection.priority_rank())
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 2]
        );
        assert_eq!(
            order
                .iter()
                .map(|selection| selection.attempt().get())
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(
            order
                .iter()
                .map(|selection| selection.candidate().priority())
                .collect::<Vec<_>>(),
            vec![30, 20, 10, 10]
        );
        assert_unique_channels(&order);
    }

    #[test]
    fn one_priority_tier_reselects_until_every_channel_is_excluded() {
        let mut plan = WeightedRetryPlan::new(vec![
            candidate(1, 1, 7),
            candidate(1, 2, 7),
            candidate(1, 3, 7),
        ])
        .unwrap();
        let mut order = Vec::new();
        while let Some(selection) = plan.select_next().unwrap() {
            order.push(selection);
        }

        assert_eq!(order.len(), 3);
        assert!(order.iter().all(|selection| selection.priority_rank() == 0));
        assert_unique_channels(&order);
        assert_eq!(plan.attempts_started(), 3);
        assert_eq!(plan.select_next(), Ok(None));
    }

    #[test]
    fn downgrade_never_returns_to_untried_higher_priority_peers() {
        let order = WeightedRetryPlan::new(vec![
            candidate(1, 1, 10),
            candidate(1, 2, 10),
            candidate(1, 3, 0),
        ])
        .unwrap()
        .into_retry_order()
        .unwrap();

        assert_eq!(order.len(), 2);
        assert_eq!(order[0].candidate().priority(), 10);
        assert_eq!(order[1].candidate().priority(), 0);
        assert_unique_channels(&order);
    }

    #[test]
    fn empty_plan_is_exhausted_without_consuming_entropy() {
        let mut plan = WeightedRetryPlan::new(Vec::new()).unwrap();

        assert_eq!(plan.select_next(), Ok(None));
        assert_eq!(plan.attempts_started(), 0);
        assert!(plan.into_retry_order().unwrap().is_empty());
    }

    #[test]
    fn cooling_channel_is_removed_before_priority_selection() {
        let first = candidate(1, 1, 10);
        let second = candidate(1, 2, 10);
        let health = std::collections::BTreeMap::from([
            (first.channel_id(), ChannelRoutingHealth::new(0, true)),
            (
                second.channel_id(),
                ChannelRoutingHealth::new(2_500_000, false),
            ),
        ]);

        let order = WeightedRetryPlan::new_with_health(vec![first, second], &health)
            .unwrap()
            .into_retry_order()
            .unwrap();

        assert_eq!(order.len(), 1);
        assert_eq!(order[0].candidate().channel_id(), second.channel_id());
    }

    #[test]
    fn exhausted_transient_retry_fails_over_to_next_healthy_priority() {
        let first = candidate(1, 11, 20);
        let cooling = candidate(1, 12, 30);
        let fallback = candidate(1, 13, 10);
        let health = std::collections::BTreeMap::from([(
            cooling.channel_id(),
            ChannelRoutingHealth::new(0, true),
        )]);
        let policy = FailurePolicy::default();
        let error = UpstreamError::ServerError {
            status: UpstreamServerStatus::new(503).unwrap(),
        };
        let mut plan =
            WeightedRetryPlan::new_with_health(vec![first, cooling, fallback], &health).unwrap();

        let initial = plan.select_next().unwrap().unwrap();
        assert_eq!(initial.candidate().channel_id(), first.channel_id());
        assert_eq!(initial.attempt().get(), 1);
        assert_eq!(
            policy.action_for(error, FailureContext::new(0, false)),
            FailureAction::RetrySameChannel
        );
        assert_eq!(
            policy.action_for(error, FailureContext::new(1, false)),
            FailureAction::Failover
        );

        let failover = plan.select_next().unwrap().unwrap();
        assert_eq!(failover.candidate().channel_id(), fallback.channel_id());
        assert_eq!(failover.priority_rank(), 1);
        assert_eq!(failover.attempt().get(), 2);
        assert_eq!(plan.select_next(), Ok(None));
    }

    #[test]
    fn constructor_rejects_mixed_groups_duplicate_channels_and_oversized_sets() {
        assert_eq!(
            WeightedRetryPlan::new(vec![candidate(1, 1, 0), candidate(2, 2, 0)]).unwrap_err(),
            WeightedRetryPlanError::MixedGroups
        );
        assert_eq!(
            WeightedRetryPlan::new(vec![candidate(1, 1, 0), candidate(1, 1, -1)]).unwrap_err(),
            WeightedRetryPlanError::DuplicateChannel
        );
        assert_eq!(
            WeightedRetryPlan::new(
                (1_i64..=65)
                    .map(|channel_id| candidate(1, channel_id, 0))
                    .collect()
            )
            .unwrap_err(),
            WeightedRetryPlanError::TooManyCandidates
        );
    }

    #[test]
    fn debug_output_contains_only_counts_and_cursor_state() {
        let mut plan = WeightedRetryPlan::new(vec![candidate(1, 41, 9)]).unwrap();
        plan.select_next().unwrap();

        let rendered = format!("{plan:?}");
        assert!(rendered.contains("candidate_count: 1"));
        assert!(rendered.contains("attempts_started: 1"));
        assert!(!rendered.contains("41"));
    }

    fn candidate(group_id: i64, channel_id: i64, priority: i32) -> SchedulerCandidate {
        SchedulerCandidate::new(
            GroupId::new(group_id).unwrap(),
            ChannelId::new(channel_id).unwrap(),
            priority,
            0,
        )
    }

    fn assert_unique_channels(order: &[RetrySelection]) {
        let channels = order
            .iter()
            .map(|selection| selection.candidate().channel_id())
            .collect::<BTreeSet<_>>();
        assert_eq!(channels.len(), order.len());
    }
}
