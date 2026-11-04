use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_routes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub owner_user_id: i64,
    pub name: String,
    pub strategy: String,
    pub enabled: bool,
    pub max_group_attempts: i32,
    pub max_price_ratio_micros: Option<i64>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
