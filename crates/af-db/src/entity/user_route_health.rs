use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_route_health")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub route_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub model: String,
    pub success_count: i64,
    pub fail_count: i64,
    pub total_latency_ms: i64,
    pub consecutive_failures: i32,
    pub cooldown_until_millis: Option<i64>,
    pub window_started_at_millis: i64,
    pub updated_at_millis: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
