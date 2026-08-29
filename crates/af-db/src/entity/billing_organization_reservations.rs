//! 企业钱包与多层预算的持久化预留快照实体。
use sea_orm::entity::prelude::*;

use super::{BillingReservationKey, SensitiveDecimal};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "billing_organization_reservations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub idempotency_key: BillingReservationKey,
    pub organization_id: i64,
    pub membership_id: i64,
    pub team_id: Option<i64>,
    pub user_id: i64,
    pub contract_price_id: Option<i64>,
    pub contract_price_version: Option<i64>,
    pub contract_input_price: Option<SensitiveDecimal>,
    pub contract_output_price: Option<SensitiveDecimal>,
    pub contract_cache_read_price: Option<SensitiveDecimal>,
    pub contract_cache_creation_5m_price: Option<SensitiveDecimal>,
    pub contract_cache_creation_1h_price: Option<SensitiveDecimal>,
    /// 部门路径使用根到叶的逗号分隔内部 ID；空值表示历史主体未绑定部门。
    pub department_path: Option<String>,
    pub token_id: Option<i64>,
    pub group_id: Option<i64>,
    pub reservation_kind: i16,
    pub status: i16,
    pub reserved_quota: i64,
    pub token_reserved_quota: i64,
    pub actual_quota: Option<i64>,
    pub organization_policy_id: Option<i64>,
    pub organization_window_id: Option<i64>,
    pub organization_policy_version: Option<i64>,
    pub organization_window_version: Option<i64>,
    pub organization_reserved_quota: Option<i64>,
    pub team_policy_id: Option<i64>,
    pub team_window_id: Option<i64>,
    pub team_policy_version: Option<i64>,
    pub team_window_version: Option<i64>,
    pub team_reserved_quota: Option<i64>,
    pub member_policy_id: Option<i64>,
    pub member_window_id: Option<i64>,
    pub member_policy_version: Option<i64>,
    pub member_window_version: Option<i64>,
    pub member_reserved_quota: Option<i64>,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub finalized_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
