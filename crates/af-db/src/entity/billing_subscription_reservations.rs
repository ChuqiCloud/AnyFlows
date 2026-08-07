//! 订阅资金来源在请求计费预留中的不可变窗口快照。

use sea_orm::entity::prelude::*;

use super::BillingReservationKey;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "billing_subscription_reservations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub idempotency_key: BillingReservationKey,
    pub user_subscription_id: i64,
    pub window_started_at: TimeDateTimeWithTimeZone,
    pub window_ends_at: TimeDateTimeWithTimeZone,
    pub reserved_quota: i64,
    pub subscription_actual_quota: Option<i64>,
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
    #[sea_orm(
        belongs_to = "super::user_subscriptions::Entity",
        from = "Column::UserSubscriptionId",
        to = "super::user_subscriptions::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    UserSubscription,
}

impl Related<super::billing_reservations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BillingReservation.def()
    }
}

impl Related<super::user_subscriptions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserSubscription.def()
    }
}
