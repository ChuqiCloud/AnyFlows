use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "merchant_pools")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub commission_bps: i32,
    pub commission_inherit_global: bool,
    pub min_ratio_micros: i64,
    pub max_ratio_micros: i64,
    pub settlement_delay_secs: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
