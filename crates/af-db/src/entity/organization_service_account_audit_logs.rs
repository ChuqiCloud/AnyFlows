//! 企业服务账号调用身份与已落库用量的关联事实。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_service_account_audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub event_id: String,
    pub organization_id: i64,
    pub service_account_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub service_account_key: String,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub credential_key: String,
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
    #[sea_orm(
        belongs_to = "super::usage_logs::Entity",
        from = "Column::EventId",
        to = "super::usage_logs::Column::EventId",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    UsageLog,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::usage_logs::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UsageLog.def()
    }
}
