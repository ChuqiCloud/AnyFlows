//! 定价、额度核算与计费会话编排。

mod batch;
mod billing_session;
mod database_sink;
mod model_price_cache;
mod pricing;
pub mod quota_math;

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
pub use model_price_cache::{
    ModelPrice, ModelPriceCache, ModelPriceCacheError, ModelPriceSnapshot, ModelPriceSource,
    ModelPriceSourceError, ModelPriceSourceFuture, ModelPriceSourceRecord,
};
pub use pricing::{
    BillingMode, CostBreakdown, PriceData, PricingContext, PricingError, PricingRatio,
    PricingRatios, PricingResolver, RatioPricingResolver, TokenPrices,
};

#[cfg(test)]
mod batch_tests;
#[cfg(test)]
mod billing_invariants_tests;
#[cfg(test)]
mod billing_session_tests;
#[cfg(test)]
mod model_price_cache_tests;
#[cfg(test)]
mod pricing_tests;
#[cfg(test)]
mod quota_math_tests;
