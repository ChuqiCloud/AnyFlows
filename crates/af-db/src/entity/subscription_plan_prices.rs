//! 订阅计划的不可变价格事实。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription_plan_prices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub plan_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub provider: String,
    #[sea_orm(column_type = "String(StringLen::N(3))")]
    pub currency: String,
    pub amount_minor: i64,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::subscription_plans::Entity",
        from = "Column::PlanId",
        to = "super::subscription_plans::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Plan,
}

impl Related<super::subscription_plans::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Plan.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
