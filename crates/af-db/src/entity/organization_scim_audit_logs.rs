//! SCIM 脱敏审计事实实体，只允许通过仓储追加写入。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_scim_audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub token_key: Option<String>,
    pub event_type: String,
    pub result: String,
    pub reason: Option<String>,
    pub resource_kind: Option<i16>,
    pub resource_digest: Option<String>,
    pub resource_version: Option<i64>,
    pub request_fingerprint: String,
    pub retry_after_seconds: Option<i32>,
    pub created_at: TimeDateTimeWithTimeZone,
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
