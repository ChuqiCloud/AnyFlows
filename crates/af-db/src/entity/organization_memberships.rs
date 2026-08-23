//! 全局用户与企业空间之间的真实成员关系实体。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_memberships")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub user_id: i64,
    pub role: i16,
    pub team_id: Option<i64>,
    pub status: i16,
    pub joined_at: TimeDateTimeWithTimeZone,
    pub suspended_at: Option<TimeDateTimeWithTimeZone>,
    pub removed_at: Option<TimeDateTimeWithTimeZone>,
    pub invited_by_user_id: Option<i64>,
    pub version: i64,
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
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::InvitedByUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    InvitedBy,
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

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::organization_teams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}
