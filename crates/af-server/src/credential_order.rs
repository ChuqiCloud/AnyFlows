use std::{cmp::Reverse, collections::BTreeMap};

use af_db::SchedulerRuntimeCredentialRecord;
use af_domain::ChannelId;
use sha2::{Digest, Sha256};

const ORDER_DOMAIN: &[u8] = b"anyflows-credential-order-v1";
const ZERO_WEIGHT_DOMAIN: &[u8] = b"anyflows-credential-zero-weight-v1";

/// 按优先级和权重生成请求内稳定、无放回的凭据顺序。
pub(crate) fn order_credentials<'a>(
    request_id: &str,
    channel_id: ChannelId,
    credentials: &'a [SchedulerRuntimeCredentialRecord],
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    let mut tiers = BTreeMap::<Reverse<i32>, Vec<&SchedulerRuntimeCredentialRecord>>::new();
    for credential in credentials {
        tiers
            .entry(Reverse(credential.priority()))
            .or_default()
            .push(credential);
    }

    let mut ordered = Vec::with_capacity(credentials.len());
    for (Reverse(priority), tier) in tiers {
        ordered.extend(order_tier(request_id, channel_id, priority, tier));
    }
    ordered
}

fn order_tier<'a>(
    request_id: &str,
    channel_id: ChannelId,
    priority: i32,
    mut tier: Vec<&'a SchedulerRuntimeCredentialRecord>,
) -> Vec<&'a SchedulerRuntimeCredentialRecord> {
    tier.sort_unstable_by_key(|credential| credential.credential_id());
    let (mut weighted, mut zero_weight): (Vec<_>, Vec<_>) = tier
        .into_iter()
        .partition(|credential| credential.weight() > 0);
    let mut ordered = Vec::with_capacity(weighted.len() + zero_weight.len());
    let mut draw = 0_u32;

    while !weighted.is_empty() {
        let total_weight = weighted
            .iter()
            .map(|credential| u64::from(credential.weight()))
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
                cumulative += u64::from(credential.weight());
                ticket < cumulative
            })
            .expect("正权重凭据总和必须覆盖抽签区间");
        ordered.push(weighted.remove(selected));
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
    candidates: &[&SchedulerRuntimeCredentialRecord],
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
            hasher.update(credential.credential_id().to_be_bytes());
            hasher.update(credential.weight().to_be_bytes());
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
    use af_db::EncryptedCredentialEnvelope;
    use af_domain::CredentialKind;

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

        assert_eq!(ids[0], 2);
        assert_eq!(ids[2], 3);
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

    fn channel_id() -> ChannelId {
        ChannelId::new(7).unwrap()
    }
}
