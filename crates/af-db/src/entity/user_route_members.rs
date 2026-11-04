use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_route_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub route_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: i64,
    pub priority: i32,
    pub weight: i32,
    pub last_selected_at_millis: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
