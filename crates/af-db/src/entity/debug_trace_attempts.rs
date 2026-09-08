//! 调试追踪中单个候选尝试元数据；敏感快照已迁移到独立密文表。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "debug_trace_attempts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub trace_id: i64,
    pub candidate_index: i16,
    pub channel_id: i64,
    pub credential_id: i64,
    pub outcome: i16,
    pub failure_kind: Option<i16>,
    pub upstream_status: Option<i16>,
    pub retry_decision: bool,
    pub elapsed_ms: i64,
    #[sea_orm(column_type = "Text", nullable)]
    pub client_simulation_profile: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub client_simulation_result: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub client_simulation_body_profile: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub client_simulation_body_result: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub request_method: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub request_url: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub request_headers_json: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub request_body_json: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub response_headers_json: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub response_body_json: Option<String>,
    pub response_status: Option<i16>,
    pub response_streamed: bool,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::debug_traces::Entity",
        from = "Column::TraceId",
        to = "super::debug_traces::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Trace,
}

impl Related<super::debug_traces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Trace.def()
    }
}
