//! 同步模型请求终态的低敏感度只追加实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "request_outcome_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(128))", unique)]
    pub request_id: String,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub protocol: String,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub operation: String,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub model: String,
    pub outcome: i16,
    #[sea_orm(column_type = "String(StringLen::N(32))", nullable)]
    pub error_kind: Option<String>,
    /// 请求终态发生时固化的下游用户、令牌和分组归属；迁移前事实为空。
    pub user_id: Option<i64>,
    pub token_id: Option<i64>,
    pub group_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub organization_team_id: Option<i64>,
    /// 面向调用方的安全错误码和文案；永不保存原始上游响应。
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub public_error_code: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub public_error_message: Option<String>,
    pub channel_id: Option<i64>,
    pub duration_ms: i64,
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
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Channel,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::channels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}
