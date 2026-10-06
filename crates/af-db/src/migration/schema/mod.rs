use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

mod analytics_export;
mod async_task;
mod async_task_billing;
mod async_task_submission;
mod auth_challenge;
mod auth_challenge_rate_limit;
mod balance_alert;
mod billing;
mod billing_batch;
mod billing_group_window;
mod billing_subscription;
mod billing_token_window;
mod debug_trace;
mod email;
mod identity;
mod indexes;
mod invite_rebate;
mod model;
mod model_sync;
mod network;
mod payment;
mod platform_audit;
mod playground;
mod playground_conversation;
mod pricing;
mod proxy;
mod redemption;
mod registration;
mod request_outcome;
mod routes;
mod routing;
mod scheduler;
mod site;
mod subscription;
mod subscription_alert;
mod usage;
mod user_notification;
mod wallet;

pub(super) use async_task::create_async_tasks;
pub(super) use async_task_billing::create_async_task_billings;
pub(super) use async_task_submission::create_async_task_submission_claims;
pub(super) use auth_challenge::{create_auth_challenge_indexes, create_auth_challenges};
pub(super) use auth_challenge_rate_limit::{
    create_auth_challenge_rate_limits, rebuild_auth_challenge_rate_limits_for_passkey,
};
pub(super) use balance_alert::create_balance_alert_storage;
pub(super) use debug_trace::create_debug_trace_storage;
pub(super) use email::create_email_settings;
pub(super) use identity::{create_groups, create_options, create_tokens, create_users};
pub(super) use indexes::{
    create_abilities_indexes, create_billing_reservations_indexes, create_channel_groups_indexes,
    create_channel_models_indexes, create_channels_indexes, create_credentials_indexes,
    create_groups_indexes, create_tokens_indexes, create_users_indexes,
};
pub(super) use invite_rebate::create_invite_rebate_events;
pub(super) use platform_audit::{create_platform_audit_storage, drop_platform_audit_storage};
pub(super) use routes::{create_route_channels, create_routes};
pub(super) use routing::{
    create_abilities, create_channel_groups, create_channel_models, create_channels,
    create_credentials,
};
pub(super) use scheduler::create_scheduler_outbox;

pub(super) fn auto_id<T>(column: T) -> ColumnDef
where
    T: IntoIden,
{
    let mut definition = ColumnDef::new(column);
    definition
        .big_integer()
        .not_null()
        .auto_increment()
        .primary_key();
    definition
}

pub(super) fn table<T>(manager: &SchemaManager<'_>, table: T) -> TableCreateStatement
where
    T: IntoTableRef,
{
    let mut statement = Table::create();
    statement.table(table);
    if manager.get_database_backend() == DbBackend::MySql {
        // 固定引擎与二进制排序规则，避免环境默认值改变外键及唯一语义。
        statement
            .engine("InnoDB")
            .character_set("utf8mb4")
            .collate("utf8mb4_bin");
    }
    statement
}

pub(super) fn timestamp<T>(manager: &SchemaManager<'_>, column: T) -> ColumnDef
where
    T: IntoIden,
{
    let mut definition = nullable_timestamp(manager, column);
    let default = if manager.get_database_backend() == DbBackend::MySql {
        Expr::cust("CURRENT_TIMESTAMP(6)")
    } else {
        Expr::current_timestamp().into()
    };
    definition.not_null().default(default);
    definition
}

pub(super) fn nullable_timestamp<T>(manager: &SchemaManager<'_>, column: T) -> ColumnDef
where
    T: IntoIden,
{
    let mut definition = ColumnDef::new(column);
    if manager.get_database_backend() == DbBackend::MySql {
        // DATETIME(6) 同时避开 2038 上限并保留审计时间的子秒更新顺序。
        definition.custom(Alias::new("DATETIME(6)"));
    } else {
        definition.timestamp_with_time_zone();
    }
    definition
}
pub(super) use analytics_export::create_analytics_export_outbox;
pub(super) use billing::create_billing_reservations;
pub(super) use billing_batch::create_billing_batch_checkpoints;
pub(super) use billing_group_window::{
    add_group_window_state, backfill_active_billing_group_window_reservations,
    create_billing_group_window_reservations, create_billing_group_window_reservations_index,
};
pub(super) use billing_subscription::create_billing_subscription_reservations;
pub(super) use billing_token_window::{
    backfill_active_billing_token_window_reservations, create_billing_token_window_reservations,
    create_billing_token_window_reservations_index,
};
pub(super) use model::create_models;
pub(super) use model_sync::create_model_sync_audit;
pub(super) use network::create_network_settings;
pub(super) use payment::{create_payment_settings, extend_topup_payment_facts};
pub(super) use payment::{create_subscription_payment_events, create_topup_payment_audit};
pub(super) use playground::{create_playground_shares, create_playground_shares_indexes};
pub(super) use playground_conversation::{
    create_playground_conversation_indexes, create_playground_conversations,
};
pub(super) use pricing::{
    create_group_model_ratios, create_model_prices, rebuild_model_prices_for_expression,
};
pub(super) use proxy::{
    CREDENTIAL_PROXY_FOREIGN_KEY, create_credential_proxy_reference, create_proxies,
};
pub(super) use redemption::create_redemption_storage;
pub(super) use registration::create_registration_rate_limits;
pub(super) use request_outcome::{create_request_outcome_storage, drop_request_outcome_storage};
pub(super) use site::{create_authentication_settings, create_site_settings};
pub(super) use subscription::create_subscription_storage;
pub(super) use subscription_alert::create_subscription_balance_alert_storage;
pub(super) use usage::{
    create_usage_logs, create_usage_logs_indexes, rebuild_usage_logs_for_per_call,
};
pub(super) use user_notification::create_user_notification_storage;
pub(super) use wallet::create_wallet_ledger;
pub(super) use wallet::rebuild_wallet_ledger_for_invite_rebate;
pub(super) use wallet::rebuild_wallet_ledger_for_redemption;
pub(super) use wallet::rebuild_wallet_ledger_for_topup;
