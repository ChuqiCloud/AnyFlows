//! 企业计费预留的部门路径逐层预算快照。
use sea_orm::entity::prelude::*;

use super::BillingReservationKey;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "billing_organization_reservation_departments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub idempotency_key: BillingReservationKey,
    #[sea_orm(primary_key, auto_increment = false)]
    pub path_index: i16,
    pub organization_id: i64,
    pub department_id: i64,
    pub policy_id: i64,
    pub window_id: i64,
    pub policy_version: i64,
    pub window_version: i64,
    pub reserved_quota: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
