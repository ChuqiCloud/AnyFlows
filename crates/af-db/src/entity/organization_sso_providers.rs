//! 企业 SSO Provider 实体；密钥封套只作为受保护 JSON 保存。
use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_sso_providers")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub provider_key: String,
    pub protocol: i16,
    pub status: i16,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub display_name: String,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub metadata_projection: Option<Json>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub secret_envelope: Option<EncryptedJson>,
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
    #[sea_orm(has_many = "super::organization_sso_domains::Entity")]
    Domains,
    #[sea_orm(has_many = "super::organization_sso_identities::Entity")]
    Identities,
    #[sea_orm(has_many = "super::organization_sso_login_transactions::Entity")]
    LoginTransactions,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_sso_domains::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Domains.def()
    }
}

impl Related<super::organization_sso_identities::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Identities.def()
    }
}

impl Related<super::organization_sso_login_transactions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::LoginTransactions.def()
    }
}
