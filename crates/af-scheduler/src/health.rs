use af_domain::ChannelId;

use crate::{SchedulerCandidate, WEIGHT_BASE};

/// 调度层接收的惩罚分固定精度，与 Redis 健康状态的百万分之一单位一致。
pub const ROUTING_HEALTH_PENALTY_SCALE: u64 = 1_000_000;
const MIN_HEALTH_MULTIPLIER_MICROS: u64 = 80_000;
const MAX_ROUTING_PENALTY_MICROS: u64 = 64 * ROUTING_HEALTH_PENALTY_SCALE;

/// 单个渠道在一次路由规划中固定的衰减惩罚与熔断状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelRoutingHealth {
    penalty_micros: u64,
    cooling: bool,
}

impl ChannelRoutingHealth {
    /// 构造已由 Redis 服务端时间归一化的健康状态；异常大分值按存储硬上限收敛。
    #[must_use]
    pub const fn new(penalty_micros: u64, cooling: bool) -> Self {
        Self {
            penalty_micros: if penalty_micros > MAX_ROUTING_PENALTY_MICROS {
                MAX_ROUTING_PENALTY_MICROS
            } else {
                penalty_micros
            },
            cooling,
        }
    }

    /// 返回当前渠道是否应在 weighted 抽样前被过滤。
    #[must_use]
    pub const fn is_cooling(self) -> bool {
        self.cooling
    }

    /// 返回不含候选配置权重的健康乘数，使用与加权选路相同的定点精度。
    pub(crate) fn multiplier_micros(self) -> u64 {
        let denominator = ROUTING_HEALTH_PENALTY_SCALE + self.penalty_micros;
        let raw_multiplier = ROUTING_HEALTH_PENALTY_SCALE
            .checked_mul(ROUTING_HEALTH_PENALTY_SCALE)
            .expect("健康固定精度平方必须可表示")
            / denominator;
        raw_multiplier.max(MIN_HEALTH_MULTIPLIER_MICROS)
    }

    /// 将 `weight + 10` 乘以健康乘数，并保留最小抽样票。
    #[must_use]
    pub(crate) fn effective_weight(self, candidate: SchedulerCandidate) -> u64 {
        let multiplier = self.multiplier_micros();
        let base = u64::from(candidate.weight()) + WEIGHT_BASE;
        base.checked_mul(multiplier)
            .expect("配置权重与健康乘数乘积必须可表示")
            .div_ceil(ROUTING_HEALTH_PENALTY_SCALE)
            .max(1)
    }
}

pub(crate) fn effective_candidate_weight(
    candidate: SchedulerCandidate,
    health: Option<ChannelRoutingHealth>,
) -> u64 {
    health.map_or_else(
        || u64::from(candidate.weight()) + WEIGHT_BASE,
        |state| state.effective_weight(candidate),
    )
}

pub(crate) fn healthy_channel(
    channel_id: ChannelId,
    health: &std::collections::BTreeMap<ChannelId, ChannelRoutingHealth>,
) -> bool {
    !health
        .get(&channel_id)
        .copied()
        .is_some_and(ChannelRoutingHealth::is_cooling)
}

#[cfg(test)]
mod tests {
    use af_domain::GroupId;

    use super::*;

    #[test]
    fn penalty_uses_metapi_multiplier_and_keeps_eight_percent_floor() {
        let candidate =
            SchedulerCandidate::new(GroupId::new(1).unwrap(), ChannelId::new(2).unwrap(), 0, 90);

        assert_eq!(
            ChannelRoutingHealth::new(0, false).effective_weight(candidate),
            100
        );
        assert_eq!(
            ChannelRoutingHealth::new(ROUTING_HEALTH_PENALTY_SCALE, false)
                .effective_weight(candidate),
            50
        );
        assert_eq!(
            ChannelRoutingHealth::new(MAX_ROUTING_PENALTY_MICROS, false)
                .effective_weight(candidate),
            8
        );
    }
}
