//! 用户、计划快照、额度用量与当前周期时间窗的订阅实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_subscriptions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub subscription_key: SensitiveString,
    pub user_id: i64,
    pub plan_id: i64,
    pub plan_version: i64,
    pub status: i16,
    pub quota_amount: i64,
    pub quota_used: i64,
    pub cycle: i16,
    pub window_started_at: TimeDateTimeWithTimeZone,
    pub window_ends_at: TimeDateTimeWithTimeZone,
    pub version: i64,
    pub bound_at: TimeDateTimeWithTimeZone,
    pub status_changed_at: TimeDateTimeWithTimeZone,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::subscription_plans::Entity",
        from = "Column::PlanId",
        to = "super::subscription_plans::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Plan,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::subscription_plans::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Plan.def()
    }
}
