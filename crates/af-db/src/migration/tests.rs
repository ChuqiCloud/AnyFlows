use std::env;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use af_domain::{
    BillingReservationId, ChannelId, CredentialKind, GatewayPrincipal, GroupId,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationGatewayPrincipal,
    OrganizationId, OrganizationMembershipId, OrganizationMembershipStatus, OrganizationRole,
    OrganizationTeamId, Quota, QuotaDelta, SubscriptionCycle, SubscriptionWindow, TokenId,
    TrustedClientIp, UserId,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rust_decimal::Decimal;
use sea_orm::entity::prelude::{Json, TimeDateTimeWithTimeZone, TimeTime};
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DbBackend, EntityTrait, IdenStatic, IntoActiveModel, Iterable, PaginatorTrait, QueryFilter,
    QuerySelect, Set, Statement,
};
use sea_orm_migration::prelude::*;
use url::Url;

use super::*;
use crate::entity::{
    BillingBatchFingerprint, BillingBatchWriterKey, BillingReservationKey, ChannelBaseUrl,
    EncryptedJson, HeaderOverrides, PasswordHash, PlaygroundConversationKey, SensitiveDecimal,
    SensitiveJson, SensitiveString, TokenHash, TokenIpAllowlist, WalletLedgerKey, abilities,
    analytics_export_outbox_events, auth_challenge_rate_limits, auth_challenges,
    authentication_settings, balance_alert_events, balance_alert_settings,
    billing_batch_checkpoints, billing_group_window_reservations,
    billing_organization_reservations, billing_reservations, billing_subscription_reservations,
    billing_token_window_reservations, channel_groups, channel_models, channels, credentials,
    debug_trace_attempts, debug_trace_snapshot_access_audits, debug_trace_snapshots, debug_traces,
    email_settings, group_model_ratios, groups, invite_rebate_events, model_prices,
    model_sync_items, model_sync_runs, models, oauth_login_providers, options,
    organization_audit_logs, organization_budget_policies, organization_budget_windows,
    organization_custom_role_permissions, organization_custom_roles, organization_entitlements,
    organization_invitations, organization_memberships, organization_scim_audit_logs,
    organization_scim_group_resources, organization_scim_tokens, organization_scim_user_resources,
    organization_sso_domains, organization_sso_identities, organization_sso_login_transactions,
    organization_sso_policies, organization_sso_providers, organization_teams,
    organization_wallet_ledger_entries, organization_wallets, organizations,
    passkey_authentication_challenges, passkey_registration_challenges, passkeys, payment_settings,
    platform_audit_logs, playground_conversations, playground_shares, redemption_batches,
    redemption_codes, refund_provider_events, refund_requests, registration_rate_limits,
    request_outcome_logs, scheduler_outbox_events, site_settings,
    subscription_balance_alert_events, subscription_orders, subscription_payment_events,
    subscription_plan_prices, subscription_plans, tokens, topup_orders, topup_payment_events,
    usage_logs, user_notification_events, user_notification_receipts, user_subscriptions, users,
    wallet_ledger_entries,
};
use crate::{
    AdminDashboardRepository, AuthChallengeConsume, AuthChallengeConsumeOutcome,
    AuthChallengeIssue, AuthChallengeIssueOutcome, AuthChallengePurpose,
    AuthChallengeRateLimitClaim, AuthChallengeRateLimitOutcome, AuthChallengeRateLimitRepository,
    AuthChallengeRepository, BalanceDisplayModeRecord, BalanceDisplayPolicyRecord,
    BalanceSymbolPositionRecord, BillingBatchRepository, BillingBatchWrite,
    BillingBatchWriteOutcome, ChannelBillingWrite, DatabaseOptions, EmailPasswordUpdate,
    EmailSettingsRepository, EmailSettingsWriteRecord, EmailTlsMode, EncryptedCredentialEnvelope,
    GroupPricingRepository, ModelPriceBillingMode, ModelPriceRepository, OrganizationAuditOutcome,
    OrganizationAuditQuery, OrganizationAuditRepository, OrganizationAuditRepositoryError,
    OrganizationAuditWrite, OrganizationAuditWriteOutcome, OrganizationCreate,
    OrganizationEntitlementWrite, OrganizationMembershipChange, OrganizationMembershipCreate,
    OrganizationRepository, OrganizationRepositoryError, OrganizationTeamCreate,
    PlatformAuditOutcome, PlatformAuditQuery, PlatformAuditRepository,
    PlatformAuditRepositoryError, PlatformAuditWrite, PlatformAuditWriteOutcome,
    PlaygroundConversationRepository, PlaygroundConversationWrite, PlaygroundShareRepository,
    PlaygroundShareWrite, PoolOptions, QuotaMutationOutcome, QuotaRepository,
    QuotaReservationStatus, RegistrationAttemptOutcome, RegistrationPolicyWriteRecord,
    RegistrationRepository, SchedulerCatalogSubject, SchedulerOutboxClaimOutcome,
    SchedulerOutboxCompletionOutcome, SchedulerOutboxRepository, SiteSettingsRepository,
    SiteSettingsWriteRecord, TokenAuthLookup, TokenAuthLookupOutcome, TokenAuthRepository,
    TokenBillingWrite, UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError,
    UsageLogSemantics, UsageLogSource, UsageLogUsage, UsageLogVideoResolution, UsageLogWrite,
    UsageLogWriteOutcome, UserBillingWrite, connect,
};

const TEST_DATABASE_URL: &str = "AF_TEST_DATABASE_URL";
const REQUIRE_LIVE_DATABASE: &str = "AF_REQUIRE_LIVE_DATABASE";
const ALLOW_DESTRUCTIVE_MIGRATION_TEST: &str = "AF_ALLOW_DESTRUCTIVE_MIGRATION_TEST";
/// 当前必须完整应用的迁移数量。
const EXPECTED_MIGRATION_COUNT: usize = 102;
const ISOLATED_DATABASE_PREFIX: &str = "anyflows_migration_test_";
const TEST_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";
const TOKEN_HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TOKEN_HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TOKEN_HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const TOKEN_HASH_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const TOKEN_HASH_E: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
const TOKEN_HASH_F: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
const RESERVATION_KEY_A: &str = "11111111111111111111111111111111";
const RESERVATION_KEY_B: &str = "22222222222222222222222222222222";
const RESERVATION_KEY_C: &str = "33333333333333333333333333333333";
const RESERVATION_KEY_D: &str = "44444444444444444444444444444444";
const PLAYGROUND_CONVERSATION_KEY: &str = "55555555555555555555555555555555";
const TOPUP_ORDER_KEY_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TOPUP_IDEMPOTENCY_KEY_A: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TOPUP_EVENT_KEY_A: &str = "cccccccccccccccccccccccccccccccc";
const TOPUP_PROVIDER_EVENT_ID_A: &str = "evt_paid_secret_a";
const CORE_TABLES: [&str; 84] = [
    "groups",
    "users",
    "passkeys",
    "passkey_registration_challenges",
    "passkey_authentication_challenges",
    "channels",
    "credentials",
    "tokens",
    "channel_models",
    "channel_groups",
    "abilities",
    "scheduler_outbox_events",
    "options",
    "billing_reservations",
    "billing_subscription_reservations",
    "billing_token_window_reservations",
    "billing_group_window_reservations",
    "billing_batch_checkpoints",
    "model_prices",
    "group_model_ratios",
    "usage_logs",
    "playground_shares",
    "playground_conversations",
    "registration_rate_limits",
    "email_settings",
    "network_settings",
    "site_settings",
    "authentication_settings",
    "auth_challenges",
    "auth_challenge_rate_limits",
    "models",
    "model_sync_runs",
    "model_sync_items",
    "wallet_ledger_entries",
    "topup_orders",
    "topup_payment_events",
    "payment_settings",
    "redemption_batches",
    "redemption_codes",
    "subscription_plans",
    "subscription_plan_prices",
    "subscription_orders",
    "subscription_payment_events",
    "user_subscriptions",
    "invite_rebate_events",
    "balance_alert_settings",
    "balance_alert_events",
    "subscription_balance_alert_events",
    "routes",
    "route_channels",
    "proxies",
    "async_tasks",
    "async_task_submission_claims",
    "async_task_billings",
    "debug_trace_settings",
    "debug_traces",
    "debug_trace_attempts",
    "debug_trace_snapshots",
    "debug_trace_snapshot_access_audits",
    "organizations",
    "organization_teams",
    "organization_memberships",
    "organization_custom_roles",
    "organization_custom_role_permissions",
    "organization_entitlements",
    "organization_invitations",
    "organization_wallets",
    "organization_wallet_ledger_entries",
    "organization_budget_policies",
    "organization_budget_windows",
    "billing_organization_reservations",
    "organization_audit_logs",
    "organization_sso_policies",
    "organization_scim_tokens",
    "organization_scim_user_resources",
    "organization_scim_group_resources",
    "organization_scim_audit_logs",
    "platform_audit_logs",
    "request_outcome_logs",
    "user_notification_events",
    "user_notification_receipts",
    "analytics_export_outbox_events",
    "refund_requests",
    "refund_provider_events",
];

#[test]
fn refund_table_idens_are_stable() {
    assert_eq!(refund_requests::Entity.to_string(), "refund_requests");
    assert_eq!(
        refund_provider_events::Entity.to_string(),
        "refund_provider_events"
    );
}

struct SlowMigrator;

impl MigratorTrait for SlowMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(SlowMigration)]
    }
}

#[derive(DeriveMigrationName)]
struct SlowMigration;

#[async_trait::async_trait]
impl MigrationTrait for SlowMigration {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok(())
    }
}

#[tokio::test]
async fn standard_startup_installs_core_schema() -> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;
    let manager = SchemaManager::new(pool.connection());

    assert!(manager.has_table("seaql_migrations").await?);
    assert!(manager.has_table("users").await?);
    assert!(manager.has_table("organization_sso_policies").await?);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn organization_sso_policy_migration_backfills_existing_organizations()
-> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = connect(&database_options).await?;
    Migrator::up(pool.connection(), Some(97)).await?;

    let group = group_active_model("organization-policy-backfill", "策略回填测试分组")
        .insert(pool.connection())
        .await?;
    let mut owner = user_active_model(
        group.id,
        "organization-policy-backfill-owner",
        "organization-policy-backfill@example.com",
        "organization-policy-backfill-aff",
    );
    owner.status = Set(1);
    let owner = owner.insert(pool.connection()).await?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let organization = organizations::ActiveModel {
        organization_key: Set("11111111111111111111111111111111".to_owned()),
        name: Set("Policy Backfill".to_owned()),
        slug: Set("policy-backfill".to_owned()),
        status: Set(1),
        created_by_user_id: Set(owner.id),
        version: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        closed_at: Set(None),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    Migrator::up(pool.connection(), Some(98)).await?;
    let policy = organization_sso_policies::Entity::find_by_id(organization.id)
        .one(pool.connection())
        .await?
        .expect("历史企业必须回填 SSO 策略");
    assert_eq!(policy.enforcement_policy, 1);
    assert!(!policy.recovery_ready);
    assert_eq!(policy.version, 1);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn standard_startup_installs_oauth_login_schema_and_indexes() -> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;
    let manager = SchemaManager::new(pool.connection());

    for table in [
        "oauth_login_providers",
        "user_oauth_identities",
        "oauth_login_transactions",
    ] {
        assert!(manager.has_table(table).await?, "缺少数据表 {table}");
    }
    for (table, index) in [
        ("user_oauth_identities", "uq_user_oauth_identities_subject"),
        (
            "user_oauth_identities",
            "uq_user_oauth_identities_user_provider",
        ),
        (
            "oauth_login_transactions",
            "uq_oauth_login_transactions_state",
        ),
        (
            "oauth_login_transactions",
            "uq_oauth_login_transactions_ticket",
        ),
        (
            "oauth_login_transactions",
            "idx_oauth_login_transactions_expires",
        ),
        (
            "oauth_login_transactions",
            "idx_oauth_login_transactions_ticket_expires",
        ),
    ] {
        assert!(manager.has_index(table, index).await?, "缺少索引 {index}");
    }
    let providers = oauth_login_providers::Entity::find()
        .all(pool.connection())
        .await?;
    let mut provider_names = providers
        .iter()
        .map(|provider| provider.provider.as_str())
        .collect::<Vec<_>>();
    provider_names.sort_unstable();
    assert_eq!(
        provider_names,
        [
            "discord", "github", "google", "linuxdo", "oidc", "telegram", "wechat"
        ]
    );
    let linuxdo = providers
        .iter()
        .find(|provider| provider.provider == "linuxdo")
        .expect("应预置 LinuxDO Provider");
    assert_eq!(
        linuxdo.issuer_url.as_deref(),
        Some("https://connect.linux.do/")
    );
    let wechat = providers
        .iter()
        .find(|provider| provider.provider == "wechat")
        .expect("应预置微信 Provider");
    assert!(!wechat.enabled);
    assert_eq!(wechat.client_id, None);
    assert_eq!(wechat.issuer_url, None);
    assert_eq!(wechat.client_secret, None);
    assert_eq!(wechat.version, 1);
    let telegram = providers
        .iter()
        .find(|provider| provider.provider == "telegram")
        .expect("应预置 Telegram Provider");
    assert!(!telegram.enabled);
    assert_eq!(telegram.client_id, None);
    assert_eq!(
        telegram.issuer_url.as_deref(),
        Some("https://oauth.telegram.org")
    );
    assert_eq!(telegram.client_secret, None);
    assert_eq!(telegram.version, 1);
    let google = providers
        .iter()
        .find(|provider| provider.provider == "google")
        .expect("预置 Google Provider");
    assert!(!google.enabled);
    assert_eq!(google.client_id, None);
    assert_eq!(
        google.issuer_url.as_deref(),
        Some("https://accounts.google.com")
    );
    assert_eq!(google.client_secret, None);
    assert_eq!(google.version, 1);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn standard_startup_installs_custom_oauth2_provider_schema() -> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;
    let manager = SchemaManager::new(pool.connection());

    assert!(manager.has_table("custom_oauth2_providers").await?);
    for column in [
        "provider_key",
        "display_name",
        "client_id",
        "authorization_endpoint",
        "token_endpoint",
        "userinfo_endpoint",
        "scope",
        "subject_field",
        "enabled",
        "client_secret",
        "version",
        "created_at",
        "updated_at",
    ] {
        assert!(
            manager
                .has_column("custom_oauth2_providers", column)
                .await?,
            "缺少自定义 OAuth2 Provider 列 {column}"
        );
    }
    for table in [
        "custom_oauth2_identities",
        "custom_oauth2_login_transactions",
    ] {
        assert!(manager.has_table(table).await?, "缺少数据表 {table}");
    }
    for (table, index) in [
        (
            "custom_oauth2_identities",
            "uq_custom_oauth2_identities_subject",
        ),
        (
            "custom_oauth2_identities",
            "uq_custom_oauth2_identities_user_provider",
        ),
        (
            "custom_oauth2_login_transactions",
            "uq_custom_oauth2_login_transactions_state",
        ),
        (
            "custom_oauth2_login_transactions",
            "uq_custom_oauth2_login_transactions_ticket",
        ),
        (
            "custom_oauth2_login_transactions",
            "idx_custom_oauth2_login_transactions_expires",
        ),
        (
            "custom_oauth2_login_transactions",
            "idx_custom_oauth2_login_transactions_ticket_expires",
        ),
    ] {
        assert!(manager.has_index(table, index).await?, "缺少索引 {index}");
    }
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn standard_startup_installs_organization_budget_schema() -> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;
    let manager = SchemaManager::new(pool.connection());

    for table in [
        "organization_budget_policies",
        "organization_budget_windows",
        "billing_organization_reservations",
    ] {
        assert!(manager.has_table(table).await?, "缺少数据表 {table}");
    }
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn standard_startup_installs_organization_custom_role_schema() -> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;
    let manager = SchemaManager::new(pool.connection());

    for table in [
        "organization_custom_roles",
        "organization_custom_role_permissions",
    ] {
        assert!(manager.has_table(table).await?, "缺少数据表 {table}");
    }
    for (table, index) in [
        (
            "organization_custom_roles",
            "uq_organization_custom_roles_key",
        ),
        (
            "organization_custom_roles",
            "uq_organization_custom_roles_scope_name",
        ),
        (
            "organization_custom_roles",
            "idx_organization_custom_roles_scope_status",
        ),
        (
            "organization_custom_roles",
            "uq_organization_custom_roles_scope_id",
        ),
        (
            "organization_memberships",
            "idx_organization_memberships_custom_role",
        ),
        (
            "organization_custom_role_permissions",
            "uq_organization_custom_role_permissions_role_code",
        ),
    ] {
        assert!(manager.has_index(table, index).await?, "缺少索引 {index}");
    }
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn standard_startup_installs_organization_sso_schema() -> Result<(), Box<dyn Error>> {
    let database_options = DatabaseOptions::new("sqlite::memory:")?;
    let pool = crate::connect_and_migrate(&database_options, MigrationOptions::default()).await?;
    let manager = SchemaManager::new(pool.connection());

    for table in [
        "organization_sso_providers",
        "organization_sso_domains",
        "organization_sso_identities",
        "organization_sso_login_transactions",
    ] {
        assert!(
            manager.has_table(table).await?,
            "缺少企业 SSO 数据表 {table}"
        );
    }
    for (table, index) in [
        (
            "organization_sso_providers",
            "uq_organization_sso_providers_scope_key",
        ),
        (
            "organization_sso_providers",
            "uq_organization_sso_providers_scope_id",
        ),
        (
            "organization_sso_providers",
            "uq_organization_sso_providers_active",
        ),
        (
            "organization_sso_domains",
            "uq_organization_sso_domains_scope_id",
        ),
        (
            "organization_sso_domains",
            "uq_organization_sso_domains_active",
        ),
        (
            "organization_sso_identities",
            "uq_organization_sso_identities_subject",
        ),
        (
            "organization_sso_identities",
            "uq_organization_sso_identities_membership",
        ),
        (
            "organization_sso_identities",
            "uq_organization_sso_identities_scope_id",
        ),
        (
            "organization_sso_login_transactions",
            "uq_organization_sso_login_transactions_state",
        ),
        (
            "organization_sso_login_transactions",
            "uq_organization_sso_login_transactions_ticket",
        ),
        (
            "organization_sso_login_transactions",
            "uq_organization_sso_login_transactions_assertion",
        ),
        (
            "organization_sso_login_transactions",
            "idx_organization_sso_login_transactions_expires",
        ),
        (
            "organization_sso_login_transactions",
            "idx_organization_sso_login_transactions_ticket_expires",
        ),
    ] {
        assert!(
            manager.has_index(table, index).await?,
            "缺少企业 SSO 索引 {index}"
        );
    }
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn credential_oauth_revision_migration_backfills_existing_rows() -> Result<(), Box<dyn Error>>
{
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(37)).await?;
    let channel = channel_active_model("oauth-revision-backfill")
        .insert(pool.connection())
        .await?;
    let credential_id = 90_038_i64;
    let mut credential = credential_active_model(channel.id, "oauth-revision-ciphertext");
    credential.id = Set(credential_id);
    credentials::Entity::insert(credential)
        .exec_without_returning(pool.connection())
        .await?;

    Migrator::up(pool.connection(), None).await?;
    let credential = credentials::Entity::find_by_id(credential_id)
        .one(pool.connection())
        .await?
        .expect("迁移后的凭据必须保留");
    assert_eq!(credential.oauth_revision, 0);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn credential_oauth_expiration_migration_preserves_existing_rows()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(38)).await?;
    let channel = channel_active_model("oauth-expiration-backfill")
        .insert(pool.connection())
        .await?;
    let credential_id = 90_039_i64;
    let mut credential = credential_active_model(channel.id, "oauth-expiration-ciphertext");
    credential.id = Set(credential_id);
    credential.kind = Set(CredentialKind::Oauth.as_str().to_owned());
    credential.oauth_provider = Set(Some("codex".to_owned()));
    credentials::Entity::insert(credential)
        .exec_without_returning(pool.connection())
        .await?;

    Migrator::up(pool.connection(), None).await?;
    let manager = SchemaManager::new(pool.connection());
    assert!(
        manager
            .has_index("credentials", "idx_credentials_oauth_refresh_due")
            .await?
    );
    let credential = credentials::Entity::find_by_id(credential_id)
        .one(pool.connection())
        .await?
        .expect("迁移后的凭据必须保留");
    assert_eq!(credential.oauth_expires_at_epoch_seconds, None);

    let mut valid = credential.into_active_model();
    valid.oauth_expires_at_epoch_seconds = Set(Some(1_800_000_000));
    let valid = valid.update(pool.connection()).await?;
    assert_eq!(valid.oauth_expires_at_epoch_seconds, Some(1_800_000_000));
    let mut invalid = valid.into_active_model();
    invalid.oauth_expires_at_epoch_seconds = Set(Some(-1));
    assert!(invalid.update(pool.connection()).await.is_err());

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn credential_oauth_pending_migration_backfills_existing_rows() -> Result<(), Box<dyn Error>>
{
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(47)).await?;
    let channel = channel_active_model("oauth-pending-backfill")
        .insert(pool.connection())
        .await?;
    let credential_id = 90_040_i64;
    let mut credential = credential_active_model(channel.id, "oauth-pending-ciphertext");
    credential.id = Set(credential_id);
    credential.kind = Set(CredentialKind::Oauth.as_str().to_owned());
    credential.oauth_provider = Set(Some("codex".to_owned()));
    credentials::Entity::insert(credential)
        .exec_without_returning(pool.connection())
        .await?;

    Migrator::up(pool.connection(), None).await?;
    let credential = credentials::Entity::find_by_id(credential_id)
        .one(pool.connection())
        .await?
        .expect("迁移后的凭据必须保留");
    assert!(!credential.oauth_token_pending);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn spark_shadow_shape_migration_preserves_legacy_rows_and_guards_new_writes()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(68)).await?;
    let channel = channel_active_model("spark-shadow-shape")
        .insert(pool.connection())
        .await?;
    let parent = oauth_credential_active_model(channel.id, "spark-parent")
        .insert(pool.connection())
        .await?;

    // 旧脚手架可能留下 parent + global 密文，迁移只能保留并阻止继续写入。
    let mut legacy = oauth_credential_active_model(channel.id, "legacy-shadow-ciphertext");
    legacy.parent_id = Set(Some(parent.id));
    let legacy = legacy.insert(pool.connection()).await?;

    Migrator::up(pool.connection(), None).await?;
    let manager = SchemaManager::new(pool.connection());
    assert!(
        manager
            .has_index("credentials", "uq_credentials_active_spark_parent")
            .await?
    );
    let preserved = credentials::Entity::find_by_id(legacy.id)
        .one(pool.connection())
        .await?
        .expect("旧脚手架行必须保留供管理员删除重建");
    assert_eq!(preserved.quota_dimension, "global");

    let mut rejected = oauth_credential_active_model(channel.id, "new-invalid-shadow");
    rejected.parent_id = Set(Some(parent.id));
    assert!(rejected.insert(pool.connection()).await.is_err());

    let mut deleted_legacy = preserved.into_active_model();
    deleted_legacy.deleted_at = Set(Some(historical_audit_time()));
    deleted_legacy.update(pool.connection()).await?;

    let shadow = spark_shadow_active_model(channel.id, parent.id, "spark-shadow-reference");
    shadow.clone().insert(pool.connection()).await?;
    assert!(shadow.insert(pool.connection()).await.is_err());

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn token_window_migration_backfills_only_active_requests() -> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(48)).await?;
    let group_id = insert_legacy_group(
        pool.connection(),
        "token-window-backfill",
        "令牌窗口回填分组",
    )
    .await?;
    let user = user_active_model(
        group_id,
        "token-window-user",
        "token-window@example.com",
        "invite-token-window",
    )
    .insert(pool.connection())
    .await?;
    let token_id = insert_legacy_token(
        pool.connection(),
        90_048,
        token_active_model(user.id, group_id, TOKEN_HASH_A, "token-window-token"),
    )
    .await?;
    let request_key = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";
    let request_created_at = historical_audit_time();
    let mut request = reservation_active_model(request_key, user.id, token_id, group_id, 80, 80);
    request.created_at = Set(request_created_at);
    // 历史 schema 尚无组织列，插入时不能让 ORM 返回并投影当前实体。
    billing_reservations::Entity::insert(request)
        .exec_without_returning(pool.connection())
        .await?;
    let batch_key = "dededededededededededededededede";
    let mut batch = reservation_active_model(batch_key, user.id, token_id, group_id, 90, 90);
    batch.reservation_kind = Set(2);
    billing_reservations::Entity::insert(batch)
        .exec_without_returning(pool.connection())
        .await?;

    Migrator::up(pool.connection(), None).await?;
    let manager = SchemaManager::new(pool.connection());
    assert!(
        manager
            .has_index(
                "billing_reservations",
                "idx_billing_reservations_token_status"
            )
            .await?
    );
    let snapshot = billing_token_window_reservations::Entity::find_by_id(
        BillingReservationKey::parse(request_key)?,
    )
    .one(pool.connection())
    .await?
    .expect("升级前在途普通请求必须获得兼容快照");
    assert_eq!(snapshot.reserved_quota, 80);
    assert_eq!(snapshot.window_5h_start, request_created_at);
    assert_eq!(snapshot.window_1d_start, request_created_at);
    assert_eq!(snapshot.window_7d_start, request_created_at);
    assert_eq!(snapshot.created_at, request_created_at);
    assert_eq!(snapshot.updated_at, request_created_at);
    assert!(
        billing_token_window_reservations::Entity::find_by_id(BillingReservationKey::parse(
            batch_key
        )?)
        .one(pool.connection())
        .await?
        .is_none()
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn group_window_migration_backfills_state_and_only_active_requests()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(49)).await?;
    let group_id = insert_legacy_group(
        pool.connection(),
        "group-window-backfill",
        "分组窗口回填分组",
    )
    .await?;
    let user = user_active_model(
        group_id,
        "group-window-user",
        "group-window@example.com",
        "invite-group-window",
    )
    .insert(pool.connection())
    .await?;
    let token_id = insert_legacy_token(
        pool.connection(),
        90_049,
        token_active_model(user.id, group_id, TOKEN_HASH_B, "group-window-token"),
    )
    .await?;
    let request_key = "efefefefefefefefefefefefefefefef";
    let request_created_at = historical_audit_time();
    let mut request = reservation_active_model(request_key, user.id, token_id, group_id, 70, 70);
    request.created_at = Set(request_created_at);
    // 历史 schema 尚无组织列，插入时不能让 ORM 返回并投影当前实体。
    billing_reservations::Entity::insert(request)
        .exec_without_returning(pool.connection())
        .await?;
    let batch_key = "fafafafafafafafafafafafafafafafa";
    let mut batch = reservation_active_model(batch_key, user.id, token_id, group_id, 90, 90);
    batch.reservation_kind = Set(2);
    billing_reservations::Entity::insert(batch)
        .exec_without_returning(pool.connection())
        .await?;

    Migrator::up(pool.connection(), None).await?;
    let manager = SchemaManager::new(pool.connection());
    assert!(
        manager
            .has_index(
                "billing_reservations",
                "idx_billing_reservations_group_status"
            )
            .await?
    );
    let group = groups::Entity::find_by_id(group_id)
        .one(pool.connection())
        .await?
        .expect("升级后的分组必须保留");
    assert_eq!(
        [group.daily_usage, group.weekly_usage, group.monthly_usage],
        [0, 0, 0]
    );
    for (cycle, started_at) in [
        (SubscriptionCycle::Daily, group.daily_window_start),
        (SubscriptionCycle::Weekly, group.weekly_window_start),
        (SubscriptionCycle::Monthly, group.monthly_window_start),
    ] {
        let seconds = <u64 as std::convert::TryFrom<i64>>::try_from(started_at.unix_timestamp())?;
        assert_eq!(
            SubscriptionWindow::initial(cycle, seconds)?.started_at(),
            seconds
        );
    }
    let snapshot = billing_group_window_reservations::Entity::find_by_id(
        BillingReservationKey::parse(request_key)?,
    )
    .one(pool.connection())
    .await?
    .expect("升级前在途普通请求必须获得分组兼容快照");
    assert_eq!(snapshot.reserved_quota, 70);
    assert_eq!(snapshot.daily_window_start, request_created_at);
    assert_eq!(snapshot.weekly_window_start, request_created_at);
    assert_eq!(snapshot.monthly_window_start, request_created_at);
    assert_eq!(snapshot.created_at, request_created_at);
    assert_eq!(snapshot.updated_at, request_created_at);
    assert!(
        billing_group_window_reservations::Entity::find_by_id(BillingReservationKey::parse(
            batch_key
        )?)
        .one(pool.connection())
        .await?
        .is_none()
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn billing_reservation_kind_migration_backfills_existing_rows() -> Result<(), Box<dyn Error>>
{
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(45)).await?;
    let group_id = insert_legacy_group(
        pool.connection(),
        "reservation-kind-backfill",
        "预留用途回填分组",
    )
    .await?;
    let user = user_active_model(
        group_id,
        "reservation-kind-user",
        "reservation-kind@example.com",
        "invite-reservation-kind",
    )
    .insert(pool.connection())
    .await?;
    let token_id = insert_legacy_token(
        pool.connection(),
        90_045,
        token_active_model(user.id, group_id, TOKEN_HASH_A, "reservation-kind-token"),
    )
    .await?;
    let reservation_id = "abababababababababababababababab";
    // 旧 schema 尚无用途列，插入时不能让 ORM 返回并投影新实体。
    billing_reservations::Entity::insert(reservation_active_model(
        reservation_id,
        user.id,
        token_id,
        group_id,
        100,
        100,
    ))
    .exec_without_returning(pool.connection())
    .await?;

    Migrator::up(pool.connection(), None).await?;
    let reservation =
        billing_reservations::Entity::find_by_id(BillingReservationKey::parse(reservation_id)?)
            .one(pool.connection())
            .await?
            .expect("迁移后的计费预留必须保留");
    assert_eq!(reservation.reservation_kind, 1);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn site_authentication_migration_moves_legacy_registration_policy()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(19)).await?;
    let group_id =
        insert_legacy_group(pool.connection(), "legacy-registration", "旧注册分组").await?;
    option_active_model(
        "system.registration.policy.v1",
        &serde_json::json!({
            "version": 1,
            "enabled": true,
            "default_group_id": group_id,
            "initial_quota": 250,
            "email_required": true,
            "rate_limit_attempts": 7,
            "rate_limit_window_seconds": 1800
        })
        .to_string(),
    )
    .insert(pool.connection())
    .await?;

    Migrator::up(pool.connection(), None).await?;
    let settings = authentication_settings::Entity::find_by_id(1_i16)
        .one(pool.connection())
        .await?
        .expect("认证设置固定记录必须存在");
    assert!(settings.password_login_enabled);
    assert!(settings.registration_enabled);
    assert_eq!(settings.registration_default_group_id, Some(group_id));
    assert_eq!(settings.registration_initial_quota, 250);
    assert_eq!(settings.invitation_rebate_quota, 0);
    assert!(settings.registration_email_required);
    assert_eq!(settings.registration_rate_limit_attempts, 7);
    assert_eq!(settings.registration_rate_limit_window_seconds, 1_800);
    assert!(
        options::Entity::find_by_id("system.registration.policy.v1".to_owned())
            .one(pool.connection())
            .await?
            .is_none()
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn wallet_migration_backfills_existing_nonzero_balances() -> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(26)).await?;
    let group_id = insert_legacy_group(pool.connection(), "legacy-wallet", "旧钱包分组").await?;
    let mut user = user_active_model(
        group_id,
        "legacy-wallet-user",
        "legacy-wallet@example.com",
        "invite-legacy-wallet",
    );
    user.quota = Set(250);
    // 历史 schema 尚无后续用户列，只取自增主键以避免 RETURNING 读取最新实体字段。
    let user_id = users::Entity::insert(user)
        .exec(pool.connection())
        .await?
        .last_insert_id;

    Migrator::up(pool.connection(), None).await?;
    let entries = wallet_ledger_entries::Entity::find()
        .filter(wallet_ledger_entries::Column::UserId.eq(user_id))
        .all(pool.connection())
        .await?;
    assert_eq!(entries.len(), 1);
    let opening = &entries[0];
    assert_eq!(
        opening.event_key.as_str(),
        format!("0000000000000001{user_id:016x}")
    );
    assert_eq!(opening.entry_type, 1);
    assert_eq!(opening.quota_delta, 250);
    assert_eq!(opening.balance_before, 0);
    assert_eq!(opening.balance_after, 250);
    assert!(opening.actor_user_id.is_none());
    assert!(opening.reason.is_none());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn wallet_topup_migration_refuses_down_with_topup_entries() -> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), None).await?;
    let group = group_active_model("topup-down-guard", "充值降级保护")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "topup-down-user",
        "topup-down@example.com",
        "invite-topup-down",
    );
    user.quota = Set(25);
    let user = user.insert(pool.connection()).await?;
    wallet_ledger_entries::ActiveModel {
        event_key: Set(WalletLedgerKey::parse("dddddddddddddddddddddddddddddddd")?),
        user_id: Set(user.id),
        actor_user_id: Set(None),
        entry_type: Set(3),
        quota_delta: Set(25),
        balance_before: Set(0),
        balance_after: Set(25),
        reason: Set(None),
        created_at: Set(historical_audit_time()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let manager = SchemaManager::new(pool.connection());
    let error = m20260801_000029_extend_wallet_ledger_topup::Migration
        .down(&manager)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        DbErr::Custom(message) if message == "存在充值到账账本，无法回退钱包账本类型"
    ));
    assert!(manager.has_table("wallet_ledger_entries").await?);
    assert!(!manager.has_table("wallet_ledger_entries_next").await?);
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(wallet_ledger_entries::Column::EntryType.eq(3))
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn model_price_expression_migration_refuses_down_with_expression_rows()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), None).await?;
    model_prices::ActiveModel {
        model: Set(SensitiveString::from("expression-down-model")),
        billing_mode: Set(3),
        input_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        output_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        billing_expression: Set(Some(SensitiveString::from(r#"tier("base", p)"#))),
        version: Set(1),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let manager = SchemaManager::new(pool.connection());
    let error = m20260806_000044_add_model_price_expression::Migration
        .down(&manager)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        DbErr::Custom(message) if message == "存在表达式模型价格，无法回退计费表达式字段"
    ));
    assert!(manager.has_table("model_prices").await?);
    assert!(!manager.has_table("model_prices_next").await?);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn wallet_redemption_migration_refuses_down_with_redemption_entries()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), None).await?;
    let group = group_active_model("redemption-down-guard", "兑换码降级保护")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "redemption-down-user",
        "redemption-down@example.com",
        "invite-redemption-down",
    );
    user.quota = Set(25);
    let user = user.insert(pool.connection()).await?;
    wallet_ledger_entries::ActiveModel {
        event_key: Set(WalletLedgerKey::parse("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee")?),
        user_id: Set(user.id),
        actor_user_id: Set(None),
        entry_type: Set(4),
        quota_delta: Set(25),
        balance_before: Set(0),
        balance_after: Set(25),
        reason: Set(None),
        created_at: Set(historical_audit_time()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let manager = SchemaManager::new(pool.connection());
    let error = m20260801_000031_extend_wallet_ledger_redemption::Migration
        .down(&manager)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        DbErr::Custom(message) if message == "存在兑换码到账账本，无法回退钱包账本类型"
    ));
    assert!(manager.has_table("wallet_ledger_entries").await?);
    assert!(!manager.has_table("wallet_ledger_entries_next").await?);
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(wallet_ledger_entries::Column::EntryType.eq(4))
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn wallet_invite_rebate_migration_refuses_down_with_invite_rebate_entries()
-> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), None).await?;
    let group = group_active_model("invite-rebate-down-guard", "邀请返利降级保护")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "invite-rebate-down-user",
        "invite-rebate-down@example.com",
        "invite-invite-rebate-down",
    );
    user.quota = Set(25);
    let user = user.insert(pool.connection()).await?;
    wallet_ledger_entries::ActiveModel {
        event_key: Set(WalletLedgerKey::parse("ffffffffffffffffffffffffffffffff")?),
        user_id: Set(user.id),
        actor_user_id: Set(None),
        entry_type: Set(5),
        quota_delta: Set(25),
        balance_before: Set(0),
        balance_after: Set(25),
        reason: Set(None),
        created_at: Set(historical_audit_time()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let manager = SchemaManager::new(pool.connection());
    let error = m20260801_000034_extend_wallet_ledger_invite_rebate::Migration
        .down(&manager)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        DbErr::Custom(message) if message == "存在邀请返利到账账本，无法回退钱包账本类型"
    ));
    assert!(manager.has_table("wallet_ledger_entries").await?);
    assert!(!manager.has_table("wallet_ledger_entries_next").await?);
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(wallet_ledger_entries::Column::EntryType.eq(5))
            .count(pool.connection())
            .await?,
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn migration_deadline_fails_without_retrying() -> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    let migration_options = MigrationOptions::new(Duration::from_millis(1))?;

    let error = run_with::<SlowMigrator>(&pool, migration_options)
        .await
        .unwrap_err();
    assert!(matches!(error, DatabaseError::MigrationTimeout { .. }));

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn generated_active_models_redact_sensitive_payloads() {
    let credential = credential_active_model(1, "credential-ciphertext");
    let user = user_active_model(1, "user", "user@example.com", "invite-a");
    let mut token = token_active_model(1, 1, TOKEN_HASH_A, "token-a");
    token.allow_ips = Set(Some(
        TokenIpAllowlist::validate(Json::Array(vec![Json::String("192.0.2.0/24".to_owned())]))
            .unwrap(),
    ));
    let option = option_active_model("site.name", "option-value");
    let reservation = reservation_active_model(RESERVATION_KEY_A, 1, 1, 1, 10, 10);
    let checkpoint = billing_batch_checkpoints::ActiveModel {
        writer_key: Set(BillingBatchWriterKey::parse(&"c".repeat(32)).unwrap()),
        last_start_sequence: Set(1),
        last_end_sequence: Set(1),
        last_event_count: Set(1),
        last_fingerprint: Set(BillingBatchFingerprint::parse(&"d".repeat(64)).unwrap()),
        ..Default::default()
    };
    let model_price = model_prices::ActiveModel {
        model: Set(SensitiveString::from("debug-model-secret")),
        billing_mode: Set(1),
        input_price: Set(SensitiveDecimal::from(Decimal::from(123_456_789_i64))),
        output_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        version: Set(1),
        ..Default::default()
    };
    let playground_share = playground_shares::ActiveModel {
        owner_user_id: Set(1),
        token_hash: Set(TokenHash::parse(TOKEN_HASH_E).unwrap()),
        snapshot: Set(SensitiveJson::from(serde_json::json!({
            "content": "share-snapshot-secret"
        }))),
        created_at: Set(historical_audit_time()),
        expires_at: Set(future_expiration()),
        revoked_at: Set(None),
        ..Default::default()
    };
    let playground_conversation = playground_conversations::ActiveModel {
        conversation_id: Set(
            PlaygroundConversationKey::parse(PLAYGROUND_CONVERSATION_KEY).unwrap(),
        ),
        owner_user_id: Set(1),
        title: Set(SensitiveString::from("conversation-title-secret")),
        models: Set(SensitiveJson::from(serde_json::json!([
            "conversation-model-secret"
        ]))),
        snapshot: Set(SensitiveJson::from(serde_json::json!({
            "content": "conversation-snapshot-secret"
        }))),
        revision: Set(1),
        created_at: Set(historical_audit_time()),
        updated_at: Set(historical_audit_time()),
    };
    let mut channel = channel_active_model("debug-channel");
    channel.base_url = Set(Some(
        ChannelBaseUrl::parse("https://api.example.com/channel-secret").unwrap(),
    ));
    channel.header_override = Set(HeaderOverrides::validate(Json::Object(
        [(
            "x-trace".to_owned(),
            Json::String("channel-secret".to_owned()),
        )]
        .into_iter()
        .collect(),
    ))
    .unwrap());
    let topup_order = topup_orders::ActiveModel {
        order_key: Set(TOPUP_ORDER_KEY_A.to_owned()),
        user_id: Set(1),
        provider: Set("stripe".to_owned()),
        payment_method: Set(Some("card".to_owned())),
        provider_order_id: Set(Some(SensitiveString::from("provider-order-secret"))),
        trade_no: Set(Some(SensitiveString::from("trade-no-secret"))),
        status: Set(1),
        amount_minor: Set(100),
        currency: Set("USD".to_owned()),
        quota_amount: Set(1_000),
        idempotency_key: Set(TOPUP_IDEMPOTENCY_KEY_A.to_owned()),
        created_at: Set(historical_audit_time()),
        updated_at: Set(historical_audit_time()),
        ..Default::default()
    };
    let signature_fingerprint_secret = "e".repeat(64);
    let payload_hash_secret = "f".repeat(64);
    let topup_event = topup_payment_events::ActiveModel {
        event_key: Set(TOPUP_EVENT_KEY_A.to_owned()),
        order_id: Set(1),
        provider: Set("stripe".to_owned()),
        provider_event_id: Set(SensitiveString::from(TOPUP_PROVIDER_EVENT_ID_A)),
        trade_no: Set(Some(SensitiveString::from("event-trade-no-secret"))),
        amount_minor: Set(Some(100)),
        currency: Set(Some("USD".to_owned())),
        payment_method: Set(Some("card".to_owned())),
        event_type: Set(1),
        signature_key_fingerprint: Set(SensitiveString::from(signature_fingerprint_secret.clone())),
        payload_sha256: Set(SensitiveString::from(payload_hash_secret.clone())),
        received_at: Set(historical_audit_time()),
        created_at: Set(historical_audit_time()),
        ..Default::default()
    };

    for (rendered, secret) in [
        (format!("{credential:?}"), "credential-ciphertext"),
        (format!("{user:?}"), TEST_PASSWORD_HASH),
        (format!("{token:?}"), TOKEN_HASH_A),
        (format!("{option:?}"), "option-value"),
        (format!("{channel:?}"), "channel-secret"),
        (format!("{reservation:?}"), RESERVATION_KEY_A),
        (format!("{checkpoint:?}"), &"c".repeat(32)),
        (format!("{checkpoint:?}"), &"d".repeat(64)),
        (format!("{model_price:?}"), "debug-model-secret"),
        (format!("{model_price:?}"), "123456789"),
        (format!("{playground_share:?}"), TOKEN_HASH_E),
        (format!("{playground_share:?}"), "share-snapshot-secret"),
        (
            format!("{playground_conversation:?}"),
            PLAYGROUND_CONVERSATION_KEY,
        ),
        (
            format!("{playground_conversation:?}"),
            "conversation-title-secret",
        ),
        (
            format!("{playground_conversation:?}"),
            "conversation-model-secret",
        ),
        (
            format!("{playground_conversation:?}"),
            "conversation-snapshot-secret",
        ),
        (format!("{topup_order:?}"), "provider-order-secret"),
        (format!("{topup_order:?}"), "trade-no-secret"),
        (format!("{topup_event:?}"), TOPUP_PROVIDER_EVENT_ID_A),
        (format!("{topup_event:?}"), "event-trade-no-secret"),
        (
            format!("{topup_event:?}"),
            signature_fingerprint_secret.as_str(),
        ),
        (format!("{topup_event:?}"), payload_hash_secret.as_str()),
    ] {
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(secret));
    }
    assert!(!format!("{token:?}").contains("192.0.2.0"));
}

#[tokio::test]
async fn passkey_authentication_migration_preserves_legacy_counter() -> Result<(), Box<dyn Error>> {
    let pool = connect(&DatabaseOptions::new("sqlite::memory:")?).await?;
    Migrator::up(pool.connection(), Some(72)).await?;

    let group = group_active_model("passkey-migration-group", "Passkey 迁移分组")
        .insert(pool.connection())
        .await?;
    let user = user_active_model(
        group.id,
        "passkey-migration-user",
        "passkey-migration@example.com",
        "invite-passkey-migration",
    )
    .insert(pool.connection())
    .await?;
    passkeys::Entity::insert(passkeys::ActiveModel {
        user_id: Set(user.id),
        credential_id: Set("bGVnYWN5LWNlZGF0".to_owned()),
        credential_id_digest: Set("a".repeat(64)),
        passkey: Set(serde_json::json!({"cred": {"counter": 37}})),
        display_name: Set("旧 Passkey".to_owned()),
        ..Default::default()
    })
    .exec_without_returning(pool.connection())
    .await?;

    Migrator::up(pool.connection(), Some(73)).await?;
    let migrated = passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user.id))
        .one(pool.connection())
        .await?
        .expect("旧 Passkey 必须保留");
    assert_eq!(migrated.sign_count, 37);
    assert_eq!(migrated.anomaly_at, None);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库迁移流水线执行"]
