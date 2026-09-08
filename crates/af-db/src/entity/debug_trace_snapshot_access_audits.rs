//! 敏感诊断快照读取审计；不保存快照、错误原文或其他请求内容。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "debug_trace_snapshot_access_audits")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub trace_id: i64,
    pub actor_user_id: i64,
    pub scope: i16,
    pub outcome: i16,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ActorUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Actor,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Actor.def()
    }
}
