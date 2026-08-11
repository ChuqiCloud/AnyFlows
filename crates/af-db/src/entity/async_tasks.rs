//! 供应商无关异步任务状态机的持久化实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "async_tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub task_key: SensitiveString,
    pub user_id: i64,
    pub token_id: i64,
    pub group_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub idempotency_key: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub protocol: String,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub requested_model: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub upstream_model: SensitiveString,
    pub channel_id: i64,
    pub credential_id: i64,
    #[sea_orm(column_type = "Char(Some(16))")]
    pub credential_revision: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(512))")]
    pub upstream_task_id: SensitiveString,
    pub status: i16,
    pub progress_basis_points: i16,
    pub failure_kind: Option<i16>,
    pub version: i64,
    pub terminal_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
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
    #[sea_orm(
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Channel,
    #[sea_orm(
        belongs_to = "super::credentials::Entity",
        from = "Column::CredentialId",
        to = "super::credentials::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Credential,
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

impl Related<super::channels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}

impl Related<super::credentials::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Credential.def()
    }
}
