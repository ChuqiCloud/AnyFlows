use std::fmt;

use af_domain::{BillingReservationId, GatewayPrincipal, Quota};
use af_protocol::{AudioDuration, Usage};

use crate::BillingMode;

/// 请求完成后随 token usage 一并持久化的可选多模态事实。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BillingUsageDimensions {
    audio_duration: Option<AudioDuration>,
}

impl BillingUsageDimensions {
    /// 返回不携带多模态事实的默认维度集合。
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            audio_duration: None,
        }
    }

    /// 构造只在存在真实或本地探测事实时携带音频时长的维度集合。
    #[must_use]
    pub const fn with_audio_duration(audio_duration: AudioDuration) -> Self {
        Self {
            audio_duration: Some(audio_duration),
        }
    }

    /// 返回可选的音频时长；缺失时不得由存储层伪造零值。
    #[must_use]
    pub const fn audio_duration(self) -> Option<AudioDuration> {
        self.audio_duration
    }
}

/// 一次请求完成后交给用量记录边界的最小计费事实。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BillingUsageRecord {
    event_id: BillingReservationId,
    principal: GatewayPrincipal,
    usage: Usage,
    dimensions: BillingUsageDimensions,
    billing_mode: BillingMode,
    quota: Quota,
}

impl BillingUsageRecord {
    pub(crate) const fn new(
        event_id: BillingReservationId,
        principal: GatewayPrincipal,
        usage: Usage,
        billing_mode: BillingMode,
        quota: Quota,
    ) -> Self {
        Self::new_with_dimensions(
            event_id,
            principal,
            usage,
            BillingUsageDimensions::empty(),
            billing_mode,
            quota,
        )
    }

    pub(crate) const fn new_with_dimensions(
        event_id: BillingReservationId,
        principal: GatewayPrincipal,
        usage: Usage,
        dimensions: BillingUsageDimensions,
        billing_mode: BillingMode,
        quota: Quota,
    ) -> Self {
        Self {
            event_id,
            principal,
            usage,
            dimensions,
            billing_mode,
            quota,
        }
    }

    /// 返回用量存储重放必须复用的请求幂等标识。
    #[must_use]
    pub const fn event_id(self) -> BillingReservationId {
        self.event_id
    }

    /// 返回已认证的请求主体。
    #[must_use]
    pub const fn principal(self) -> GatewayPrincipal {
        self.principal
    }

    /// 返回本次请求的最终规范化用量。
    #[must_use]
    pub const fn usage(self) -> Usage {
        self.usage
    }

    /// 返回随本次请求冻结的可选多模态审计维度。
    #[must_use]
    pub const fn dimensions(self) -> BillingUsageDimensions {
        self.dimensions
    }

    /// 返回快照解析得到的最终计费模式。
    #[must_use]
    pub const fn billing_mode(self) -> BillingMode {
        self.billing_mode
    }

    /// 返回最终结算额度；显式免费与零成本按量都可能为零。
    #[must_use]
    pub const fn quota(self) -> Quota {
        self.quota
    }
}

impl fmt::Debug for BillingUsageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingUsageRecord")
            .field("billing_mode", &self.billing_mode)
            .finish_non_exhaustive()
    }
}

/// 已结算并已被用量记录端口接收的请求结果。
#[must_use = "计费完成结果应交给后续响应或审计编排"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BillingCompletion {
    record: BillingUsageRecord,
}

impl BillingCompletion {
    pub(super) const fn new(record: BillingUsageRecord) -> Self {
        Self { record }
    }

    /// 返回本次请求的最终规范化用量。
    #[must_use]
    pub const fn usage(self) -> Usage {
        self.record.usage()
    }

    /// 返回本次请求已成功入队的多模态审计维度。
    #[must_use]
    pub const fn dimensions(self) -> BillingUsageDimensions {
        self.record.dimensions()
    }

    /// 返回本次请求的最终计费模式。
    #[must_use]
    pub const fn billing_mode(self) -> BillingMode {
        self.record.billing_mode()
    }

    /// 返回本次请求的最终结算额度。
    #[must_use]
    pub const fn quota(self) -> Quota {
        self.record.quota()
    }

    /// 返回已经成功入队的用量记录。
    #[must_use]
    pub const fn usage_record(self) -> BillingUsageRecord {
        self.record
    }
}
