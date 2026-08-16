//! 企业空间主体实体；与平台分组和个人钱包保持独立。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organizations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub organization_key: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub name: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub slug: String,
    pub status: i16,
    pub created_by_user_id: i64,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub closed_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedByUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    CreatedBy,
    #[sea_orm(has_many = "super::organization_memberships::Entity")]
    Memberships,
    #[sea_orm(has_many = "super::organization_teams::Entity")]
    Teams,
    #[sea_orm(has_one = "super::organization_entitlements::Entity")]
    Entitlement,
    #[sea_orm(has_many = "super::organization_invitations::Entity")]
    Invitations,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CreatedBy.def()
    }
}

impl Related<super::organization_memberships::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Memberships.def()
    }
}

impl Related<super::organization_teams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Teams.def()
    }
}

impl Related<super::organization_entitlements::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Entitlement.def()
    }
}

impl Related<super::organization_invitations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Invitations.def()
    }
}
