mod lifecycle;
mod repository;
#[cfg(test)]
mod tests;
mod types;

use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;

pub use lifecycle::{
    SubscriptionExpirationDueCursor, UserSubscriptionExpirationDuePageRecord,
    UserSubscriptionLifecycleTransition, UserSubscriptionLifecycleTransitionOutcome,
    UserSubscriptionLifecycleTransitionRecord,
};
pub use repository::SubscriptionRepository;
pub use types::{
    MAX_SUBSCRIPTION_CURRENCY_BYTES, MAX_SUBSCRIPTION_IDEMPOTENCY_KEY_BYTES,
    MAX_SUBSCRIPTION_PAGE_SIZE, MAX_SUBSCRIPTION_PAYMENT_METHOD_BYTES,
    MAX_SUBSCRIPTION_PLAN_NAME_BYTES, MAX_SUBSCRIPTION_PROVIDER_BYTES,
    MAX_SUBSCRIPTION_PROVIDER_EVENT_ID_BYTES, MAX_SUBSCRIPTION_TRADE_NO_BYTES,
    SubscriptionInputError, SubscriptionOrderCreate, SubscriptionOrderCreateOutcome,
    SubscriptionOrderRecord, SubscriptionOrderSubmission, SubscriptionOrderSubmitOutcome,
    SubscriptionPaymentEventOutcome, SubscriptionPaymentEventRejection,
    SubscriptionPaymentEventWrite, SubscriptionPlanCreateOutcome, SubscriptionPlanDisable,
    SubscriptionPlanDisableOutcome, SubscriptionPlanPageRecord, SubscriptionPlanPriceRecord,
    SubscriptionPlanRecord, SubscriptionPlanWrite, SubscriptionRepositoryConfigError,
    SubscriptionRepositoryError, SubscriptionResetDueCursor, UserSubscriptionBind,
    UserSubscriptionBindOutcome, UserSubscriptionPageRecord, UserSubscriptionRecord,
    UserSubscriptionResetDuePageRecord, UserSubscriptionWindowAdvance,
    UserSubscriptionWindowAdvanceOutcome, UserSubscriptionWindowAdvanceRecord,
};

/// 生成不会回拨且适合 SQLite 文本比较的订阅审计时间。
pub(crate) fn monotonic_updated_at(
    current: TimeDateTimeWithTimeZone,
    business_time: TimeDateTimeWithTimeZone,
) -> TimeDateTimeWithTimeZone {
    // SQLite 按 RFC3339 文本比较时间，可选小数秒在同一秒内不保持词典序。
    let wall_clock = TimeDateTimeWithTimeZone::now_utc()
        .replace_nanosecond(0)
        .expect("零纳秒始终是合法时间");
    let business_time = business_time
        .replace_nanosecond(0)
        .expect("零纳秒始终是合法时间");
    current.max(wall_clock).max(business_time)
}
