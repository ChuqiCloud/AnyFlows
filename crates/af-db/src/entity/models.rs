//! 独立模型商品元数据实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "models")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub model: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub display_name: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub provider: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub description: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub icon_url: Option<String>,
    #[sea_orm(column_type = "JsonBinary")]
    pub tags: Json,
    pub context_window: Option<i64>,
    pub supports_text_input: bool,
    pub supports_image_input: bool,
    pub supports_audio_input: bool,
    pub supports_video_input: bool,
    pub supports_text_output: bool,
    pub supports_image_output: bool,
    pub supports_audio_output: bool,
    pub supports_video_output: bool,
    pub supports_reasoning: bool,
    pub supports_tool_calls: bool,
    pub visibility: i16,
    pub lifecycle: i16,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
