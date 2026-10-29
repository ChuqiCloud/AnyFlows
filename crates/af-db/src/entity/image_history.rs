//! 用户私有生图广场历史元数据。

use sea_orm::entity::prelude::*;

use super::{ImageHistoryKey, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "image_history")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub image_id: ImageHistoryKey,
    pub owner_user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub model: SensitiveString,
    #[sea_orm(column_type = "Text")]
    pub prompt: SensitiveString,
    #[sea_orm(column_type = "Text", nullable)]
    pub revised_prompt: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub request_id: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(8))")]
    pub format: String,
    pub byte_size: i64,
    #[sea_orm(column_type = "String(StringLen::N(8))", nullable)]
    pub thumbnail_format: Option<String>,
    pub thumbnail_byte_size: Option<i64>,
    pub created_at: TimeDateTimeWithTimeZone,
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

impl ActiveModelBehavior for ActiveModel {}
