//! 企业 SSO 域名实体；证明令牌只保存域分离摘要。
use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_sso_domains")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub provider_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(253))")]
    pub domain: String,
    pub status: i16,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub proof_digest: SensitiveString,
    pub proof_expires_at: Option<TimeDateTimeWithTimeZone>,
    pub verified_at: Option<TimeDateTimeWithTimeZone>,
    pub recheck_at: Option<TimeDateTimeWithTimeZone>,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
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
