use std::time::Duration;

use sea_orm_migration::{MigrationTrait, MigratorTrait};
use tokio::time::timeout;

use crate::{DatabaseError, DatabaseOptionsError, DatabasePool};

mod iden;
mod m20260718_000001_create_groups;
mod m20260718_000002_create_users;
mod m20260718_000003_create_channels;
mod m20260718_000004_create_credentials;
mod m20260718_000005_create_tokens;
mod m20260718_000006_create_channel_models;
mod m20260718_000007_create_channel_groups;
mod m20260718_000008_create_abilities;
mod m20260718_000009_create_options;
mod m20260718_000010_create_billing_reservations;
mod m20260722_000011_create_billing_batch_checkpoints;
mod m20260722_000012_create_model_prices;
mod m20260723_000013_create_group_model_ratios;
mod m20260723_000014_create_usage_logs;
mod m20260729_000015_create_playground_shares;
mod m20260729_000016_create_playground_conversations;
mod m20260730_000017_create_registration_rate_limits;
mod m20260730_000018_add_channel_timeout;
mod m20260730_000019_create_email_settings;
mod m20260730_000020_create_site_authentication_settings;
mod m20260731_000021_create_auth_challenges;
mod m20260731_000022_create_auth_challenge_rate_limits;
mod m20260731_000023_add_user_session_version;
mod m20260731_000024_add_user_notification_preferences;
mod m20260731_000025_create_models;
mod m20260731_000026_create_model_sync_audit;
mod m20260801_000027_create_wallet_ledger;
mod m20260801_000028_create_topup_payment_audit;
mod m20260801_000029_extend_wallet_ledger_topup;
mod m20260801_000030_create_redemption_storage;
mod m20260801_000031_extend_wallet_ledger_redemption;
mod m20260801_000032_create_subscription_storage;
mod m20260801_000033_create_invite_rebate_events;
mod m20260801_000034_extend_wallet_ledger_invite_rebate;
mod m20260801_000035_add_invitation_rebate_quota;
mod m20260801_000036_create_balance_alerts;
mod m20260802_000037_create_network_settings;
mod m20260803_000038_add_credential_oauth_revision;
mod m20260803_000039_add_credential_oauth_expiration;
mod m20260804_000040_create_scheduler_outbox;
mod m20260805_000041_create_routes;
mod m20260805_000042_create_proxies;
mod m20260806_000043_create_debug_traces;
mod m20260806_000044_add_model_price_expression;
mod m20260807_000045_create_subscription_billing_reservations;
mod m20260808_000046_add_billing_reservation_kind;
mod m20260808_000047_create_subscription_balance_alerts;
mod m20260808_000048_add_credential_oauth_pending;
mod m20260809_000049_create_token_window_reservations;
mod m20260809_000050_create_group_window_reservations;
mod m20260810_000051_add_usage_audio_duration;
mod m20260811_000052_create_async_tasks;
mod m20260811_000053_create_async_task_submission_claims;
mod m20260811_000054_add_usage_video_dimensions;
mod m20260811_000055_create_async_task_billings;
mod schema;

#[cfg(test)]
mod tests;

const DEFAULT_MIGRATION_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// 待处理迁移的执行参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationOptions {
    timeout: Duration,
}

impl MigrationOptions {
    /// 创建迁移参数；零截止时间会被拒绝。
    pub fn new(timeout: Duration) -> Result<Self, DatabaseOptionsError> {
        if timeout.is_zero() {
            return Err(DatabaseOptionsError::ZeroTimeout {
                field: "migration_timeout",
            });
        }
        Ok(Self { timeout })
    }

    /// 返回完整迁移批次的截止时间。
    pub fn timeout(self) -> Duration {
        self.timeout
    }
}

impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_MIGRATION_TIMEOUT,
        }
    }
}

