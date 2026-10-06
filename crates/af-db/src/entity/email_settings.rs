//! 系统 SMTP 邮件设置实体。

use sea_orm::entity::prelude::*;

use super::{EncryptedJson, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "email_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub enabled: bool,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub host: SensitiveString,
    pub port: i32,
    pub tls_mode: i16,
    #[sea_orm(column_type = "String(StringLen::N(320))", nullable)]
    pub username: Option<SensitiveString>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub password_secret: Option<EncryptedJson>,
    #[sea_orm(column_type = "String(StringLen::N(320))")]
    pub from_address: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub from_name: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(320))", nullable)]
    pub reply_to: Option<SensitiveString>,
    pub timeout_seconds: i32,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
