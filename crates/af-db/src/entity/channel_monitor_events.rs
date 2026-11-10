//! Deduplicated outage transition and recovery facts for active probes.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "channel_monitor_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub target_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub event_kind: String,
    pub latency_ms: Option<i64>,
    pub occurred_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::channel_monitor_targets::Entity",
        from = "Column::TargetId",
        to = "super::channel_monitor_targets::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Target,
}

impl Related<super::channel_monitor_targets::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Target.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
