//! 调试追踪敏感快照实体；载荷只允许保存经过字段级加密的封套。

use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "debug_trace_snapshots")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub trace_id: i64,
    pub attempt_id: Option<i64>,
    pub kind: i16,
    #[sea_orm(column_type = "JsonBinary")]
    pub encrypted_payload: EncryptedJson,
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
    #[sea_orm(
        belongs_to = "super::debug_trace_attempts::Entity",
        from = "Column::AttemptId",
        to = "super::debug_trace_attempts::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Attempt,
}

impl Related<super::debug_traces::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Trace.def()
    }
}

impl Related<super::debug_trace_attempts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Attempt.def()
    }
}
