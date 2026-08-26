//! 企业 SCIM 令牌实体；仅保存摘要、资源范围和生命周期，不保存明文令牌。
use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_scim_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub token_key: String,
    pub token_prefix: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub token_digest: SensitiveString,
    pub name: String,
    pub resource_scope: i16,
    pub status: i16,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
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
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}
