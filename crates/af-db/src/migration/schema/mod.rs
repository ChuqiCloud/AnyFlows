use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

mod billing;
mod billing_batch;
mod email;
mod identity;
mod indexes;
mod playground;
mod playground_conversation;
mod pricing;
mod registration;
mod routing;
mod usage;

pub(super) use email::create_email_settings;
pub(super) use identity::{create_groups, create_options, create_tokens, create_users};
pub(super) use indexes::{
    create_abilities_indexes, create_billing_reservations_indexes, create_channel_groups_indexes,
    create_channel_models_indexes, create_channels_indexes, create_credentials_indexes,
    create_groups_indexes, create_tokens_indexes, create_users_indexes,
};
pub(super) use routing::{
    create_abilities, create_channel_groups, create_channel_models, create_channels,
    create_credentials,
};

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
pub(super) use playground::{create_playground_shares, create_playground_shares_indexes};
pub(super) use playground_conversation::{
    create_playground_conversation_indexes, create_playground_conversations,
};
pub(super) use pricing::{create_group_model_ratios, create_model_prices};
pub(super) use registration::create_registration_rate_limits;
pub(super) use usage::{create_usage_logs, create_usage_logs_indexes};
