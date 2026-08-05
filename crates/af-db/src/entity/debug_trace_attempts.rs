//! 调试追踪中单个候选尝试的脱敏结果实体。

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
