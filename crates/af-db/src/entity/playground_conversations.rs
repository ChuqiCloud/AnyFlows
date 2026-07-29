//! Playground 私有会话历史实体。

use sea_orm::entity::prelude::*;

use super::{PlaygroundConversationKey, SensitiveJson, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "playground_conversations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub conversation_id: PlaygroundConversationKey,
    pub owner_user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(120))")]
    pub title: SensitiveString,
    #[sea_orm(column_type = "JsonBinary")]
    pub models: SensitiveJson,
    #[sea_orm(column_type = "JsonBinary")]
    pub snapshot: SensitiveJson,
    pub revision: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::OwnerUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Owner,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Owner.def()
    }
}
