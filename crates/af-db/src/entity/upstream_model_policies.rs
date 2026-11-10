use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "upstream_model_policies")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub owner_kind: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub owner_id: i64,
    pub channel_id: i64,
    pub policy: Json,
    pub version: i64,
    pub auto_add_discovered: bool,
    pub refresh_interval_secs: Option<i64>,
    pub updated_at: i64,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
