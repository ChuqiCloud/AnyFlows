use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

mod auth_challenge;
mod auth_challenge_rate_limit;
mod balance_alert;
mod billing;
mod billing_batch;
mod email;
mod identity;
mod indexes;
mod invite_rebate;
mod model;
mod model_sync;
mod network;
mod payment;
mod playground;
mod playground_conversation;
mod pricing;
mod redemption;
mod registration;
mod routes;
mod routing;
mod scheduler;
mod site;
mod subscription;
mod usage;
mod wallet;

pub(super) use auth_challenge::{create_auth_challenge_indexes, create_auth_challenges};
pub(super) use auth_challenge_rate_limit::create_auth_challenge_rate_limits;
pub(super) use balance_alert::create_balance_alert_storage;
pub(super) use email::create_email_settings;
pub(super) use identity::{create_groups, create_options, create_tokens, create_users};
pub(super) use indexes::{
    create_abilities_indexes, create_billing_reservations_indexes, create_channel_groups_indexes,
    create_channel_models_indexes, create_channels_indexes, create_credentials_indexes,
    create_groups_indexes, create_tokens_indexes, create_users_indexes,
};
pub(super) use invite_rebate::create_invite_rebate_events;
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
pub(super) use billing::create_billing_reservations;
pub(super) use billing_batch::create_billing_batch_checkpoints;
pub(super) use model::create_models;
pub(super) use model_sync::create_model_sync_audit;
pub(super) use network::create_network_settings;
pub(super) use payment::create_topup_payment_audit;
pub(super) use playground::{create_playground_shares, create_playground_shares_indexes};
pub(super) use playground_conversation::{
    create_playground_conversation_indexes, create_playground_conversations,
};
pub(super) use pricing::{create_group_model_ratios, create_model_prices};
pub(super) use redemption::create_redemption_storage;
pub(super) use registration::create_registration_rate_limits;
pub(super) use site::{create_authentication_settings, create_site_settings};
pub(super) use subscription::create_subscription_storage;
pub(super) use usage::{create_usage_logs, create_usage_logs_indexes};
pub(super) use wallet::create_wallet_ledger;
pub(super) use wallet::rebuild_wallet_ledger_for_invite_rebate;
pub(super) use wallet::rebuild_wallet_ledger_for_redemption;
pub(super) use wallet::rebuild_wallet_ledger_for_topup;
