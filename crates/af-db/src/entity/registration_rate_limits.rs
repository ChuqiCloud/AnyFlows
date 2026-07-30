//! 公开注册固定窗口限流状态；只保存带密钥的客户端 IP 指纹。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "registration_rate_limits")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "String(StringLen::N(64))"
    )]
    pub ip_fingerprint: SensitiveString,
    pub window_started_at: i64,
    pub attempts: i32,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
