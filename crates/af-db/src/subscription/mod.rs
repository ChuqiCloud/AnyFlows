mod repository;
#[cfg(test)]
mod tests;
mod types;

pub use repository::SubscriptionRepository;
pub use types::{
    MAX_SUBSCRIPTION_PAGE_SIZE, MAX_SUBSCRIPTION_PLAN_NAME_BYTES, SubscriptionInputError,
    SubscriptionPlanCreateOutcome, SubscriptionPlanDisable, SubscriptionPlanDisableOutcome,
    SubscriptionPlanPageRecord, SubscriptionPlanRecord, SubscriptionPlanWrite,
    SubscriptionRepositoryConfigError, SubscriptionRepositoryError, UserSubscriptionBind,
    UserSubscriptionBindOutcome, UserSubscriptionPageRecord, UserSubscriptionRecord,
};
