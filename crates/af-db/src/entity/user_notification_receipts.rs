//! 用户通知已读回执；每个用户对每条通知最多保留一条幂等回执。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_notification_receipts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub notification_id: i64,
    pub read_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::user_notification_events::Entity",
        from = "Column::NotificationId",
        to = "super::user_notification_events::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Notification,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::user_notification_events::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Notification.def()
    }
}
