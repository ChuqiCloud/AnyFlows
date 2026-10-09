use std::{collections::HashSet, future::Future, marker::PhantomData, pin::Pin, time::Duration};

use sea_orm::sea_query::{Expr, Query};
use sea_orm::{ConnectionTrait, EntityTrait, QueryOrder};
use sea_orm_migration::prelude::async_trait;
use sea_orm_migration::{MigrationTrait, MigratorTrait, SchemaManager};
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
mod m20260831_000123_create_refund_reconciliations;
mod m20260903_000126_add_manual_refund_completion;
mod m20260903_000127_extend_refund_status;
mod m20260903_000128_add_client_simulation_body_observability;
mod m20260906_000134_create_announcements;
mod m20260906_000135_add_announcement_audience_notifications;
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

/// 公共核心迁移使用的历史表名。
pub const PUBLIC_MIGRATION_TABLE_NAME: &str = "seaql_migrations";

/// 扩展迁移推荐使用的默认表名。
pub const EXTENSION_MIGRATION_TABLE_NAME: &str = "anyflows_extension_migrations";

/// 待处理迁移的执行参数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationOptions {
    timeout: Duration,
}

/// 动态数据库扩展的迁移执行契约。
///
/// 公共服务负责创建并持有迁移连接，扩展只提供自己的注册表执行逻辑。这样企业
/// 仓库可以实现独立迁移表，同时避免再次建立连接或把扩展迁移混入公共注册表。
pub trait DatabaseMigrationExtension: Send + Sync {
    /// Run the public registry with any declared extension history compatibility.
    /// Implementations that wrap another extension must forward this hook.
    fn run_public_migrations<'a>(
        &'a self,
        pool: &'a DatabasePool,
        options: MigrationOptions,
    ) -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'a>> {
        Box::pin(run_pending_migrations(pool, options))
    }

    fn run_pending<'a>(
        &'a self,
        pool: &'a DatabasePool,
        options: MigrationOptions,
    ) -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'a>>;
}

/// 从历史公共迁移表接管已执行的扩展迁移版本。
///
/// 这个声明只用于已经在 `seaql_migrations` 中执行过、但后续要从公共注册表移入
/// 私有扩展注册表的版本。不存在于旧表中的版本不会被写入目标表，仍会按正常流程执行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationHistoryAdoption {
    versions: Vec<String>,
}

impl MigrationHistoryAdoption {
    /// 创建一个历史接管声明；重复版本会被折叠。
    pub fn from_versions<I, S>(versions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut versions = versions.into_iter().map(Into::into).collect::<Vec<_>>();
        versions.sort_unstable();
        versions.dedup();
        Self { versions }
    }

    /// 返回声明中的迁移版本。
    pub fn versions(&self) -> &[String] {
        &self.versions
    }
}

/// 将一个 SeaORM `MigratorTrait` 适配为数据库扩展迁移执行器。
///
/// 企业仓库只需将自己的 `Migrator` 包装为
/// `MigratorExtension::<EnterpriseMigrator>::new()`，并覆盖该 Migrator 的
/// `migration_table_name`。
pub struct MigratorExtension<M> {
    marker: PhantomData<fn() -> M>,
    adoption: Option<MigrationHistoryAdoption>,
}

impl<M> MigratorExtension<M> {
    pub const fn new() -> Self {
        Self {
            marker: PhantomData,
            adoption: None,
        }
    }

    /// 声明需要从旧公共迁移表接管的扩展版本。
    pub fn with_legacy_history(mut self, adoption: MigrationHistoryAdoption) -> Self {
        self.adoption = Some(adoption);
        self
    }
}

impl<M> Default for MigratorExtension<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> DatabaseMigrationExtension for MigratorExtension<M>
where
    M: MigratorTrait + 'static,
{
    fn run_public_migrations<'a>(
        &'a self,
        pool: &'a DatabasePool,
        options: MigrationOptions,
    ) -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'a>> {
        let table = M::migration_table_name().to_string();
        Box::pin(async move {
            if table == PUBLIC_MIGRATION_TABLE_NAME {
                return Err(DatabaseError::MigrationTableConflict { table });
            }
            run_with::<PublicMigratorWithExtensionHistory<M>>(pool, options).await
        })
    }

    fn run_pending<'a>(
        &'a self,
        pool: &'a DatabasePool,
        options: MigrationOptions,
    ) -> Pin<Box<dyn Future<Output = Result<(), DatabaseError>> + Send + 'a>> {
        let table = M::migration_table_name().to_string();
        Box::pin(async move {
            if table == PUBLIC_MIGRATION_TABLE_NAME {
                return Err(DatabaseError::MigrationTableConflict { table });
            }
            match self.adoption.as_ref() {
                Some(adoption) => run_with_adoption::<M>(pool, options, adoption).await,
                None => run_pending_migrations_with::<M>(pool, options).await,
            }
        })
    }
}

