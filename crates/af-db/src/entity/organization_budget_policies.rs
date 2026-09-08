//! 企业、团队或成员预算策略实体。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_budget_policies")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub scope_kind: i16,
    pub team_id: Option<i64>,
    pub membership_id: Option<i64>,
    pub department_id: Option<i64>,
    pub period_kind: i16,
    pub fixed_period_seconds: Option<i64>,
    pub quota_limit: Option<i64>,
    pub effective_from: TimeDateTimeWithTimeZone,
    pub effective_until: Option<TimeDateTimeWithTimeZone>,
    pub created_by_user_id: i64,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
