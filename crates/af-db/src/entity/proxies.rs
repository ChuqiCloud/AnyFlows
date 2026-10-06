//! 凭据专属出口代理目录实体。

use sea_orm::entity::prelude::*;

use super::{EncryptedJson, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "proxies")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub name: String,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub active_name: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub scheme: String,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub host: SensitiveString,
    pub port: i32,
    #[sea_orm(column_type = "String(StringLen::N(320))", nullable)]
    pub username: Option<SensitiveString>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub password_secret: Option<EncryptedJson>,
    pub trust_proxy_dns: bool,
    pub enabled: bool,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::credentials::Entity")]
    Credentials,
}

impl Related<super::credentials::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Credentials.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
