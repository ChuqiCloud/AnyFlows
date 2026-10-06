//! 系统全局出站网络设置实体。

use sea_orm::entity::prelude::*;

use super::{EncryptedJson, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "network_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub mode: i16,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub proxy_host: Option<SensitiveString>,
    pub proxy_port: Option<i32>,
    #[sea_orm(column_type = "String(StringLen::N(320))", nullable)]
    pub username: Option<SensitiveString>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub password_secret: Option<EncryptedJson>,
    pub trust_proxy_dns: bool,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
