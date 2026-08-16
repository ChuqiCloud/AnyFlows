//! 企业管理审计的只追加持久化实体；不保存请求正文、Header、IP 或任何凭据。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub actor_user_id: i64,
    pub actor_role: Option<i16>,
    #[sea_orm(column_type = "String(StringLen::N(96))")]
    pub permission_code: String,
    #[sea_orm(column_type = "String(StringLen::N(96))")]
    pub operation: String,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub target_type: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub target_identifier: String,
    pub outcome: i16,
    pub platform_proxy: bool,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub platform_reason: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub request_id: String,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub client_fingerprint: String,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub change_summary: Option<String>,
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
        belongs_to = "super::users::Entity",
        from = "Column::ActorUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Actor,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Actor.def()
    }
}
