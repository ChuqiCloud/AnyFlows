//! 自定义 OAuth2 Provider 的持久化实体；Client Secret 只保存加密 envelope。
use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "custom_oauth2_providers")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "String(StringLen::N(32))"
    )]
    pub provider_key: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub display_name: String,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub client_id: String,
    #[sea_orm(column_type = "String(StringLen::N(2048))")]
    pub authorization_endpoint: String,
    #[sea_orm(column_type = "String(StringLen::N(2048))")]
    pub token_endpoint: String,
    #[sea_orm(column_type = "String(StringLen::N(2048))")]
    pub userinfo_endpoint: String,
    #[sea_orm(column_type = "String(StringLen::N(2048))")]
    pub scope: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub subject_field: String,
    pub enabled: bool,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub client_secret: Option<EncryptedJson>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
