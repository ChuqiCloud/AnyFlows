//! 企业 SSO 身份实体；subject 永不以明文进入数据库。
use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_sso_identities")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub provider_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub subject_digest: SensitiveString,
    pub organization_membership_id: i64,
    pub status: i16,
    pub binding_method: i16,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub disabled_at: Option<TimeDateTimeWithTimeZone>,
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
        belongs_to = "super::organization_sso_providers::Entity",
        from = "Column::ProviderId",
        to = "super::organization_sso_providers::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Provider,
    #[sea_orm(
        belongs_to = "super::organization_memberships::Entity",
        from = "Column::OrganizationMembershipId",
        to = "super::organization_memberships::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Membership,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_sso_providers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Provider.def()
    }
}

impl Related<super::organization_memberships::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Membership.def()
    }
}
