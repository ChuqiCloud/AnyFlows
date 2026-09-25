use std::{
    cmp::{Ordering, Reverse},
    collections::BTreeMap,
};

use af_db::SchedulerRuntimeCredentialRecord;
use af_domain::{ChannelId, ConcurrencyLimit};
use sha2::{Digest, Sha256};

use crate::health_runtime::RuntimeHealthState;
use af_cache::HEALTH_PENALTY_SCALE;

const ORDER_DOMAIN: &[u8] = b"anyflows-credential-order-v1";
const ZERO_WEIGHT_DOMAIN: &[u8] = b"anyflows-credential-zero-weight-v1";
const MIN_HEALTH_MULTIPLIER_MICROS: u64 = 80_000;

#[derive(Clone, Copy)]
struct WeightedCredential<'a> {
    credential: &'a SchedulerRuntimeCredentialRecord,
    effective_weight: u64,
}

/// 单个凭据在本次 Redis 批量读取中固定的活跃与等待负载。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CredentialLoad {
    active: u32,
    waiting: u32,
    limit: Option<ConcurrencyLimit>,
}

impl CredentialLoad {
    /// 组合 Redis 返回的计数与同代运行时并发上限。
    #[must_use]
    pub(crate) const fn new(active: u32, waiting: u32, limit: Option<ConcurrencyLimit>) -> Self {
        Self {
            active,
            waiting,
            limit,
        }
    }

    fn compare_utilization(self, other: Self) -> Ordering {
        let left_total = u128::from(self.active) + u128::from(self.waiting);
        let right_total = u128::from(other.active) + u128::from(other.waiting);
        let left_capacity = u128::from(self.limit.map_or(1, ConcurrencyLimit::get));
        let right_capacity = u128::from(other.limit.map_or(1, ConcurrencyLimit::get));
        (left_total * right_capacity)
            .cmp(&(right_total * left_capacity))
            .then_with(|| self.waiting.cmp(&other.waiting))
            .then_with(|| self.active.cmp(&other.active))
    }

    fn has_session_headroom(self) -> bool {
        if self.waiting != 0 {
            return false;
        }
        self.limit
            .is_none_or(|limit| u64::from(self.active) * 4 < u64::from(limit.get()) * 3)
    }
}

/// 按优先级和权重生成请求内稳定、无放回的凭据顺序。
pub(crate) fn order_credentials<'a>(
    request_id: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    order_credentials_with_optional_health(request_id, channel_id, credentials, None)
}

/// 在原优先级与稳定抽签语义内过滤冷却凭据，并按半衰期惩罚修正正权重。
pub(crate) fn order_credentials_with_health<'a>(
    request_id: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
    health: &BTreeMap<i64, RuntimeHealthState>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    order_credentials_with_optional_health(request_id, channel_id, credentials, Some(health))
}

fn order_credentials_with_optional_health<'a>(
    request_id: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
    health: Option<&BTreeMap<i64, RuntimeHealthState>>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    let mut tiers = BTreeMap::<Reverse<i32>, Vec<&SchedulerRuntimeCredentialRecord>>::new();
    for credential in credentials.iter().filter(|credential| {
        !health
            .and_then(|states| states.get(&credential.credential_id()))
            .copied()
            .is_some_and(RuntimeHealthState::is_cooling)
    }) {
        tiers
            .entry(Reverse(credential.priority()))
            .or_default()
            .push(credential);
    }

    let mut ordered = Vec::with_capacity(credentials.len());
    for (Reverse(priority), tier) in tiers {
        ordered.extend(order_tier(request_id, channel_id, priority, tier, health));
    }
    ordered
}

/// 在优先级不变的前提下，把普通回退凭据按实时负载稳定重排。
///
/// 相同负载保持请求级加权抽签顺序，使配置权重仍能决定同负载账号的首选概率。
pub(crate) fn order_credentials_by_load<'a>(
    request_id: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
    loads: &BTreeMap<i64, CredentialLoad>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    order_credentials_by_load_with_optional_health(
        request_id,
        channel_id,
        credentials,
        loads,
        None,
        false,
    )
}

