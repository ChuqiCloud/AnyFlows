//! 平台管理审计的只追加持久化实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "platform_audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub operator_user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(96))")]
    pub permission_code: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub route: String,
    #[sea_orm(column_type = "String(StringLen::N(96))")]
    pub operation: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub resource: String,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub resource_id: Option<String>,
    pub outcome: i16,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub before_value: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub after_value: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub audit_info: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub request_id: String,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::OperatorUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Operator,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Operator.def()
    }
}
