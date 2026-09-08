//! 企业预算策略的不可变周期快照实体。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_budget_windows")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub policy_id: i64,
    pub organization_id: i64,
    pub scope_kind: i16,
    pub team_id: Option<i64>,
    pub membership_id: Option<i64>,
    pub department_id: Option<i64>,
    pub policy_version: i64,
    pub limit_snapshot: i64,
    pub used_quota: i64,
    pub frozen_quota: i64,
    pub window_starts_at: TimeDateTimeWithTimeZone,
    pub window_ends_at: TimeDateTimeWithTimeZone,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