struct PublicMigratorWithExtensionHistory<M>(PhantomData<fn() -> M>);

#[async_trait::async_trait]
impl<M: MigratorTrait + 'static> MigratorTrait for PublicMigratorWithExtensionHistory<M> {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Migrator::migrations()
    }

    async fn get_migration_models<C>(
        db: &C,
    ) -> Result<Vec<sea_orm_migration::seaql_migrations::Model>, sea_orm::DbErr>
    where
        C: ConnectionTrait,
    {
        let public = Migrator::migrations()
            .iter()
            .map(|migration| migration.name().to_owned())
            .collect::<HashSet<_>>();
        let private = M::migrations()
            .iter()
            .map(|migration| migration.name().to_owned())
            .collect::<HashSet<_>>();
        if !public.is_disjoint(&private) {
            return Err(sea_orm::DbErr::Custom(
                "extension migration versions overlap the public registry".to_owned(),
            ));
        }
        // Filter only the status projection. Keep original rows and timestamps
        // available for adoption; unknown versions still fail SeaORM validation.
        Ok(Migrator::get_migration_models(db)
            .await?
            .into_iter()
            .filter(|record| !private.contains(&record.version))
            .collect())
    }
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
            Box::new(m20260831_000123_create_refund_reconciliations::Migration),
            Box::new(m20260903_000126_add_manual_refund_completion::Migration),
            Box::new(m20260903_000127_extend_refund_status::Migration),
            Box::new(m20260903_000128_add_client_simulation_body_observability::Migration),
            Box::new(m20260906_000134_create_announcements::Migration),
            Box::new(m20260906_000135_add_announcement_audience_notifications::Migration),
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

async fn run_with_adoption<M>(
    pool: &DatabasePool,
    options: MigrationOptions,
    adoption: &MigrationHistoryAdoption,
) -> Result<(), DatabaseError>
where
    M: MigratorTrait,
{
    let future = async {
        adopt_migration_history::<M>(pool, adoption).await?;
        M::up(pool.connection(), None)
            .await
            .map_err(DatabaseError::Migration)
    };

    match timeout(options.timeout(), future).await {
        Ok(result) => result,
        Err(source) => Err(DatabaseError::MigrationTimeout {
            timeout: options.timeout(),
            source,
        }),
    }
}

async fn adopt_migration_history<M>(
    pool: &DatabasePool,
    adoption: &MigrationHistoryAdoption,
) -> Result<(), DatabaseError>
where
    M: MigratorTrait,
{
    let known_versions = M::migrations()
        .into_iter()
        .map(|migration| migration.name().to_owned())
        .collect::<HashSet<_>>();
    for version in adoption.versions() {
        if !known_versions.contains(version) {
            return Err(DatabaseError::MigrationAdoptionVersionUnknown {
                version: version.clone(),
            });
        }
    }

    M::install(pool.connection())
        .await
        .map_err(DatabaseError::Migration)?;
    if !SchemaManager::new(pool.connection())
        .has_table(PUBLIC_MIGRATION_TABLE_NAME)
        .await
        .map_err(DatabaseError::Migration)?
    {
        return Ok(());
    }

    let backend = pool.connection().get_database_backend();
    let source = sea_orm_migration::seaql_migrations::Entity::find()
        .order_by_asc(sea_orm_migration::seaql_migrations::Column::Version)
        .all(pool.connection())
        .await
        .map_err(DatabaseError::Migration)?;
    let versions = adoption
        .versions()
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    for record in source
        .into_iter()
        .filter(|record| versions.contains(record.version.as_str()))
    {
        let target_exists = Query::select()
            .column(sea_orm_migration::seaql_migrations::Column::Version)
            .from(M::migration_table_name())
            .and_where(
                Expr::col(sea_orm_migration::seaql_migrations::Column::Version).eq(&record.version),
            )
            .to_owned();
        if pool
            .connection()
            .query_one(backend.build(&target_exists))
            .await
            .map_err(DatabaseError::Migration)?
            .is_some()
        {
            continue;
        }

        let insert = Query::insert()
            .into_table(M::migration_table_name())
            .columns([
                sea_orm_migration::seaql_migrations::Column::Version,
                sea_orm_migration::seaql_migrations::Column::AppliedAt,
            ])
            .values_panic([record.version.into(), record.applied_at.into()])
            .to_owned();
        pool.connection()
            .execute(backend.build(&insert))
            .await
            .map_err(DatabaseError::Migration)?;
    }
    Ok(())
}
