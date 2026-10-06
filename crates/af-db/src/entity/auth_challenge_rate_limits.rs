//! 认证挑战发送固定窗口限流状态；只保存带密钥的主体或客户端指纹。

use sea_orm::entity::prelude::*;

use super::AuthChallengeHash;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "auth_challenge_rate_limits")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub purpose: i16,
    pub scope: i16,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub fingerprint: AuthChallengeHash,
    pub window_started_at: i64,
    pub attempts: i32,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
