//! 企业认证案件事实实体；审核理由和摘要在调试输出中保持脱敏。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_verification_cases")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub applicant_user_id: i64,
    pub status: i16,
    pub version: i64,
    #[sea_orm(column_type = "String(StringLen::N(512))")]
    pub summary: SensitiveString,
    pub reviewer_user_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(512))", nullable)]
    pub review_reason: Option<SensitiveString>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub decided_at: Option<TimeDateTimeWithTimeZone>,
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
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Applicant.def()
    }
}
