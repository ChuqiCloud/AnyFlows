//! 每周期额度与启停状态不可变绑定的订阅计划实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "subscription_plans")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub plan_key: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(80))")]
    pub name: String,
    pub created_by_user_id: i64,
    pub status: i16,
    pub quota_amount: i64,
    pub cycle: i16,
    pub version: i64,
    pub disabled_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedByUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    CreatedByUser,
    #[sea_orm(has_many = "super::subscription_plan_prices::Entity")]
    Prices,
}

impl Related<super::subscription_plan_prices::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Prices.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CreatedByUser.def()
    }
}
