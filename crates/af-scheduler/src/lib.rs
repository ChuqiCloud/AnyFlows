//! 候选选路、健康状态、并发控制与调度快照。

mod candidate;
mod database;
mod failure;
mod retry;
mod weighted;

pub use candidate::SchedulerCandidate;
pub use database::{DatabaseWeightedScheduler, DatabaseWeightedSchedulerError};
pub use failure::{
    DEFAULT_TRANSIENT_SERVER_RETRY_LIMIT, FailureAction, FailureContext, FailurePolicy,
};
pub use retry::{RetrySelection, WeightedRetryPlan, WeightedRetryPlanError};
pub use weighted::{
    MAX_SCHEDULER_CANDIDATES, WEIGHT_BASE, WeightedSelection, WeightedSelectionError,
    WeightedStrategy,
};
