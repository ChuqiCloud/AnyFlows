//! 管理员线下退款完成的不可变事实。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "refund_manual_completions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub completion_key: String,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub request_key: String,
    pub expected_version: i64,
    pub actor_user_id: i64,
    pub result: i16,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub reference_sha256: String,
    pub completed_at: TimeDateTimeWithTimeZone,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::refund_requests::Entity",
        from = "Column::RequestKey",
        to = "super::refund_requests::Column::RequestKey",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Request,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::ActorUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Actor,
}

impl Related<super::refund_requests::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Request.def()
    }
}