async fn migration_runner_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let is_sqlite = database_url.starts_with("sqlite:");
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        // SQLite 内存库必须固定单连接；网络数据库使用多连接覆盖真实锁竞争。
        max_connections: if is_sqlite { 1 } else { 4 },
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    assert_database_session(pool.connection()).await?;

    Migrator::up(pool.connection(), Some(54)).await?;
    exercise_usage_per_call_migration_upgrade(&pool).await?;
    let pending_migrations = Migrator::get_pending_migrations(pool.connection()).await?;
    assert_eq!(pending_migrations.len(), 42);
    assert_eq!(
        pending_migrations[0].name(),
        "m20260815_000060_secure_debug_trace_snapshots"
    );
    assert_eq!(
        pending_migrations[1].name(),
        "m20260815_000061_create_organization_foundation"
    );
    assert_eq!(
        pending_migrations[2].name(),
        "m20260815_000062_create_organization_entitlements"
    );
    assert_eq!(
        pending_migrations[3].name(),
        "m20260816_000063_create_organization_invitations"
    );
    assert_eq!(
        pending_migrations[4].name(),
        "m20260816_000064_create_organization_wallets"
    );
    assert_eq!(
        pending_migrations[5].name(),
        "m20260816_000065_create_organization_budgets"
    );
    assert_eq!(
        pending_migrations[6].name(),
        "m20260817_000066_add_organization_tokens"
    );
    assert_eq!(
        pending_migrations[7].name(),
        "m20260817_000067_create_organization_activity"
    );
    assert_eq!(
        pending_migrations[8].name(),
        "m20260817_000068_add_client_simulation_observability"
    );
    assert_eq!(
        pending_migrations[9].name(),
        "m20260817_000069_enforce_spark_shadow_shape"
    );
    assert_eq!(
        pending_migrations[10].name(),
        "m20260817_000070_create_oauth_login"
    );
    assert_eq!(
        pending_migrations[11].name(),
        "m20260818_000071_seed_discord_oauth_login"
    );
    assert_eq!(
        pending_migrations[12].name(),
        "m20260818_000072_create_passkeys"
    );
    assert_eq!(
        pending_migrations[13].name(),
        "m20260818_000073_add_passkey_authentication_state"
    );
    assert_eq!(
        pending_migrations[14].name(),
        "m20260819_000074_create_platform_audit"
    );
    assert_eq!(
        pending_migrations[15].name(),
        "m20260819_000075_create_request_outcomes"
    );
    assert_eq!(
        pending_migrations[16].name(),
        "m20260819_000076_create_user_notification_events"
    );
    assert_eq!(
        pending_migrations[17].name(),
        "m20260819_000077_create_analytics_export_outbox"
    );
    assert_eq!(
        pending_migrations[18].name(),
        "m20260820_000078_extend_oauth_login_providers"
    );
    assert_eq!(
        pending_migrations[19].name(),
        "m20260820_000079_seed_wechat_oauth_login"
    );
    assert_eq!(
        pending_migrations[20].name(),
        "m20260820_000080_seed_telegram_oauth_login"
    );
    assert_eq!(
        pending_migrations[21].name(),
        "m20260820_000081_seed_google_oauth_login"
    );
    assert_eq!(
        pending_migrations[22].name(),
        "m20260821_000082_create_user_notification_receipts"
    );
    assert_eq!(
        pending_migrations[23].name(),
        "m20260821_000083_create_subscription_plan_prices"
    );
    assert_eq!(
        pending_migrations[24].name(),
        "m20260821_000084_create_subscription_orders"
    );
    assert_eq!(
        pending_migrations[25].name(),
        "m20260822_000085_create_subscription_payment_events"
    );
    assert_eq!(
        pending_migrations[26].name(),
        "m20260822_000086_create_refund_requests"
    );
    assert_eq!(
        pending_migrations[27].name(),
        "m20260822_000087_add_refund_provider_receipts"
    );
    assert_eq!(
        pending_migrations[28].name(),
        "m20260823_000088_add_refund_settings"
    );
    assert_eq!(
        pending_migrations[29].name(),
        "m20260823_000089_add_refund_approval"
    );
    assert_eq!(
        pending_migrations[30].name(),
        "m20260823_000090_add_refund_payment_reference"
    );
    assert_eq!(
        pending_migrations[31].name(),
        "m20260823_000091_create_custom_oauth2_providers"
    );
    assert_eq!(
        pending_migrations[32].name(),
        "m20260823_000092_create_custom_oauth2_login"
    );
    assert_eq!(
        pending_migrations[33].name(),
        "m20260823_000093_create_organization_custom_roles"
    );
    assert_eq!(
        pending_migrations[34].name(),
        "m20260823_000094_add_organization_custom_role_bindings"
    );
    assert_eq!(
        pending_migrations[35].name(),
        "m20260824_000095_create_organization_sso_persistence"
    );
    assert_eq!(
        pending_migrations[36].name(),
        "m20260825_000096_add_organization_sso_login_material"
    );
    assert_eq!(
        pending_migrations[37].name(),
        "m20260825_000097_add_organization_sso_ticket_email_fingerprint"
    );
    assert_eq!(
        pending_migrations[38].name(),
        "m20260826_000098_create_organization_sso_policies"
    );
    assert_eq!(
        pending_migrations[39].name(),
        "m20260827_000099_create_organization_scim_tokens"
    );
    assert_eq!(
        pending_migrations[40].name(),
        "m20260827_000100_create_organization_scim_user_resources"
    );
    assert_eq!(
        pending_migrations[41].name(),
        "m20260827_000101_create_organization_scim_group_resources"
    );
    assert_eq!(
        pending_migrations[42].name(),
        "m20260828_000102_create_organization_scim_audit_logs"
    );
    let legacy_debug_trace_id = insert_legacy_debug_trace_snapshot(&pool).await?;
    run_with::<Migrator>(&pool, MigrationOptions::default()).await?;
    assert_legacy_debug_trace_snapshot_cleared(&pool, legacy_debug_trace_id).await?;

    assert_core_schema(pool.connection()).await?;
    assert!(
        Migrator::get_pending_migrations(pool.connection())
            .await?
            .is_empty()
    );
    assert_eq!(
        Migrator::get_applied_migrations(pool.connection())
            .await?
            .len(),
        EXPECTED_MIGRATION_COUNT
    );

    eprintln!("迁移冒烟阶段：更新时间行为");
    assert_updated_at_behaviors(pool.connection()).await?;
    eprintln!("迁移冒烟阶段：余额预警迁移");
    exercise_balance_alert_migration(pool.connection()).await?;
    eprintln!("迁移冒烟阶段：调度发件箱仓储");
    exercise_scheduler_outbox_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：令牌鉴权仓储");
    exercise_token_auth_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：额度仓储");
    exercise_quota_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：批量计费仓储");
    exercise_billing_batch_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：模型价格仓储");
    exercise_model_price_repository(&pool).await?;
    // Expression 降级保护由独立测试覆盖；全量回滚前清理业务测试行以允许迁移降级。
    model_prices::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    eprintln!("迁移冒烟阶段：用量日志仓储");
    exercise_usage_log_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：试炼场分享仓储");
    exercise_playground_share_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：试炼场会话仓储");
    exercise_playground_conversation_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：注册仓储");
    exercise_registration_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：认证挑战仓储");
    exercise_auth_challenge_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：Passkey 登录限流仓储");
    exercise_passkey_rate_limit_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：站点设置仓储");
    exercise_site_settings_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：邮件设置仓储");
    exercise_email_settings_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：管理看板仓储");
    exercise_admin_dashboard_repository(&pool).await?;
    eprintln!("迁移冒烟阶段：核心结构校验");
    exercise_core_schema(pool.connection()).await?;
    eprintln!("迁移冒烟阶段：企业仓储");
    exercise_organization_repository(&pool).await?;

    Migrator::down(pool.connection(), None).await?;
    assert_core_tables_absent(pool.connection()).await?;

    // 回滚后的完整重建可发现 down 顺序或残留索引导致的二次部署问题。
    Migrator::up(pool.connection(), None).await?;
    assert_core_schema(pool.connection()).await?;
    Migrator::down(pool.connection(), None).await?;
    assert_core_tables_absent(pool.connection()).await?;

    pool.close().await?;
    Ok(())
}

