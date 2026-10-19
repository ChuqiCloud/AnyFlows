use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "proxy_pool_nodes")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub pool_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub proxy_id: i64,
    pub fingerprint: String,
    pub remark: String,
    pub present: bool,
    pub healthy: bool,
    pub latency_ms: Option<i64>,
    pub checked_at: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
