use getrandom::fill;
use thiserror::Error;

use af_domain::ChannelId;
use std::collections::BTreeMap;

use crate::{ChannelRoutingHealth, SchedulerCandidate, health::effective_candidate_weight};

/// 单次交给 relay 的候选上限，与当前有界重试状态机保持一致。
pub const MAX_SCHEDULER_CANDIDATES: usize = 64;
/// 每个同优先级候选都拥有的基础权重，保证配置权重为零时仍可被抽中。
pub const WEIGHT_BASE: u64 = 10;

/// 一次加权选择的不可变结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WeightedSelection {
    candidate: SchedulerCandidate,
    priority_rank: usize,
}

impl WeightedSelection {
    /// 返回最终选中的渠道候选。
    #[must_use]
    pub const fn candidate(self) -> SchedulerCandidate {
        self.candidate
    }

    /// 返回本次选择使用的去重优先级层序号，零表示最高层。
    #[must_use]
    pub const fn priority_rank(self) -> usize {
        self.priority_rank
    }
}

/// 加权选路失败；错误不携带分组、渠道或模型标识。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum WeightedSelectionError {
    /// 候选数量超过 relay 当前可安全接收的硬上限。
    #[error("调度候选数量超过上限")]
    TooManyCandidates,
    /// 计算同层总有效权重时发生整数溢出。
    #[error("调度候选权重总和溢出")]
    WeightOverflow,
    /// 操作系统随机源不可用，无法做无偏加权选择。
    #[error("调度随机源不可用")]
    EntropyUnavailable,
}

/// 优先级分层、同层按 `weight + WEIGHT_BASE` 抽样的默认策略。
#[derive(Clone, Copy, Debug, Default)]
pub struct WeightedStrategy;

impl WeightedStrategy {
    /// 从最高优先级层选择一个候选；空集合返回 `None`。
    pub fn select(
        self,
        candidates: &[SchedulerCandidate],
    ) -> Result<Option<WeightedSelection>, WeightedSelectionError> {
        self.select_at_priority_rank(candidates, 0)
    }

    /// 从第 `priority_rank` 个去重优先级层选择候选，供后续重试逐层降级复用。
    pub fn select_at_priority_rank(
        self,
        candidates: &[SchedulerCandidate],
        priority_rank: usize,
    ) -> Result<Option<WeightedSelection>, WeightedSelectionError> {
        validate_candidate_count(candidates)?;
        let Some(priority) = priority_at_rank(candidates, priority_rank) else {
            return Ok(None);
        };
        let tier = sorted_tier(candidates, priority);
        let total_weight = total_effective_weight(&tier, None)?;
        let draw = random_below(total_weight)?;
        Ok(select_for_draw(&tier, priority_rank, draw, None))
    }

    pub(crate) fn select_with_health(
        self,
        candidates: &[SchedulerCandidate],
        health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
    ) -> Result<Option<WeightedSelection>, WeightedSelectionError> {
        validate_candidate_count(candidates)?;
        let Some(priority) = priority_at_rank(candidates, 0) else {
            return Ok(None);
        };
        let tier = sorted_tier(candidates, priority);
        let total_weight = total_effective_weight(&tier, Some(health))?;
        let draw = random_below(total_weight)?;
        Ok(select_for_draw(&tier, 0, draw, Some(health)))
    }
}

fn validate_candidate_count(
    candidates: &[SchedulerCandidate],
) -> Result<(), WeightedSelectionError> {
    if candidates.len() > MAX_SCHEDULER_CANDIDATES {
        return Err(WeightedSelectionError::TooManyCandidates);
    }
    Ok(())
}

fn priority_at_rank(candidates: &[SchedulerCandidate], rank: usize) -> Option<i32> {
    let mut priorities = candidates
        .iter()
        .map(|candidate| candidate.priority())
        .collect::<Vec<_>>();
    priorities.sort_unstable_by(|left, right| right.cmp(left));
    priorities.dedup();
    priorities.get(rank).copied()
}

fn sorted_tier(candidates: &[SchedulerCandidate], priority: i32) -> Vec<SchedulerCandidate> {
    let mut tier = candidates
        .iter()
        .copied()
        .filter(|candidate| candidate.priority() == priority)
        .collect::<Vec<_>>();
    // 固定渠道顺序只影响 draw 到候选的映射，不改变各候选概率。
    tier.sort_unstable_by_key(|candidate| candidate.channel_id());
    tier
}

fn total_effective_weight(
    candidates: &[SchedulerCandidate],
    health: Option<&BTreeMap<ChannelId, ChannelRoutingHealth>>,
) -> Result<u64, WeightedSelectionError> {
    candidates.iter().try_fold(0_u64, |total, candidate| {
        total
            .checked_add(effective_weight(*candidate, health))
            .ok_or(WeightedSelectionError::WeightOverflow)
    })
}

fn effective_weight(
    candidate: SchedulerCandidate,
    health: Option<&BTreeMap<ChannelId, ChannelRoutingHealth>>,
) -> u64 {
    effective_candidate_weight(
        candidate,
        health.and_then(|states| states.get(&candidate.channel_id()).copied()),
    )
}

