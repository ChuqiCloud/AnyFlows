use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroUsize,
    sync::Arc,
};

use af_db::SchedulerRuntimeTargetRecord;
use af_domain::{ChannelId, CredentialId, GroupId, RouteChannelId, RouteStrategy};
use thiserror::Error;

use crate::{
    ChannelRoutingHealth, RetrySelection, SchedulerCandidate, StableFirstStrategy,
    WeightedRetryPlan, WeightedRetryPlanError, health::healthy_channel,
};

use super::{
    ChannelIndexCacheError, ChannelIndexSnapshot, ChannelIndexSnapshotError, IndexedRouteCandidate,
    IndexedRoutePlan, IndexedWeightedScheduler, IndexedWeightedSchedulerError,
};

/// 管理员智能路由绑定到一条渠道凭据的运行时输入。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundRouteCandidate {
    route_channel_id: RouteChannelId,
    channel_id: ChannelId,
    credential_id: CredentialId,
    priority: i32,
    weight: u32,
    last_selected_at_millis: Option<i64>,
}

impl BoundRouteCandidate {
    /// 构造已经完成强类型标识和非负调度参数校验的规则候选。
    pub fn new(
        route_channel_id: RouteChannelId,
        channel_id: ChannelId,
        credential_id: CredentialId,
        priority: i32,
        weight: u32,
        last_selected_at_millis: Option<i64>,
    ) -> Result<Self, BoundRouteCandidateError> {
        if priority < 0 || last_selected_at_millis.is_some_and(|value| value < 0) {
            return Err(BoundRouteCandidateError);
        }
        Ok(Self {
            route_channel_id,
            channel_id,
            credential_id,
            priority,
            weight,
            last_selected_at_millis,
        })
    }
}

/// 智能路由候选输入违反闭合调度边界。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("智能路由候选无效")]
pub struct BoundRouteCandidateError;

struct BoundChannelCandidate {
    candidate: SchedulerCandidate,
    runtime_target: Arc<SchedulerRuntimeTargetRecord>,
    route_channel_ids: BTreeMap<CredentialId, RouteChannelId>,
    last_selected_at_millis: Option<i64>,
}

impl IndexedWeightedScheduler {
    /// 在能力索引内将管理员规则候选收敛为同代、同计费分组的固定计划。
    pub fn bound_route_plan(
        &self,
        group_id: GroupId,
        model: &str,
        strategy: RouteStrategy,
        bindings: &[BoundRouteCandidate],
        health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
    ) -> Result<Option<IndexedRoutePlan>, IndexedWeightedSchedulerError> {
        let snapshot = self.index.snapshot()?;
        build_bound_route_plan(&snapshot, group_id, model, strategy, bindings, health)
    }
}

