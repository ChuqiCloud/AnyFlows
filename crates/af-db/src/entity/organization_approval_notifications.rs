//! 企业审批决定站内通知事实；不承载邮件投递或已读状态。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_approval_notifications")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub notification_key: String,
    pub organization_id: i64,
    pub request_id: i64,
    pub decision_id: i64,
    pub recipient_user_id: i64,
    pub decision_kind: i16,
    #[sea_orm(column_type = "Char(Some(16))")]
    pub template_version: String,
    pub occurred_at: TimeDateTimeWithTimeZone,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrganizationId",
        to = "super::organizations::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Organization,
    #[sea_orm(
        belongs_to = "super::organization_approval_requests::Entity",
        from = "Column::RequestId",
        to = "super::organization_approval_requests::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Request,
    #[sea_orm(
        belongs_to = "super::organization_approval_decisions::Entity",
        from = "Column::DecisionId",
        to = "super::organization_approval_decisions::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Decision,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::RecipientUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Recipient,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_approval_requests::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Request.def()
    }
}

impl Related<super::organization_approval_decisions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Decision.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Recipient.def()
    }
}
