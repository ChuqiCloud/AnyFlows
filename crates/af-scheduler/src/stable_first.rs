use std::collections::{BTreeMap, BTreeSet};

use af_domain::ChannelId;
use thiserror::Error;

use crate::{
    ChannelRoutingHealth, MAX_SCHEDULER_CANDIDATES, ROUTING_HEALTH_PENALTY_SCALE,
    SchedulerCandidate,
};

/// StableFirst 默认把健康分数接近主池最佳值的候选保留在主池中。
pub const DEFAULT_PRIMARY_SCORE_RATIO_MICROS: u64 = 920_000;

/// StableFirst 将候选分配到的运行池。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StableFirstPool {
    /// 当前稳定主流量池，按配置优先级参与正常选路。
    Main,
    /// 健康惩罚尚未冷却的观察池，仅供后续灰度探测使用。
    Observation,
}

/// StableFirst 的一次纯内存分类结果。
///
/// 该结果只描述候选应该落在哪个池，不主动发起探测或改变现有生产选路，
/// 让调用方可以在明确的灰度策略下消费观察池。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableFirstPlan {
    main_pool: Vec<SchedulerCandidate>,
    observation_pool: Vec<SchedulerCandidate>,
    excluded_count: usize,
}

impl StableFirstPlan {
    /// 返回稳定主流量池，顺序按优先级降序和渠道编号升序固定。
    #[must_use]
    pub fn main_pool(&self) -> &[SchedulerCandidate] {
        &self.main_pool
    }

    /// 返回观察池，顺序与主流量池使用相同的确定性排序。
    #[must_use]
    pub fn observation_pool(&self) -> &[SchedulerCandidate] {
        &self.observation_pool
    }

    /// 返回因冷却状态被排除的候选数量。
    #[must_use]
    pub const fn excluded_count(&self) -> usize {
        self.excluded_count
    }

    /// 返回候选总数（包含主池、观察池和冷却剔除项）。
    #[must_use]
    pub fn candidate_count(&self) -> usize {
        self.main_pool.len() + self.observation_pool.len() + self.excluded_count
    }

    /// 返回指定候选应归属的池；调用方可用此方法记录可观测事件。
    #[must_use]
    pub fn pool_for(&self, channel_id: ChannelId) -> Option<StableFirstPool> {
        if self
            .main_pool
            .iter()
            .any(|candidate| candidate.channel_id() == channel_id)
        {
            return Some(StableFirstPool::Main);
        }
        if self
            .observation_pool
            .iter()
            .any(|candidate| candidate.channel_id() == channel_id)
        {
            return Some(StableFirstPool::Observation);
        }
        None
    }
}

/// StableFirst 分类失败时返回的安全错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum StableFirstPlanError {
    /// 候选数量超过单次调度的固定上限。
    #[error("StableFirst 候选数量超过上限")]
    TooManyCandidates,
    /// 同一个计划混入了不同计费分组。
    #[error("StableFirst 候选分组不一致")]
    MixedGroups,
    /// 同一个渠道在计划中重复出现。
    #[error("StableFirst 候选渠道重复")]
    DuplicateChannel,
    /// 主池健康分数阈值不在定点精度范围内。
    #[error("StableFirst 主池健康阈值无效")]
    InvalidPrimaryScoreRatio,
}

/// 将健康候选稳定地拆分为主池、观察池和冷却剔除项。
#[derive(Clone, Copy, Debug)]
pub struct StableFirstStrategy {
    primary_score_ratio_micros: u64,
}

impl Default for StableFirstStrategy {
    fn default() -> Self {
        Self {
            primary_score_ratio_micros: DEFAULT_PRIMARY_SCORE_RATIO_MICROS,
        }
    }
}

impl StableFirstStrategy {
    /// 使用显式主池健康阈值创建策略。
    pub fn new(primary_score_ratio_micros: u64) -> Result<Self, StableFirstPlanError> {
        if primary_score_ratio_micros == 0
            || primary_score_ratio_micros > ROUTING_HEALTH_PENALTY_SCALE
        {
            return Err(StableFirstPlanError::InvalidPrimaryScoreRatio);
        }
        Ok(Self {
            primary_score_ratio_micros,
        })
    }

    /// 返回主池健康分数阈值。
    #[must_use]
    pub const fn primary_score_ratio_micros(self) -> u64 {
        self.primary_score_ratio_micros
    }

    /// 按固定排序和健康状态生成一次 StableFirst 计划。
    pub fn plan(
        self,
        candidates: &[SchedulerCandidate],
        health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
    ) -> Result<StableFirstPlan, StableFirstPlanError> {
        validate_candidates(candidates)?;

        let mut ordered = candidates.to_vec();
        ordered.sort_unstable_by(|left, right| {
            right
                .priority()
                .cmp(&left.priority())
                .then_with(|| left.channel_id().cmp(&right.channel_id()))
        });

        let eligible = ordered
            .into_iter()
            .filter_map(|candidate| {
                let state = health.get(&candidate.channel_id()).copied();
                if state.is_some_and(ChannelRoutingHealth::is_cooling) {
                    return None;
                }
                let score = state.map_or(
                    ROUTING_HEALTH_PENALTY_SCALE,
                    ChannelRoutingHealth::multiplier_micros,
                );
                Some((candidate, score))
            })
            .collect::<Vec<_>>();
        let best_score = eligible
            .iter()
            .map(|(_, score)| *score)
            .max()
            .unwrap_or_default();
        let threshold = best_score * self.primary_score_ratio_micros / ROUTING_HEALTH_PENALTY_SCALE;
        let mut main_pool = Vec::with_capacity(eligible.len());
        let mut observation_pool = Vec::with_capacity(eligible.len());
        let excluded_count = candidates.len() - eligible.len();
        for (candidate, score) in eligible {
            if score >= threshold {
                main_pool.push(candidate);
            } else {
                observation_pool.push(candidate);
            }
        }

        Ok(StableFirstPlan {
            main_pool,
            observation_pool,
            excluded_count,
        })
    }
}

