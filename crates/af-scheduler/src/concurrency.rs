use std::time::Duration;

use af_domain::ConcurrencyLimit;

/// 首次重试槽位的基础退避。
pub const INITIAL_CONCURRENCY_WAIT_BACKOFF: Duration = Duration::from_millis(100);
/// 单次槽位重试最多等待两秒，避免低频请求长期沉睡。
pub const MAX_CONCURRENCY_WAIT_BACKOFF: Duration = Duration::from_secs(2);
/// 每个受限层级允许额外登记的等待请求数。
pub const CONCURRENCY_EXTRA_WAIT_SLOTS: u32 = 20;

/// 指数退避与有界抖动状态，降低多个等待者同时冲击 Redis 的概率。
#[derive(Clone, Copy, Debug)]
pub struct ConcurrencyWaitBackoff {
    current: Duration,
}

impl ConcurrencyWaitBackoff {
    /// 从一百毫秒基础间隔开始新的等待循环。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: INITIAL_CONCURRENCY_WAIT_BACKOFF,
        }
    }

    /// 返回不超过剩余截止时间的下一次延迟，并把基础值按 3/2 增长到两秒上限。
    pub fn next_delay(&mut self, remaining: Duration) -> Duration {
        let base = self.current;
        self.current = base
            .checked_mul(3)
            .and_then(|value| value.checked_div(2))
            .unwrap_or(MAX_CONCURRENCY_WAIT_BACKOFF)
            .min(MAX_CONCURRENCY_WAIT_BACKOFF);
        jitter(base).min(remaining)
    }
}

impl Default for ConcurrencyWaitBackoff {
    fn default() -> Self {
        Self::new()
    }
}

/// 按限制值增加固定等待余量，溢出时封顶到 `u32::MAX`。
#[must_use]
pub fn concurrency_wait_queue_limit(limit: ConcurrencyLimit) -> ConcurrencyLimit {
    ConcurrencyLimit::new(limit.get().saturating_add(CONCURRENCY_EXTRA_WAIT_SLOTS))
        .expect("正整数并发限制增加等待余量后仍必须为正")
}

fn jitter(base: Duration) -> Duration {
    let mut random = [0_u8; 2];
    if getrandom::fill(&mut random).is_err() {
        return base;
    }
    // 使用 80% 到 120% 的整数抖动，避免浮点参与热路径时长计算。
    let percent = 80_u32 + u32::from(u16::from_be_bytes(random)) % 41;
    base.checked_mul(percent)
        .and_then(|value| value.checked_div(100))
        .unwrap_or(base)
        .clamp(
            INITIAL_CONCURRENCY_WAIT_BACKOFF,
            MAX_CONCURRENCY_WAIT_BACKOFF,
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_is_bounded_and_respects_deadline() {
        let mut backoff = ConcurrencyWaitBackoff::new();
        for _ in 0..32 {
            let delay = backoff.next_delay(Duration::from_secs(10));
            assert!(delay >= INITIAL_CONCURRENCY_WAIT_BACKOFF);
            assert!(delay <= MAX_CONCURRENCY_WAIT_BACKOFF);
        }
        assert_eq!(
            backoff.next_delay(Duration::from_millis(17)),
            Duration::from_millis(17)
        );
    }

    #[test]
    fn wait_queue_limit_adds_fixed_capacity_and_saturates() {
        assert_eq!(
            concurrency_wait_queue_limit(ConcurrencyLimit::new(3).unwrap()).get(),
            23
        );
        assert_eq!(
            concurrency_wait_queue_limit(ConcurrencyLimit::new(u32::MAX).unwrap()).get(),
            u32::MAX
        );
    }
}