/// 在普通回退负载排序中应用凭据冷却与健康调权；同负载继续保持健康加权顺序。
pub(crate) fn order_credentials_by_load_and_health<'a>(
    request_id: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
    loads: &BTreeMap<i64, CredentialLoad>,
    health: &BTreeMap<i64, RuntimeHealthState>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    order_credentials_by_load_with_optional_health(
        request_id,
        channel_id,
        credentials,
        loads,
        Some(health),
        false,
    )
}

/// 有稳定会话标识时保留健康账号的首选位置，减少连续对话的上游缓存失效。
pub(crate) fn order_credentials_by_load_with_session_affinity<'a>(
    session_scope: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
    loads: &BTreeMap<i64, CredentialLoad>,
    health: Option<&BTreeMap<i64, RuntimeHealthState>>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    order_credentials_by_load_with_optional_health(
        session_scope,
        channel_id,
        credentials,
        loads,
        health,
        true,
    )
}

fn order_credentials_by_load_with_optional_health<'a>(
    request_id: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
    loads: &BTreeMap<i64, CredentialLoad>,
    health: Option<&BTreeMap<i64, RuntimeHealthState>>,
    session_affinity: bool,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    let mut ordered =
        order_credentials_with_optional_health(request_id, channel_id, credentials, health);
    let preferred = session_affinity
        .then(|| ordered.first().copied())
        .flatten()
        .filter(|credential| {
            credential_load(credential, loads).has_session_headroom()
                && health
                    .and_then(|states| states.get(&credential.credential_id()))
                    .is_none_or(|state| state.penalty_micros() <= HEALTH_PENALTY_SCALE)
        });
    ordered.sort_by(|left, right| {
        right.priority().cmp(&left.priority()).then_with(|| {
            credential_load(left, loads).compare_utilization(credential_load(right, loads))
        })
    });
    if let Some(preferred) = preferred
        && let Some(index) = ordered
            .iter()
            .position(|credential| credential.credential_id() == preferred.credential_id())
    {
        ordered.remove(index);
        ordered.insert(0, preferred);
    }
    ordered
}

fn credential_load(
    credential: &SchedulerRuntimeCredentialRecord,
    loads: &BTreeMap<i64, CredentialLoad>,
) -> CredentialLoad {
    loads
        .get(&credential.concurrency_owner_id().get())
        .copied()
        .unwrap_or(CredentialLoad::new(0, 0, credential.concurrency()))
}

fn order_tier<'a>(
    request_id: &str,
    channel_id: ChannelId,
    priority: i32,
    mut tier: Vec<&'a SchedulerRuntimeCredentialRecord>,
    health: Option<&BTreeMap<i64, RuntimeHealthState>>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    tier.sort_unstable_by_key(|credential| credential.credential_id());
    let (weighted, mut zero_weight): (Vec<_>, Vec<_>) = tier
        .into_iter()
        .partition(|credential| credential.weight() > 0);
    let mut weighted = weighted
        .into_iter()
        .map(|credential| WeightedCredential {
            credential,
            effective_weight: effective_credential_weight(credential, health),
        })
        .collect::<Vec<_>>();
    let mut ordered = Vec::with_capacity(weighted.len() + zero_weight.len());
    let mut draw = 0_u32;

    while !weighted.is_empty() {
        let total_weight = weighted
            .iter()
            .map(|credential| credential.effective_weight)
            .sum::<u64>();
        let ticket = unbiased_ticket(
            request_id,
            channel_id,
            priority,
            draw,
            total_weight,
            &weighted,
        );
        let mut cumulative = 0_u64;
        let selected = weighted
            .iter()
            .position(|credential| {
                cumulative += credential.effective_weight;
                ticket < cumulative
            })
            .expect("正权重凭据总和必须覆盖抽签区间");
        ordered.push(weighted.remove(selected).credential);
        draw = draw.checked_add(1).expect("单渠道凭据数量受 64 条上限保护");
    }

    // 零权重凭据不参与首选概率，但在正权重成员耗尽后仍可作为故障转移兜底。
    zero_weight.sort_unstable_by(|left, right| {
        zero_weight_key(request_id, channel_id, priority, left.credential_id())
            .cmp(&zero_weight_key(
                request_id,
                channel_id,
                priority,
                right.credential_id(),
            ))
            .then_with(|| left.credential_id().cmp(&right.credential_id()))
    });
    ordered.extend(zero_weight);
    ordered
}

