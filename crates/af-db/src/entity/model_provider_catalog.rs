//! 模型厂商目录持久化实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "model_provider_catalog")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "String(StringLen::N(64))"
    )]
    pub provider_key: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub display_name: String,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub logo: Option<String>,
    #[sea_orm(column_type = "JsonBinary")]
    pub aliases: Json,
    pub enabled: bool,
    pub sort_order: i32,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
