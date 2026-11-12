use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "access_token_rate_limits")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub subject_key: String,
    pub window_started_at: i64,
    pub used_requests: i32,
    pub expires_at: i64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
