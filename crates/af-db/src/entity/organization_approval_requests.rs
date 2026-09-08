//! 企业审批申请事实实体；资源摘要为受限文本快照。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_approval_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub request_key: String,
    pub organization_id: i64,
    pub resource_type: i16,
    #[sea_orm(column_type = "String(StringLen::N(1024))")]
    pub resource_summary: SensitiveString,
    pub template_id: i64,
    pub template_version: i64,
    pub requested_by_user_id: i64,
    pub expires_at: TimeDateTimeWithTimeZone,
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
        belongs_to = "super::organization_approval_templates::Entity",
        from = "Column::TemplateId",
        to = "super::organization_approval_templates::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Template,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::RequestedByUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    RequestedBy,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_approval_templates::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Template.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RequestedBy.def()
    }
}