/// 在第 59 号 schema 写入旧诊断明文，供第 60 号迁移验证清理而非伪加密。
async fn insert_legacy_debug_trace_snapshot(pool: &DatabasePool) -> Result<i64, Box<dyn Error>> {
    let group = group_active_model("debug-snapshot-upgrade-group", "诊断快照升级分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "debug-snapshot-upgrade-user",
        "debug-snapshot-upgrade@example.com",
        "invite-debug-snapshot-upgrade",
    );
    user.status = Set(1);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(
        user.id,
        group.id,
        TOKEN_HASH_F,
        "debug-snapshot-upgrade-token",
    );
    token.status = Set(1);
    let token_id = insert_legacy_token(pool.connection(), 90_059, token).await?;
    let trace = debug_traces::ActiveModel {
        request_id: Set("debug-snapshot-upgrade-request".to_owned()),
        user_id: Set(user.id),
        token_id: Set(token_id),
        group_id: Set(group.id),
        requested_model: Set("gpt-5.5".to_owned()),
        downstream_protocol: Set(1),
        upstream_protocol: Set(1),
        operation: Set(1),
        outcome: Set(1),
        selected_channel_id: Set(Some(101)),
        selected_credential_id: Set(Some(201)),
        routing_elapsed_ms: Set(9),
        attempt_count: Set(1),
        downstream_method: Set(Some("POST".to_owned())),
        downstream_path: Set(Some("/v1/chat/completions".to_owned())),
        downstream_headers_json: Set(Some(
            r#"[{"name":"authorization","value":"<redacted>"}]"#.to_owned(),
        )),
        downstream_body_json: Set(Some(r#"{"content":"legacy-body"}"#.to_owned())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let attempt = debug_trace_attempts::ActiveModel {
        trace_id: Set(trace.id),
        candidate_index: Set(0),
        channel_id: Set(101),
        credential_id: Set(201),
        outcome: Set(1),
        failure_kind: Set(None),
        upstream_status: Set(None),
        retry_decision: Set(false),
        elapsed_ms: Set(8),
        request_method: Set(Some("POST".to_owned())),
        request_url: Set(Some(
            "https://upstream.example/v1/chat/completions".to_owned(),
        )),
        request_headers_json: Set(Some(
            r#"[{"name":"authorization","value":"<redacted>"}]"#.to_owned(),
        )),
        request_body_json: Set(Some(r#"{"content":"legacy-request"}"#.to_owned())),
        response_headers_json: Set(Some(
            r#"[{"name":"content-type","value":"application/json"}]"#.to_owned(),
        )),
        response_body_json: Set(Some(r#"{"content":"legacy-response"}"#.to_owned())),
        response_status: Set(Some(200)),
        response_streamed: Set(false),
        ..Default::default()
    };
    // 历史 schema 尚无第 68 号列，禁止当前实体通过 RETURNING 提前投影新列。
    debug_trace_attempts::Entity::insert(attempt)
        .exec_without_returning(pool.connection())
        .await?;
    Ok(trace.id)
}

async fn assert_legacy_debug_trace_snapshot_cleared(
    pool: &DatabasePool,
    trace_id: i64,
) -> Result<(), Box<dyn Error>> {
    let trace = debug_traces::Entity::find_by_id(trace_id)
        .one(pool.connection())
        .await?
        .expect("升级后必须保留调试追踪元数据");
    assert_eq!(trace.request_id, "debug-snapshot-upgrade-request");
    assert!(trace.downstream_headers_json.is_none());
    assert!(trace.downstream_body_json.is_none());
    let attempt = debug_trace_attempts::Entity::find()
        .filter(debug_trace_attempts::Column::TraceId.eq(trace_id))
        .one(pool.connection())
        .await?
        .expect("升级后必须保留候选时间线");
    assert!(attempt.request_headers_json.is_none());
    assert!(attempt.request_body_json.is_none());
    assert!(attempt.response_headers_json.is_none());
    assert!(attempt.response_body_json.is_none());
    assert_eq!(
        debug_trace_snapshots::Entity::find()
            .count(pool.connection())
            .await?,
        0
    );
    assert_eq!(
        debug_trace_snapshot_access_audits::Entity::find()
            .count(pool.connection())
            .await?,
        0
    );
    Ok(())
}

/// 从第 54 号历史 schema 升级，验证旧用量保留、按次模式放行及自增序列连续。
async fn exercise_usage_per_call_migration_upgrade(
    pool: &DatabasePool,
) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("usage-v55-upgrade-group", "按次迁移升级分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "usage-v55-upgrade-user",
        "usage-v55-upgrade@example.com",
        "invite-usage-v55-upgrade",
    );
    user.status = Set(1);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_E, "usage-v55-upgrade-token");
    token.status = Set(1);
    let token_id = insert_legacy_token(pool.connection(), 90_054, token).await?;
    let principal = GatewayPrincipal::new(
        TokenId::new(token_id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    );
    // 迁移前只能写入第 54 版已有列，不能通过当前实体提前触碰新字段。
    let legacy_event_id = BillingReservationId::new([0xd1; 16])?;
    let legacy_key = BillingReservationKey::parse(&legacy_event_id.persistence_key())?;
    usage_logs::Entity::insert(usage_logs::ActiveModel {
        event_id: Set(legacy_key),
        event_type: Set(1),
        user_id: Set(principal.user_id().get()),
        token_id: Set(principal.token_id().get()),
        group_id: Set(principal.group_id().get()),
        billing_mode: Set(1),
        input_tokens: Set(1),
        output_tokens: Set(1),
        cache_read: Set(0),
        cache_creation_5m: Set(0),
        cache_creation_1h: Set(0),
        reasoning_tokens: Set(0),
        audio_input_tokens: Set(0),
        audio_output_tokens: Set(0),
        usage_source: Set(1),
        usage_semantics: Set(1),
        quota: Set(2),
        created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        ..Default::default()
    })
    .exec_without_returning(pool.connection())
    .await?;

    // 先升级到第 59 号并显式使用当时的列投影，避免当前仓储提前引用第 67 号企业归属列。
    Migrator::up(pool.connection(), Some(5)).await?;
    let per_call_event_id = BillingReservationId::new([0xd2; 16])?;
    let per_call_key = BillingReservationKey::parse(&per_call_event_id.persistence_key())?;
    usage_logs::Entity::insert(usage_logs::ActiveModel {
        event_id: Set(per_call_key),
        event_type: Set(1),
        user_id: Set(principal.user_id().get()),
        token_id: Set(principal.token_id().get()),
        group_id: Set(principal.group_id().get()),
        billing_mode: Set(3),
        input_tokens: Set(0),
        output_tokens: Set(0),
        cache_read: Set(0),
        cache_creation_5m: Set(0),
        cache_creation_1h: Set(0),
        reasoning_tokens: Set(0),
        audio_input_tokens: Set(0),
        audio_output_tokens: Set(0),
        audio_duration_nanoseconds: Set(None),
        video_duration_seconds: Set(Some(8)),
        video_resolution: Set(None),
        request_id: Set(None),
        model: Set(None),
        protocol: Set(None),
        operation: Set(None),
        is_stream: Set(None),
        reasoning_effort: Set(None),
        reasoning_budget_tokens: Set(None),
        first_token_ms: Set(None),
        duration_ms: Set(None),
        usage_source: Set(1),
        usage_semantics: Set(1),
        quota: Set(320_000),
        created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        ..Default::default()
    })
    .exec_without_returning(pool.connection())
    .await?;
    let ids = usage_logs::Entity::find()
        .select_only()
        .column(usage_logs::Column::Id)
        .filter(usage_logs::Column::UserId.eq(user.id))
        .into_tuple::<i64>()
        .all(pool.connection())
        .await?;
    assert_eq!(ids.len(), 2);
    let minimum_id = ids.iter().copied().min().unwrap_or_default();
    let maximum_id = ids.iter().copied().max().unwrap_or_default();
    assert!(maximum_id > minimum_id, "迁移后自增主键必须继续递增");

    usage_logs::Entity::delete_many()
        .filter(usage_logs::Column::UserId.eq(user.id))
        .exec(pool.connection())
        .await?;
    tokens::Entity::delete_by_id(token_id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言真实连接上覆盖 outbox 领取 CAS、失败退避和发布确认，而不只验证建表。
async fn exercise_scheduler_outbox_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    crate::scheduler_outbox::enqueue_scheduler_catalog_change(
        pool.connection(),
        SchedulerCatalogSubject::Channel(ChannelId::new(7_001)?),
        now,
    )
    .await?;
    let first = SchedulerOutboxRepository::new(pool.clone(), Duration::from_secs(5))?;
    let second = SchedulerOutboxRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (first_claim, second_claim) = tokio::join!(first.claim_next(now), second.claim_next(now));
    let mut owner = None;
    let mut empty_count = 0;
    for outcome in [first_claim?, second_claim?] {
        match outcome {
            SchedulerOutboxClaimOutcome::Claimed(lease) => {
                assert!(owner.replace(lease).is_none());
            }
            SchedulerOutboxClaimOutcome::Empty => empty_count += 1,
        }
    }
    assert_eq!(empty_count, 1);
    let owner = owner.expect("跨方言 CAS 竞争必须产生唯一 owner");
    assert_eq!(
        first.mark_published(&owner, now).await?,
        SchedulerOutboxCompletionOutcome::Completed
    );

    crate::scheduler_outbox::enqueue_scheduler_catalog_change(
        pool.connection(),
        SchedulerCatalogSubject::Group(GroupId::new(8_001)?),
        now,
    )
    .await?;
    let SchedulerOutboxClaimOutcome::Claimed(failed) = first.claim_next(now).await? else {
        panic!("第二条 outbox 事件必须可领取");
    };
    assert_eq!(
        first.record_failure(&failed, now).await?,
        SchedulerOutboxCompletionOutcome::Completed
    );
    assert_eq!(
        first.claim_next(now).await?,
        SchedulerOutboxClaimOutcome::Empty
    );
    let retry_at = now + Duration::from_secs(1);
    let SchedulerOutboxClaimOutcome::Claimed(retried) = second.claim_next(retry_at).await? else {
        panic!("失败退避结束后必须允许重新领取");
    };
    assert_eq!(
        second.mark_published(&retried, retry_at).await?,
        SchedulerOutboxCompletionOutcome::Completed
    );
    assert_eq!(
        scheduler_outbox_events::Entity::find()
            .filter(scheduler_outbox_events::Column::Status.eq(3))
            .count(pool.connection())
            .await?,
        2
    );
    assert!(
        second.latest_event_id().await?
            >= <u64 as std::convert::TryFrom<i64>>::try_from(retried.event_id())?
    );
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库模型价格流水线执行"]
async fn model_price_migration_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let is_sqlite = database_url.starts_with("sqlite:");
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        max_connections: if is_sqlite { 1 } else { 4 },
        min_connections: 1,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    let manager = SchemaManager::new(pool.connection());
    let migration = m20260722_000012_create_model_prices::Migration;
    let expression_migration = m20260806_000044_add_model_price_expression::Migration;

    migration.up(&manager).await?;
    expression_migration.up(&manager).await?;
    assert!(manager.has_table("model_prices").await?);
    exercise_model_price_repository(&pool).await?;
    model_prices::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    expression_migration.down(&manager).await?;
    migration.down(&manager).await?;
    assert!(!manager.has_table("model_prices").await?);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库分组计费流水线执行"]
async fn group_pricing_migration_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        min_connections: 1,
        max_connections: 4,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    let manager = SchemaManager::new(pool.connection());
    let groups_migration = m20260718_000001_create_groups::Migration;
    let group_pricing_migration = m20260723_000013_create_group_model_ratios::Migration;

    groups_migration.up(&manager).await?;
    group_pricing_migration.up(&manager).await?;
    assert!(manager.has_table("groups").await?);
    assert!(manager.has_table("group_model_ratios").await?);
    exercise_group_pricing_repository(&pool).await?;
    group_pricing_migration.down(&manager).await?;
    assert!(!manager.has_table("group_model_ratios").await?);
    groups_migration.down(&manager).await?;
    assert!(!manager.has_table("groups").await?);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
#[ignore = "仅由显式启用的隔离数据库用量日志流水线执行"]
async fn usage_log_migration_smoke() -> Result<(), Box<dyn Error>> {
    let database_url = test_database_url();
    validate_destructive_test_target(
        &database_url,
        env::var(ALLOW_DESTRUCTIVE_MIGRATION_TEST).ok().as_deref(),
    )?;
    let options = DatabaseOptions::new(database_url)?.with_pool_options(PoolOptions {
        min_connections: 1,
        max_connections: 4,
        ..PoolOptions::default()
    });
    let pool = connect(&options).await?;
    let manager = SchemaManager::new(pool.connection());
    let groups_migration = m20260718_000001_create_groups::Migration;
    let users_migration = m20260718_000002_create_users::Migration;
    let tokens_migration = m20260718_000005_create_tokens::Migration;
    let usage_migration = m20260723_000014_create_usage_logs::Migration;

    // 仅迁移 usage_logs 的外键依赖，允许本机旧 MySQL 独立验证新表。
    groups_migration.up(&manager).await?;
    users_migration.up(&manager).await?;
    tokens_migration.up(&manager).await?;
    usage_migration.up(&manager).await?;
    assert!(manager.has_table("usage_logs").await?);
    exercise_usage_log_repository(&pool).await?;

    usage_migration.down(&manager).await?;
    tokens_migration.down(&manager).await?;
    users_migration.down(&manager).await?;
    groups_migration.down(&manager).await?;
    assert!(!manager.has_table("usage_logs").await?);

    pool.close().await?;
    Ok(())
}

/// 在三方言隔离数据库中验证高精度价格与显式免费模式读取一致。
async fn exercise_model_price_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    model_prices::ActiveModel {
        model: Set(SensitiveString::from("smoke-priced-model")),
        billing_mode: Set(1),
        input_price: Set(SensitiveDecimal::from(Decimal::new(1, 28))),
        output_price: Set(SensitiveDecimal::from(Decimal::new(25, 1))),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::new(3, 2))),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::new(375, 2))),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::from(6))),
        version: Set(9),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    model_prices::ActiveModel {
        model: Set(SensitiveString::from("smoke-free-model")),
        billing_mode: Set(2),
        input_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        output_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        version: Set(4),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    model_prices::ActiveModel {
        model: Set(SensitiveString::from("smoke-expression-model")),
        billing_mode: Set(3),
        input_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        output_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        billing_expression: Set(Some(SensitiveString::from(r#"tier("base", p)"#))),
        version: Set(2),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let records = ModelPriceRepository::new(pool.clone()).load_all().await?;
    let priced = records
        .iter()
        .find(|record| record.model() == "smoke-priced-model")
        .ok_or("定价模型必须存在")?;
    assert_eq!(priced.billing_mode(), ModelPriceBillingMode::PerToken);
    assert_eq!(
        priced.prices(),
        [
            Decimal::new(1, 28),
            Decimal::new(25, 1),
            Decimal::new(3, 2),
            Decimal::new(375, 2),
            Decimal::from(6),
        ]
    );
    assert_eq!(priced.version(), 9);
    let free = records
        .iter()
        .find(|record| record.model() == "smoke-free-model")
        .ok_or("免费模型必须存在")?;
    assert_eq!(free.billing_mode(), ModelPriceBillingMode::Free);
    assert_eq!(free.prices(), [Decimal::ZERO; 5]);
    let expression = records
        .iter()
        .find(|record| record.model() == "smoke-expression-model")
        .ok_or("表达式模型必须存在")?;
    assert_eq!(expression.billing_mode(), ModelPriceBillingMode::Expression);
    assert_eq!(expression.billing_expression(), Some(r#"tier("base", p)"#));
    Ok(())
}

/// 在三方言实库中验证用量事实的幂等追加与冲突拒绝。
async fn exercise_usage_log_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("usage-smoke-group", "用量日志冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "usage-smoke-user",
        "usage-smoke@example.com",
        "invite-usage-smoke",
    );
    user.status = Set(1);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_C, "usage-smoke-token");
    token.status = Set(1);
    let token = token.insert(pool.connection()).await?;
    let principal = GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    );
    let repository = UsageLogRepository::new(pool.clone());
    let event_id = BillingReservationId::new([0xc1; 16])?;
    let usage = UsageLogUsage::new(10, 2, 3, 4, 5, 1, 0, 0)?;
    let write = UsageLogWrite::new(
        event_id,
        principal,
        UsageLogBillingMode::PerToken,
        usage,
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(7)?,
    )
    .with_video_dimensions(Some(8), Some(UsageLogVideoResolution::P720))?;

    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Applied
    );
    assert_eq!(
        repository.record(&write).await?,
        UsageLogWriteOutcome::Existing
    );
    assert_eq!(
        usage_logs::Entity::find().count(pool.connection()).await?,
        1
    );
    let conflicting_write = UsageLogWrite::new(
        event_id,
        principal,
        UsageLogBillingMode::PerToken,
        usage,
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(8)?,
    );
    assert_eq!(
        repository.record(&conflicting_write).await,
        Err(UsageLogRepositoryError::Conflict)
    );
    let per_call_write = UsageLogWrite::new(
        BillingReservationId::new([0xc2; 16])?,
        principal,
        UsageLogBillingMode::PerCall,
        UsageLogUsage::new(0, 0, 0, 0, 0, 0, 0, 0)?,
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(320_000)?,
    )
    .with_video_dimensions(Some(8), None)?;
    assert_eq!(
        repository.record(&per_call_write).await?,
        UsageLogWriteOutcome::Applied
    );
    assert_eq!(
        usage_logs::Entity::find().count(pool.connection()).await?,
        2
    );

    usage_logs::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言实库中验证分享快照写入、读取和用户删除级联。
async fn exercise_playground_share_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("share-smoke-group", "分享冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut owner = user_active_model(
        group.id,
        "share-smoke-owner",
        "share-smoke@example.com",
        "invite-share-smoke",
    );
    owner.status = Set(1);
    let owner = owner.insert(pool.connection()).await?;
    let owner_user_id = UserId::new(owner.id)?;
    let repository = PlaygroundShareRepository::new(pool.clone(), Duration::from_secs(2))?;
    let expires_at = TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp()
        .checked_add(24 * 60 * 60)
        .ok_or_else(|| std::io::Error::other("分享冒烟过期时间溢出"))?;
    repository
        .create(PlaygroundShareWrite::new(
            owner_user_id,
            TOKEN_HASH_E.to_owned(),
            serde_json::json!({
                "version": 1,
                "sessions": [{
                    "model": "smoke-model",
                    "messages": [
                        {"role": "user", "content": "ping"},
                        {"role": "assistant", "content": "pong"}
                    ]
                }]
            }),
            expires_at,
        ))
        .await?;
    assert!(repository.find_active(TOKEN_HASH_E).await?.is_some());

    users::Entity::delete_by_id(owner.id)
        .exec(pool.connection())
        .await?;
    assert_eq!(
        playground_shares::Entity::find()
            .count(pool.connection())
            .await?,
        0
    );
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言实库中验证私有历史写入、列表、版本更新和用户删除级联。
async fn exercise_playground_conversation_repository(
    pool: &DatabasePool,
) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("history-smoke-group", "历史冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut owner = user_active_model(
        group.id,
        "history-smoke-owner",
        "history-smoke@example.com",
        "invite-history-smoke",
    );
    owner.status = Set(1);
    let owner = owner.insert(pool.connection()).await?;
    let owner_user_id = UserId::new(owner.id)?;
    let repository = PlaygroundConversationRepository::new(pool.clone(), Duration::from_secs(2))?;
    let snapshot = serde_json::json!({
        "version": 1,
        "sessions": [{
            "model": "smoke-model",
            "messages": [
                {"role": "user", "content": "ping"},
                {"role": "assistant", "content": "pong"}
            ]
        }]
    });
    repository
        .save(
            owner_user_id,
            PlaygroundConversationWrite::new(
                PLAYGROUND_CONVERSATION_KEY.to_owned(),
                "ping".to_owned(),
                vec!["smoke-model".to_owned()],
                snapshot.clone(),
                None,
            ),
        )
        .await?;
    assert_eq!(repository.list(owner_user_id).await?.len(), 1);
    assert!(
        repository
            .find(owner_user_id, PLAYGROUND_CONVERSATION_KEY)
            .await?
            .is_some()
    );
    assert_eq!(
        repository
            .save(
                owner_user_id,
                PlaygroundConversationWrite::new(
                    PLAYGROUND_CONVERSATION_KEY.to_owned(),
                    "ping".to_owned(),
                    vec!["smoke-model".to_owned()],
                    snapshot,
                    Some(1),
                ),
            )
            .await?
            .revision(),
        1
    );

    users::Entity::delete_by_id(owner.id)
        .exec(pool.connection())
        .await?;
    assert_eq!(
        playground_conversations::Entity::find()
            .count(pool.connection())
            .await?,
        0
    );
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言实库中执行空看板聚合，覆盖 COUNT 与 SUM 的真实返回类型。
async fn exercise_admin_dashboard_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let period_end = TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp()
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("管理看板冒烟时间溢出"))?;
    let period_start = period_end
        .checked_sub(24 * 60 * 60)
        .ok_or_else(|| std::io::Error::other("管理看板冒烟窗口下溢"))?;
    let dashboard = AdminDashboardRepository::new(pool.clone(), Duration::from_secs(2))?
        .snapshot(period_start, period_end)
        .await?;
    assert_eq!(dashboard.request_count(), 0);
    assert_eq!(dashboard.quota_consumed(), 0);
    assert_eq!(dashboard.upstream_usage_count(), 0);
    assert_eq!(dashboard.estimated_usage_count(), 0);
    assert_eq!(dashboard.per_token_request_count(), 0);
    assert_eq!(dashboard.free_request_count(), 0);
    assert_eq!(dashboard.hourly().len(), 24);
    assert_eq!(dashboard.performance().first_token_sample_count(), 0);
    assert_eq!(dashboard.performance().average_first_token_ms(), None);
    Ok(())
}

/// 在三方言隔离数据库中验证分组基础、高峰与跨分组附加倍率读取一致。
async fn exercise_group_pricing_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let source = group_active_model("smoke-source", "冒烟来源分组")
        .insert(pool.connection())
        .await?;
    let mut target = group_active_model("smoke-target", "冒烟目标分组");
    target.ratio_micros = Set(1_200_000);
    target.peak_ratio_micros = Set(Some(2_000_000));
    target.peak_start = Set(Some(fixed_peak_time()));
    target.peak_end = Set(Some(fixed_peak_end_time()));
    let target = target.insert(pool.connection()).await?;
    group_model_ratios::ActiveModel {
        source_group_id: Set(source.id),
        target_group_id: Set(target.id),
        ratio_micros: Set(750_000),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;

    let catalog = GroupPricingRepository::new(pool.clone())
        .load_snapshot()
        .await?;
    assert_eq!(catalog.groups().len(), 2);
    assert_eq!(catalog.group_model_ratios().len(), 1);
    assert_eq!(catalog.group_model_ratios()[0].ratio_micros(), 750_000);
    Ok(())
}

/// 在三方言隔离数据库中验证 checkpoint、主体增量与并发重放处于同一事务边界。
async fn exercise_billing_batch_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("batch-smoke-group", "批量落盘冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "batch-smoke-user",
        "batch-smoke@example.com",
        "invite-batch-smoke",
    );
    user.status = Set(1);
    user.quota = Set(200);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_C, "batch-smoke-token");
    token.status = Set(1);
    token.remain_quota = Set(200);
    let token = token.insert(pool.connection()).await?;
    let channel = channel_active_model("batch-smoke-channel")
        .insert(pool.connection())
        .await?;
    let repository = BillingBatchRepository::new(pool.clone());
    let writer_id = [0xe1; 16];
    let first = billing_batch_write(writer_id, 1, 2, &user, &token, &channel, 10, 2)?;

    assert_eq!(
        repository.apply(&first).await?,
        BillingBatchWriteOutcome::Applied
    );
    assert_eq!(
        repository.apply(&first).await?,
        BillingBatchWriteOutcome::Existing
    );

    let next = Arc::new(billing_batch_write(
        writer_id, 3, 3, &user, &token, &channel, 5, 1,
    )?);
    let barrier = Arc::new(tokio::sync::Barrier::new(5));
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let repository = repository.clone();
        let batch = Arc::clone(&next);
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            repository.apply(&batch).await
        }));
    }
    barrier.wait().await;
    let mut applied = 0;
    let mut existing = 0;
    for task in tasks {
        match task.await?? {
            BillingBatchWriteOutcome::Applied => applied += 1,
            BillingBatchWriteOutcome::Existing => existing += 1,
        }
    }
    assert_eq!((applied, existing), (1, 3));

    let user = users::Entity::find_by_id(user.id)
        .one(pool.connection())
        .await?
        .expect("批量落盘冒烟用户必须存在");
    let token = tokens::Entity::find_by_id(token.id)
        .one(pool.connection())
        .await?
        .expect("批量落盘冒烟令牌必须存在");
    let channel = channels::Entity::find_by_id(channel.id)
        .one(pool.connection())
        .await?
        .expect("批量落盘冒烟渠道必须存在");
    assert_eq!(
        (user.quota, user.used_quota, user.request_count),
        (185, 15, 3)
    );
    assert_eq!((token.remain_quota, token.used_quota), (185, 15));
    assert_eq!(channel.used_quota, 15);

    billing_batch_checkpoints::Entity::delete_by_id(BillingBatchWriterKey::parse(
        &first.writer_key(),
    )?)
    .exec(pool.connection())
    .await?;
    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    channels::Entity::delete_by_id(channel.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

fn billing_batch_write(
    writer_id: [u8; 16],
    start_sequence: u64,
    end_sequence: u64,
    user: &users::Model,
    token: &tokens::Model,
    channel: &channels::Model,
    used_quota: i64,
    request_count: i64,
) -> Result<BillingBatchWrite, Box<dyn Error>> {
    let event_ids = (start_sequence..=end_sequence)
        .map(|sequence| {
            let marker = <u8 as TryFrom<u64>>::try_from(sequence).unwrap_or(0xfe);
            BillingReservationId::new([marker; 16])
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(BillingBatchWrite::new(
        writer_id,
        start_sequence,
        end_sequence,
        event_ids,
        vec![UserBillingWrite::new(
            UserId::new(user.id)?,
            QuotaDelta::new(-used_quota)?,
            QuotaDelta::new(used_quota)?,
            request_count,
        )?],
        vec![TokenBillingWrite::new(
            TokenId::new(token.id)?,
            QuotaDelta::new(-used_quota)?,
            QuotaDelta::new(used_quota)?,
        )?],
        vec![ChannelBillingWrite::new(
            ChannelId::new(channel.id)?,
            QuotaDelta::new(used_quota)?,
        )?],
    )?)
}

/// 在三方言隔离数据库中执行并发预扣、结算、退款及终态重放。
async fn exercise_quota_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("quota-smoke-group", "额度仓储冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "quota-smoke-user",
        "quota-smoke@example.com",
        "invite-quota-smoke",
    );
    user.status = Set(1);
    user.quota = Set(200);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_D, "quota-smoke-token");
    token.status = Set(1);
    token.remain_quota = Set(200);
    let token = token.insert(pool.connection()).await?;
    let principal = GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    );
    let settled_id = BillingReservationId::new([0xd1; 16])?;
    let refunded_id = BillingReservationId::new([0xd2; 16])?;
    let batch_id = BillingReservationId::new([0xd3; 16])?;
    let repository = QuotaRepository::new(pool.clone());

    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let first_repository = repository.clone();
    let first_barrier = Arc::clone(&barrier);
    let first = tokio::spawn(async move {
        first_barrier.wait().await;
        first_repository
            .precharge(settled_id, principal, Quota::new(40).unwrap())
            .await
    });
    let second_repository = repository.clone();
    let second_barrier = Arc::clone(&barrier);
    let second = tokio::spawn(async move {
        second_barrier.wait().await;
        second_repository
            .precharge(refunded_id, principal, Quota::new(40).unwrap())
            .await
    });
    barrier.wait().await;
    assert_eq!(first.await??, QuotaMutationOutcome::Applied);
    assert_eq!(second.await??, QuotaMutationOutcome::Applied);

    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let settle_repository = repository.clone();
    let settle_barrier = Arc::clone(&barrier);
    let settle = tokio::spawn(async move {
        settle_barrier.wait().await;
        settle_repository
            .settle(settled_id, Quota::new(50).unwrap())
            .await
    });
    let refund_repository = repository.clone();
    let refund_barrier = Arc::clone(&barrier);
    let refund = tokio::spawn(async move {
        refund_barrier.wait().await;
        refund_repository.refund(refunded_id).await
    });
    barrier.wait().await;
    assert_eq!(settle.await??, QuotaMutationOutcome::Applied);
    assert_eq!(refund.await??, QuotaMutationOutcome::Applied);
    assert_eq!(
        repository
            .settle(settled_id, Quota::new(50).unwrap())
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        repository.refund(refunded_id).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(
        repository
            .reserve_batch_task(batch_id, principal, Quota::new(30).unwrap())
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        repository
            .settle_batch_task(batch_id, Quota::new(20).unwrap())
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        repository
            .settle_batch_task(batch_id, Quota::new(20).unwrap())
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );

    let user = users::Entity::find_by_id(user.id)
        .one(pool.connection())
        .await?
        .expect("额度冒烟用户必须存在");
    let token = tokens::Entity::find_by_id(token.id)
        .one(pool.connection())
        .await?
        .expect("额度冒烟令牌必须存在");
    assert_eq!(
        (
            user.quota,
            user.used_quota,
            user.frozen_quota,
            user.request_count
        ),
        (130, 70, 0, 2)
    );
    assert_eq!((token.remain_quota, token.used_quota), (130, 70));

    for (id, expected_status) in [
        (settled_id, QuotaReservationStatus::Settled),
        (refunded_id, QuotaReservationStatus::Refunded),
        (batch_id, QuotaReservationStatus::Settled),
    ] {
        let key = BillingReservationKey::parse(&id.persistence_key())?;
        let reservation = billing_reservations::Entity::find_by_id(key.clone())
            .one(pool.connection())
            .await?
            .expect("额度冒烟预留记录必须存在");
        let finalized_at = reservation
            .finalized_at
            .expect("额度冒烟终态必须记录完成时间");
        assert_eq!(reservation.status, expected_status.code());
        assert_eq!(
            reservation.reservation_kind,
            if id == batch_id { 2 } else { 1 }
        );
        assert!(reservation.updated_at >= reservation.created_at);
        assert!(finalized_at >= reservation.created_at);
        assert_eq!(finalized_at, reservation.updated_at);
        billing_reservations::Entity::delete_by_id(key)
            .exec(pool.connection())
            .await?;
    }
    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在现有三方言迁移流水线中执行一次真实联表鉴权，防止方言差异只在生产暴露。
async fn exercise_token_auth_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("auth-smoke-group", "鉴权冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut user = user_active_model(
        group.id,
        "auth-smoke-user",
        "auth-smoke@example.com",
        "invite-auth-smoke",
    );
    user.status = Set(1);
    let user = user.insert(pool.connection()).await?;
    let mut token = token_active_model(user.id, group.id, TOKEN_HASH_E, "auth-smoke-token");
    token.status = Set(1);
    token.allow_ips = Set(Some(TokenIpAllowlist::validate(Json::Array(vec![
        Json::String("192.0.2.0/24".to_owned()),
        Json::String("2001:db8::/32".to_owned()),
    ]))?));
    let token = token.insert(pool.connection()).await?;

    let repository = TokenAuthRepository::new(pool.clone(), Duration::from_secs(2))?;
    let outcome = repository
        .lookup(
            &TokenAuthLookup::new(TOKEN_HASH_E)?,
            TrustedClientIp::new("192.0.2.10".parse()?),
        )
        .await?;
    let TokenAuthLookupOutcome::Authenticated {
        principal,
        model_policy,
        ..
    } = outcome
    else {
        return Err("三方言令牌鉴权冒烟未返回已认证主体".into());
    };
    assert_eq!(principal.token_id().get(), token.id);
    assert_eq!(principal.user_id().get(), user.id);
    assert_eq!(principal.group_id().get(), group.id);
    assert!(model_policy.allows("auth-smoke-model"));
    assert_eq!(
        repository
            .lookup(
                &TokenAuthLookup::new(TOKEN_HASH_E)?,
                TrustedClientIp::new("198.51.100.10".parse()?),
            )
            .await?,
        TokenAuthLookupOutcome::Rejected
    );

    tokens::Entity::delete_by_id(token.id)
        .exec(pool.connection())
        .await?;
    users::Entity::delete_by_id(user.id)
        .exec(pool.connection())
        .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

#[test]
fn destructive_migration_test_requires_gate_and_isolated_target() {
    assert!(validate_destructive_test_target("sqlite::memory:", None).is_err());
    assert!(validate_destructive_test_target("postgres://localhost/anyflows", Some("1")).is_err());
    assert!(
        validate_destructive_test_target(
            "postgres://localhost/anyflows_migration_test_ci?dbname=anyflows",
            Some("1"),
        )
        .is_err()
    );
    assert!(
        validate_destructive_test_target(
            "postgres://localhost/anyflows_migration_test_ci?options=-csearch_path%3Dpublic",
            Some("1"),
        )
        .is_err()
    );
    assert!(
        validate_destructive_test_target(
            "mysql://localhost/anyflows_migration_test_ci?timezone=%2B08:00",
            Some("1"),
        )
        .is_ok()
    );
    assert!(
        validate_destructive_test_target(
            "mysql://localhost/anyflows_migration_test_ci?database=anyflows",
            Some("1"),
        )
        .is_err()
    );
    assert!(validate_destructive_test_target("sqlite::memory:", Some("1")).is_ok());
}

/// 完整回滚只允许显式门禁下的内存库或约定前缀测试库。
fn validate_destructive_test_target(database_url: &str, gate: Option<&str>) -> Result<(), DbErr> {
    if gate != Some("1") {
        return Err(DbErr::Custom(
            "完整迁移回滚缺少显式破坏性测试门禁".to_owned(),
        ));
    }
    if database_url == "sqlite::memory:" {
        return Ok(());
    }

    let parsed = Url::parse(database_url)
        .map_err(|_| DbErr::Custom("迁移冒烟测试数据库地址无效".to_owned()))?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql" | "mysql") {
        return Err(DbErr::Custom(
            "完整迁移回滚仅支持隔离的 PostgreSQL、MySQL 或内存 SQLite".to_owned(),
        ));
    }
    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| DbErr::Custom("迁移冒烟测试数据库名称无效".to_owned()))?;
    let database_name = segments
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| DbErr::Custom("迁移冒烟测试数据库名称无效".to_owned()))?;
    let isolated_name = database_name.starts_with(ISOLATED_DATABASE_PREFIX)
        && database_name.len() > ISOLATED_DATABASE_PREFIX.len()
        && database_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        && segments.next().is_none();
    if !isolated_name {
        return Err(DbErr::Custom(
            "完整迁移回滚目标必须使用专用测试数据库名称".to_owned(),
        ));
    }
    let query_is_safe = match parsed.scheme() {
        "postgres" | "postgresql" => parsed.query().is_none(),
        "mysql" => {
            let mut pairs = parsed.query_pairs();
            pairs
                .next()
                .is_none_or(|(name, _)| name == "timezone" && pairs.next().is_none())
        }
        _ => false,
    };
    if !query_is_safe {
        return Err(DbErr::Custom(
            "完整迁移回滚目标禁止使用数据库重定向参数".to_owned(),
        ));
    }
    Ok(())
}

/// 在三方言真实事务中验证注册策略与固定窗口限流的原子判定。
async fn exercise_registration_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("registration-smoke", "注册冒烟分组")
        .insert(pool.connection())
        .await?;
    let group_id = GroupId::new(group.id)?;
    let repository = RegistrationRepository::new(pool.clone(), Duration::from_secs(5))?;
    repository
        .update_policy(RegistrationPolicyWriteRecord::new(
            true, true, group_id, 100, 0, true, 1, 60,
        ))
        .await
        .expect("注册冒烟必须成功写入策略");

    let first = repository
        .claim_attempt([0x5a; 32], 120)
        .await
        .expect("注册冒烟必须成功占用首次尝试");
    assert!(matches!(first, RegistrationAttemptOutcome::Allowed(_)));
    assert_eq!(
        repository
            .claim_attempt([0x5a; 32], 121)
            .await
            .expect("注册冒烟必须完成限流判定"),
        RegistrationAttemptOutcome::RateLimited {
            retry_after_seconds: 59
        }
    );

    // 冒烟数据不得改变后续核心表计数与外键行为基线。
    registration_rate_limits::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    authentication_settings::ActiveModel {
        id: Set(1),
        password_login_enabled: Set(true),
        registration_enabled: Set(false),
        registration_default_group_id: Set(None),
        registration_initial_quota: Set(0),
        invitation_rebate_quota: Set(0),
        registration_email_required: Set(false),
        registration_rate_limit_attempts: Set(5),
        registration_rate_limit_window_seconds: Set(3_600),
        ..Default::default()
    }
    .update(pool.connection())
    .await?;
    groups::Entity::delete_by_id(group.id)
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言真实事务中验证挑战签发、冷却、摘要替换与单次消费。
async fn exercise_auth_challenge_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let repository = AuthChallengeRepository::new(pool.clone(), Duration::from_secs(5))?;
    let issued = repository
        .issue(AuthChallengeIssue::new(
            AuthChallengePurpose::RegistrationEmail,
            [0x6a; 32],
            [0x6b; 32],
            None,
            10_000,
            600,
            60,
            3,
        )?)
        .await?;
    assert!(matches!(
        issued,
        AuthChallengeIssueOutcome::Issued(record)
            if record.version() == 1 && record.next_send_at() == 10_060
    ));
    let consumed = repository
        .consume(AuthChallengeConsume::new(
            AuthChallengePurpose::RegistrationEmail,
            [0x6a; 32],
            [0x6b; 32],
            10_001,
        )?)
        .await?;
    assert!(matches!(
        consumed,
        AuthChallengeConsumeOutcome::Consumed(record)
            if record.target_user_id().is_none() && record.version() == 2
    ));

    // 冒烟数据只验证短生命周期状态，不进入后续核心实体计数。
    auth_challenges::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言真实事务中验证 Passkey 主体与客户端双作用域原子限流。
