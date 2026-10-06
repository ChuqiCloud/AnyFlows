mod claim;
mod completion;
mod discovery;
mod repository;
mod storage;
mod types;

#[cfg(test)]
mod tests;

pub use repository::SubscriptionBalanceAlertRepository;
pub use types::{
    SubscriptionBalanceAlertClaimOutcome, SubscriptionBalanceAlertDeliveryLease,
    SubscriptionBalanceAlertEnqueueReport, SubscriptionBalanceAlertRepositoryConfigError,
    SubscriptionBalanceAlertRepositoryError,
};

const STATUS_PENDING: i16 = 1;
const STATUS_SENDING: i16 = 2;
const STATUS_SENT: i16 = 3;
const STATUS_FAILED: i16 = 4;
const STATUS_CANCELED: i16 = 5;
const ENABLED_USER_STATUS: i16 = 1;
const DELIVERY_LEASE_SECONDS: u64 = 120;
const CLAIM_CANDIDATE_LIMIT: u64 = 16;
