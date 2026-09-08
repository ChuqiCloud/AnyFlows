//! 企业成员与部门多归属事实实体。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_department_memberships")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub membership_id: i64,
    pub department_id: i64,
    pub is_primary: i16,
    pub status: i16,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub removed_at: Option<TimeDateTimeWithTimeZone>,
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
        belongs_to = "super::organization_memberships::Entity",
        from = "Column::MembershipId",
        to = "super::organization_memberships::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Membership,
    #[sea_orm(
        belongs_to = "super::organization_departments::Entity",
        from = "Column::DepartmentId",
        to = "super::organization_departments::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Department,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_memberships::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Membership.def()
    }
}

impl Related<super::organization_departments::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Department.def()
    }
}