fn unbiased_ticket(
    request_id: &str,
    channel_id: ChannelId,
    priority: i32,
    draw: u32,
    upper_bound: u64,
    candidates: &[WeightedCredential<'_>],
) -> u64 {
    debug_assert!(upper_bound > 0);
    let rejection_threshold = upper_bound.wrapping_neg() % upper_bound;
    let mut nonce = 0_u32;
    loop {
        let mut hasher = Sha256::new();
        hasher.update(ORDER_DOMAIN);
        hash_request_scope(&mut hasher, request_id, channel_id, priority);
        hasher.update(draw.to_be_bytes());
        hasher.update(nonce.to_be_bytes());
        for credential in candidates {
            hasher.update(credential.credential.credential_id().to_be_bytes());
            hasher.update(credential.effective_weight.to_be_bytes());
        }
        let digest = hasher.finalize();
        let sample = u64::from_be_bytes(digest[..8].try_into().expect("SHA-256 前缀固定为 8 字节"));
        if sample >= rejection_threshold {
            return sample % upper_bound;
        }
        nonce = nonce
            .checked_add(1)
            .expect("SHA-256 拒绝采样重试次数不可耗尽");
    }
}

fn effective_credential_weight(
    credential: &SchedulerRuntimeCredentialRecord,
    health: Option<&BTreeMap<i64, RuntimeHealthState>>,
) -> u64 {
    let configured = u64::from(credential.weight());
    let Some(state) = health.and_then(|states| states.get(&credential.credential_id()).copied())
    else {
        return configured;
    };
    let penalty = state.penalty_micros().min(64 * HEALTH_PENALTY_SCALE);
    let raw_multiplier = HEALTH_PENALTY_SCALE
        .checked_mul(HEALTH_PENALTY_SCALE)
        .expect("健康固定精度平方必须可表示")
        / (HEALTH_PENALTY_SCALE + penalty);
    let multiplier = raw_multiplier.max(MIN_HEALTH_MULTIPLIER_MICROS);
    configured
        .checked_mul(multiplier)
        .expect("凭据权重与健康乘数乘积必须可表示")
        .div_ceil(HEALTH_PENALTY_SCALE)
        .max(1)
}

fn zero_weight_key(
    request_id: &str,
    channel_id: ChannelId,
    priority: i32,
    credential_id: i64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(ZERO_WEIGHT_DOMAIN);
    hash_request_scope(&mut hasher, request_id, channel_id, priority);
    hasher.update(credential_id.to_be_bytes());
    hasher.finalize().into()
}

fn hash_request_scope(hasher: &mut Sha256, request_id: &str, channel_id: ChannelId, priority: i32) {
    hasher.update(
        u64::try_from(request_id.len())
            .expect("字符串长度必须可表示为 u64")
            .to_be_bytes(),
    );
    hasher.update(request_id.as_bytes());
    hasher.update(channel_id.get().to_be_bytes());
    hasher.update(priority.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use af_cache::RedisHealthTarget;
    use af_db::EncryptedCredentialEnvelope;
    use af_domain::{CredentialId, CredentialKind};

    use super::*;

    #[test]
    fn priority_precedes_weight_and_zero_weight_stays_as_fallback() {
        let credentials = vec![
            credential(1, 1, 1),
            credential(2, 0, u32::MAX),
            credential(3, 1, 0),
        ];

        let ordered = order_credentials("request-priority", channel_id(), &credentials);
        let ids = ordered
            .iter()
            .map(|credential| credential.credential_id())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![1, 3, 2]);
    }

    #[test]
    fn identical_request_produces_identical_complete_order() {
        let credentials = vec![
            credential(11, 0, 10),
            credential(12, 0, 20),
            credential(13, 0, 30),
        ];

        let first = credential_ids(order_credentials(
            "stable-request",
            channel_id(),
            &credentials,
        ));
        let second = credential_ids(order_credentials(
            "stable-request",
            channel_id(),
            &credentials,
        ));

        assert_eq!(first, second);
        assert_eq!(first.len(), credentials.len());
        first
            .iter()
            .for_each(|id| assert_eq!(first.iter().filter(|item| *item == id).count(), 1));
    }

    #[test]
    fn first_choice_tracks_configured_weight_ratio() {
        let credentials = vec![credential(21, 0, 1), credential(22, 0, 3)];
        let heavier_first = (0..4_096)
            .filter(|request| {
                order_credentials(&format!("weighted-{request}"), channel_id(), &credentials)[0]
                    .credential_id()
                    == 22
            })
            .count();

        assert!((2_850..=3_300).contains(&heavier_first));
    }

    #[test]
    fn fallback_order_prefers_lower_utilization_and_waiting_count() {
        let credentials = vec![
            credential_with_limit(31, 0, 1, Some(10)),
            credential_with_limit(32, 0, 1, Some(2)),
            credential_with_limit(33, 0, 1, None),
        ];
        let loads = BTreeMap::from([
            (31, load(2, 0, Some(10))),
            (32, load(0, 1, Some(2))),
            (33, load(1, 0, None)),
        ]);

        assert_eq!(
            credential_ids(order_credentials_by_load(
                "load-aware",
                channel_id(),
                &credentials,
                &loads,
            )),
            vec![31, 32, 33]
        );
    }

    #[test]
    fn fallback_order_never_crosses_priority_and_preserves_weighted_ties() {
        let credentials = vec![
            credential_with_limit(41, 0, 1, Some(1)),
            credential_with_limit(42, 0, 3, Some(1)),
            credential_with_limit(43, 1, 1, Some(1)),
        ];
        let loads = BTreeMap::from([
            (41, load(0, 0, Some(1))),
            (42, load(0, 0, Some(1))),
            (43, load(1, 20, Some(1))),
        ]);
        let weighted = credential_ids(order_credentials("load-tie", channel_id(), &credentials));
        let load_aware = credential_ids(order_credentials_by_load(
            "load-tie",
            channel_id(),
            &credentials,
            &loads,
        ));

        assert_eq!(load_aware[0], 43);
        assert_eq!(&load_aware[1..], &weighted[1..]);
    }

    #[test]
    fn session_affinity_keeps_healthy_account_but_yields_to_congestion() {
        let credentials = vec![
            credential_with_limit(61, 0, 1, Some(8)),
            credential_with_limit(62, 0, 1, Some(8)),
        ];
        let scope = (0..100)
            .map(|index| format!("session-{index}"))
            .find(|scope| {
                order_credentials(scope, channel_id(), &credentials)[0].credential_id() == 61
            })
            .unwrap();
        let light_load = BTreeMap::from([(61, load(3, 0, Some(8))), (62, load(0, 0, Some(8)))]);
        assert_eq!(
            credential_ids(order_credentials_by_load_with_session_affinity(
                &scope,
                channel_id(),
                &credentials,
                &light_load,
                None,
            )),
            vec![61, 62]
        );
        let congested = BTreeMap::from([(61, load(6, 0, Some(8))), (62, load(0, 0, Some(8)))]);
        assert_eq!(
            credential_ids(order_credentials_by_load_with_session_affinity(
                &scope,
                channel_id(),
                &credentials,
                &congested,
                None,
            )),
            vec![62, 61]
        );
    }

    #[test]
    fn session_affinity_does_not_revive_cooling_account() {
        let credentials = vec![
            credential_with_limit(71, 0, 1, Some(8)),
            credential_with_limit(72, 0, 1, Some(8)),
        ];
        let health = BTreeMap::from([(
            71,
            RuntimeHealthState::new(
                RedisHealthTarget::Credential(CredentialId::new(71).unwrap()),
                0,
                true,
            ),
        )]);
        let loads = BTreeMap::new();
        assert_eq!(
            credential_ids(order_credentials_by_load_with_session_affinity(
                "cooling-session",
                channel_id(),
                &credentials,
                &loads,
                Some(&health),
            )),
            vec![72]
        );
    }

    #[test]
    fn session_affinity_yields_when_preferred_account_is_degraded() {
        let credentials = vec![
            credential_with_limit(81, 0, 1, Some(8)),
            credential_with_limit(82, 0, 1, Some(8)),
        ];
        let health = BTreeMap::from([(
            81,
            RuntimeHealthState::new(
                RedisHealthTarget::Credential(CredentialId::new(81).unwrap()),
                HEALTH_PENALTY_SCALE * 2,
                false,
            ),
        )]);
        let scope = (0..100)
            .map(|index| format!("degraded-session-{index}"))
            .find(|scope| {
                order_credentials_with_health(scope, channel_id(), &credentials, &health)[0]
                    .credential_id()
                    == 81
            })
            .unwrap();
        let loads = BTreeMap::from([(81, load(3, 0, Some(8))), (82, load(0, 0, Some(8)))]);
        assert_eq!(
            credential_ids(order_credentials_by_load_with_session_affinity(
                &scope,
                channel_id(),
                &credentials,
                &loads,
                Some(&health),
            )),
            vec![82, 81]
        );
    }

    #[test]
    fn health_filters_cooling_credential_and_reduces_positive_weight() {
        let credentials = vec![credential(51, 0, 100), credential(52, 0, 100)];
        let health = BTreeMap::from([
            (
                51,
                RuntimeHealthState::new(
                    RedisHealthTarget::Credential(CredentialId::new(51).unwrap()),
                    0,
                    true,
                ),
            ),
            (
                52,
                RuntimeHealthState::new(
                    RedisHealthTarget::Credential(CredentialId::new(52).unwrap()),
                    HEALTH_PENALTY_SCALE,
                    false,
                ),
            ),
        ]);

        let ordered =
            order_credentials_with_health("health-filter", channel_id(), &credentials, &health);
        assert_eq!(credential_ids(ordered), vec![52]);
        assert_eq!(
            effective_credential_weight(&credentials[1], Some(&health)),
            50
        );
    }

    fn credential_ids(credentials: Vec<&SchedulerRuntimeCredentialRecord>) -> Vec<i64> {
        credentials
            .into_iter()
            .map(SchedulerRuntimeCredentialRecord::credential_id)
            .collect()
    }

    fn credential(
        credential_id: i64,
        priority: i32,
        weight: u32,
    ) -> SchedulerRuntimeCredentialRecord {
        SchedulerRuntimeCredentialRecord::with_scheduling(
            credential_id,
            CredentialKind::ApiKey,
            EncryptedCredentialEnvelope::new("order-test", [0x42; 24], vec![0x24; 16]).unwrap(),
            false,
            priority,
            weight,
        )
        .unwrap()
    }

    fn credential_with_limit(
        credential_id: i64,
        priority: i32,
        weight: u32,
        limit: Option<u32>,
    ) -> SchedulerRuntimeCredentialRecord {
        SchedulerRuntimeCredentialRecord::with_scheduling_and_concurrency(
            credential_id,
            CredentialKind::ApiKey,
            EncryptedCredentialEnvelope::new("order-test", [0x42; 24], vec![0x24; 16]).unwrap(),
            false,
            priority,
            weight,
            limit.map(|value| ConcurrencyLimit::new(value).unwrap()),
        )
        .unwrap()
    }

    fn load(active: u32, waiting: u32, limit: Option<u32>) -> CredentialLoad {
        CredentialLoad::new(
            active,
            waiting,
            limit.map(|value| ConcurrencyLimit::new(value).unwrap()),
        )
    }

    fn channel_id() -> ChannelId {
        ChannelId::new(7).unwrap()
    }
}
