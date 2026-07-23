//! 定价、额度核算与计费会话编排。

mod batch;
mod billing_session;
mod database_sink;
mod group_pricing_cache;
mod lifecycle;
mod model_price_cache;
mod pricing;
mod pricing_snapshot;
pub mod quota_math;
mod usage_record_queue;

pub use batch::{
    BillingBatch, BillingBatchApplyOutcome, BillingBatchError, BillingBatchEvent,
    BillingBatchFlushOutcome, BillingBatchRecordOutcome, BillingBatchSink, BillingBatchSinkError,
    BillingBatchSinkFuture, BillingWriterId, ChannelBillingDelta, FileBillingBatcher,
    TokenBillingDelta, UserBillingDelta,
};
pub use billing_session::{
    BillingSession, BillingSessionError, BillingSessionState, RefundSignalOutcome,
    RefundSignalPort, SettlementRequest,
};
pub use database_sink::DatabaseBillingBatchSink;
pub use group_pricing_cache::{
    GroupModelRatioSourceRecord, GroupPeakPricing, GroupPricingCache, GroupPricingCacheError,
    GroupPricingLookupError, GroupPricingSnapshot, GroupPricingSource, GroupPricingSourceCatalog,
    GroupPricingSourceError, GroupPricingSourceFuture, GroupPricingSourceRecord,
};
pub use lifecycle::{
    BillingCompletion, BillingLifecycleError, BillingLifecycleState, BillingPrechargeError,
    BillingPrechargeFuture, BillingPrechargePort, BillingRequestLifecycle, BillingRequestPlan,
    BillingSettlementError, BillingSettlementFuture, BillingSettlementPort, BillingUsageRecord,
    UsageRecordOutcome, UsageRecordPort,
};
pub use model_price_cache::{
    ModelPrice, ModelPriceCache, ModelPriceCacheError, ModelPriceSnapshot, ModelPriceSource,
    ModelPriceSourceError, ModelPriceSourceFuture, ModelPriceSourceRecord,
};
pub use pricing::{
    BillingMode, CostBreakdown, PriceData, PricingContext, PricingError, PricingRatio,
    PricingRatios, PricingResolver, RatioPricingResolver, TokenPrices,
};
pub use pricing_snapshot::{
    CachedRequestPricingSnapshotSource, RequestPricingSnapshot, RequestPricingSnapshotError,
    RequestPricingSnapshotSource,
};
pub use usage_record_queue::{
    UsageRecordDelivery, UsageRecordQueue, UsageRecordQueueError, UsageRecordReceiver,
};

#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod billing_invariants_tests;
#[cfg(test)]
mod billing_session_tests;
#[cfg(test)]
mod group_pricing_cache_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod model_price_cache_tests;
#[cfg(test)]
mod pricing_snapshot_tests;
#[cfg(test)]
mod pricing_tests;
#[cfg(test)]
mod quota_math_tests;
