//! 令牌额度窗口在请求计费预留中的不可变起点快照。

use sea_orm::entity::prelude::*;

use super::BillingReservationKey;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "billing_token_window_reservations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub idempotency_key: BillingReservationKey,
    pub window_5h_start: TimeDateTimeWithTimeZone,
    pub window_1d_start: TimeDateTimeWithTimeZone,
    pub window_7d_start: TimeDateTimeWithTimeZone,
    pub reserved_quota: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::billing_reservations::Entity",
        from = "Column::IdempotencyKey",
        to = "super::billing_reservations::Column::IdempotencyKey",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    BillingReservation,
}

impl Related<super::billing_reservations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BillingReservation.def()
    }
}