pub(crate) struct Migrator;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260718_000001_create_groups::Migration),
            Box::new(m20260718_000002_create_users::Migration),
            Box::new(m20260718_000003_create_channels::Migration),
            Box::new(m20260718_000004_create_credentials::Migration),
            Box::new(m20260718_000005_create_tokens::Migration),
            Box::new(m20260718_000006_create_channel_models::Migration),
            Box::new(m20260718_000007_create_channel_groups::Migration),
            Box::new(m20260718_000008_create_abilities::Migration),
            Box::new(m20260718_000009_create_options::Migration),
            Box::new(m20260718_000010_create_billing_reservations::Migration),
            Box::new(m20260722_000011_create_billing_batch_checkpoints::Migration),
            Box::new(m20260722_000012_create_model_prices::Migration),
            Box::new(m20260723_000013_create_group_model_ratios::Migration),
            Box::new(m20260723_000014_create_usage_logs::Migration),
            Box::new(m20260729_000015_create_playground_shares::Migration),
            Box::new(m20260729_000016_create_playground_conversations::Migration),
            Box::new(m20260730_000017_create_registration_rate_limits::Migration),
            Box::new(m20260730_000018_add_channel_timeout::Migration),
            Box::new(m20260730_000019_create_email_settings::Migration),
            Box::new(m20260730_000020_create_site_authentication_settings::Migration),
            Box::new(m20260731_000021_create_auth_challenges::Migration),
            Box::new(m20260731_000022_create_auth_challenge_rate_limits::Migration),
            Box::new(m20260731_000023_add_user_session_version::Migration),
            Box::new(m20260731_000024_add_user_notification_preferences::Migration),
            Box::new(m20260731_000025_create_models::Migration),
            Box::new(m20260731_000026_create_model_sync_audit::Migration),
            Box::new(m20260801_000027_create_wallet_ledger::Migration),
            Box::new(m20260801_000028_create_topup_payment_audit::Migration),
            Box::new(m20260801_000029_extend_wallet_ledger_topup::Migration),
            Box::new(m20260801_000030_create_redemption_storage::Migration),
            Box::new(m20260801_000031_extend_wallet_ledger_redemption::Migration),
            Box::new(m20260801_000032_create_subscription_storage::Migration),
            Box::new(m20260801_000033_create_invite_rebate_events::Migration),
            Box::new(m20260801_000034_extend_wallet_ledger_invite_rebate::Migration),
            Box::new(m20260801_000035_add_invitation_rebate_quota::Migration),
            Box::new(m20260801_000036_create_balance_alerts::Migration),
            Box::new(m20260802_000037_create_network_settings::Migration),
            Box::new(m20260803_000038_add_credential_oauth_revision::Migration),
            Box::new(m20260803_000039_add_credential_oauth_expiration::Migration),
            Box::new(m20260804_000040_create_scheduler_outbox::Migration),
            Box::new(m20260805_000041_create_routes::Migration),
            Box::new(m20260805_000042_create_proxies::Migration),
            Box::new(m20260806_000043_create_debug_traces::Migration),
            Box::new(m20260806_000044_add_model_price_expression::Migration),
            Box::new(m20260807_000045_create_subscription_billing_reservations::Migration),
            Box::new(m20260808_000046_add_billing_reservation_kind::Migration),
            Box::new(m20260808_000047_create_subscription_balance_alerts::Migration),
            Box::new(m20260808_000048_add_credential_oauth_pending::Migration),
            Box::new(m20260809_000049_create_token_window_reservations::Migration),
            Box::new(m20260809_000050_create_group_window_reservations::Migration),
            Box::new(m20260810_000051_add_usage_audio_duration::Migration),
            Box::new(m20260811_000052_create_async_tasks::Migration),
            Box::new(m20260811_000053_create_async_task_submission_claims::Migration),
            Box::new(m20260811_000054_add_usage_video_dimensions::Migration),
            Box::new(m20260811_000055_create_async_task_billings::Migration),
        ]
    }
}

/// 执行项目注册表中的全部待处理迁移。
///
/// SeaORM 不提供跨实例迁移锁，部署时必须保证只有一个迁移执行者。PostgreSQL
/// 会为迁移批次开启事务，而 MySQL/SQLite 不具备同等 DDL 原子性；失败或超时后不会自动
/// 重试，调用方必须拒绝启动并检查数据库状态。
pub async fn run_pending_migrations(
    pool: &DatabasePool,
    options: MigrationOptions,
) -> Result<(), DatabaseError> {
    run_with::<Migrator>(pool, options).await
}

async fn run_with<M>(pool: &DatabasePool, options: MigrationOptions) -> Result<(), DatabaseError>
where
    M: MigratorTrait,
{
    match timeout(options.timeout(), M::up(pool.connection(), None)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(DatabaseError::Migration(error)),
        Err(source) => Err(DatabaseError::MigrationTimeout {
            timeout: options.timeout(),
            source,
        }),
    }
}
