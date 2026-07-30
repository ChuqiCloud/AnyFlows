//! 站点身份与公开品牌设置实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "site_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    #[sea_orm(column_type = "String(StringLen::N(80))")]
    pub site_name: String,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub public_base_url: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub brand_logo_url: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(160))", nullable)]
    pub brand_tagline: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(500))", nullable)]
    pub brand_description: Option<String>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
