//! 模型定价目录持久化实体。

use sea_orm::entity::prelude::*;

use super::{SensitiveDecimal, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "model_prices")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "String(StringLen::N(256))"
    )]
    pub model: SensitiveString,
    pub billing_mode: i16,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub input_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub output_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub cache_read_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub cache_creation_5m_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub cache_creation_1h_price: SensitiveDecimal,
    #[sea_orm(column_type = "Text", nullable)]
    pub billing_expression: Option<SensitiveString>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
