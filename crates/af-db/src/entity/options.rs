//! 系统动态 KV 配置实体；通用选项不得承载凭据或加密密钥。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "options")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "String(StringLen::N(255))"
    )]
    pub key: String,
    #[sea_orm(column_type = "Text")]
    pub value: SensitiveString,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
