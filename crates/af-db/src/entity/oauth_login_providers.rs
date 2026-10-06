//! 用户登录 OAuth Provider 的受控配置实体。

use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "oauth_login_providers")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "String(StringLen::N(32))"
    )]
    pub provider: String,
    pub enabled: bool,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub client_id: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub issuer_url: Option<String>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub client_secret: Option<EncryptedJson>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
