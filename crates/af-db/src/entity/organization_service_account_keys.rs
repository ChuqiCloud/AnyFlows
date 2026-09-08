//! 企业服务账号密钥实体；仅保存摘要和轮换事实，不保存明文密钥。
use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_service_account_keys")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub service_account_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub key_key: String,
    pub key_prefix: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub key_digest: SensitiveString,
    pub status: i16,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organization_service_accounts::Entity",
        from = "Column::ServiceAccountId",
        to = "super::organization_service_accounts::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    ServiceAccount,
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrganizationId",
        to = "super::organizations::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Organization,
}

impl Related<super::organization_service_accounts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServiceAccount.def()
    }
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}
