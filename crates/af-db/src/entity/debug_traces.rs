//! 请求级调试追踪元数据；旧诊断文本列仅保留滚动升级结构兼容。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "debug_traces")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub request_id: String,
    pub user_id: i64,
    pub token_id: i64,
    pub group_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub requested_model: String,
    pub downstream_protocol: i16,
    pub upstream_protocol: i16,
    pub operation: i16,
    pub outcome: i16,
    pub selected_channel_id: Option<i64>,
    pub selected_credential_id: Option<i64>,
    pub routing_elapsed_ms: i64,
    pub attempt_count: i32,
    #[sea_orm(column_type = "Text", nullable)]
    pub downstream_method: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub downstream_path: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub downstream_headers_json: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub downstream_body_json: Option<String>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::debug_trace_attempts::Entity")]
    Attempts,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::tokens::Entity",
        from = "Column::TokenId",
        to = "super::tokens::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Token,
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::GroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Group,
}

impl Related<super::debug_trace_attempts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Attempts.def()
    }
}