async fn exercise_passkey_rate_limit_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let repository = AuthChallengeRateLimitRepository::new(pool.clone(), Duration::from_secs(5))?;
    let claim = || {
        AuthChallengeRateLimitClaim::new(
            AuthChallengePurpose::PasskeyAuthentication,
            [0x7a; 32],
            [0x7b; 32],
            10_020,
            1,
            60,
        )
    };
    assert_eq!(
        repository.claim(claim()?).await?,
        AuthChallengeRateLimitOutcome::Allowed
    );
    assert_eq!(
        repository.claim(claim()?).await?,
        AuthChallengeRateLimitOutcome::RateLimited {
            retry_after_seconds: 60
        }
    );
    auth_challenge_rate_limits::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    Ok(())
}

/// 在三方言真实事务中验证站点固定记录和单调版本。
async fn exercise_site_settings_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let repository = SiteSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let initial = repository.settings().await?;
    assert_eq!(initial.site_name(), "AnyFlows");
    assert_eq!(initial.version(), 1);
    assert_eq!(
        initial.balance_display().mode(),
        BalanceDisplayModeRecord::Quota
    );

    let saved = repository
        .update(SiteSettingsWriteRecord::new(
            "AnyFlows Smoke".to_owned(),
            Some("https://example.com/gateway".to_owned()),
            Some("/brand.svg".to_owned()),
            Some("统一访问模型".to_owned()),
            Some("三方言站点设置冒烟。".to_owned()),
            BalanceDisplayPolicyRecord::new(
                BalanceDisplayModeRecord::CustomUnit,
                "算力积分".to_owned(),
                "积分".to_owned(),
                10_000,
                BalanceSymbolPositionRecord::Suffix,
                2,
            )?,
            initial.version(),
        ))
        .await?;
    assert_eq!(saved.version(), 2);
    assert_eq!(saved.site_name(), "AnyFlows Smoke");
    assert_eq!(
        saved.balance_display().mode(),
        BalanceDisplayModeRecord::CustomUnit
    );
    Ok(())
}

/// 在三方言真实事务中验证固定邮件设置行、密文保留和单调版本。
async fn exercise_email_settings_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let repository = EmailSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let initial = repository.settings().await?;
    assert!(!initial.enabled());
    assert_eq!(initial.port(), 587);
    assert_eq!(initial.timeout_seconds(), 10);
    assert_eq!(initial.version(), 1);
    assert!(!initial.password_configured());

    let envelope = EncryptedCredentialEnvelope::new("email-smoke-key", [0x2a; 24], vec![0x5a; 32])?;
    let replaced = repository
        .update(EmailSettingsWriteRecord::new(
            true,
            "smtp.example.com".to_owned(),
            587,
            EmailTlsMode::StartTls,
            Some("mailer@example.com".to_owned()),
            EmailPasswordUpdate::Replace(envelope.clone()),
            "from@example.com".to_owned(),
            Some("AnyFlows".to_owned()),
            Some("reply@example.com".to_owned()),
            15,
        ))
        .await?;
    assert_eq!(replaced.version(), 2);
    assert_eq!(replaced.password_secret(), Some(&envelope));

    let kept = repository
        .update(EmailSettingsWriteRecord::new(
            true,
            "smtp.example.com".to_owned(),
            465,
            EmailTlsMode::Tls,
            Some("mailer@example.com".to_owned()),
            EmailPasswordUpdate::Keep,
            "from@example.com".to_owned(),
            Some("AnyFlows".to_owned()),
            None,
            20,
        ))
        .await?;
    assert_eq!(kept.version(), 3);
    assert_eq!(kept.password_secret(), Some(&envelope));
    Ok(())
}

async fn assert_database_session(connection: &DatabaseConnection) -> Result<(), DbErr> {
    if connection.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }

    let row = connection
        .query_one(Statement::from_string(
            DbBackend::MySql,
            "SELECT @@session.time_zone AS session_time_zone",
        ))
        .await?
        .expect("MySQL 会话时区查询必须返回一行");
    let timezone: String = row.try_get("", "session_time_zone")?;
    assert_eq!(timezone, "+00:00");
    Ok(())
}

async fn assert_core_schema(connection: &DatabaseConnection) -> Result<(), DbErr> {
    let manager = SchemaManager::new(connection);
    for table in CORE_TABLES {
        assert!(manager.has_table(table).await?, "缺少数据表 {table}");
    }

    assert_entity_columns::<groups::Column>(&manager, "groups").await?;
    assert_entity_columns::<users::Column>(&manager, "users").await?;
    assert_entity_columns::<passkeys::Column>(&manager, "passkeys").await?;
    assert_entity_columns::<passkey_authentication_challenges::Column>(
        &manager,
        "passkey_authentication_challenges",
    )
    .await?;
    assert_entity_columns::<passkey_registration_challenges::Column>(
        &manager,
        "passkey_registration_challenges",
    )
    .await?;
    assert_entity_columns::<organizations::Column>(&manager, "organizations").await?;
    assert_entity_columns::<organization_teams::Column>(&manager, "organization_teams").await?;
    assert_entity_columns::<organization_memberships::Column>(&manager, "organization_memberships")
        .await?;
    assert_entity_columns::<organization_custom_roles::Column>(
        &manager,
        "organization_custom_roles",
    )
    .await?;
    assert_entity_columns::<organization_custom_role_permissions::Column>(
        &manager,
        "organization_custom_role_permissions",
    )
    .await?;
    assert_entity_columns::<organization_entitlements::Column>(
        &manager,
        "organization_entitlements",
    )
    .await?;
    assert_entity_columns::<organization_sso_policies::Column>(
        &manager,
        "organization_sso_policies",
    )
    .await?;
    assert_entity_columns::<organization_scim_tokens::Column>(&manager, "organization_scim_tokens")
        .await?;
    assert_entity_columns::<organization_scim_user_resources::Column>(
        &manager,
        "organization_scim_user_resources",
    )
    .await?;
    assert_entity_columns::<organization_scim_group_resources::Column>(
        &manager,
        "organization_scim_group_resources",
    )
    .await?;
    assert_entity_columns::<organization_scim_audit_logs::Column>(
        &manager,
        "organization_scim_audit_logs",
    )
    .await?;
    assert_entity_columns::<organization_audit_logs::Column>(&manager, "organization_audit_logs")
        .await?;
    assert_entity_columns::<platform_audit_logs::Column>(&manager, "platform_audit_logs").await?;
    assert_entity_columns::<request_outcome_logs::Column>(&manager, "request_outcome_logs").await?;
    assert_entity_columns::<analytics_export_outbox_events::Column>(
        &manager,
        "analytics_export_outbox_events",
    )
    .await?;
    assert_entity_columns::<user_notification_events::Column>(&manager, "user_notification_events")
        .await?;
    assert_entity_columns::<user_notification_receipts::Column>(
        &manager,
        "user_notification_receipts",
    )
    .await?;
    assert_entity_columns::<organization_invitations::Column>(&manager, "organization_invitations")
        .await?;
    assert_entity_columns::<organization_wallets::Column>(&manager, "organization_wallets").await?;
    assert_entity_columns::<organization_wallet_ledger_entries::Column>(
        &manager,
        "organization_wallet_ledger_entries",
    )
    .await?;
    assert_entity_columns::<organization_budget_policies::Column>(
        &manager,
        "organization_budget_policies",
    )
    .await?;
    assert_entity_columns::<organization_budget_windows::Column>(
        &manager,
        "organization_budget_windows",
    )
    .await?;
    assert_entity_columns::<billing_organization_reservations::Column>(
        &manager,
        "billing_organization_reservations",
    )
    .await?;
    assert_entity_columns::<channels::Column>(&manager, "channels").await?;
    assert_entity_columns::<credentials::Column>(&manager, "credentials").await?;
    assert_entity_columns::<tokens::Column>(&manager, "tokens").await?;
    assert_entity_columns::<usage_logs::Column>(&manager, "usage_logs").await?;
    assert_entity_columns::<channel_models::Column>(&manager, "channel_models").await?;
    assert_entity_columns::<channel_groups::Column>(&manager, "channel_groups").await?;
    assert_entity_columns::<abilities::Column>(&manager, "abilities").await?;
    assert_entity_columns::<options::Column>(&manager, "options").await?;
    assert_entity_columns::<billing_reservations::Column>(&manager, "billing_reservations").await?;
    assert_entity_columns::<billing_subscription_reservations::Column>(
        &manager,
        "billing_subscription_reservations",
    )
    .await?;
    assert_entity_columns::<billing_token_window_reservations::Column>(
        &manager,
        "billing_token_window_reservations",
    )
    .await?;
    assert_entity_columns::<billing_group_window_reservations::Column>(
        &manager,
        "billing_group_window_reservations",
    )
    .await?;
    assert_entity_columns::<billing_batch_checkpoints::Column>(
        &manager,
        "billing_batch_checkpoints",
    )
    .await?;
    assert_entity_columns::<group_model_ratios::Column>(&manager, "group_model_ratios").await?;
    assert_entity_columns::<playground_shares::Column>(&manager, "playground_shares").await?;
    assert_entity_columns::<playground_conversations::Column>(&manager, "playground_conversations")
        .await?;
    assert_entity_columns::<registration_rate_limits::Column>(&manager, "registration_rate_limits")
        .await?;
    assert_entity_columns::<email_settings::Column>(&manager, "email_settings").await?;
    assert_entity_columns::<site_settings::Column>(&manager, "site_settings").await?;
    assert_entity_columns::<authentication_settings::Column>(&manager, "authentication_settings")
        .await?;
    assert_entity_columns::<auth_challenges::Column>(&manager, "auth_challenges").await?;
    assert_entity_columns::<auth_challenge_rate_limits::Column>(
        &manager,
        "auth_challenge_rate_limits",
    )
    .await?;
    assert_entity_columns::<models::Column>(&manager, "models").await?;
    assert_entity_columns::<model_sync_runs::Column>(&manager, "model_sync_runs").await?;
    assert_entity_columns::<model_sync_items::Column>(&manager, "model_sync_items").await?;
    assert_entity_columns::<wallet_ledger_entries::Column>(&manager, "wallet_ledger_entries")
        .await?;
    assert_entity_columns::<topup_orders::Column>(&manager, "topup_orders").await?;
    assert_entity_columns::<topup_payment_events::Column>(&manager, "topup_payment_events").await?;
    assert_entity_columns::<payment_settings::Column>(&manager, "payment_settings").await?;
    assert_entity_columns::<redemption_batches::Column>(&manager, "redemption_batches").await?;
    assert_entity_columns::<redemption_codes::Column>(&manager, "redemption_codes").await?;
    assert_entity_columns::<subscription_plans::Column>(&manager, "subscription_plans").await?;
    assert_entity_columns::<subscription_plan_prices::Column>(&manager, "subscription_plan_prices")
        .await?;
    assert_entity_columns::<subscription_orders::Column>(&manager, "subscription_orders").await?;
    assert_entity_columns::<subscription_payment_events::Column>(
        &manager,
        "subscription_payment_events",
    )
    .await?;
    assert_entity_columns::<refund_requests::Column>(&manager, "refund_requests").await?;
    assert_entity_columns::<refund_provider_events::Column>(&manager, "refund_provider_events")
        .await?;
    assert_entity_columns::<organization_sso_providers::Column>(
        &manager,
        "organization_sso_providers",
    )
    .await?;
    assert_entity_columns::<organization_sso_domains::Column>(&manager, "organization_sso_domains")
        .await?;
    assert_entity_columns::<organization_sso_identities::Column>(
        &manager,
        "organization_sso_identities",
    )
    .await?;
    assert_entity_columns::<organization_sso_login_transactions::Column>(
        &manager,
        "organization_sso_login_transactions",
    )
    .await?;
    assert_entity_columns::<user_subscriptions::Column>(&manager, "user_subscriptions").await?;
    assert_entity_columns::<invite_rebate_events::Column>(&manager, "invite_rebate_events").await?;
    assert_entity_columns::<balance_alert_settings::Column>(&manager, "balance_alert_settings")
        .await?;
    assert_entity_columns::<balance_alert_events::Column>(&manager, "balance_alert_events").await?;
    assert_entity_columns::<subscription_balance_alert_events::Column>(
        &manager,
        "subscription_balance_alert_events",
    )
    .await?;
    assert_entity_columns::<debug_trace_attempts::Column>(&manager, "debug_trace_attempts").await?;

    assert!(manager.has_column("tokens", "key_hash").await?);
    assert!(manager.has_column("tokens", "key_prefix").await?);
    assert!(!manager.has_column("tokens", "key").await?);

    for (table, index) in [
        ("users", "uq_users_active_username"),
        ("users", "uq_users_active_email"),
        ("users", "uq_users_aff_code"),
        ("passkeys", "uq_passkeys_credential_id_digest"),
        ("passkeys", "idx_passkeys_user_active"),
        (
            "passkey_registration_challenges",
            "idx_passkey_registration_challenges_user_state",
        ),
        (
            "passkey_authentication_challenges",
            "uq_passkey_authentication_challenges_digest",
        ),
        (
            "passkey_authentication_challenges",
            "idx_passkey_authentication_challenges_user_state",
        ),
        (
            "passkey_authentication_challenges",
            "idx_passkey_authentication_challenges_expires_at",
        ),
        ("groups", "uq_groups_active_name"),
        ("tokens", "uq_tokens_key_hash"),
        ("credentials", "idx_credentials_channel_status_schedulable"),
        ("credentials", "uq_credentials_active_spark_parent"),
        ("abilities", "idx_abilities_group_model_enabled"),
        (
            "billing_reservations",
            "idx_billing_reservations_status_expires",
        ),
        (
            "billing_reservations",
            "idx_billing_reservations_user_created",
        ),
        (
            "billing_reservations",
            "idx_billing_reservations_token_created",
        ),
        (
            "billing_subscription_reservations",
            "idx_billing_subscription_reservations_subscription",
        ),
        ("group_model_ratios", "idx_group_model_ratios_target"),
        ("playground_shares", "uq_playground_shares_token_hash"),
        ("playground_shares", "idx_playground_shares_owner_created"),
        ("playground_shares", "idx_playground_shares_expires_at"),
        (
            "playground_conversations",
            "idx_playground_conversations_owner_updated",
        ),
        ("auth_challenges", "uq_auth_challenges_purpose_subject"),
        ("auth_challenges", "idx_auth_challenges_target_purpose"),
        ("auth_challenges", "idx_auth_challenges_expires_at"),
        (
            "auth_challenge_rate_limits",
            "uq_auth_challenge_rate_limits_scope",
        ),
        ("models", "uq_models_active_model"),
        ("models", "idx_models_lifecycle_visibility_id"),
        ("model_sync_runs", "uq_model_sync_runs_preview_id"),
        ("model_sync_runs", "idx_model_sync_runs_channel_created"),
        ("model_sync_items", "uq_model_sync_items_run_ordinal"),
        ("model_sync_items", "idx_model_sync_items_run_relation"),
        ("wallet_ledger_entries", "uq_wallet_ledger_event_key"),
        ("wallet_ledger_entries", "idx_wallet_ledger_user_id"),
        ("topup_orders", "uq_topup_orders_order_key"),
        ("topup_orders", "uq_topup_orders_idempotency_key"),
        ("topup_orders", "uq_topup_orders_provider_trade_no"),
        ("topup_orders", "idx_topup_orders_user_created"),
        ("topup_orders", "idx_topup_orders_status_expires"),
        ("topup_payment_events", "uq_topup_payment_events_event_key"),
        (
            "topup_payment_events",
            "uq_topup_payment_events_provider_event",
        ),
        (
            "topup_payment_events",
            "idx_topup_payment_events_order_created",
        ),
        ("redemption_batches", "uq_redemption_batches_batch_key"),
        (
            "redemption_batches",
            "idx_redemption_batches_status_expires",
        ),
        ("redemption_codes", "uq_redemption_codes_code_key"),
        ("redemption_codes", "uq_redemption_codes_sha256"),
        ("redemption_codes", "idx_redemption_codes_batch_status"),
        ("redemption_codes", "idx_redemption_codes_user_redeemed"),
        ("subscription_plans", "uq_subscription_plans_plan_key"),
        ("subscription_plans", "idx_subscription_plans_status_id"),
        ("subscription_plans", "idx_subscription_plans_creator"),
        (
            "subscription_plan_prices",
            "uq_subscription_plan_prices_plan",
        ),
        ("subscription_orders", "uq_subscription_orders_order_key"),
        (
            "subscription_orders",
            "uq_subscription_orders_user_idempotency_key",
        ),
        (
            "subscription_payment_events",
            "uq_subscription_payment_events_event_key",
        ),
        (
            "subscription_payment_events",
            "uq_subscription_payment_events_provider_event",
        ),
        (
            "subscription_payment_events",
            "idx_subscription_payment_events_order_created",
        ),
        (
            "user_subscriptions",
            "uq_user_subscriptions_subscription_key",
        ),
        ("user_subscriptions", "idx_user_subscriptions_user_window"),
        ("user_subscriptions", "idx_user_subscriptions_plan_status"),
        ("user_subscriptions", "idx_user_subscriptions_reset_due"),
        ("invite_rebate_events", "uq_invite_rebate_events_event_key"),
        ("invite_rebate_events", "uq_invite_rebate_events_invitee"),
        (
            "invite_rebate_events",
            "uq_invite_rebate_events_wallet_entry",
        ),
        (
            "invite_rebate_events",
            "idx_invite_rebate_events_inviter_created",
        ),
        (
            "balance_alert_events",
            "uq_balance_alert_events_user_window",
        ),
        ("balance_alert_events", "idx_balance_alert_events_due"),
        (
            "balance_alert_events",
            "idx_balance_alert_events_user_created",
        ),
        (
            "subscription_balance_alert_events",
            "uq_subscription_balance_alert_window",
        ),
        (
            "subscription_balance_alert_events",
            "idx_subscription_balance_alert_due",
        ),
        (
            "subscription_balance_alert_events",
            "idx_subscription_balance_alert_user_created",
        ),
        (
            "user_notification_events",
            "uq_user_notification_events_source",
        ),
        (
            "user_notification_events",
            "idx_user_notification_events_user_cursor",
        ),
        (
            "user_notification_events",
            "idx_user_notification_events_delivery",
        ),
        (
            "user_notification_receipts",
            "idx_user_notification_receipts_user_read",
        ),
        ("organizations", "uq_organizations_key"),
        ("organizations", "uq_organizations_slug"),
        ("organizations", "idx_organizations_status_id"),
        ("organization_teams", "uq_organization_teams_scope_id"),
        ("organization_teams", "uq_organization_teams_scope_name"),
        ("organization_teams", "idx_organization_teams_scope_status"),
        (
            "organization_memberships",
            "uq_organization_memberships_scope_user",
        ),
        (
            "organization_scim_user_resources",
            "uq_organization_scim_user_resources_scope_resource",
        ),
        (
            "organization_scim_user_resources",
            "uq_organization_scim_user_resources_scope_user",
        ),
        (
            "organization_scim_group_resources",
            "uq_organization_scim_group_resources_scope_resource",
        ),
        (
            "organization_scim_group_resources",
            "uq_organization_scim_group_resources_scope_team",
        ),
        (
            "organization_scim_audit_logs",
            "uq_organization_scim_audit_logs_request_event",
        ),
        (
            "organization_scim_audit_logs",
            "ix_organization_scim_audit_logs_scope_id",
        ),
        (
            "organization_memberships",
            "idx_organization_memberships_scope_status",
        ),
        (
            "organization_memberships",
            "idx_organization_memberships_scope_team_status",
        ),
        (
            "organization_entitlements",
            "uq_organization_entitlements_organization",
        ),
        (
            "organization_entitlements",
            "idx_organization_entitlements_validity",
        ),
        (
            "organization_custom_roles",
            "uq_organization_custom_roles_key",
        ),
        (
            "organization_custom_roles",
            "uq_organization_custom_roles_scope_name",
        ),
        (
            "organization_custom_roles",
            "idx_organization_custom_roles_scope_status",
        ),
        (
            "organization_custom_roles",
            "uq_organization_custom_roles_scope_id",
        ),
        (
            "organization_memberships",
            "idx_organization_memberships_custom_role",
        ),
        (
            "organization_custom_role_permissions",
            "uq_organization_custom_role_permissions_role_code",
        ),
        (
            "organization_invitations",
            "uq_organization_invitations_key",
        ),
        (
            "organization_invitations",
            "uq_organization_invitations_token_digest",
        ),
        (
            "organization_invitations",
            "idx_organization_invitations_email_status",
        ),
        (
            "organization_invitations",
            "idx_organization_invitations_status_expiry",
        ),
        (
            "organization_invitations",
            "uq_organization_invitations_batch_email",
        ),
        ("topup_orders", "idx_topup_orders_organization"),
        (
            "organization_wallets",
            "uq_organization_wallets_organization",
        ),
        (
            "organization_wallet_ledger_entries",
            "uq_organization_wallet_ledger_scope_event",
        ),
        (
            "organization_wallet_ledger_entries",
            "idx_organization_wallet_ledger_scope_id",
        ),
        (
            "organization_memberships",
            "uq_organization_memberships_scope_id",
        ),
        ("tokens", "idx_tokens_organization_status_id"),
        ("tokens", "idx_tokens_organization_owner_id"),
        ("tokens", "idx_tokens_organization_membership_status"),
        (
            "billing_reservations",
            "idx_billing_reservations_organization_status",
        ),
        ("usage_logs", "idx_usage_logs_organization_id"),
        ("usage_logs", "idx_usage_logs_organization_user_id"),
        ("usage_logs", "idx_usage_logs_organization_team_id"),
        ("organization_audit_logs", "uq_org_audit_request_operation"),
        ("organization_audit_logs", "idx_org_audit_organization_id"),
        (
            "organization_audit_logs",
            "idx_org_audit_organization_actor_id",
        ),
        (
            "organization_audit_logs",
            "idx_org_audit_organization_operation_id",
        ),
        (
            "organization_audit_logs",
            "idx_org_audit_organization_created",
        ),
        ("organization_audit_logs", "idx_org_audit_platform_id"),
        ("platform_audit_logs", "uq_platform_audit_request_action"),
        ("platform_audit_logs", "idx_platform_audit_operator_id"),
        ("platform_audit_logs", "idx_platform_audit_route_id"),
        ("platform_audit_logs", "idx_platform_audit_created_id"),
        ("request_outcome_logs", "uq_request_outcome_request_id"),
        ("request_outcome_logs", "idx_request_outcome_created_id"),
        (
            "request_outcome_logs",
            "idx_request_outcome_outcome_created",
        ),
        (
            "request_outcome_logs",
            "idx_request_outcome_channel_created",
        ),
        ("request_outcome_logs", "idx_request_outcome_flow_created"),
        (
            "analytics_export_outbox_events",
            "uq_analytics_export_outbox_fact",
        ),
        (
            "analytics_export_outbox_events",
            "idx_analytics_export_outbox_due",
        ),
        (
            "analytics_export_outbox_events",
            "idx_analytics_export_outbox_fact_cursor",
        ),
    ] {
        assert!(
            manager.has_index(table, index).await?,
            "数据表 {table} 缺少索引 {index}"
        );
    }
    Ok(())
}

