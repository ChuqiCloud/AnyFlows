//! 企业审批通知的用户已读回执；回执与审批通知事实分离保存。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_approval_notification_receipts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub recipient_user_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub notification_id: i64,
    pub read_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::RecipientUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Recipient,
    #[sea_orm(
        belongs_to = "super::organization_approval_notifications::Entity",
        from = "Column::NotificationId",
        to = "super::organization_approval_notifications::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Notification,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Recipient.def()
    }
}

impl Related<super::organization_approval_notifications::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Notification.def()
    }
}