fn validate_candidates(candidates: &[SchedulerCandidate]) -> Result<(), StableFirstPlanError> {
    if candidates.len() > MAX_SCHEDULER_CANDIDATES {
        return Err(StableFirstPlanError::TooManyCandidates);
    }
    let Some(group_id) = candidates.first().map(|candidate| candidate.group_id()) else {
        return Ok(());
    };
    if candidates
        .iter()
        .any(|candidate| candidate.group_id() != group_id)
    {
        return Err(StableFirstPlanError::MixedGroups);
    }
    let mut channels = BTreeSet::new();
    if candidates
        .iter()
        .any(|candidate| !channels.insert(candidate.channel_id()))
    {
        return Err(StableFirstPlanError::DuplicateChannel);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use af_domain::{ChannelId, GroupId};

    use super::*;

    #[test]
    fn healthy_candidates_stay_in_the_main_pool_in_stable_order() {
        let candidates = [candidate(3, 2, 1), candidate(1, 5, 0), candidate(2, 5, 1)];

        let plan = StableFirstStrategy::default()
            .plan(&candidates, &BTreeMap::new())
            .unwrap();

        assert_eq!(ids(plan.main_pool()), &[1, 2, 3]);
        assert!(plan.observation_pool().is_empty());
        assert_eq!(plan.excluded_count(), 0);
        assert_eq!(plan.candidate_count(), 3);
    }

    #[test]
    fn penalized_candidates_enter_observation_and_cooling_candidates_are_excluded() {
        let candidates = [candidate(1, 10, 0), candidate(2, 10, 0), candidate(3, 9, 0)];
        let health = BTreeMap::from([
            (
                ChannelId::new(2).unwrap(),
                ChannelRoutingHealth::new(10_000_000, false),
            ),
            (
                ChannelId::new(3).unwrap(),
                ChannelRoutingHealth::new(0, true),
            ),
        ]);

        let plan = StableFirstStrategy::default()
            .plan(&candidates, &health)
            .unwrap();

        assert_eq!(ids(plan.main_pool()), &[1]);
        assert_eq!(ids(plan.observation_pool()), &[2]);
        assert_eq!(plan.excluded_count(), 1);
        assert_eq!(
            plan.pool_for(ChannelId::new(1).unwrap()),
            Some(StableFirstPool::Main)
        );
        assert_eq!(
            plan.pool_for(ChannelId::new(2).unwrap()),
            Some(StableFirstPool::Observation)
        );
        assert_eq!(plan.pool_for(ChannelId::new(3).unwrap()), None);
    }

    #[test]
    fn all_penalized_candidates_do_not_get_promoted_to_main_pool() {
        let candidates = [candidate(1, 2, 0), candidate(2, 1, 0)];
        let health = BTreeMap::from([
            (
                ChannelId::new(1).unwrap(),
                ChannelRoutingHealth::new(20_000_000, false),
            ),
            (
                ChannelId::new(2).unwrap(),
                ChannelRoutingHealth::new(4_000_000, false),
            ),
        ]);

        let plan = StableFirstStrategy::default()
            .plan(&candidates, &health)
            .unwrap();

        assert_eq!(ids(plan.main_pool()), &[2]);
        assert_eq!(ids(plan.observation_pool()), &[1]);
    }

    #[test]
    fn invalid_candidate_sets_fail_closed() {
        assert_eq!(
            StableFirstStrategy::default()
                .plan(
                    &[candidate(1, 1, 0), candidate_in_group(2, 2, 0, 0)],
                    &BTreeMap::new()
                )
                .unwrap_err(),
            StableFirstPlanError::MixedGroups
        );
        assert_eq!(
            StableFirstStrategy::default()
                .plan(&[candidate(1, 1, 0), candidate(1, 1, 0)], &BTreeMap::new())
                .unwrap_err(),
            StableFirstPlanError::DuplicateChannel
        );
        let oversized = (1_i64..=65)
            .map(|id| candidate(id, 0, 0))
            .collect::<Vec<_>>();
        assert_eq!(
            StableFirstStrategy::default()
                .plan(&oversized, &BTreeMap::new())
                .unwrap_err(),
            StableFirstPlanError::TooManyCandidates
        );
        assert_eq!(
            StableFirstStrategy::new(0).unwrap_err(),
            StableFirstPlanError::InvalidPrimaryScoreRatio
        );
        assert_eq!(
            StableFirstStrategy::new(ROUTING_HEALTH_PENALTY_SCALE + 1).unwrap_err(),
            StableFirstPlanError::InvalidPrimaryScoreRatio
        );
    }

    fn candidate(channel_id: i64, priority: i32, weight: u32) -> SchedulerCandidate {
        candidate_in_group(1, channel_id, priority, weight)
    }

    fn candidate_in_group(
        group_id: i64,
        channel_id: i64,
        priority: i32,
        weight: u32,
    ) -> SchedulerCandidate {
        SchedulerCandidate::new(
            GroupId::new(group_id).unwrap(),
            ChannelId::new(channel_id).unwrap(),
            priority,
            weight,
        )
    }

    fn ids(candidates: &[SchedulerCandidate]) -> Vec<i64> {
        candidates
            .iter()
            .map(|candidate| candidate.channel_id().get())
            .collect()
    }
}
