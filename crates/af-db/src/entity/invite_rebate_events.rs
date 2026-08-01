//! 邀请返利到账事件的只追加审计实体。

use sea_orm::entity::prelude::*;

use super::WalletLedgerKey;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "invite_rebate_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub event_key: WalletLedgerKey,
    pub inviter_user_id: i64,
    pub invitee_user_id: i64,
    pub quota_amount: i64,
    pub balance_after: i64,
    pub wallet_ledger_entry_id: i64,
    pub credited_at: TimeDateTimeWithTimeZone,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::InviterUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Inviter,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::InviteeUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Invitee,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Inviter.def()
    }
}
