//! 用户钱包余额变更的只追加审计实体。

use sea_orm::entity::prelude::*;

use super::{SensitiveString, WalletLedgerKey};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "wallet_ledger_entries")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub event_key: WalletLedgerKey,
    pub user_id: i64,
    pub actor_user_id: Option<i64>,
    pub entry_type: i16,
    pub quota_delta: i64,
    pub balance_before: i64,
    pub balance_after: i64,
    #[sea_orm(column_type = "String(StringLen::N(500))", nullable)]
    pub reason: Option<SensitiveString>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ActorUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Actor,
}