fn random_below(upper_bound: u64) -> Result<u64, WeightedSelectionError> {
    debug_assert!(upper_bound > 0);
    // 拒绝低端残余区间，避免直接取模让部分候选产生极小概率偏差。
    let rejection_threshold = upper_bound.wrapping_neg() % upper_bound;
    loop {
        let mut entropy = [0_u8; size_of::<u64>()];
        fill(&mut entropy).map_err(|_| WeightedSelectionError::EntropyUnavailable)?;
        let value = u64::from_le_bytes(entropy);
        if value >= rejection_threshold {
            return Ok(value % upper_bound);
        }
    }
}

fn select_for_draw(
    candidates: &[SchedulerCandidate],
    priority_rank: usize,
    draw: u64,
    health: Option<&BTreeMap<ChannelId, ChannelRoutingHealth>>,
) -> Option<WeightedSelection> {
    let mut cursor = draw;
    for candidate in candidates {
        let weight = effective_weight(*candidate, health);
        if cursor < weight {
            return Some(WeightedSelection {
                candidate: *candidate,
                priority_rank,
            });
        }
        cursor -= weight;
    }
    None
}

#[cfg(test)]
mod tests {
    use af_domain::{ChannelId, GroupId};

    use super::*;

    #[test]
    fn highest_priority_excludes_all_lower_layers() {
        let candidates = [candidate(1, 10, 0), candidate(2, 9, u32::MAX)];
        let priority = priority_at_rank(&candidates, 0).expect("最高优先级必须存在");
        let tier = sorted_tier(&candidates, priority);

        assert_eq!(tier, vec![candidate(1, 10, 0)]);
        assert_eq!(
            select_for_draw(&tier, 0, 0, None).unwrap().candidate(),
            tier[0]
        );
    }

    #[test]
    fn zero_weight_candidates_keep_the_base_probability() {
        let candidates = [candidate(2, 5, 0), candidate(1, 5, 0)];
        let tier = sorted_tier(&candidates, 5);

        assert_eq!(total_effective_weight(&tier, None), Ok(20));
        assert_eq!(
            select_for_draw(&tier, 0, 0, None)
                .unwrap()
                .candidate()
                .channel_id(),
            ChannelId::new(1).unwrap()
        );
        assert_eq!(
            select_for_draw(&tier, 0, WEIGHT_BASE, None)
                .unwrap()
                .candidate()
                .channel_id(),
            ChannelId::new(2).unwrap()
        );
    }

    #[test]
    fn configured_weight_is_added_to_the_base_weight() {
        let candidates = [candidate(1, 5, 0), candidate(2, 5, 90)];
        let tier = sorted_tier(&candidates, 5);

        assert_eq!(total_effective_weight(&tier, None), Ok(110));
        assert_eq!(
            select_for_draw(&tier, 0, 9, None)
                .unwrap()
                .candidate()
                .channel_id(),
            ChannelId::new(1).unwrap()
        );
        assert_eq!(
            select_for_draw(&tier, 0, 10, None)
                .unwrap()
                .candidate()
                .channel_id(),
            ChannelId::new(2).unwrap()
        );
    }

    #[test]
    fn health_penalty_reduces_draw_range_without_removing_candidate() {
        let candidates = [candidate(1, 5, 90), candidate(2, 5, 90)];
        let tier = sorted_tier(&candidates, 5);
        let health = BTreeMap::from([(
            ChannelId::new(2).unwrap(),
            ChannelRoutingHealth::new(crate::ROUTING_HEALTH_PENALTY_SCALE, false),
        )]);

        assert_eq!(total_effective_weight(&tier, Some(&health)), Ok(150));
        assert_eq!(
            select_for_draw(&tier, 0, 99, Some(&health))
                .unwrap()
                .candidate()
                .channel_id(),
            ChannelId::new(1).unwrap()
        );
        assert_eq!(
            select_for_draw(&tier, 0, 100, Some(&health))
                .unwrap()
                .candidate()
                .channel_id(),
            ChannelId::new(2).unwrap()
        );
    }

    #[test]
    fn priority_rank_selects_the_requested_distinct_layer() {
        let candidates = [candidate(1, 8, 0), candidate(2, 8, 0), candidate(3, 3, 0)];

        assert_eq!(priority_at_rank(&candidates, 0), Some(8));
        assert_eq!(priority_at_rank(&candidates, 1), Some(3));
        assert_eq!(priority_at_rank(&candidates, 2), None);
    }

    #[test]
    fn empty_and_oversized_candidate_sets_are_bounded() {
        assert_eq!(WeightedStrategy.select(&[]), Ok(None));

        let candidates = (1_i64..=65)
            .map(|channel_id| candidate(channel_id, 0, 0))
            .collect::<Vec<_>>();
        assert_eq!(
            WeightedStrategy.select(&candidates),
            Err(WeightedSelectionError::TooManyCandidates)
        );
    }

    #[test]
    fn system_entropy_path_selects_the_only_candidate() {
        let only = candidate(7, 3, 0);

        let selected = WeightedStrategy
            .select(&[only])
            .expect("系统随机源应可用")
            .expect("单候选必须被选中");

        assert_eq!(selected.candidate(), only);
        assert_eq!(selected.priority_rank(), 0);
    }

    fn candidate(channel_id: i64, priority: i32, weight: u32) -> SchedulerCandidate {
        SchedulerCandidate::new(
            GroupId::new(1).unwrap(),
            ChannelId::new(channel_id).unwrap(),
            priority,
            weight,
        )
    }
}
