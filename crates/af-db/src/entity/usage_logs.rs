//! 已确认用量事实的只追加持久化实体。

use sea_orm::entity::prelude::*;

use super::BillingReservationKey;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "usage_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub event_id: BillingReservationKey,
    pub event_type: i16,
    pub user_id: i64,
    pub token_id: i64,
    pub group_id: i64,
    pub organization_id: Option<i64>,
    pub organization_team_id: Option<i64>,
    pub billing_mode: i16,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_creation_5m: i64,
    pub cache_creation_1h: i64,
    pub reasoning_tokens: i64,
    pub audio_input_tokens: i64,
    pub audio_output_tokens: i64,
    pub audio_duration_nanoseconds: Option<i64>,
    pub video_duration_seconds: Option<i64>,
    pub video_resolution: Option<i16>,
    pub request_id: Option<String>,
    pub model: Option<String>,
    pub protocol: Option<i16>,
    pub operation: Option<i16>,
    pub is_stream: Option<bool>,
    pub reasoning_effort: Option<i16>,
    pub reasoning_budget_tokens: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub usage_source: i16,
    pub usage_semantics: i16,
    pub quota: i64,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
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

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::tokens::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Token.def()
    }
}

impl Related<super::groups::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}
