//! 上游渠道配置实体。

use sea_orm::entity::prelude::*;

use super::{ChannelBaseUrl, HeaderOverrides, SensitiveJson};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "channels")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub name: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub r#type: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub protocol: String,
    #[sea_orm(column_type = "Text", nullable)]
    pub base_url: Option<ChannelBaseUrl>,
    /// 空值使用对应场景的服务默认值；非空值同时覆盖读取停顿和完整请求超时。
    pub timeout_secs: Option<i32>,
    pub status: i16,
    pub weight: i32,
    pub priority: i32,
    pub auto_ban: bool,
    #[sea_orm(column_type = "JsonBinary")]
    pub model_mapping: Json,
    #[sea_orm(column_type = "JsonBinary")]
    pub param_override: Json,
    #[sea_orm(column_type = "JsonBinary")]
    pub header_override: HeaderOverrides,
    pub balance: Option<i64>,
    pub used_quota: i64,
    #[sea_orm(column_type = "JsonBinary")]
    pub settings: SensitiveJson,
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub tag: Option<String>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
