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
    successes_micros: u64,
    failures_micros: u64,
    latency_ema_millis: u64,
    recently_failed: bool,
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
            successes_micros: 0,
            failures_micros: 0,
            latency_ema_millis: 0,
            recently_failed: false,
        }
    }

    /// 同一维度的衰减成功/失败样本和延迟；旧状态无样本时保持中性。
    #[must_use]
    pub fn with_quality(
        mut self,
        successes_micros: u64,
        failures_micros: u64,
        latency_ema_millis: u64,
    ) -> Self {
        self.successes_micros = successes_micros.min(1_000_000_000_000);
        self.failures_micros = failures_micros.min(1_000_000_000_000);
        self.latency_ema_millis = latency_ema_millis.min(3_600_000);
        self
    }

    /// 普通瞬时故障的短暂软避让；它与不可绕过的熔断分开。
    #[must_use]
    pub const fn with_recent_failure(mut self, recently_failed: bool) -> Self {
        self.recently_failed = recently_failed;
        self
    }

    #[must_use]
    pub const fn is_recently_failed(self) -> bool {
        self.recently_failed
    }

    /// 返回当前渠道是否应在 weighted 抽样前被过滤。
    #[must_use]
    pub const fn is_cooling(self) -> bool {
        self.cooling
    }

    /// 返回不含候选配置权重的健康乘数，使用与加权选路相同的定点精度。
    #[must_use]
    pub fn multiplier_micros(self) -> u64 {
        let denominator = ROUTING_HEALTH_PENALTY_SCALE + self.penalty_micros;
        let raw_multiplier = ROUTING_HEALTH_PENALTY_SCALE
            .checked_mul(ROUTING_HEALTH_PENALTY_SCALE)
            .expect("健康固定精度平方必须可表示")
            / denominator;
        let sample_count = self.successes_micros + self.failures_micros;
        // Beta(1,1) 先验只用于估计失败率；12个有效样本前按置信度渐进应用。
        let confidence = sample_count.min(12 * ROUTING_HEALTH_PENALTY_SCALE);
        let failure_rate = u64::try_from(
            u128::from(self.failures_micros + ROUTING_HEALTH_PENALTY_SCALE)
                * u128::from(ROUTING_HEALTH_PENALTY_SCALE)
                / u128::from(sample_count + 2 * ROUTING_HEALTH_PENALTY_SCALE),
        )
        .expect("失败率限制在固定精度内");
        let quality_penalty = u128::from(failure_rate) * u128::from(confidence) * 550_000
            / u128::from(12 * ROUTING_HEALTH_PENALTY_SCALE)
            / u128::from(ROUTING_HEALTH_PENALTY_SCALE);
        let quality_multiplier = ROUTING_HEALTH_PENALTY_SCALE
            - u64::try_from(quality_penalty).expect("质量惩罚限制在固定精度内");
        let latency_excess = self.latency_ema_millis.saturating_sub(2_500).min(30_000);
        let latency_multiplier = ROUTING_HEALTH_PENALTY_SCALE - latency_excess * 350_000 / 30_000;
        let combined = u128::from(raw_multiplier)
            * u128::from(quality_multiplier)
            * u128::from(latency_multiplier)
            / u128::from(ROUTING_HEALTH_PENALTY_SCALE).pow(2);
        u64::try_from(combined)
            .expect("组合倍率限制在固定精度内")
            .max(MIN_HEALTH_MULTIPLIER_MICROS)
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
    fn quality_is_neutral_without_samples_and_recovers_with_successes() {
        let empty = ChannelRoutingHealth::new(0, false);
        assert_eq!(empty.multiplier_micros(), ROUTING_HEALTH_PENALTY_SCALE);
        let failed = empty.with_quality(0, 12_000_000, 0).multiplier_micros();
        let recovering = empty
            .with_quality(12_000_000, 12_000_000, 0)
            .multiplier_micros();
        let healthy = empty.with_quality(24_000_000, 0, 0).multiplier_micros();
        assert!(failed < recovering && recovering < healthy);
        assert!(empty.with_quality(0, 1_000_000, 0).multiplier_micros() > failed);
    }

    #[test]
    fn slow_successes_reduce_weight_but_never_exhaust_all_tickets() {
        let candidate =
            SchedulerCandidate::new(GroupId::new(1).unwrap(), ChannelId::new(2).unwrap(), 0, 90);
        let baseline = ChannelRoutingHealth::new(0, false).with_quality(12_000_000, 0, 2_500);
        let slow = ChannelRoutingHealth::new(0, false).with_quality(12_000_000, 0, 32_500);
        assert!(slow.effective_weight(candidate) < baseline.effective_weight(candidate));
        assert_eq!(
            ChannelRoutingHealth::new(u64::MAX, false)
                .with_quality(u64::MAX, u64::MAX, u64::MAX)
                .effective_weight(candidate),
            8
        );
    }

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
