//! 独立的系统 API 个人访问令牌，仅持久化领域分隔摘要。
use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_access_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub secret_hash: String,
    pub token_hint: String,
    pub name: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub scopes: Json,
    pub enabled: bool,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub allow_ips: Option<Json>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
    pub last_used_ip: Option<String>,
    pub version: i64,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserAccessToken(<redacted>)")
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
