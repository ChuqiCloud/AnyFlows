use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credential_model_observations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub credential_id: i64,
    pub channel_id: i64,
    pub identity_fingerprint: String,
    pub items: Json,
    pub status: String,
    pub error_code: Option<String>,
    pub last_success_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    pub lease_token: Option<String>,
    pub lease_until: Option<i64>,
    pub next_refresh_at: i64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
