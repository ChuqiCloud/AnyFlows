use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "access_token_audit_logs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: Option<i64>,
    pub access_token_id: Option<i64>,
    pub resource_id: Option<String>,
    pub operation: String,
    pub request_id: String,
    pub method: String,
    pub route: String,
    pub required_scope: Option<String>,
    pub status_code: i32,
    pub allowed: bool,
    pub client_ip: Option<String>,
    pub created_at: i64,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessTokenAudit(<redacted>)")
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
