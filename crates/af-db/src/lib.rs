//! 数据库连接池、迁移、实体与仓储边界。

mod auth;
#[cfg(test)]
mod auth_tests;
mod billing_batch;
#[cfg(test)]
mod billing_batch_tests;
mod channel_state;
#[cfg(test)]
mod channel_state_tests;
mod connection;
mod entity;
mod error;
mod group_pricing;
#[cfg(test)]
mod group_pricing_tests;
mod migration;
mod model_price;
#[cfg(test)]
mod model_price_tests;
mod options;
mod quota;
#[cfg(test)]
mod quota_tests;
mod scheduler_ability;
#[cfg(test)]
mod scheduler_ability_tests;
mod usage_log;
#[cfg(test)]
mod usage_log_tests;

pub use auth::{
    TokenAuthLookup, TokenAuthLookupError, TokenAuthLookupOutcome, TokenAuthRepository,
    TokenAuthRepositoryConfigError, TokenAuthRepositoryError,
};
pub use billing_batch::{
    BillingBatchRepository, BillingBatchRepositoryError, BillingBatchWrite, BillingBatchWriteError,
    BillingBatchWriteOutcome, ChannelBillingWrite, TokenBillingWrite, UserBillingWrite,
};
pub use channel_state::{
    ChannelAutoDisableOutcome, ChannelProbeLease, ChannelProbeRecoveryOutcome,
    ChannelStateRepository, ChannelStateRepositoryError, MAX_CHANNEL_PROBE_BATCH,
};
pub use connection::DatabasePool;
pub(crate) use connection::connect;
pub use error::{DatabaseError, DatabaseOptionsError};
pub use group_pricing::{
    GroupModelRatioRecord, GroupPeakPricingRecord, GroupPricingCatalog, GroupPricingRecord,
    GroupPricingRepository, GroupPricingRepositoryError, MAX_GROUP_MODEL_RATIO_ENTRIES,
    MAX_GROUP_PRICING_ENTRIES,
};
pub use migration::{MigrationOptions, run_pending_migrations};
pub use model_price::{
    MAX_MODEL_PRICE_ENTRIES, ModelPriceBillingMode, ModelPriceRecord, ModelPriceRepository,
    ModelPriceRepositoryError,
};
pub use options::{DatabaseDialect, DatabaseOptions, PoolOptions};
pub use quota::{
    QuotaMutationOutcome, QuotaRepository, QuotaRepositoryError, QuotaReservationStatus,
};
pub use scheduler_ability::{
    MAX_SCHEDULER_ABILITY_ENTRIES, SchedulerAbilityRecord, SchedulerAbilityRepository,
    SchedulerAbilityRepositoryError,
};
pub use usage_log::{
    UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError, UsageLogSemantics,
    UsageLogSource, UsageLogUsage, UsageLogWrite, UsageLogWriteError, UsageLogWriteOutcome,
};

/// 连接数据库并执行全部待处理迁移。
///
/// 这是服务启动的标准入口。迁移失败时不会返回连接池，调用方必须拒绝启动。
pub async fn connect_and_migrate(
    database_options: &DatabaseOptions,
    migration_options: MigrationOptions,
) -> Result<DatabasePool, DatabaseError> {
    let pool = connect(database_options).await?;
    if let Err(error) = run_pending_migrations(&pool, migration_options).await {
        // 迁移失败时主动执行有界关闭，同时保留原始迁移错误供启动层判定。
        let _ = pool.close().await;
        return Err(error);
    }
    Ok(pool)
}
