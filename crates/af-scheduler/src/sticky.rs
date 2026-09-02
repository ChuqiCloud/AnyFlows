use std::time::Duration;

use af_domain::ChannelId;
use thiserror::Error;

/// 粘性命中与普通回退候选共用的等待边界配置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StickyWaitPolicy {
    sticky_wait_timeout: Duration,
    fallback_wait_timeout: Duration,
    clear_unavailable_binding: bool,
}

impl StickyWaitPolicy {
    /// 创建等待策略；粘性等待必须严格短于普通回退等待。
    pub fn new(
        sticky_wait_timeout: Duration,
        fallback_wait_timeout: Duration,
    ) -> Result<Self, StickyWaitPolicyError> {
        if sticky_wait_timeout.is_zero() {
            return Err(StickyWaitPolicyError::ZeroStickyTimeout);
        }
        if fallback_wait_timeout.is_zero() {
            return Err(StickyWaitPolicyError::ZeroFallbackTimeout);
        }
        if sticky_wait_timeout >= fallback_wait_timeout {
            return Err(StickyWaitPolicyError::StickyNotShorter);
        }
        Ok(Self {
            sticky_wait_timeout,
            fallback_wait_timeout,
            clear_unavailable_binding: true,
        })
    }

    /// 覆盖渠道失效时是否条件删除旧粘性绑定。
    #[must_use]
    pub const fn with_clear_unavailable_binding(mut self, enabled: bool) -> Self {
        self.clear_unavailable_binding = enabled;
        self
    }

    /// 返回粘性候选等待上限。
    #[must_use]
    pub const fn sticky_wait_timeout(self) -> Duration {
        self.sticky_wait_timeout
    }

    /// 返回普通候选等待上限。
    #[must_use]
    pub const fn fallback_wait_timeout(self) -> Duration {
        self.fallback_wait_timeout
    }

    /// 返回是否清理当前快照中已不可用的旧绑定。
    #[must_use]
    pub const fn clear_unavailable_binding(self) -> bool {
        self.clear_unavailable_binding
    }

    pub(crate) const fn sticky_plan(self, channel_id: ChannelId) -> RouteWaitPlan {
        RouteWaitPlan {
            channel_id,
            kind: RouteWaitKind::Sticky,
            timeout: self.sticky_wait_timeout,
        }
    }

    pub(crate) const fn fallback_plan(self, channel_id: ChannelId) -> RouteWaitPlan {
        RouteWaitPlan {
            channel_id,
            kind: RouteWaitKind::Fallback,
            timeout: self.fallback_wait_timeout,
        }
    }
}

impl Default for StickyWaitPolicy {
    fn default() -> Self {
        Self {
            sticky_wait_timeout: Duration::from_secs(2),
            fallback_wait_timeout: Duration::from_secs(30),
            clear_unavailable_binding: true,
        }
    }
}

/// 等待策略参数不满足退让不变量时的错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum StickyWaitPolicyError {
    /// 粘性等待不能为零。
    #[error("粘性等待超时必须大于零")]
    ZeroStickyTimeout,
    /// 普通回退等待不能为零。
    #[error("回退等待超时必须大于零")]
    ZeroFallbackTimeout,
    /// 粘性等待必须严格短于普通回退等待。
    #[error("粘性等待超时必须短于回退等待超时")]
    StickyNotShorter,
}

/// 当前路由候选对应的等待阶段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RouteWaitKind {
    /// 已命中 Redis 粘性绑定的候选。
    Sticky,
    /// 未命中或粘性等待退让后的普通候选。
    Fallback,
}

/// 供并发槽位层消费的候选等待契约。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteWaitPlan {
    channel_id: ChannelId,
    kind: RouteWaitKind,
    timeout: Duration,
}

impl RouteWaitPlan {
    /// 返回等待契约所属渠道。
    #[must_use]
    pub const fn channel_id(self) -> ChannelId {
        self.channel_id
    }

    /// 返回候选等待阶段。
    #[must_use]
    pub const fn kind(self) -> RouteWaitKind {
        self.kind
    }

    /// 返回该阶段的等待上限。
    #[must_use]
    pub const fn timeout(self) -> Duration {
        self.timeout
    }
}

/// 应用粘性渠道优先级时的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StickyRouteOutcome {
    /// 粘性渠道仍在当前快照候选集中，已被移动到首位。
    Applied,
    /// 当前快照没有该渠道，调用方可按策略条件删除旧绑定。
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_policy_rejects_zero_equal_and_reversed_timeouts() {
        assert_eq!(
            StickyWaitPolicy::new(Duration::ZERO, Duration::from_secs(1)),
            Err(StickyWaitPolicyError::ZeroStickyTimeout)
        );
        assert_eq!(
            StickyWaitPolicy::new(Duration::from_secs(1), Duration::ZERO),
            Err(StickyWaitPolicyError::ZeroFallbackTimeout)
        );
        assert_eq!(
            StickyWaitPolicy::new(Duration::from_secs(1), Duration::from_secs(1)),
            Err(StickyWaitPolicyError::StickyNotShorter)
        );
        assert_eq!(
            StickyWaitPolicy::new(Duration::from_secs(2), Duration::from_secs(1)),
            Err(StickyWaitPolicyError::StickyNotShorter)
        );
    }

    #[test]
    fn default_policy_keeps_short_sticky_wait_and_clears_unavailable_binding() {
        let policy = StickyWaitPolicy::default();

        assert_eq!(policy.sticky_wait_timeout(), Duration::from_secs(2));
        assert_eq!(policy.fallback_wait_timeout(), Duration::from_secs(30));
        assert!(policy.clear_unavailable_binding());
        assert!(
            !policy
                .with_clear_unavailable_binding(false)
                .clear_unavailable_binding()
        );
    }

    #[test]
    fn wait_plans_preserve_channel_and_stage_specific_timeout() {
        let channel_id = ChannelId::new(42).expect("测试渠道标识必须为正");
        let policy = StickyWaitPolicy::new(Duration::from_secs(1), Duration::from_secs(8))
            .expect("测试等待策略必须满足严格退让关系");

        let sticky = policy.sticky_plan(channel_id);
        assert_eq!(sticky.channel_id(), channel_id);
        assert_eq!(sticky.kind(), RouteWaitKind::Sticky);
        assert_eq!(sticky.timeout(), Duration::from_secs(1));

        let fallback = policy.fallback_plan(channel_id);
        assert_eq!(fallback.channel_id(), channel_id);
        assert_eq!(fallback.kind(), RouteWaitKind::Fallback);
        assert_eq!(fallback.timeout(), Duration::from_secs(8));
    }
}
