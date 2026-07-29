//! 数据库连接池、迁移、实体与仓储边界。

mod ability_write;
#[cfg(test)]
mod ability_write_tests;
mod admin_channel;
#[cfg(test)]
mod admin_channel_tests;
mod admin_channel_write;
#[cfg(test)]
mod admin_channel_write_tests;
mod admin_credential;
mod admin_dashboard;
#[cfg(test)]
mod admin_dashboard_tests;
mod admin_group;
#[cfg(test)]
mod admin_group_tests;
mod admin_group_write;
#[cfg(test)]
mod admin_group_write_tests;
mod admin_token;
#[cfg(test)]
mod admin_token_tests;
mod admin_token_write;
#[cfg(test)]
mod admin_token_write_tests;
mod admin_usage_log;
#[cfg(test)]
mod admin_usage_log_tests;
mod admin_user;
#[cfg(test)]
mod admin_user_tests;
mod admin_user_write;
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
mod credential_probe;
#[cfg(test)]
mod credential_probe_tests;
mod credential_state;
#[cfg(test)]
mod credential_state_tests;
mod entity;
mod error;
mod group_pricing;
#[cfg(test)]
mod group_pricing_tests;
mod identity_secret;
mod initial_setup;
#[cfg(test)]
mod initial_setup_tests;
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
mod scheduler_runtime;
mod usage_log;
#[cfg(test)]
mod usage_log_tests;
mod user_session;
#[cfg(test)]
mod user_session_tests;

pub use ability_write::{
    MAX_ADMIN_CHANNEL_ABILITIES, MAX_ADMIN_CHANNEL_GROUPS, MAX_ADMIN_CHANNEL_MODEL_BYTES,
    MAX_ADMIN_CHANNEL_MODELS,
};
pub use admin_channel::{
    AdminChannelLookupOutcome, AdminChannelPageRecord, AdminChannelRecord, AdminChannelRepository,
    AdminChannelRepositoryConfigError, AdminChannelRepositoryError, MAX_ADMIN_CHANNEL_JSON_BYTES,
    MAX_ADMIN_CHANNEL_PAGE_SIZE,
};
pub use admin_channel_write::{
    AdminChannelDeleteOutcome, AdminChannelMutationOutcome, AdminChannelWriteRecord,
    AdminChannelWriteRepositoryError, AdminCredentialCreateOutcome, AdminCredentialDeleteOutcome,
    AdminCredentialMutationOutcome, AdminCredentialWriteRecord,
};
pub use admin_credential::{
    AdminCredentialLookupOutcome, AdminCredentialPageOutcome, AdminCredentialPageRecord,
    AdminCredentialRecord,
};
pub use admin_dashboard::{
    AdminDashboardRecord, AdminDashboardRepository, AdminDashboardRepositoryConfigError,
    AdminDashboardRepositoryError,
};
pub use admin_group::{
    AdminGroupLookupOutcome, AdminGroupPageRecord, AdminGroupPeakRecord, AdminGroupRecord,
    AdminGroupRepository, AdminGroupRepositoryConfigError, AdminGroupRepositoryError,
    MAX_ADMIN_GROUP_FLAGS_BYTES, MAX_ADMIN_GROUP_PAGE_SIZE,
};
pub use admin_group_write::{
    AdminGroupDeleteOutcome, AdminGroupMutationOutcome, AdminGroupPeakWriteRecord,
    AdminGroupWriteRecord,
};
pub use admin_token::{
    AdminTokenLookupOutcome, AdminTokenPageRecord, AdminTokenRecord, AdminTokenRepository,
    AdminTokenRepositoryConfigError, AdminTokenRepositoryError, MAX_ADMIN_TOKEN_PAGE_SIZE,
};
pub use admin_token_write::{
    AdminTokenCreateRecord, AdminTokenDeleteOutcome, AdminTokenMutationOutcome,
    AdminTokenWriteRecord, AdminTokenWriteRepositoryError,
};
pub use admin_usage_log::{
    AdminUsageLogPageRecord, AdminUsageLogRecord, AdminUsageLogRepository,
    AdminUsageLogRepositoryConfigError, AdminUsageLogRepositoryError, AdminUsageLogUsageRecord,
    MAX_ADMIN_USAGE_LOG_PAGE_SIZE,
};
pub use admin_user::{
    AdminUserLookupOutcome, AdminUserPageRecord, AdminUserRecord, AdminUserRepository,
    AdminUserRepositoryConfigError, AdminUserRepositoryError, MAX_ADMIN_USER_PAGE_SIZE,
};
pub use admin_user_write::{
    AdminUserCreateRecord, AdminUserDeleteOutcome, AdminUserMutationOutcome, AdminUserUpdateRecord,
};
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
pub use credential_probe::{
    CREDENTIAL_ENVELOPE_NONCE_BYTES, CREDENTIAL_ENVELOPE_TAG_BYTES, ChannelProbeHeader,
    ChannelProbeTargetRecord, ChannelProbeTargetRepository, ChannelProbeTargetRepositoryError,
    EncryptedCredentialEnvelope, EncryptedCredentialEnvelopeError,
    MAX_CREDENTIAL_ENVELOPE_CIPHERTEXT_BYTES, MAX_CREDENTIAL_ENVELOPE_KEY_ID_BYTES,
};
pub use credential_state::{
    CredentialStateChange, CredentialStateEvent, CredentialStateRepository,
    CredentialStateRepositoryError, CredentialStateWriteReport, MAX_CREDENTIAL_STATE_BATCH,
};
pub use error::{DatabaseError, DatabaseOptionsError};
pub use group_pricing::{
    GroupModelRatioRecord, GroupPeakPricingRecord, GroupPricingCatalog, GroupPricingRecord,
    GroupPricingRepository, GroupPricingRepositoryError, MAX_GROUP_MODEL_RATIO_ENTRIES,
    MAX_GROUP_PRICING_ENTRIES,
};
pub use initial_setup::{
    InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    InitialSetupRepositoryConfigError, InitialSetupRepositoryError, InitialSetupStatus,
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
    MAX_SCHEDULER_ABILITY_ENTRIES, MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES, SchedulerAbilityRecord,
    SchedulerAbilityRepository, SchedulerAbilityRepositoryError,
};
pub use scheduler_runtime::{
    ChannelModelMappings, ChannelParameterOverrides, ChannelRequestPolicyError,
    MAX_CHANNEL_MODEL_MAPPINGS, MAX_CHANNEL_OUTPUT_TOKENS, MAX_CHANNEL_PARAMETER_OVERRIDES,
    MAX_CHANNEL_REQUEST_POLICY_BYTES, MAX_CHANNEL_STOP_SEQUENCE_BYTES, MAX_CHANNEL_STOP_SEQUENCES,
    MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES, MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL,
    SchedulerRuntimeCredentialRecord, SchedulerRuntimeHeader, SchedulerRuntimeRecord,
    SchedulerRuntimeRepository, SchedulerRuntimeRepositoryError, SchedulerRuntimeTargetRecord,
    SchedulerRuntimeTargetRecordError,
};
pub use usage_log::{
    UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError, UsageLogSemantics,
    UsageLogSource, UsageLogUsage, UsageLogWrite, UsageLogWriteError, UsageLogWriteOutcome,
};
pub use user_session::{
    UserSessionLookupByIdOutcome, UserSessionLookupOutcome, UserSessionRepository,
    UserSessionRepositoryConfigError, UserSessionRepositoryError,
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
