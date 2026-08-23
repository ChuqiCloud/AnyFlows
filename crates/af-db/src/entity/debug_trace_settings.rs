//! 请求级调试追踪的全局固定设置实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "debug_trace_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub enabled: bool,
    pub sample_per_million: i64,
    pub retention_hours: i32,
    pub capture_headers: bool,
    pub capture_bodies: bool,
    pub max_body_bytes: i32,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