/// 在当前方言上验证组织事务仓储的 Owner、团队归属和禁用保护。
async fn exercise_organization_repository(pool: &DatabasePool) -> Result<(), Box<dyn Error>> {
    let group = group_active_model("organization-smoke-group", "企业迁移冒烟分组")
        .insert(pool.connection())
        .await?;
    let mut owner = user_active_model(
        group.id,
        "organization-smoke-owner",
        "organization-smoke-owner@example.com",
        "organization-smoke-owner-aff",
    );
    owner.status = Set(1);
    let owner = owner.insert(pool.connection()).await?;
    let mut member = user_active_model(
        group.id,
        "organization-smoke-member",
        "organization-smoke-member@example.com",
        "organization-smoke-member-aff",
    );
    member.status = Set(1);
    let member = member.insert(pool.connection()).await?;
    let owner_user_id = UserId::new(owner.id)?;
    let member_user_id = UserId::new(member.id)?;
    let repository = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, owner_membership) = repository
        .create(OrganizationCreate::new(
            "Organization Smoke".to_owned(),
            "organization-smoke".to_owned(),
            owner_user_id,
            migration_test_entitlement()?,
        ))
        .await?
        .into_parts();

    let mut enterprise_order = topup_order_active_model(
        owner.id,
        "abababababababababababababababab",
        "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd",
    );
    enterprise_order.organization_id = Set(Some(organization.id().get()));
    enterprise_order.insert(pool.connection()).await?;
    let mut invalid_order = topup_order_active_model(
        owner.id,
        "efefefefefefefefefefefefefefefef",
        "12121212121212121212121212121212",
    );
    invalid_order.organization_id = Set(Some(i64::MAX));
    assert!(invalid_order.insert(pool.connection()).await.is_err());

    assert_eq!(
        repository
            .change_membership(
                organization.id(),
                owner_membership.id(),
                OrganizationMembershipChange::new(
                    OrganizationRole::Admin,
                    OrganizationMembershipStatus::Active,
                    None,
                    owner_membership.version(),
                ),
            )
            .await,
        Err(OrganizationRepositoryError::LastOwner)
    );

    let team = repository
        .create_team(
            organization.id(),
            OrganizationTeamCreate::new("Smoke Team".to_owned(), None),
        )
        .await?;
    let membership = repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                member_user_id,
                OrganizationRole::Member,
                Some(team.id()),
                Some(owner_user_id),
            ),
        )
        .await?;
    let valid_hash = "f0".repeat(32);
    let mut enterprise_token =
        token_active_model(member.id, group.id, &valid_hash, "organization-smoke-token");
    enterprise_token.group_id = Set(None);
    enterprise_token.organization_id = Set(Some(organization.id().get()));
    enterprise_token.organization_membership_id = Set(Some(membership.id().get()));
    enterprise_token.organization_team_id = Set(Some(team.id().get()));
    let enterprise_token = enterprise_token.insert(pool.connection()).await?;

    let invalid_shape_hash = "f1".repeat(32);
    let mut invalid_shape = token_active_model(
        member.id,
        group.id,
        &invalid_shape_hash,
        "organization-invalid-shape-token",
    );
    invalid_shape.group_id = Set(None);
    invalid_shape.organization_id = Set(Some(organization.id().get()));
    assert!(invalid_shape.insert(pool.connection()).await.is_err());

    let (other_organization, other_owner_membership) = repository
        .create(OrganizationCreate::new(
            "Other Organization Smoke".to_owned(),
            "other-organization-smoke".to_owned(),
            owner_user_id,
            migration_test_entitlement()?,
        ))
        .await?
        .into_parts();
    let other_team = repository
        .create_team(
            other_organization.id(),
            OrganizationTeamCreate::new("Other Smoke Team".to_owned(), None),
        )
        .await?;
    exercise_organization_activity_repository(
        pool,
        organization.id(),
        membership.id(),
        team.id(),
        other_organization.id(),
        other_team.id(),
        owner_user_id,
        member_user_id,
        TokenId::new(enterprise_token.id)?,
        GroupId::new(group.id)?,
    )
    .await?;
    exercise_platform_audit_repository(pool, owner_user_id, member_user_id).await?;
    let cross_scope_hash = "f2".repeat(32);
    let mut cross_scope = token_active_model(
        owner.id,
        group.id,
        &cross_scope_hash,
        "organization-cross-scope-token",
    );
    cross_scope.group_id = Set(None);
    cross_scope.organization_id = Set(Some(organization.id().get()));
    cross_scope.organization_membership_id = Set(Some(other_owner_membership.id().get()));
    assert!(cross_scope.insert(pool.connection()).await.is_err());
    assert_eq!(
        repository
            .disable_team(organization.id(), team.id(), team.version())
            .await,
        Err(OrganizationRepositoryError::TeamHasActiveMembers)
    );
    repository
        .change_membership(
            organization.id(),
            membership.id(),
            OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Active,
                None,
                membership.version(),
            ),
        )
        .await?;
    assert_eq!(
        tokens::Entity::find_by_id(enterprise_token.id)
            .one(pool.connection())
            .await?
            .expect("调组后的企业 Key 必须保留审计事实")
            .status,
        2
    );
    repository
        .disable_team(organization.id(), team.id(), team.version())
        .await?;
    Ok(())
}

/// 在当前方言上验证平台审计的幂等追加、稳定分页和更新保护。
async fn exercise_platform_audit_repository(
    pool: &DatabasePool,
    owner_user_id: UserId,
    member_user_id: UserId,
) -> Result<(), Box<dyn Error>> {
    let repository = PlatformAuditRepository::new(pool.clone());
    let succeeded = PlatformAuditWrite::new(
        owner_user_id,
        "platform.user_directory.read_all".to_owned(),
        "/api/admin/users".to_owned(),
        "platform.user_directory.list".to_owned(),
        "user_directory".to_owned(),
        None,
        PlatformAuditOutcome::Succeeded,
        None,
        None,
        Some(r#"{"after":null,"limit":2}"#.to_owned()),
        "platform-audit-success".to_owned(),
    )?;
    assert_eq!(
        repository.record(&succeeded).await?,
        PlatformAuditWriteOutcome::Applied
    );
    assert_eq!(
        repository.record(&succeeded).await?,
        PlatformAuditWriteOutcome::Existing
    );
    let conflicting = PlatformAuditWrite::new(
        owner_user_id,
        "platform.user_directory.read_all".to_owned(),
        "/api/admin/users".to_owned(),
        "platform.user_directory.list".to_owned(),
        "user_directory".to_owned(),
        None,
        PlatformAuditOutcome::Succeeded,
        None,
        None,
        Some(r#"{"after":null,"limit":10}"#.to_owned()),
        "platform-audit-success".to_owned(),
    )?;
    assert_eq!(
        repository.record(&conflicting).await,
        Err(PlatformAuditRepositoryError::Conflict)
    );
    for (user_id, request_id, outcome) in [
        (
            member_user_id,
            "platform-audit-denied",
            PlatformAuditOutcome::Denied,
        ),
        (
            owner_user_id,
            "platform-audit-failed",
            PlatformAuditOutcome::Failed,
        ),
    ] {
        let write = PlatformAuditWrite::new(
            user_id,
            "platform.user_directory.read_all".to_owned(),
            "/api/admin/users".to_owned(),
            "platform.user_directory.list".to_owned(),
            "user_directory".to_owned(),
            None,
            outcome,
            None,
            None,
            None,
            request_id.to_owned(),
        )?;
        assert_eq!(
            repository.record(&write).await?,
            PlatformAuditWriteOutcome::Applied
        );
    }
    let (first_page, next_cursor) = repository
        .list(&PlatformAuditQuery::new(None, 2, None)?)
        .await?
        .into_parts();
    assert_eq!(first_page.len(), 2);
    assert!(next_cursor.is_some());
    let (second_page, final_cursor) = repository
        .list(&PlatformAuditQuery::new(next_cursor, 2, None)?)
        .await?
        .into_parts();
    assert_eq!(second_page.len(), 1);
    assert_eq!(final_cursor, None);
    let (owner_records, owner_cursor) = repository
        .list(&PlatformAuditQuery::new(None, 10, Some(owner_user_id))?)
        .await?
        .into_parts();
    assert_eq!(owner_records.len(), 2);
    assert_eq!(owner_cursor, None);
    let stored = platform_audit_logs::Entity::find()
        .filter(platform_audit_logs::Column::RequestId.eq("platform-audit-success"))
        .one(pool.connection())
        .await?
        .expect("平台审计事实必须存在");
    let mut forbidden_update = stored.into_active_model();
    forbidden_update.outcome = Set(PlatformAuditOutcome::Failed as i16);
    assert!(forbidden_update.update(pool.connection()).await.is_err());
    Ok(())
}

/// 在三方言实库中闭合企业用量归属、审计追加、筛选分页与保留期清理。
#[allow(
    clippy::too_many_arguments,
    reason = "参数逐一对应已创建的企业测试主体"
)]
async fn exercise_organization_activity_repository(
    pool: &DatabasePool,
    organization_id: OrganizationId,
    membership_id: OrganizationMembershipId,
    team_id: OrganizationTeamId,
    other_organization_id: OrganizationId,
    other_team_id: OrganizationTeamId,
    owner_user_id: UserId,
    member_user_id: UserId,
    token_id: TokenId,
    group_id: GroupId,
) -> Result<(), Box<dyn Error>> {
    let usage_repository = UsageLogRepository::new(pool.clone());
    let enterprise_principal = GatewayPrincipal::organization(
        token_id,
        member_user_id,
        group_id,
        OrganizationGatewayPrincipal::new(organization_id, membership_id, Some(team_id)),
    );
    let usage_write = UsageLogWrite::new(
        BillingReservationId::new([0xe1; 16])?,
        enterprise_principal,
        UsageLogBillingMode::PerToken,
        UsageLogUsage::new(12, 3, 0, 0, 0, 1, 0, 0)?,
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(15)?,
    );
    assert_eq!(
        usage_repository.record(&usage_write).await?,
        UsageLogWriteOutcome::Applied
    );
    let stored_usage = usage_logs::Entity::find()
        .filter(usage_logs::Column::OrganizationId.eq(organization_id.get()))
        .one(pool.connection())
        .await?
        .expect("企业用量事实必须写入组织归属");
    assert_eq!(stored_usage.user_id, member_user_id.get());
    assert_eq!(stored_usage.organization_team_id, Some(team_id.get()));

    let cross_team_principal = GatewayPrincipal::organization(
        token_id,
        member_user_id,
        group_id,
        OrganizationGatewayPrincipal::new(organization_id, membership_id, Some(other_team_id)),
    );
    let cross_team_write = UsageLogWrite::new(
        BillingReservationId::new([0xe2; 16])?,
        cross_team_principal,
        UsageLogBillingMode::PerToken,
        UsageLogUsage::new(1, 1, 0, 0, 0, 0, 0, 0)?,
        UsageLogSource::Upstream,
        UsageLogSemantics::Inclusive,
        Quota::new(2)?,
    );
    assert_eq!(
        usage_repository.record(&cross_team_write).await,
        Err(UsageLogRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        usage_logs::Entity::find()
            .filter(usage_logs::Column::OrganizationId.eq(organization_id.get()))
            .count(pool.connection())
            .await?,
        1
    );

    let audit_repository = OrganizationAuditRepository::new(pool.clone());
    let succeeded = OrganizationAuditWrite::new(
        organization_id,
        owner_user_id,
        Some(OrganizationRole::Owner),
        "organization.teams.manage".to_owned(),
        "organization.team.create".to_owned(),
        "team".to_owned(),
        team_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        None,
        "audit-request-success".to_owned(),
        [0x11; 32],
        Some(r#"{"name":"Smoke Team"}"#.to_owned()),
    )?;
    assert_eq!(
        audit_repository.record(&succeeded).await?,
        OrganizationAuditWriteOutcome::Applied
    );
    assert_eq!(
        audit_repository.record(&succeeded).await?,
        OrganizationAuditWriteOutcome::Existing
    );
    let conflicting = OrganizationAuditWrite::new(
        organization_id,
        owner_user_id,
        Some(OrganizationRole::Owner),
        "organization.teams.manage".to_owned(),
        "organization.team.create".to_owned(),
        "team".to_owned(),
        "conflicting-target".to_owned(),
        OrganizationAuditOutcome::Succeeded,
        None,
        "audit-request-success".to_owned(),
        [0x11; 32],
        Some(r#"{"name":"Smoke Team"}"#.to_owned()),
    )?;
    assert_eq!(
        audit_repository.record(&conflicting).await,
        Err(OrganizationAuditRepositoryError::Conflict)
    );

    for (request_id, operation, outcome, fingerprint) in [
        (
            "audit-request-failed",
            "organization.member.update",
            OrganizationAuditOutcome::Failed,
            [0x22; 32],
        ),
        (
            "audit-request-denied",
            "organization.member.disable",
            OrganizationAuditOutcome::Denied,
            [0x33; 32],
        ),
    ] {
        let write = OrganizationAuditWrite::new(
            organization_id,
            member_user_id,
            Some(OrganizationRole::Member),
            "organization.members.manage".to_owned(),
            operation.to_owned(),
            "membership".to_owned(),
            membership_id.get().to_string(),
            outcome,
            None,
            request_id.to_owned(),
            fingerprint,
            None,
        )?;
        assert_eq!(
            audit_repository.record(&write).await?,
            OrganizationAuditWriteOutcome::Applied
        );
    }
    let platform_write = OrganizationAuditWrite::new(
        organization_id,
        owner_user_id,
        None,
        "platform.organizations.read".to_owned(),
        "organization.audit.list".to_owned(),
        "organization".to_owned(),
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some("企业支持工单核查".to_owned()),
        "audit-request-platform".to_owned(),
        [0x44; 32],
        None,
    )?;
    assert_eq!(
        audit_repository.record(&platform_write).await?,
        OrganizationAuditWriteOutcome::Applied
    );
    let other_write = OrganizationAuditWrite::new(
        other_organization_id,
        owner_user_id,
        Some(OrganizationRole::Owner),
        "organization.audit.read".to_owned(),
        "organization.audit.list".to_owned(),
        "organization".to_owned(),
        other_organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        None,
        "audit-request-other".to_owned(),
        [0x55; 32],
        None,
    )?;
    assert_eq!(
        audit_repository.record(&other_write).await?,
        OrganizationAuditWriteOutcome::Applied
    );

    let (first_page, next_cursor) = audit_repository
        .list(
            organization_id,
            &OrganizationAuditQuery::new(None, 2, None, None, None, None)?,
        )
        .await?
        .into_parts();
    assert_eq!(first_page.len(), 2);
    let (second_page, final_cursor) = audit_repository
        .list(
            organization_id,
            &OrganizationAuditQuery::new(next_cursor, 2, None, None, None, None)?,
        )
        .await?
        .into_parts();
    assert_eq!(second_page.len(), 2);
    assert_eq!(final_cursor, None);

    let (denied_records, denied_cursor) = audit_repository
        .list(
            organization_id,
            &OrganizationAuditQuery::new(
                None,
                10,
                Some(member_user_id),
                None,
                Some("membership".to_owned()),
                Some(OrganizationAuditOutcome::Denied),
            )?,
        )
        .await?
        .into_parts();
    assert_eq!(denied_records.len(), 1);
    assert_eq!(denied_records[0].request_id(), "audit-request-denied");
    assert_eq!(denied_cursor, None);

    let stored_audit = organization_audit_logs::Entity::find()
        .filter(organization_audit_logs::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_audit_logs::Column::RequestId.eq("audit-request-success"))
        .one(pool.connection())
        .await?
        .expect("企业审计事实必须存在");
    let mut forbidden_update = stored_audit.into_active_model();
    forbidden_update.outcome = Set(OrganizationAuditOutcome::Failed as i16);
    assert!(forbidden_update.update(pool.connection()).await.is_err());

    assert_eq!(audit_repository.retention_days(organization_id).await?, 90);
    let cutoff = TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp()
        .checked_add(60)
        .ok_or("审计清理截止时间溢出")?;
    assert_eq!(
        audit_repository
            .prune_before(organization_id, cutoff)
            .await?,
        4
    );
    let (remaining, _) = audit_repository
        .list(
            organization_id,
            &OrganizationAuditQuery::new(None, 10, None, None, None, None)?,
        )
        .await?
        .into_parts();
    assert!(remaining.is_empty());
    let (other_remaining, _) = audit_repository
        .list(
            other_organization_id,
            &OrganizationAuditQuery::new(None, 10, None, None, None, None)?,
        )
        .await?
        .into_parts();
    assert_eq!(other_remaining.len(), 1);
    Ok(())
}

fn migration_test_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 2)?,
            true,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )?,
        "migration-smoke".to_owned(),
        None,
        None,
    )?)
}

