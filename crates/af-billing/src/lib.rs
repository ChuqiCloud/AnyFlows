//! 定价、额度核算与计费会话编排。

mod billing_session;
mod pricing;
pub mod quota_math;

pub use billing_session::{
    BillingSession, BillingSessionError, BillingSessionState, RefundSignalOutcome,
    RefundSignalPort, SettlementRequest,
};
pub use pricing::{
    BillingMode, CostBreakdown, PriceData, PricingContext, PricingError, PricingRatio,
    PricingRatios, PricingResolver, RatioPricingResolver, TokenPrices,
};

#[cfg(test)]
mod billing_session_tests;
#[cfg(test)]
mod pricing_tests;
#[cfg(test)]
mod quota_math_tests;
