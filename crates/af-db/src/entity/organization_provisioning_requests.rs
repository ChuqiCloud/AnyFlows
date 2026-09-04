//! 企业开通申请事实实体；业务说明在调试输出中保持脱敏。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_provisioning_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub request_key: String,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub idempotency_key: String,
    pub applicant_user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub organization_name: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub organization_slug: String,
    #[sea_orm(column_type = "String(StringLen::N(2000))")]
    pub business_reason: SensitiveString,
    pub status: i16,
    pub active_slot: Option<i16>,
    pub version: i64,
    pub reviewer_user_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(512))", nullable)]
    pub decision_reason: Option<SensitiveString>,
    pub organization_id: Option<i64>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub decided_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ApplicantUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Applicant,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ReviewerUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Reviewer,
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrganizationId",
        to = "super::organizations::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Organization,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}