fn build_bound_route_plan(
    snapshot: &ChannelIndexSnapshot,
    group_id: GroupId,
    model: &str,
    strategy: RouteStrategy,
    bindings: &[BoundRouteCandidate],
    health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
) -> Result<Option<IndexedRoutePlan>, IndexedWeightedSchedulerError> {
    if bindings.len() > crate::MAX_SCHEDULER_CANDIDATES {
        return Err(WeightedRetryPlanError::TooManyCandidates.into());
    }
    let mut route_channel_ids = BTreeSet::new();
    let mut pairs = BTreeSet::new();
    for binding in bindings {
        if !route_channel_ids.insert(binding.route_channel_id)
            || !pairs.insert((binding.channel_id, binding.credential_id))
        {
            return Err(WeightedRetryPlanError::Invariant.into());
        }
    }

    let ability_channels = snapshot
        .candidates(group_id, model)?
        .iter()
        .map(|candidate| candidate.channel_id())
        .collect::<BTreeSet<_>>();
    let mut by_channel = BTreeMap::<ChannelId, Vec<BoundRouteCandidate>>::new();
    for binding in bindings {
        if ability_channels.contains(&binding.channel_id) {
            by_channel
                .entry(binding.channel_id)
                .or_default()
                .push(*binding);
        }
    }

    let mut channels = Vec::with_capacity(by_channel.len());
    for (channel_id, channel_bindings) in by_channel {
        let Some(target) = snapshot.runtime_targets.get(&channel_id) else {
            return Err(IndexedWeightedSchedulerError::Cache(
                ChannelIndexCacheError::Snapshot(ChannelIndexSnapshotError::Invariant),
            ));
        };
        let available_credentials = target
            .credentials()
            .iter()
            .map(|credential| credential.credential_id())
            .collect::<BTreeSet<_>>();
        let retained = channel_bindings
            .into_iter()
            .filter(|binding| available_credentials.contains(&binding.credential_id.get()))
            .collect::<Vec<_>>();
        let Some(effective_priority) = retained.iter().map(|binding| binding.priority).max() else {
            continue;
        };
        let effective_weight = retained
            .iter()
            .filter(|binding| binding.priority == effective_priority)
            .try_fold(0_u32, |total, binding| total.checked_add(binding.weight))
            .ok_or(WeightedRetryPlanError::Invariant)?;
        let scheduling = retained
            .iter()
            .map(|binding| {
                (
                    binding.credential_id.get(),
                    (binding.priority, binding.weight),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let route_bindings = retained
            .iter()
            .map(|binding| (binding.credential_id, binding.route_channel_id))
            .collect::<BTreeMap<_, _>>();
        let last_selected_at_millis = retained
            .iter()
            .filter_map(|binding| binding.last_selected_at_millis)
            .max();
        let runtime_target = target
            .as_ref()
            .clone()
            .bind_route_credentials(&scheduling)
            .map_err(|_| WeightedRetryPlanError::Invariant)?;
        channels.push(BoundChannelCandidate {
            candidate: SchedulerCandidate::new(
                group_id,
                channel_id,
                effective_priority,
                effective_weight,
            ),
            runtime_target: Arc::new(runtime_target),
            route_channel_ids: route_bindings,
            last_selected_at_millis,
        });
    }
    if channels.is_empty() {
        return Ok(None);
    }

    let effective_health = health
        .iter()
        .filter(|(channel_id, _)| {
            !channels.iter().any(|candidate| {
                candidate.candidate.channel_id() == **channel_id
                    && candidate.runtime_target.pool_mode()
            })
        })
        .map(|(channel_id, state)| (*channel_id, *state))
        .collect::<BTreeMap<_, _>>();
    let ordered = order_bound_channels(strategy, &channels, &effective_health)?;
    if ordered.is_empty() {
        return Ok(None);
    }
    let mut planned = Vec::with_capacity(ordered.len());
    for selection in ordered {
        let channel = channels
            .iter()
            .find(|candidate| {
                candidate.candidate.channel_id() == selection.candidate().channel_id()
            })
            .ok_or(WeightedRetryPlanError::Invariant)?;
        planned.push(IndexedRouteCandidate::new_bound(
            selection,
            Arc::clone(&channel.runtime_target),
            channel.route_channel_ids.clone(),
        ));
    }
    Ok(Some(IndexedRoutePlan::new(
        snapshot.generation(),
        group_id,
        planned,
    )))
}

fn order_bound_channels(
    strategy: RouteStrategy,
    channels: &[BoundChannelCandidate],
    health: &BTreeMap<ChannelId, ChannelRoutingHealth>,
) -> Result<Vec<RetrySelection>, IndexedWeightedSchedulerError> {
    let candidates = channels
        .iter()
        .map(|channel| channel.candidate)
        .collect::<Vec<_>>();
    match strategy {
        RouteStrategy::Weighted => {
            Ok(WeightedRetryPlan::new_with_health(candidates, health)?.into_retry_order()?)
        }
        RouteStrategy::RoundRobin => {
            let mut eligible = channels
                .iter()
                .filter(|channel| healthy_channel(channel.candidate.channel_id(), health))
                .collect::<Vec<_>>();
            eligible.sort_unstable_by(|left, right| {
                left.last_selected_at_millis
                    .cmp(&right.last_selected_at_millis)
                    .then_with(|| {
                        left.candidate
                            .channel_id()
                            .cmp(&right.candidate.channel_id())
                    })
            });
            ordered_selections(eligible.into_iter().map(|channel| channel.candidate))
        }
        RouteStrategy::StableFirst => {
            let plan = StableFirstStrategy::default().plan(&candidates, health)?;
            ordered_selections(plan.main_pool().iter().copied())
        }
    }
}

fn ordered_selections(
    candidates: impl IntoIterator<Item = SchedulerCandidate>,
) -> Result<Vec<RetrySelection>, IndexedWeightedSchedulerError> {
    candidates
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| {
            let attempt = NonZeroUsize::new(index + 1).ok_or(WeightedRetryPlanError::Invariant)?;
            Ok(RetrySelection::from_ordered_candidate(
                candidate, attempt, 0,
            ))
        })
        .collect::<Result<Vec<_>, WeightedRetryPlanError>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use af_db::{EncryptedCredentialEnvelope, SchedulerRuntimeCredentialRecord};
    use af_domain::{ChannelType, CredentialKind, Protocol};

    use super::*;
    use crate::index::ChannelIndexSourceRecord;

    #[test]
    fn multiple_bindings_on_one_channel_aggregate_the_highest_priority_weight() {
        let snapshot = snapshot(&[(1, &[101, 102, 103])]);
        let bindings = [
            binding(11, 1, 101, 10, 2, None),
            binding(12, 1, 102, 20, 3, None),
            binding(13, 1, 103, 20, 5, None),
        ];

        let plan = build_bound_route_plan(
            &snapshot,
            group_id(),
            "gpt-routed",
            RouteStrategy::Weighted,
            &bindings,
            &BTreeMap::new(),
        )
        .unwrap()
        .unwrap();

        let candidate = &plan.candidates()[0];
        assert_eq!(candidate.selection().candidate().priority(), 20);
        assert_eq!(candidate.selection().candidate().weight(), 8);
        assert_eq!(
            candidate
                .runtime_target()
                .credentials()
                .iter()
                .map(|credential| {
                    (
                        credential.credential_id(),
                        credential.priority(),
                        credential.weight(),
                    )
                })
                .collect::<Vec<_>>(),
            [(101, 10, 2), (102, 20, 3), (103, 20, 5)]
        );
    }

    #[test]
    fn round_robin_orders_channels_by_the_oldest_selection_only() {
        let snapshot = snapshot(&[(1, &[101]), (2, &[201]), (3, &[301])]);
        let bindings = [
            binding(11, 1, 101, 100, 100, Some(300)),
            binding(12, 2, 201, 1, 1, None),
            binding(13, 3, 301, 50, 50, Some(100)),
        ];

        let plan = build_bound_route_plan(
            &snapshot,
            group_id(),
            "gpt-routed",
            RouteStrategy::RoundRobin,
            &bindings,
            &BTreeMap::new(),
        )
        .unwrap()
        .unwrap();

        assert_eq!(channel_ids(&plan), [2, 3, 1]);
    }

    #[test]
    fn stable_first_excludes_the_observation_pool_from_live_traffic() {
        let snapshot = snapshot(&[(1, &[101]), (2, &[201])]);
        let bindings = [
            binding(11, 1, 101, 10, 1, None),
            binding(12, 2, 201, 20, 1, None),
        ];
        let health = BTreeMap::from([
            (
                ChannelId::new(1).unwrap(),
                ChannelRoutingHealth::new(0, false),
            ),
            (
                ChannelId::new(2).unwrap(),
                ChannelRoutingHealth::new(1_000_000, false),
            ),
        ]);

        let plan = build_bound_route_plan(
            &snapshot,
            group_id(),
            "gpt-routed",
            RouteStrategy::StableFirst,
            &bindings,
            &health,
        )
        .unwrap()
        .unwrap();

        assert_eq!(channel_ids(&plan), [1]);
    }

    fn group_id() -> GroupId {
        GroupId::new(7).unwrap()
    }

    fn binding(
        route_channel_id: i64,
        channel_id: i64,
        credential_id: i64,
        priority: i32,
        weight: u32,
        last_selected_at_millis: Option<i64>,
    ) -> BoundRouteCandidate {
        BoundRouteCandidate::new(
            RouteChannelId::new(route_channel_id).unwrap(),
            ChannelId::new(channel_id).unwrap(),
            CredentialId::new(credential_id).unwrap(),
            priority,
            weight,
            last_selected_at_millis,
        )
        .unwrap()
    }

    fn snapshot(channels: &[(i64, &[i64])]) -> ChannelIndexSnapshot {
        let records = channels
            .iter()
            .map(|(channel_id, credential_ids)| {
                ChannelIndexSourceRecord::with_runtime_target(
                    group_id(),
                    "gpt-routed",
                    0,
                    1,
                    Arc::new(runtime_target(*channel_id, credential_ids)),
                )
                .unwrap()
            })
            .collect();
        ChannelIndexSnapshot::from_records(records, 9).unwrap()
    }

    fn runtime_target(channel_id: i64, credential_ids: &[i64]) -> SchedulerRuntimeTargetRecord {
        let credentials = credential_ids
            .iter()
            .map(|credential_id| {
                SchedulerRuntimeCredentialRecord::new(
                    *credential_id,
                    CredentialKind::ApiKey,
                    EncryptedCredentialEnvelope::new(
                        "test-key",
                        [u8::try_from(*credential_id).unwrap_or(0x42); 24],
                        vec![0x24; 16],
                    )
                    .unwrap(),
                    false,
                )
                .unwrap()
            })
            .collect();
        SchedulerRuntimeTargetRecord::new_pool(
            ChannelId::new(channel_id).unwrap(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some(format!("https://channel-{channel_id}.example.com")),
            credentials,
            Vec::new(),
        )
        .unwrap()
    }

    fn channel_ids(plan: &IndexedRoutePlan) -> Vec<i64> {
        plan.candidates()
            .iter()
            .map(|candidate| candidate.channel_id().get())
            .collect()
    }
}
