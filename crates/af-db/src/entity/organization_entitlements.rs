//! 企业商业授权实体，仅允许受信授权流程读取和更新。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_entitlements")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub team_management_enabled: bool,
    pub seat_limit: i64,
    pub team_limit: i64,
    pub organization_key_limit: i64,
    pub audit_retention_days: i64,
    pub sso_enabled: bool,
    pub custom_roles_enabled: bool,
    pub valid_from: TimeDateTimeWithTimeZone,
    pub valid_until: TimeDateTimeWithTimeZone,
    pub grace_until: Option<TimeDateTimeWithTimeZone>,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub source: String,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub external_reference: Option<String>,
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
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}