async fn assert_entity_columns<C>(manager: &SchemaManager<'_>, table: &str) -> Result<(), DbErr>
where
    C: Iterable + IdenStatic,
{
    for column in C::iter() {
        assert!(
            manager.has_column(table, column.as_str()).await?,
            "数据表 {table} 缺少实体字段 {}",
            column.as_str()
        );
    }
    Ok(())
}

async fn assert_core_tables_absent(connection: &DatabaseConnection) -> Result<(), DbErr> {
    let manager = SchemaManager::new(connection);
    for table in CORE_TABLES {
        assert!(
            !manager.has_table(table).await?,
            "回滚后仍存在数据表 {table}"
        );
    }
    Ok(())
}

async fn assert_updated_at_behaviors(connection: &DatabaseConnection) -> Result<(), DbErr> {
    macro_rules! assert_behavior {
        ($active_model:path) => {{
            let insert_model = <$active_model as ActiveModelBehavior>::before_save(
                <$active_model as Default>::default(),
                connection,
                true,
            )
            .await?;
            assert!(insert_model.updated_at.is_not_set());

            let update_model = <$active_model as ActiveModelBehavior>::before_save(
                <$active_model as Default>::default(),
                connection,
                false,
            )
            .await?;
            assert!(update_model.updated_at.is_set());
        }};
    }

    assert_behavior!(abilities::ActiveModel);
    assert_behavior!(authentication_settings::ActiveModel);
    assert_behavior!(balance_alert_settings::ActiveModel);
    assert_behavior!(billing_batch_checkpoints::ActiveModel);
    assert_behavior!(billing_reservations::ActiveModel);
    assert_behavior!(channel_groups::ActiveModel);
    assert_behavior!(channel_models::ActiveModel);
    assert_behavior!(channels::ActiveModel);
    assert_behavior!(credentials::ActiveModel);
    assert_behavior!(group_model_ratios::ActiveModel);
    assert_behavior!(groups::ActiveModel);
    assert_behavior!(options::ActiveModel);
    assert_behavior!(registration_rate_limits::ActiveModel);
    assert_behavior!(site_settings::ActiveModel);
    assert_behavior!(email_settings::ActiveModel);
    assert_behavior!(tokens::ActiveModel);
    assert_behavior!(users::ActiveModel);
    Ok(())
}

/// 验证余额预警迁移的固定默认值、个人阈值和事件状态约束。
async fn exercise_balance_alert_migration(
    connection: &DatabaseConnection,
) -> Result<(), Box<dyn Error>> {
    let settings = balance_alert_settings::Entity::find_by_id(1_i16)
        .one(connection)
        .await?
        .expect("余额预警固定设置行必须存在");
    assert!(!settings.enabled);
    assert_eq!(settings.default_threshold_quota, 1_000);
    assert_eq!(settings.reminder_interval_seconds, 86_400);
    assert!(!settings.subscription_alert_enabled);
    assert_eq!(settings.subscription_remaining_percent, 20);
    assert_eq!(settings.version, 1);

    let invalid_subscription_percent = Query::update()
        .table(balance_alert_settings::Entity)
        .value(
            balance_alert_settings::Column::SubscriptionRemainingPercent,
            0_i16,
        )
        .and_where(Expr::col(balance_alert_settings::Column::Id).eq(1_i16))
        .to_owned();
    assert!(
        connection
            .execute(
                connection
                    .get_database_backend()
                    .build(&invalid_subscription_percent),
            )
            .await
            .is_err()
    );

    let group = group_active_model("balance-alert-smoke", "余额预警迁移验证")
        .insert(connection)
        .await?;
    let user = user_active_model(
        group.id,
        "balance-alert-user",
        "balance-alert@example.com",
        "BALANCEALERTSMOKE",
    )
    .insert(connection)
    .await?;
    assert_eq!(user.balance_alert_threshold, None);

    let mut active_user = user.clone().into_active_model();
    active_user.balance_alert_threshold = Set(Some(250));
    let user = active_user.update(connection).await?;
    assert_eq!(user.balance_alert_threshold, Some(250));

    // 绕过 ActiveModel 校验，确保三种数据库自身都会拒绝非正阈值。
    let invalid_threshold = Query::update()
        .table(users::Entity)
        .value(users::Column::BalanceAlertThreshold, 0_i64)
        .and_where(Expr::col(users::Column::Id).eq(user.id))
        .to_owned();
    assert!(
        connection
            .execute(connection.get_database_backend().build(&invalid_threshold))
            .await
            .is_err()
    );

    let now = TimeDateTimeWithTimeZone::now_utc();
    let pending_event = || balance_alert_events::ActiveModel {
        user_id: Set(user.id),
        window_started_at_epoch: Set(86_400),
        threshold_quota: Set(250),
        observed_quota: Set(100),
        status: Set(1),
        attempt_count: Set(0),
        next_attempt_at: Set(now),
        lease_expires_at: Set(None),
        last_error_kind: Set(None),
        version: Set(1),
        sent_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    balance_alert_events::Entity::insert(pending_event())
        .exec_without_returning(connection)
        .await?;
    assert!(
        balance_alert_events::Entity::insert(pending_event())
            .exec_without_returning(connection)
            .await
            .is_err()
    );

    // Sending 状态必须持有租约，防止崩溃后出现无法重领的永久悬挂事件。
    let mut invalid_state = pending_event();
    invalid_state.window_started_at_epoch = Set(172_800);
    invalid_state.status = Set(2);
    invalid_state.attempt_count = Set(1);
    assert!(
        balance_alert_events::Entity::insert(invalid_state)
            .exec_without_returning(connection)
            .await
            .is_err()
    );

    let plan = subscription_plans::ActiveModel {
        plan_key: Set(SensitiveString::from("1234567890abcdef1234567890abcdef")),
        name: Set("迁移预警计划".to_owned()),
        created_by_user_id: Set(user.id),
        status: Set(1),
        quota_amount: Set(1_000),
        cycle: Set(1),
        version: Set(1),
        disabled_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    let window_ends_at = now + Duration::from_secs(86_400);
    let subscription = user_subscriptions::ActiveModel {
        subscription_key: Set(SensitiveString::from("abcdef1234567890abcdef1234567890")),
        user_id: Set(user.id),
        plan_id: Set(plan.id),
        plan_version: Set(1),
        status: Set(1),
        quota_amount: Set(1_000),
        quota_used: Set(0),
        cycle: Set(1),
        window_started_at: Set(now),
        window_ends_at: Set(window_ends_at),
        version: Set(1),
        bound_at: Set(now),
        status_changed_at: Set(now),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    let subscription_event = || subscription_balance_alert_events::ActiveModel {
        user_subscription_id: Set(subscription.id),
        user_id: Set(user.id),
        window_started_at: Set(now),
        window_ends_at: Set(window_ends_at),
        threshold_percent: Set(20),
        quota_amount: Set(1_000),
        observed_quota_used: Set(800),
        status: Set(1),
        attempt_count: Set(0),
        next_attempt_at: Set(now),
        lease_expires_at: Set(None),
        last_error_kind: Set(None),
        version: Set(1),
        sent_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    subscription_balance_alert_events::Entity::insert(subscription_event())
        .exec_without_returning(connection)
        .await?;
    assert!(
        subscription_balance_alert_events::Entity::insert(subscription_event())
            .exec_without_returning(connection)
            .await
            .is_err()
    );
    Ok(())
}

async fn exercise_core_schema(connection: &DatabaseConnection) -> Result<(), DbErr> {
    let group = group_active_model("default", "默认分组")
        .insert(connection)
        .await?;
    assert_eq!(group.ratio_micros, 1_000_000);
    group_model_ratios::ActiveModel {
        source_group_id: Set(group.id),
        target_group_id: Set(group.id),
        ratio_micros: Set(1_000_000),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    assert!(
        group_active_model("default", "重复分组")
            .insert(connection)
            .await
            .is_err()
    );

    let user = user_active_model(group.id, "alice", "alice@example.com", "invite-a")
        .insert(connection)
        .await?;
    assert_eq!(user.status, 2);
    assert_debug_redacted(&user, "password-hash");
    assert!(
        user_active_model(
            group.id,
            "alice",
            "alice-other@example.com",
            "invite-duplicate-username",
        )
        .insert(connection)
        .await
        .is_err()
    );
    assert!(
        user_active_model(
            group.id,
            "alice-other",
            "alice@example.com",
            "invite-duplicate-email",
        )
        .insert(connection)
        .await
        .is_err()
    );

    let channel = channel_active_model("primary").insert(connection).await?;
    assert_eq!(channel.status, 2);
    assert_debug_redacted(&channel, "https://api.example.com");
    let credential = oauth_credential_active_model(channel.id, "credential-ciphertext")
        .insert(connection)
        .await?;
    assert_eq!(credential.status, 2);
    assert!(!credential.schedulable);
    assert_eq!(credential.oauth_revision, 0);
    assert_debug_redacted(&credential, "credential-ciphertext");

    let child_credential = spark_shadow_active_model(channel.id, credential.id, "child-ciphertext");
    child_credential.insert(connection).await?;

    let mut self_parent = credential.clone().into_active_model();
    self_parent.parent_id = Set(Some(credential.id));
    assert!(self_parent.update(connection).await.is_err());

    let direct_parent = oauth_credential_active_model(channel.id, "direct-parent")
        .insert(connection)
        .await?;
    let direct_child = spark_shadow_active_model(channel.id, direct_parent.id, "direct-child");
    let direct_child = direct_child.insert(connection).await?;
    credentials::Entity::delete_by_id(direct_parent.id)
        .exec(connection)
        .await?;
    assert!(
        credentials::Entity::find_by_id(direct_child.id)
            .one(connection)
            .await?
            .is_none()
    );
    let token = token_active_model(user.id, group.id, TOKEN_HASH_A, "token-a")
        .insert(connection)
        .await?;
    assert_eq!(token.status, 2);
    assert_eq!(token.expired_at, Some(future_expiration()));
    assert_debug_redacted(&token, TOKEN_HASH_A);

    let reservation =
        reservation_active_model(RESERVATION_KEY_A, user.id, token.id, group.id, 100, 100)
            .insert(connection)
            .await?;
    assert_eq!(reservation.status, 1);
    assert_eq!(reservation.actual_quota, None);
    assert_eq!(reservation.finalized_at, None);
    assert_debug_redacted(&reservation, RESERVATION_KEY_A);
    assert_billing_reservation_constraints(connection, user.id, token.id, group.id).await?;

    let payment = payment_settings::Entity::find_by_id(1_i16)
        .one(connection)
        .await?
        .expect("支付设置固定行必须存在");
    assert!(!payment.initialized);
    assert!(!payment.stripe_enabled);
    assert!(!payment.epay_enabled);
    assert!(!payment.epay_refund_enabled);
    assert!(!payment.refund_auto_submit_enabled);
    assert_eq!(payment.stripe_signature_tolerance_seconds, 300);
    assert_eq!(payment.epay_quota_per_cny, 500_000);
    assert_eq!(payment.version, 1);

    let topup_order = topup_order_active_model(user.id, TOPUP_ORDER_KEY_A, TOPUP_IDEMPOTENCY_KEY_A)
        .insert(connection)
        .await?;
    assert_eq!(topup_order.status, 1);
    let old_order_updated_at = historical_audit_time();
    let mut updated_order = topup_order.into_active_model();
    updated_order.provider_order_id = Set(Some(SensitiveString::from("provider-order-a")));
    updated_order.updated_at = Set(old_order_updated_at);
    let topup_order = updated_order.update(connection).await?;
    assert!(topup_order.updated_at > old_order_updated_at);
    assert_debug_redacted(&topup_order, "provider-order-a");

    let topup_event = topup_payment_event_active_model(
        topup_order.id,
        TOPUP_EVENT_KEY_A,
        TOPUP_PROVIDER_EVENT_ID_A,
    )
    .insert(connection)
    .await?;
    assert_eq!(topup_event.event_type, 1);
    assert_debug_redacted(&topup_event, TOPUP_PROVIDER_EVENT_ID_A);
    assert_topup_payment_constraints(connection, user.id, topup_order.id).await?;

    channel_models::ActiveModel {
        channel_id: Set(channel.id),
        model: Set("gpt-5".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channel_groups::ActiveModel {
        channel_id: Set(channel.id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    let option = option_active_model("site.name", "AnyFlows")
        .insert(connection)
        .await?;
    assert_debug_redacted(&option, "AnyFlows");
    let old_audit_time = historical_audit_time();
    let mut updated_option = option.into_active_model();
    updated_option.value = Set(SensitiveString::from("AnyFlows updated"));
    updated_option.updated_at = Set(old_audit_time);
    let updated_option = updated_option.update(connection).await?;
    assert!(updated_option.updated_at > old_audit_time);
    assert_persistence_constraints(
        connection,
        group.id,
        user.id,
        channel.id,
        credential.id,
        token.id,
    )
    .await?;
    assert_foreign_key_actions(connection, group.id).await?;

    let cascade_channel = channel_active_model("cascade").insert(connection).await?;
    let cross_channel_parent = oauth_credential_active_model(channel.id, "cross-channel-parent")
        .insert(connection)
        .await?;
    let cross_channel_child = spark_shadow_active_model(
        cascade_channel.id,
        cross_channel_parent.id,
        "cross-channel-child",
    );
    assert!(cross_channel_child.insert(connection).await.is_err());
    let raw_cross_channel_child =
        credential_active_model(cascade_channel.id, "raw-cross-channel-child")
            .insert(connection)
            .await?;
    assert!(
        execute_raw_update(
            connection,
            Query::update()
                .table(credentials::Entity)
                .value(credentials::Column::Kind, CredentialKind::Oauth.as_str())
                .value(credentials::Column::ParentId, cross_channel_parent.id)
                .value(credentials::Column::QuotaDimension, "spark")
                .and_where(Expr::col(credentials::Column::Id).eq(raw_cross_channel_child.id))
                .to_owned(),
        )
        .await
        .is_err()
    );
    credentials::Entity::delete_by_id(raw_cross_channel_child.id)
        .exec(connection)
        .await?;
    credentials::Entity::delete_by_id(cross_channel_parent.id)
        .exec(connection)
        .await?;
    let cascade_parent = oauth_credential_active_model(cascade_channel.id, "cascade-ciphertext")
        .insert(connection)
        .await?;
    let cascade_child =
        spark_shadow_active_model(cascade_channel.id, cascade_parent.id, "cascade-child");
    cascade_child.insert(connection).await?;
    channel_models::ActiveModel {
        channel_id: Set(cascade_channel.id),
        model: Set("gpt-5".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channel_groups::ActiveModel {
        channel_id: Set(cascade_channel.id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(cascade_channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channels::Entity::delete_by_id(cascade_channel.id)
        .exec(connection)
        .await?;

    assert_eq!(credentials::Entity::find().count(connection).await?, 2);
    assert_eq!(abilities::Entity::find().count(connection).await?, 1);
    assert_eq!(channel_models::Entity::find().count(connection).await?, 1);
    assert_eq!(channel_groups::Entity::find().count(connection).await?, 1);
    assert_eq!(options::Entity::find().count(connection).await?, 1);

    assert!(
        channel_models::ActiveModel {
            channel_id: Set(channel.id),
            model: Set("gpt-5".to_owned()),
            ..Default::default()
        }
        .insert(connection)
        .await
        .is_err()
    );
    channel_models::Entity::delete_by_id((channel.id, "gpt-5".to_owned()))
        .exec(connection)
        .await?;
    assert_eq!(abilities::Entity::find().count(connection).await?, 0);
    channel_models::ActiveModel {
        channel_id: Set(channel.id),
        model: Set("gpt-5".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    channel_groups::Entity::delete_by_id((channel.id, group.id))
        .exec(connection)
        .await?;
    assert_eq!(abilities::Entity::find().count(connection).await?, 0);
    channel_groups::ActiveModel {
        channel_id: Set(channel.id),
        group_id: Set(group.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    abilities::ActiveModel {
        group_id: Set(group.id),
        model: Set("gpt-5".to_owned()),
        channel_id: Set(channel.id),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    assert!(
        token_active_model(user.id, i64::MAX, TOKEN_HASH_B, "invalid-group")
            .insert(connection)
            .await
            .is_err()
    );
    assert!(
        token_active_model(user.id, group.id, TOKEN_HASH_A, "duplicate-hash")
            .insert(connection)
            .await
            .is_err()
    );

    let mut deleted_token = token.into_active_model();
    deleted_token.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    deleted_token.update(connection).await?;
    assert!(
        token_active_model(user.id, group.id, TOKEN_HASH_A, "reused-hash")
            .insert(connection)
            .await
            .is_err()
    );
    assert!(
        groups::Entity::delete_by_id(group.id)
            .exec(connection)
            .await
            .is_err()
    );

    let reusable_group = group_active_model("reusable", "可复用分组")
        .insert(connection)
        .await?;
    let mut deleted_group = reusable_group.into_active_model();
    deleted_group.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    deleted_group.update(connection).await?;
    group_active_model("reusable", "新可复用分组")
        .insert(connection)
        .await?;

    let reusable_user = user_active_model(
        group.id,
        "reusable-user",
        "reusable@example.com",
        "invite-reusable",
    )
    .insert(connection)
    .await?;
    let mut deleted_user = reusable_user.into_active_model();
    deleted_user.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    deleted_user.update(connection).await?;
    assert!(
        user_active_model(group.id, "bob", "bob@example.com", "invite-reusable")
            .insert(connection)
            .await
            .is_err()
    );
    user_active_model(
        group.id,
        "reusable-user",
        "reusable@example.com",
        "invite-replacement",
    )
    .insert(connection)
    .await?;

    assert_invalid_sensitive_rows_fail_closed(connection, group.id).await?;

    Ok(())
}

async fn assert_persistence_constraints(
    connection: &DatabaseConnection,
    group_id: i64,
    user_id: i64,
    channel_id: i64,
    credential_id: i64,
    token_id: i64,
) -> Result<(), DbErr> {
    let mut invalid_ratio = group_active_model("invalid-ratio", "非法倍率");
    invalid_ratio.ratio_micros = Set(-1);
    assert!(invalid_ratio.insert(connection).await.is_err());

    let mut invalid_limit = group_active_model("invalid-limit", "非法限额");
    invalid_limit.daily_limit = Set(Some(-1));
    assert!(invalid_limit.insert(connection).await.is_err());

    assert!(
        execute_raw_update(
            connection,
            Query::update()
                .table(tokens::Entity)
                .value(tokens::Column::KeyHash, "C".repeat(64))
                .and_where(Expr::col(tokens::Column::Id).eq(token_id))
                .to_owned(),
        )
        .await
        .is_err()
    );

    let mut incomplete_peak = group_active_model("incomplete-peak", "不完整高峰窗口");
    incomplete_peak.peak_start = Set(Some(fixed_peak_time()));
    assert!(incomplete_peak.insert(connection).await.is_err());

    let mut ratio_without_peak = group_active_model("ratio-without-peak", "缺少高峰窗口");
    ratio_without_peak.peak_ratio_micros = Set(Some(2_000_000));
    assert!(ratio_without_peak.insert(connection).await.is_err());

    let mut peak_without_ratio = group_active_model("peak-without-ratio", "缺少高峰倍率");
    peak_without_ratio.peak_start = Set(Some(fixed_peak_time()));
    peak_without_ratio.peak_end = Set(Some(fixed_peak_end_time()));
    assert!(peak_without_ratio.insert(connection).await.is_err());

    let self_fallback = group_active_model("self-fallback", "自引用分组")
        .insert(connection)
        .await?;
    let mut self_fallback_update = self_fallback.clone().into_active_model();
    self_fallback_update.fallback_group_id = Set(Some(self_fallback.id));
    assert!(self_fallback_update.update(connection).await.is_err());
    if connection.get_database_backend() != DbBackend::MySql {
        assert!(
            execute_raw_update(
                connection,
                Query::update()
                    .table(groups::Entity)
                    .value(groups::Column::FallbackGroupId, self_fallback.id)
                    .and_where(Expr::col(groups::Column::Id).eq(self_fallback.id))
                    .to_owned(),
            )
            .await
            .is_err()
        );
    }
    groups::Entity::delete_by_id(self_fallback.id)
        .exec(connection)
        .await?;

    let mut invalid_role = user_active_model(
        group_id,
        "invalid-role",
        "invalid-role@example.com",
        "invite-invalid-role",
    );
    invalid_role.role = Set(2);
    assert!(invalid_role.insert(connection).await.is_err());

    let mut invalid_status = user_active_model(
        group_id,
        "invalid-status",
        "invalid-status@example.com",
        "invite-invalid-status",
    );
    invalid_status.status = Set(0);
    assert!(invalid_status.insert(connection).await.is_err());

    let mut invalid_concurrency = user_active_model(
        group_id,
        "invalid-concurrency",
        "invalid-concurrency@example.com",
        "invite-invalid-concurrency",
    );
    invalid_concurrency.concurrency = Set(Some(-1));
    assert!(invalid_concurrency.insert(connection).await.is_err());

    let self_inviter = user_active_model(
        group_id,
        "self-inviter",
        "self-inviter@example.com",
        "invite-self-inviter",
    )
    .insert(connection)
    .await?;
    let mut self_inviter_update = self_inviter.clone().into_active_model();
    self_inviter_update.inviter_id = Set(Some(self_inviter.id));
    assert!(self_inviter_update.update(connection).await.is_err());
    if connection.get_database_backend() != DbBackend::MySql {
        assert!(
            execute_raw_update(
                connection,
                Query::update()
                    .table(users::Entity)
                    .value(users::Column::InviterId, self_inviter.id)
                    .and_where(Expr::col(users::Column::Id).eq(self_inviter.id))
                    .to_owned(),
            )
            .await
            .is_err()
        );
    }
    users::Entity::delete_by_id(self_inviter.id)
        .exec(connection)
        .await?;

    let mut invalid_token_status =
        token_active_model(user_id, group_id, TOKEN_HASH_B, "invalid-token-status");
    invalid_token_status.status = Set(0);
    assert!(invalid_token_status.insert(connection).await.is_err());

    let mut invalid_token_quota =
        token_active_model(user_id, group_id, TOKEN_HASH_B, "invalid-token-quota");
    invalid_token_quota.remain_quota = Set(-1);
    assert!(invalid_token_quota.insert(connection).await.is_err());

    let mut invalid_channel_status = channel_active_model("invalid-channel-status");
    invalid_channel_status.status = Set(0);
    assert!(invalid_channel_status.insert(connection).await.is_err());

    let mut invalid_channel_weight = channel_active_model("invalid-channel-weight");
    invalid_channel_weight.weight = Set(-1);
    assert!(invalid_channel_weight.insert(connection).await.is_err());

    for timeout_secs in [0, 901] {
        let mut invalid_channel_timeout =
            channel_active_model(&format!("invalid-channel-timeout-{timeout_secs}"));
        invalid_channel_timeout.timeout_secs = Set(Some(timeout_secs));
        assert!(invalid_channel_timeout.insert(connection).await.is_err());
    }

    let mut invalid_credential_status = credential_active_model(channel_id, "invalid-status");
    invalid_credential_status.status = Set(0);
    assert!(invalid_credential_status.insert(connection).await.is_err());

    let mut invalid_credential_weight = credential_active_model(channel_id, "invalid-weight");
    invalid_credential_weight.weight = Set(-1);
    assert!(invalid_credential_weight.insert(connection).await.is_err());

    let mut invalid_credential_concurrency =
        credential_active_model(channel_id, "invalid-concurrency");
    invalid_credential_concurrency.concurrency = Set(Some(-1));
    assert!(
        invalid_credential_concurrency
            .insert(connection)
            .await
            .is_err()
    );

    let mut invalid_load_factor = credential_active_model(channel_id, "invalid-load-factor");
    invalid_load_factor.load_factor_micros = Set(Some(-1));
    assert!(invalid_load_factor.insert(connection).await.is_err());

    let mut invalid_rate_multiplier =
        credential_active_model(channel_id, "invalid-rate-multiplier");
    invalid_rate_multiplier.rate_multiplier_micros = Set(Some(-1));
    assert!(invalid_rate_multiplier.insert(connection).await.is_err());

    let mut invalid_quota_dimension =
        credential_active_model(channel_id, "invalid-quota-dimension");
    invalid_quota_dimension.quota_dimension = Set("unknown".to_owned());
    assert!(invalid_quota_dimension.insert(connection).await.is_err());

    let missing_parent = spark_shadow_active_model(channel_id, i64::MAX, "missing-parent");
    assert!(missing_parent.insert(connection).await.is_err());

    if connection.get_database_backend() != DbBackend::MySql {
        assert!(
            execute_raw_update(
                connection,
                Query::update()
                    .table(credentials::Entity)
                    .value(credentials::Column::Kind, CredentialKind::Oauth.as_str())
                    .value(credentials::Column::ParentId, credential_id)
                    .value(credentials::Column::QuotaDimension, "spark")
                    .and_where(Expr::col(credentials::Column::Id).eq(credential_id))
                    .to_owned(),
            )
            .await
            .is_err()
        );
    }

    let mut incomplete_session_window =
        credential_active_model(channel_id, "incomplete-session-window");
    incomplete_session_window.session_window_start = Set(Some(historical_audit_time()));
    assert!(incomplete_session_window.insert(connection).await.is_err());

    channel_models::ActiveModel {
        channel_id: Set(channel_id),
        model: Set("negative-weight-model".to_owned()),
        ..Default::default()
    }
    .insert(connection)
    .await?;
    let invalid_ability = abilities::ActiveModel {
        group_id: Set(group_id),
        model: Set("negative-weight-model".to_owned()),
        channel_id: Set(channel_id),
        weight: Set(-1),
        ..Default::default()
    };
    assert!(invalid_ability.insert(connection).await.is_err());
    channel_models::Entity::delete_by_id((channel_id, "negative-weight-model".to_owned()))
        .exec(connection)
        .await?;

    Ok(())
}

async fn assert_topup_payment_constraints(
    connection: &DatabaseConnection,
    user_id: i64,
    order_id: i64,
) -> Result<(), DbErr> {
    for (order_key, idempotency_key) in [
        (
            "00000000000000000000000000000000",
            "dddddddddddddddddddddddddddddddd",
        ),
        (
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        ),
        (
            "dddddddddddddddddddddddddddddddd",
            "00000000000000000000000000000000",
        ),
    ] {
        let invalid = topup_order_active_model(user_id, order_key, idempotency_key);
        assert!(invalid.insert(connection).await.is_err());
    }

    let duplicate_order_key = topup_order_active_model(
        user_id,
        TOPUP_ORDER_KEY_A,
        "ffffffffffffffffffffffffffffffff",
    );
    assert!(duplicate_order_key.insert(connection).await.is_err());
    let duplicate_idempotency = topup_order_active_model(
        user_id,
        "11111111111111111111111111111111",
        TOPUP_IDEMPOTENCY_KEY_A,
    );
    assert!(duplicate_idempotency.insert(connection).await.is_err());

    let mut invalid_currency = topup_order_active_model(
        user_id,
        "12121212121212121212121212121212",
        "13131313131313131313131313131313",
    );
    invalid_currency.currency = Set("usd".to_owned());
    assert!(invalid_currency.insert(connection).await.is_err());

    let mut invalid_amount = topup_order_active_model(
        user_id,
        "14141414141414141414141414141414",
        "15151515151515151515151515151515",
    );
    invalid_amount.amount_minor = Set(0);
    assert!(invalid_amount.insert(connection).await.is_err());

    let mut paid_without_time = topup_order_active_model(
        user_id,
        "16161616161616161616161616161616",
        "17171717171717171717171717171717",
    );
    paid_without_time.status = Set(3);
    assert!(paid_without_time.insert(connection).await.is_err());

    let mut paid_order = topup_order_active_model(
        user_id,
        "18181818181818181818181818181818",
        "19191919191919191919191919191919",
    );
    paid_order.status = Set(3);
    paid_order.paid_at = Set(Some(fixed_finalization_time()));
    let paid_order = paid_order.insert(connection).await?;
    assert_eq!(paid_order.closed_at, None);

    let mut closed_without_time = topup_order_active_model(
        user_id,
        "20202020202020202020202020202020",
        "21212121212121212121212121212121",
    );
    closed_without_time.status = Set(4);
    assert!(closed_without_time.insert(connection).await.is_err());

    let mut closed_order = topup_order_active_model(
        user_id,
        "22222222222222222222222222222222",
        "23232323232323232323232323232323",
    );
    closed_order.status = Set(4);
    closed_order.closed_at = Set(Some(fixed_finalization_time()));
    let closed_order = closed_order.insert(connection).await?;
    assert_eq!(closed_order.paid_at, None);

    let mut trade_order = topup_order_active_model(
        user_id,
        "24242424242424242424242424242424",
        "25252525252525252525252525252525",
    );
    trade_order.trade_no = Set(Some(SensitiveString::from("trade-duplicate")));
    trade_order.insert(connection).await?;
    let mut duplicate_trade_order = topup_order_active_model(
        user_id,
        "26262626262626262626262626262626",
        "27272727272727272727272727272727",
    );
    duplicate_trade_order.trade_no = Set(Some(SensitiveString::from("trade-duplicate")));
    assert!(duplicate_trade_order.insert(connection).await.is_err());

    let duplicate_event_key =
        topup_payment_event_active_model(order_id, TOPUP_EVENT_KEY_A, "evt_duplicate_event_key");
    assert!(duplicate_event_key.insert(connection).await.is_err());
    let duplicate_provider_event = topup_payment_event_active_model(
        order_id,
        "28282828282828282828282828282828",
        TOPUP_PROVIDER_EVENT_ID_A,
    );
    assert!(duplicate_provider_event.insert(connection).await.is_err());

    let mut invalid_fingerprint = topup_payment_event_active_model(
        order_id,
        "29292929292929292929292929292929",
        "evt_invalid_fingerprint",
    );
    invalid_fingerprint.signature_key_fingerprint = Set(SensitiveString::from("A".repeat(64)));
    assert!(invalid_fingerprint.insert(connection).await.is_err());

    let missing_order = topup_payment_event_active_model(
        i64::MAX,
        "30303030303030303030303030303030",
        "evt_missing_order",
    );
    assert!(missing_order.insert(connection).await.is_err());

    let event = topup_payment_event_active_model(
        order_id,
        "31313131313131313131313131313131",
        "evt_append_only",
    )
    .insert(connection)
    .await?;
    let mut event_update = event.into_active_model();
    event_update.processed_at = Set(Some(fixed_finalization_time()));
    assert!(event_update.update(connection).await.is_err());

    Ok(())
}

async fn assert_billing_reservation_constraints(
    connection: &DatabaseConnection,
    user_id: i64,
    token_id: i64,
    group_id: i64,
) -> Result<(), DbErr> {
    for invalid_key in [
        "00000000000000000000000000000000",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "gggggggggggggggggggggggggggggggg",
        "1111111111111111111111111111111",
        "111111111111111111111111111111111",
    ] {
        // 绕过实体值对象，确认数据库本身仍会拒绝非规范幂等键。
        let statement = Query::insert()
            .into_table(billing_reservations::Entity)
            .columns([
                billing_reservations::Column::IdempotencyKey,
                billing_reservations::Column::UserId,
                billing_reservations::Column::TokenId,
                billing_reservations::Column::GroupId,
                billing_reservations::Column::ReservedQuota,
                billing_reservations::Column::ExpiresAt,
            ])
            .values_panic([
                invalid_key.into(),
                user_id.into(),
                token_id.into(),
                group_id.into(),
                10_i64.into(),
                future_expiration().into(),
            ])
            .to_owned();
        assert!(
            connection
                .execute(connection.get_database_backend().build(&statement))
                .await
                .is_err()
        );
    }

    assert!(
        reservation_active_model(RESERVATION_KEY_A, user_id, token_id, group_id, 100, 100,)
            .insert(connection)
            .await
            .is_err()
    );

    let pending = reservation_active_model(RESERVATION_KEY_B, user_id, token_id, group_id, 100, 0)
        .insert(connection)
        .await?;
    let mut pending_update = pending.into_active_model();
    pending_update.status = Set(2);
    pending_update.actual_quota = Set(Some(80));
    let pending = pending_update.update(connection).await?;
    let mut settled_update = pending.into_active_model();
    settled_update.status = Set(3);
    settled_update.finalized_at = Set(Some(fixed_finalization_time()));
    settled_update.update(connection).await?;

    let mut invalid_reserved =
        reservation_active_model(RESERVATION_KEY_C, user_id, token_id, group_id, 1, 0);
    invalid_reserved.reserved_quota = Set(0);
    assert!(invalid_reserved.insert(connection).await.is_err());

    let invalid_token_reserved =
        reservation_active_model(RESERVATION_KEY_D, user_id, token_id, group_id, 100, 50);
    assert!(invalid_token_reserved.insert(connection).await.is_err());

    let mut missing_actual = reservation_active_model(
        "55555555555555555555555555555555",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    missing_actual.status = Set(2);
    assert!(missing_actual.insert(connection).await.is_err());

    let mut invalid_refund = reservation_active_model(
        "66666666666666666666666666666666",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    invalid_refund.status = Set(4);
    invalid_refund.actual_quota = Set(Some(0));
    invalid_refund.finalized_at = Set(Some(fixed_finalization_time()));
    assert!(invalid_refund.insert(connection).await.is_err());

    let missing_user = reservation_active_model(
        "77777777777777777777777777777777",
        i64::MAX,
        token_id,
        group_id,
        100,
        100,
    );
    assert!(missing_user.insert(connection).await.is_err());

    let missing_token = reservation_active_model(
        "88888888888888888888888888888888",
        user_id,
        i64::MAX,
        group_id,
        100,
        100,
    );
    assert!(missing_token.insert(connection).await.is_err());

    let missing_group = reservation_active_model(
        "99999999999999999999999999999999",
        user_id,
        token_id,
        i64::MAX,
        100,
        100,
    );
    assert!(missing_group.insert(connection).await.is_err());

    let mut negative_actual = reservation_active_model(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    negative_actual.status = Set(2);
    negative_actual.actual_quota = Set(Some(-1));
    assert!(negative_actual.insert(connection).await.is_err());

    let mut negative_token_reserved = reservation_active_model(
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    negative_token_reserved.token_reserved_quota = Set(-1);
    assert!(negative_token_reserved.insert(connection).await.is_err());

    let mut invalid_status = reservation_active_model(
        "cccccccccccccccccccccccccccccccc",
        user_id,
        token_id,
        group_id,
        100,
        100,
    );
    invalid_status.status = Set(5);
    assert!(invalid_status.insert(connection).await.is_err());

    // 计费记录必须阻止主体被硬删除，否则迟到重放将失去退款和审计依据。
    assert!(
        tokens::Entity::delete_by_id(token_id)
            .exec(connection)
            .await
            .is_err()
    );
    assert!(
        users::Entity::delete_by_id(user_id)
            .exec(connection)
            .await
            .is_err()
    );
    assert!(
        groups::Entity::delete_by_id(group_id)
            .exec(connection)
            .await
            .is_err()
    );
    Ok(())
}

async fn assert_invalid_sensitive_rows_fail_closed(
    connection: &DatabaseConnection,
    group_id: i64,
) -> Result<(), DbErr> {
    let raw_user = user_active_model(
        group_id,
        "raw-sensitive-user",
        "raw-sensitive@example.com",
        "invite-raw-sensitive",
    )
    .insert(connection)
    .await?;
    let raw_token = token_active_model(raw_user.id, group_id, TOKEN_HASH_B, "raw-sensitive-token")
        .insert(connection)
        .await?;
    let invalid_password_hash =
        "$argon2id$v=19$m=19456,t=2,p=1,x=raw-password-secret$c2FsdHNhbHQ$MDEyMzQ1Njc4OWFiY2RlZg";
    execute_raw_update(
        connection,
        Query::update()
            .table(users::Entity)
            .value(users::Column::PasswordHash, invalid_password_hash)
            .and_where(Expr::col(users::Column::Id).eq(raw_user.id))
            .to_owned(),
    )
    .await?;
    let error = users::Entity::find_by_id(raw_user.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-password-secret");

    let invalid_hash = "z".repeat(64);
    execute_raw_update(
        connection,
        Query::update()
            .table(tokens::Entity)
            .value(tokens::Column::KeyHash, invalid_hash.clone())
            .and_where(Expr::col(tokens::Column::Id).eq(raw_token.id))
            .to_owned(),
    )
    .await?;
    let error = tokens::Entity::find_by_id(raw_token.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, &invalid_hash);

    let raw_url_channel = channel_active_model("raw-url-channel")
        .insert(connection)
        .await?;
    let invalid_url = "https://user:raw-url-secret@example.com";
    execute_raw_update(
        connection,
        Query::update()
            .table(channels::Entity)
            .value(channels::Column::BaseUrl, invalid_url)
            .and_where(Expr::col(channels::Column::Id).eq(raw_url_channel.id))
            .to_owned(),
    )
    .await?;
    let error = channels::Entity::find_by_id(raw_url_channel.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-url-secret");

    let raw_header_channel = channel_active_model("raw-header-channel")
        .insert(connection)
        .await?;
    let invalid_headers = Json::Object(
        [("x-raw-header-secret".to_owned(), Json::from(1))]
            .into_iter()
            .collect(),
    );
    execute_raw_update(
        connection,
        Query::update()
            .table(channels::Entity)
            .value(channels::Column::HeaderOverride, invalid_headers)
            .and_where(Expr::col(channels::Column::Id).eq(raw_header_channel.id))
            .to_owned(),
    )
    .await?;
    let error = channels::Entity::find_by_id(raw_header_channel.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-header-secret");

    let raw_secret_channel = channel_active_model("raw-secret-channel")
        .insert(connection)
        .await?;
    let raw_credential = credential_active_model(raw_secret_channel.id, "valid-envelope")
        .insert(connection)
        .await?;
    let invalid_envelope = Json::Object(
        [(
            "ciphertext".to_owned(),
            Json::String("raw-envelope-secret".to_owned()),
        )]
        .into_iter()
        .collect(),
    );
    execute_raw_update(
        connection,
        Query::update()
            .table(credentials::Entity)
            .value(credentials::Column::Secret, invalid_envelope)
            .and_where(Expr::col(credentials::Column::Id).eq(raw_credential.id))
            .to_owned(),
    )
    .await?;
    let error = credentials::Entity::find_by_id(raw_credential.id)
        .one(connection)
        .await
        .unwrap_err();
    assert_redacted_type_error(&error, "raw-envelope-secret");

    Ok(())
}

async fn assert_foreign_key_actions(
    connection: &DatabaseConnection,
    default_group_id: i64,
) -> Result<(), DbErr> {
    let token_group = group_active_model("token-restrict", "令牌限制分组")
        .insert(connection)
        .await?;
    let token_user = user_active_model(
        default_group_id,
        "token-restrict-user",
        "token-restrict@example.com",
        "invite-token-restrict",
    )
    .insert(connection)
    .await?;
    let restricted_token = token_active_model(
        token_user.id,
        token_group.id,
        TOKEN_HASH_C,
        "restricted-token",
    )
    .insert(connection)
    .await?;
    assert!(
        groups::Entity::delete_by_id(token_group.id)
            .exec(connection)
            .await
            .is_err()
    );
    tokens::Entity::delete_by_id(restricted_token.id)
        .exec(connection)
        .await?;
    groups::Entity::delete_by_id(token_group.id)
        .exec(connection)
        .await?;
    users::Entity::delete_by_id(token_user.id)
        .exec(connection)
        .await?;

    let user_group = group_active_model("user-restrict", "用户限制分组")
        .insert(connection)
        .await?;
    let restricted_user = user_active_model(
        user_group.id,
        "group-restrict-user",
        "group-restrict@example.com",
        "invite-group-restrict",
    )
    .insert(connection)
    .await?;
    assert!(
        groups::Entity::delete_by_id(user_group.id)
            .exec(connection)
            .await
            .is_err()
    );
    users::Entity::delete_by_id(restricted_user.id)
        .exec(connection)
        .await?;
    groups::Entity::delete_by_id(user_group.id)
        .exec(connection)
        .await?;

    let fallback_target = group_active_model("fallback-target", "降级目标")
        .insert(connection)
        .await?;
    let mut fallback_source = group_active_model("fallback-source", "降级来源");
    fallback_source.fallback_group_id = Set(Some(fallback_target.id));
    let fallback_source = fallback_source.insert(connection).await?;
    groups::Entity::delete_by_id(fallback_target.id)
        .exec(connection)
        .await?;
    assert_eq!(
        groups::Entity::find_by_id(fallback_source.id)
            .one(connection)
            .await?
            .expect("降级来源分组必须存在")
            .fallback_group_id,
        None
    );
    groups::Entity::delete_by_id(fallback_source.id)
        .exec(connection)
        .await?;

    let inviter = user_active_model(
        default_group_id,
        "set-null-inviter",
        "set-null-inviter@example.com",
        "invite-set-null-inviter",
    )
    .insert(connection)
    .await?;
    let mut invitee = user_active_model(
        default_group_id,
        "set-null-invitee",
        "set-null-invitee@example.com",
        "invite-set-null-invitee",
    );
    invitee.inviter_id = Set(Some(inviter.id));
    let invitee = invitee.insert(connection).await?;
    users::Entity::delete_by_id(inviter.id)
        .exec(connection)
        .await?;
    assert_eq!(
        users::Entity::find_by_id(invitee.id)
            .one(connection)
            .await?
            .expect("被邀请用户必须存在")
            .inviter_id,
        None
    );
    users::Entity::delete_by_id(invitee.id)
        .exec(connection)
        .await?;

    let cascade_user = user_active_model(
        default_group_id,
        "cascade-token-user",
        "cascade-token@example.com",
        "invite-cascade-token",
    )
    .insert(connection)
    .await?;
    let cascade_token = token_active_model(
        cascade_user.id,
        default_group_id,
        TOKEN_HASH_D,
        "cascade-token",
    )
    .insert(connection)
    .await?;
    users::Entity::delete_by_id(cascade_user.id)
        .exec(connection)
        .await?;
    assert!(
        tokens::Entity::find_by_id(cascade_token.id)
            .one(connection)
            .await?
            .is_none()
    );

    Ok(())
}

async fn execute_raw_update(
    connection: &DatabaseConnection,
    statement: UpdateStatement,
) -> Result<(), DbErr> {
    connection
        .execute(connection.get_database_backend().build(&statement))
        .await?;
    Ok(())
}

fn assert_redacted_type_error(error: &DbErr, secret: &str) {
    assert!(matches!(error, DbErr::TryIntoErr { .. }));
    assert!(!error.to_string().contains(secret));
    assert!(!format!("{error:?}").contains(secret));
}

fn group_active_model(name: &str, display_name: &str) -> groups::ActiveModel {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(display_name.to_owned()),
        flags: Set(empty_json_object()),
        ..Default::default()
    }
}

/// 在历史 schema 上只取自增主键，避免 RETURNING 投影尚未迁移的新分组列。
async fn insert_legacy_group(
    connection: &DatabaseConnection,
    name: &str,
    display_name: &str,
) -> Result<i64, DbErr> {
    Ok(
        groups::Entity::insert(group_active_model(name, display_name))
            .exec(connection)
            .await?
            .last_insert_id,
    )
}

fn user_active_model(
    group_id: i64,
    username: &str,
    email: &str,
    aff_code: &str,
) -> users::ActiveModel {
    users::ActiveModel {
        username: Set(username.to_owned()),
        email: Set(Some(email.to_owned())),
        password_hash: Set(Some(PasswordHash::parse(TEST_PASSWORD_HASH).unwrap())),
        default_group_id: Set(group_id),
        aff_code: Set(aff_code.to_owned()),
        settings: Set(empty_json_object()),
        ..Default::default()
    }
}

fn channel_active_model(name: &str) -> channels::ActiveModel {
    channels::ActiveModel {
        name: Set(name.to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        base_url: Set(Some(
            ChannelBaseUrl::parse("https://api.example.com").unwrap(),
        )),
        model_mapping: Set(empty_json_object()),
        param_override: Set(empty_json_object()),
        header_override: Set(HeaderOverrides::validate(empty_json_object()).unwrap()),
        settings: Set(SensitiveJson::from(empty_json_object())),
        ..Default::default()
    }
}

fn credential_active_model(channel_id: i64, marker: &str) -> credentials::ActiveModel {
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set("api_key".to_owned()),
        secret: Set(EncryptedJson::from_envelope(encrypted_envelope(marker)).unwrap()),
        ..Default::default()
    }
}

fn oauth_credential_active_model(channel_id: i64, marker: &str) -> credentials::ActiveModel {
    let mut credential = credential_active_model(channel_id, marker);
    credential.kind = Set(CredentialKind::Oauth.as_str().to_owned());
    credential
}

fn spark_shadow_active_model(
    channel_id: i64,
    parent_id: i64,
    marker: &str,
) -> credentials::ActiveModel {
    let mut shadow = oauth_credential_active_model(channel_id, marker);
    shadow.parent_id = Set(Some(parent_id));
    shadow.quota_dimension = Set("spark".to_owned());
    shadow.concurrency = Set(None);
    shadow.proxy_id = Set(None);
    shadow.oauth_provider = Set(None);
    shadow.oauth_account_key = Set(None);
    shadow.oauth_project_id = Set(None);
    shadow.oauth_token_pending = Set(false);
    shadow
}

fn token_active_model(
    user_id: i64,
    group_id: i64,
    key_hash: &str,
    name: &str,
) -> tokens::ActiveModel {
    tokens::ActiveModel {
        user_id: Set(user_id),
        key_hash: Set(TokenHash::parse(key_hash).unwrap()),
        key_prefix: Set("sk-af-test".to_owned()),
        name: Set(name.to_owned()),
        group_id: Set(Some(group_id)),
        expired_at: Set(Some(future_expiration())),
        ..Default::default()
    }
}

/// 在历史 schema 上写入令牌时跳过当前实体的返回投影，避免提前读取尚未迁移的新列。
async fn insert_legacy_token(
    connection: &DatabaseConnection,
    token_id: i64,
    mut token: tokens::ActiveModel,
) -> Result<i64, DbErr> {
    token.id = Set(token_id);
    tokens::Entity::insert(token)
        .exec_without_returning(connection)
        .await?;
    Ok(token_id)
}

fn reservation_active_model(
    idempotency_key: &str,
    user_id: i64,
    token_id: i64,
    group_id: i64,
    reserved_quota: i64,
    token_reserved_quota: i64,
) -> billing_reservations::ActiveModel {
    billing_reservations::ActiveModel {
        idempotency_key: Set(BillingReservationKey::parse(idempotency_key).unwrap()),
        user_id: Set(user_id),
        token_id: Set(token_id),
        group_id: Set(group_id),
        reserved_quota: Set(reserved_quota),
        token_reserved_quota: Set(token_reserved_quota),
        expires_at: Set(future_expiration()),
        ..Default::default()
    }
}

fn topup_order_active_model(
    user_id: i64,
    order_key: &str,
    idempotency_key: &str,
) -> topup_orders::ActiveModel {
    topup_orders::ActiveModel {
        order_key: Set(order_key.to_owned()),
        user_id: Set(user_id),
        provider: Set("stripe".to_owned()),
        payment_method: Set(Some("card".to_owned())),
        provider_order_id: Set(None),
        trade_no: Set(None),
        status: Set(1),
        amount_minor: Set(1_000),
        currency: Set("USD".to_owned()),
        quota_amount: Set(10_000),
        idempotency_key: Set(idempotency_key.to_owned()),
        expires_at: Set(Some(future_expiration())),
        paid_at: Set(None),
        closed_at: Set(None),
        created_at: Set(historical_audit_time()),
        updated_at: Set(historical_audit_time()),
        ..Default::default()
    }
}

fn topup_payment_event_active_model(
    order_id: i64,
    event_key: &str,
    provider_event_id: &str,
) -> topup_payment_events::ActiveModel {
    topup_payment_events::ActiveModel {
        event_key: Set(event_key.to_owned()),
        order_id: Set(order_id),
        provider: Set("stripe".to_owned()),
        provider_event_id: Set(SensitiveString::from(provider_event_id)),
        trade_no: Set(Some(SensitiveString::from("trade-event-a"))),
        amount_minor: Set(Some(1_000)),
        currency: Set(Some("USD".to_owned())),
        payment_method: Set(Some("card".to_owned())),
        event_type: Set(1),
        signature_key_fingerprint: Set(SensitiveString::from("e".repeat(64))),
        payload_sha256: Set(SensitiveString::from("f".repeat(64))),
        received_at: Set(historical_audit_time()),
        processed_at: Set(None),
        created_at: Set(historical_audit_time()),
        ..Default::default()
    }
}

fn option_active_model(key: &str, value: &str) -> options::ActiveModel {
    options::ActiveModel {
        key: Set(key.to_owned()),
        value: Set(SensitiveString::from(value)),
        ..Default::default()
    }
}

fn empty_json_object() -> Json {
    Json::Object(Default::default())
}

fn encrypted_envelope(marker: &str) -> Json {
    Json::Object(
        [
            ("version".to_owned(), Json::from(1)),
            (
                "algorithm".to_owned(),
                Json::String("xchacha20poly1305".to_owned()),
            ),
            ("key_id".to_owned(), Json::String(format!("test-{marker}"))),
            (
                "nonce".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0x42; 24])),
            ),
            (
                "ciphertext".to_owned(),
                Json::String(URL_SAFE_NO_PAD.encode([0xa5; 32])),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn future_expiration() -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(2_208_988_800)
        .expect("固定的 2040-01-01 时间戳必须有效")
}

fn historical_audit_time() -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(946_684_800)
        .expect("固定的 2000-01-01 时间戳必须有效")
}

fn fixed_finalization_time() -> TimeDateTimeWithTimeZone {
    TimeDateTimeWithTimeZone::from_unix_timestamp(1_893_456_000)
        .expect("固定的 2030-01-01 时间戳必须有效")
}

fn fixed_peak_time() -> TimeTime {
    TimeTime::from_hms(9, 0, 0).expect("固定的 09:00:00 时间必须有效")
}

fn fixed_peak_end_time() -> TimeTime {
    TimeTime::from_hms(17, 0, 0).expect("固定的 17:00:00 时间必须有效")
}

fn assert_debug_redacted(value: &impl std::fmt::Debug, secret: &str) {
    let rendered = format!("{value:?}");
    assert!(rendered.contains("<redacted>"));
    assert!(!rendered.contains(secret));
}

fn test_database_url() -> String {
    match env::var(TEST_DATABASE_URL) {
        Ok(url) if !url.trim().is_empty() => url,
        _ if env::var(REQUIRE_LIVE_DATABASE).as_deref() == Ok("1") => {
            panic!("强制数据库冒烟测试缺少 {TEST_DATABASE_URL}")
        }
        _ => "sqlite::memory:".to_owned(),
    }
}
