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
mod m20260812_000056_create_payment_settings;
mod m20260814_000057_extend_usage_call_observations;
mod m20260814_000058_extend_debug_trace_diagnostics;
mod m20260815_000059_add_balance_display_policy;
mod m20260815_000060_secure_debug_trace_snapshots;
mod m20260815_000061_create_organization_foundation;
mod m20260815_000062_create_organization_entitlements;
mod m20260816_000063_create_organization_invitations;
mod m20260816_000064_create_organization_wallets;
mod m20260816_000065_create_organization_budgets;
mod m20260817_000066_add_organization_tokens;
mod m20260817_000067_create_organization_activity;
mod m20260817_000068_add_client_simulation_observability;
mod m20260817_000069_enforce_spark_shadow_shape;
mod m20260817_000070_create_oauth_login;
mod m20260818_000071_seed_discord_oauth_login;
mod m20260818_000072_create_passkeys;
mod m20260818_000073_add_passkey_authentication_state;
mod m20260819_000074_create_platform_audit;
mod m20260819_000075_create_request_outcomes;
mod m20260819_000076_create_user_notification_events;
mod m20260819_000077_create_analytics_export_outbox;
mod m20260820_000078_extend_oauth_login_providers;
mod m20260820_000079_seed_wechat_oauth_login;
mod m20260820_000080_seed_telegram_oauth_login;
mod m20260820_000081_seed_google_oauth_login;
mod m20260821_000082_create_user_notification_receipts;
mod m20260821_000083_create_subscription_plan_prices;
mod m20260821_000084_create_subscription_orders;
mod m20260822_000085_create_subscription_payment_events;
mod m20260822_000086_create_refund_requests;
mod m20260822_000087_add_refund_provider_receipts;
mod m20260823_000088_add_refund_settings;
mod m20260823_000089_add_refund_approval;
mod m20260823_000090_add_refund_payment_reference;
mod m20260823_000091_create_custom_oauth2_providers;
mod m20260823_000092_create_custom_oauth2_login;
mod m20260823_000093_create_organization_custom_roles;
mod m20260823_000094_add_organization_custom_role_bindings;
mod m20260824_000095_create_organization_sso_persistence;
mod m20260825_000096_add_organization_sso_login_material;
mod m20260825_000097_add_organization_sso_ticket_email_fingerprint;
mod m20260826_000098_create_organization_sso_policies;
mod m20260827_000099_create_organization_scim_tokens;
mod m20260827_000100_create_organization_scim_user_resources;
mod m20260827_000101_create_organization_scim_group_resources;
mod m20260828_000102_create_organization_scim_audit_logs;
mod m20260828_000103_create_organization_service_accounts;
mod m20260828_000104_create_organization_service_account_budget_bindings;
mod m20260828_000105_create_organization_service_account_audit_logs;
mod m20260829_000106_create_organization_departments;
mod m20260829_000107_create_organization_department_memberships;
mod m20260829_000108_add_organization_department_snapshots;
mod m20260829_000109_add_department_budget_snapshots;
mod m20260829_000110_create_organization_contract_prices;
mod m20260829_000111_add_contract_price_snapshots;
mod m20260830_000112_create_organization_credit_terms;
mod m20260830_000113_create_organization_credit_invoices;
mod m20260830_000114_create_organization_credit_repayments;
mod m20260830_000115_create_organization_approvals;
mod m20260830_000116_create_organization_approval_execution;
mod m20260830_000117_create_organization_approval_budget_execution;
mod m20260830_000118_create_organization_approval_service_account_execution;
mod m20260830_000119_create_organization_approval_contract_price_execution;
mod m20260830_000120_create_organization_approval_funds_execution;
mod m20260830_000121_create_organization_approval_timeout_lease;
mod m20260831_000122_create_organization_approval_notifications;
mod m20260831_000123_create_refund_reconciliations;
mod m20260901_000124_add_organization_sso_domain_reverification;
mod m20260902_000125_add_organization_sso_login_transaction_cleanup;
mod m20260903_000126_add_manual_refund_completion;
mod m20260903_000127_extend_refund_status;
mod m20260903_000128_add_client_simulation_body_observability;
mod m20260903_000129_create_organization_approval_notification_receipts;
mod m20260904_000130_create_organization_provisioning_requests;
mod m20260904_000131_create_organization_plan_catalog;
mod m20260905_000132_create_organization_plan_orders;
mod m20260905_000133_add_organization_plan_order_changes;
mod m20260906_000134_create_announcements;
mod m20260906_000135_add_announcement_audience_notifications;
mod m20260907_000136_create_organization_verification;
mod m20260907_000137_add_organization_verification_material_content;
mod m20260907_000138_create_organization_provisioning_materials;
mod m20260912_000139_add_frontend_template;
mod m20260921_000140_extend_request_outcomes_for_call_logs;
mod m20260923_000141_account_verification;
mod m20260923_000142_account_verification_document;
mod m20260924_000143_account_verification_provider;
mod m20260924_000144_create_model_provider_catalog;
mod m20260924_000145_account_verification_settings;
mod m20260925_000146_add_manual_verification_setting;
mod m20260926_000147_add_site_navigation;
mod m20260926_000148_split_verification_policy;
mod m20260929_000149_add_epay_qr_enabled;
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
            Box::new(m20260812_000056_create_payment_settings::Migration),
            Box::new(m20260814_000057_extend_usage_call_observations::Migration),
            Box::new(m20260814_000058_extend_debug_trace_diagnostics::Migration),
            Box::new(m20260815_000059_add_balance_display_policy::Migration),
            Box::new(m20260815_000060_secure_debug_trace_snapshots::Migration),
            Box::new(m20260815_000061_create_organization_foundation::Migration),
            Box::new(m20260815_000062_create_organization_entitlements::Migration),
            Box::new(m20260816_000063_create_organization_invitations::Migration),
            Box::new(m20260816_000064_create_organization_wallets::Migration),
            Box::new(m20260816_000065_create_organization_budgets::Migration),
            Box::new(m20260817_000066_add_organization_tokens::Migration),
            Box::new(m20260817_000067_create_organization_activity::Migration),
            Box::new(m20260817_000068_add_client_simulation_observability::Migration),
            Box::new(m20260817_000069_enforce_spark_shadow_shape::Migration),
            Box::new(m20260817_000070_create_oauth_login::Migration),
            Box::new(m20260818_000071_seed_discord_oauth_login::Migration),
            Box::new(m20260818_000072_create_passkeys::Migration),
            Box::new(m20260818_000073_add_passkey_authentication_state::Migration),
            Box::new(m20260819_000074_create_platform_audit::Migration),
            Box::new(m20260819_000075_create_request_outcomes::Migration),
            Box::new(m20260819_000076_create_user_notification_events::Migration),
            Box::new(m20260819_000077_create_analytics_export_outbox::Migration),
            Box::new(m20260820_000078_extend_oauth_login_providers::Migration),
            Box::new(m20260820_000079_seed_wechat_oauth_login::Migration),
            Box::new(m20260820_000080_seed_telegram_oauth_login::Migration),
            Box::new(m20260820_000081_seed_google_oauth_login::Migration),
            Box::new(m20260821_000082_create_user_notification_receipts::Migration),
            Box::new(m20260821_000083_create_subscription_plan_prices::Migration),
            Box::new(m20260821_000084_create_subscription_orders::Migration),
            Box::new(m20260822_000085_create_subscription_payment_events::Migration),
            Box::new(m20260822_000086_create_refund_requests::Migration),
            Box::new(m20260822_000087_add_refund_provider_receipts::Migration),
            Box::new(m20260823_000088_add_refund_settings::Migration),
            Box::new(m20260823_000089_add_refund_approval::Migration),
            Box::new(m20260823_000090_add_refund_payment_reference::Migration),
            Box::new(m20260823_000091_create_custom_oauth2_providers::Migration),
            Box::new(m20260823_000092_create_custom_oauth2_login::Migration),
            Box::new(m20260823_000093_create_organization_custom_roles::Migration),
            Box::new(m20260823_000094_add_organization_custom_role_bindings::Migration),
            Box::new(m20260824_000095_create_organization_sso_persistence::Migration),
            Box::new(m20260825_000096_add_organization_sso_login_material::Migration),
            Box::new(m20260825_000097_add_organization_sso_ticket_email_fingerprint::Migration),
            Box::new(m20260826_000098_create_organization_sso_policies::Migration),
            Box::new(m20260827_000099_create_organization_scim_tokens::Migration),
            Box::new(m20260827_000100_create_organization_scim_user_resources::Migration),
            Box::new(m20260827_000101_create_organization_scim_group_resources::Migration),
            Box::new(m20260828_000102_create_organization_scim_audit_logs::Migration),
            Box::new(m20260828_000103_create_organization_service_accounts::Migration),
            Box::new(
                m20260828_000104_create_organization_service_account_budget_bindings::Migration,
            ),
            Box::new(m20260828_000105_create_organization_service_account_audit_logs::Migration),
            Box::new(m20260829_000106_create_organization_departments::Migration),
            Box::new(m20260829_000107_create_organization_department_memberships::Migration),
            Box::new(m20260829_000108_add_organization_department_snapshots::Migration),
            Box::new(m20260829_000109_add_department_budget_snapshots::Migration),
            Box::new(m20260829_000110_create_organization_contract_prices::Migration),
            Box::new(m20260829_000111_add_contract_price_snapshots::Migration),
            Box::new(m20260830_000112_create_organization_credit_terms::Migration),
            Box::new(m20260830_000113_create_organization_credit_invoices::Migration),
            Box::new(m20260830_000114_create_organization_credit_repayments::Migration),
            Box::new(m20260830_000115_create_organization_approvals::Migration),
            Box::new(m20260830_000116_create_organization_approval_execution::Migration),
            Box::new(m20260830_000117_create_organization_approval_budget_execution::Migration),
            Box::new(
                m20260830_000118_create_organization_approval_service_account_execution::Migration,
            ),
            Box::new(
                m20260830_000119_create_organization_approval_contract_price_execution::Migration,
            ),
            Box::new(m20260830_000120_create_organization_approval_funds_execution::Migration),
            Box::new(m20260830_000121_create_organization_approval_timeout_lease::Migration),
            Box::new(m20260831_000122_create_organization_approval_notifications::Migration),
            Box::new(m20260831_000123_create_refund_reconciliations::Migration),
            Box::new(m20260901_000124_add_organization_sso_domain_reverification::Migration),
            Box::new(m20260902_000125_add_organization_sso_login_transaction_cleanup::Migration),
            Box::new(m20260903_000126_add_manual_refund_completion::Migration),
            Box::new(m20260903_000127_extend_refund_status::Migration),
            Box::new(m20260903_000128_add_client_simulation_body_observability::Migration),
            Box::new(
                m20260903_000129_create_organization_approval_notification_receipts::Migration,
            ),
            Box::new(m20260904_000130_create_organization_provisioning_requests::Migration),
            Box::new(m20260904_000131_create_organization_plan_catalog::Migration),
            Box::new(m20260905_000132_create_organization_plan_orders::Migration),
            Box::new(m20260905_000133_add_organization_plan_order_changes::Migration),
            Box::new(m20260906_000134_create_announcements::Migration),
            Box::new(m20260906_000135_add_announcement_audience_notifications::Migration),
            Box::new(m20260907_000136_create_organization_verification::Migration),
            Box::new(m20260907_000137_add_organization_verification_material_content::Migration),
            Box::new(m20260907_000138_create_organization_provisioning_materials::Migration),
            Box::new(m20260912_000139_add_frontend_template::Migration),
            Box::new(m20260921_000140_extend_request_outcomes_for_call_logs::Migration),
            Box::new(m20260923_000141_account_verification::Migration),
            Box::new(m20260923_000142_account_verification_document::Migration),
            Box::new(m20260924_000143_account_verification_provider::Migration),
            Box::new(m20260924_000144_create_model_provider_catalog::Migration),
            Box::new(m20260924_000145_account_verification_settings::Migration),
            Box::new(m20260925_000146_add_manual_verification_setting::Migration),
            Box::new(m20260926_000147_add_site_navigation::Migration),
            Box::new(m20260926_000148_split_verification_policy::Migration),
            Box::new(m20260929_000149_add_epay_qr_enabled::Migration),
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
    run_pending_migrations_with::<Migrator>(pool, options).await
}

/// 执行扩展注册表中的全部待处理迁移。
///
/// 扩展迁移必须由扩展仓库提供自己的 [`MigratorTrait`] 实现，并覆盖
/// [`MigratorTrait::migration_table_name`] 使用独立的迁移表。这样公共核心的
/// `seaql_migrations` 记录不会包含企业模块的迁移，两个仓库可以分别演进。
pub async fn run_pending_migrations_with<M>(
    pool: &DatabasePool,
    options: MigrationOptions,
) -> Result<(), DatabaseError>
where
    M: MigratorTrait,
{
    run_with::<M>(pool, options).await
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
