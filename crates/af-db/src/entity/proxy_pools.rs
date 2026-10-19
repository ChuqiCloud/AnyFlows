use super::EncryptedJson;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "proxy_pools")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    #[sea_orm(column_type = "JsonBinary")]
    pub settings_secret: EncryptedJson,
    pub subscribed: bool,
    pub interval_secs: i32,
    pub revision: i64,
    pub last_refresh_at: Option<i64>,
    pub last_refresh_attempt_at: Option<i64>,
    pub last_check_at: Option<i64>,
    pub status: String,
    pub work_until: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
