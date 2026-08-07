mod lifecycle;
mod repository;
#[cfg(test)]
mod tests;
mod types;

pub use lifecycle::{
    SubscriptionExpirationDueCursor, UserSubscriptionExpirationDuePageRecord,
    UserSubscriptionLifecycleTransition, UserSubscriptionLifecycleTransitionOutcome,
    UserSubscriptionLifecycleTransitionRecord,
};
pub use repository::SubscriptionRepository;
pub use types::{
    MAX_SUBSCRIPTION_PAGE_SIZE, MAX_SUBSCRIPTION_PLAN_NAME_BYTES, SubscriptionInputError,
    SubscriptionPlanCreateOutcome, SubscriptionPlanDisable, SubscriptionPlanDisableOutcome,
    SubscriptionPlanPageRecord, SubscriptionPlanRecord, SubscriptionPlanWrite,
    SubscriptionRepositoryConfigError, SubscriptionRepositoryError, SubscriptionResetDueCursor,
    UserSubscriptionBind, UserSubscriptionBindOutcome, UserSubscriptionPageRecord,
    UserSubscriptionRecord, UserSubscriptionResetDuePageRecord, UserSubscriptionWindowAdvance,
    UserSubscriptionWindowAdvanceOutcome, UserSubscriptionWindowAdvanceRecord,
};
