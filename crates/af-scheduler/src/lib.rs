//! 候选选路、健康状态、并发控制与调度快照。

mod auto_ban;
mod auto_ban_keyword;
mod candidate;
mod concurrency;
mod database;
mod failure;
mod health;
mod index;
mod probe;
mod retry;
mod stable_first;
mod sticky;
mod subscription_cycle;
mod weighted;

pub use auto_ban::{
    AutoBanPolicy, AutoBanPolicyError, AutoBanRule, ChannelAutoDisableStore,
    ChannelFailureCoordinator, ChannelFailureOutcome, ChannelStateEffectOutcome,
    MAX_AUTO_BAN_RULES,
};
pub use auto_ban_keyword::{
    AutoBanEvidence, AutoBanEvidenceError, MAX_AUTO_BAN_EVIDENCE_BYTES, MAX_AUTO_BAN_KEYWORD_BYTES,
    MAX_AUTO_BAN_KEYWORD_TOTAL_BYTES,
};
pub use candidate::SchedulerCandidate;
pub use concurrency::{
    CONCURRENCY_EXTRA_WAIT_SLOTS, ConcurrencyWaitBackoff, INITIAL_CONCURRENCY_WAIT_BACKOFF,
    MAX_CONCURRENCY_WAIT_BACKOFF, concurrency_wait_queue_limit,
};
pub use database::{DatabaseWeightedScheduler, DatabaseWeightedSchedulerError};
pub use failure::{
    CredentialFailureContext, CredentialFailureDisposition, DEFAULT_TRANSIENT_SERVER_RETRY_LIMIT,
    FailureAction, FailureContext, FailurePolicy, PermanentCredentialFailure,
    TemporaryCredentialFailure, credential_failure_disposition,
};
pub use health::{ChannelRoutingHealth, ROUTING_HEALTH_PENALTY_SCALE};
pub use index::{
    BoundRouteCandidate, BoundRouteCandidateError, ChannelIndexCacheError,
    ChannelIndexProjectionApplyReport, ChannelIndexRefreshConfig, ChannelIndexRefreshConfigError,
    ChannelIndexRefreshReport, ChannelIndexRefreshSupervisor, ChannelIndexSnapshot,
    ChannelIndexSnapshotError, ChannelIndexSource, ChannelIndexSourceError,
    ChannelIndexSourceFuture, ChannelIndexSourceRecord, ChannelIndexSourceSnapshot,
    ChannelIndexVersionedSourceFuture, DEFAULT_CHANNEL_INDEX_REFRESH_INTERVAL,
    InMemoryChannelIndex, IndexedRouteCandidate, IndexedRoutePlan, IndexedWeightedScheduler,
    IndexedWeightedSchedulerError,
};
pub use probe::{
    ChannelProbe, ChannelProbeRunReport, ChannelProbeStatus, ChannelProbeStore,
    ChannelProbeSupervisor, ChannelProbeSupervisorConfig, ChannelProbeSupervisorConfigError,
    ChannelProbeSupervisorError, DEFAULT_CHANNEL_PROBE_BATCH_SIZE, DEFAULT_CHANNEL_PROBE_INTERVAL,
    DEFAULT_CHANNEL_PROBE_TIMEOUT,
};
pub use retry::{RetrySelection, WeightedRetryPlan, WeightedRetryPlanError};
pub use stable_first::{
    DEFAULT_PRIMARY_SCORE_RATIO_MICROS, StableFirstPlan, StableFirstPlanError, StableFirstPool,
    StableFirstStrategy,
};
pub use sticky::{
    RouteWaitKind, RouteWaitPlan, StickyRouteOutcome, StickyWaitPolicy, StickyWaitPolicyError,
};
pub use subscription_cycle::{
    DEFAULT_SUBSCRIPTION_CYCLE_BATCH_SIZE, DEFAULT_SUBSCRIPTION_CYCLE_INTERVAL,
    DEFAULT_SUBSCRIPTION_CYCLE_MAX_BATCHES_PER_RUN, MAX_SUBSCRIPTION_CYCLE_BATCHES_PER_RUN,
    SubscriptionCycleMutationOutcome, SubscriptionCyclePhaseReport, SubscriptionCycleRunReport,
    SubscriptionCycleStore, SubscriptionCycleSupervisor, SubscriptionCycleSupervisorConfig,
    SubscriptionCycleSupervisorConfigError, SubscriptionCycleSupervisorError,
    SubscriptionExpirationBatch, SubscriptionWindowAdvanceBatch,
};
pub use weighted::{
    MAX_SCHEDULER_CANDIDATES, WEIGHT_BASE, WeightedSelection, WeightedSelectionError,
    WeightedStrategy,
};
