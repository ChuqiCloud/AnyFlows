//! 企业 SCIM 组资源与团队绑定实体。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_scim_group_resources")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub resource_id: String,
    pub team_id: i64,
    pub resource_version: i64,
    pub status: i16,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
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
        belongs_to = "super::organization_teams::Entity",
        from = "Column::TeamId",
        to = "super::organization_teams::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Team,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_teams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}
