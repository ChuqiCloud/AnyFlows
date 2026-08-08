//! 订阅窗口剩余额度预警的持久化投递事件实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription_balance_alert_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_subscription_id: i64,
    pub user_id: i64,
    pub window_started_at: TimeDateTimeWithTimeZone,
    pub window_ends_at: TimeDateTimeWithTimeZone,
    pub threshold_percent: i16,
    pub quota_amount: i64,
    pub observed_quota_used: i64,
    pub status: i16,
    pub attempt_count: i16,
    pub next_attempt_at: TimeDateTimeWithTimeZone,
    pub lease_expires_at: Option<TimeDateTimeWithTimeZone>,
    pub last_error_kind: Option<i16>,
    pub version: i64,
    pub sent_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::user_subscriptions::Entity",
        from = "Column::UserSubscriptionId",
        to = "super::user_subscriptions::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    UserSubscription,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
}

impl Related<super::user_subscriptions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UserSubscription.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}
