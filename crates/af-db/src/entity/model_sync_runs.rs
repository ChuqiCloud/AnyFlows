//! 上游模型同步预览与应用终态的审计实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "model_sync_runs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(36))")]
    pub preview_id: String,
    pub channel_id: i64,
    pub actor_user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub channel_type: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub protocol: String,
    pub state: i16,
    pub candidate_count: i32,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub applied_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Channel,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ActorUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Actor,
}

impl Related<super::channels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Actor.def()
    }
}
