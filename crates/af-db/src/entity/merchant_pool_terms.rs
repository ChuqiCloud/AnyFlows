use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "merchant_pool_terms")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub pool_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub merchant_id: i64,
    pub commission_bps: Option<i32>,
    pub updated_at: TimeDateTimeWithTimeZone,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
