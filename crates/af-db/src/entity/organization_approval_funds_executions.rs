//! 企业钱包调账审批执行事实实体；不承载支付 Provider 或退款回执状态。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_approval_funds_executions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub request_id: i64,
    pub decision_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub event_key: String,
    pub quota_delta: i64,
    #[sea_orm(column_type = "String(StringLen::N(500))")]
    pub reason: SensitiveString,
    pub expected_wallet_version: i64,
    pub applied_wallet_version: i64,
    pub balance_before: i64,
    pub balance_after: i64,
    pub ledger_entry_id: i64,
    pub executed_by_membership_id: i64,
    pub executed_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrganizationId",
        to = "super::organizations::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Organization,
    #[sea_orm(
        belongs_to = "super::organization_approval_requests::Entity",
        from = "Column::RequestId",
        to = "super::organization_approval_requests::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Request,
    #[sea_orm(
        belongs_to = "super::organization_approval_decisions::Entity",
        from = "Column::DecisionId",
        to = "super::organization_approval_decisions::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Decision,
    #[sea_orm(
        belongs_to = "super::organization_wallet_ledger_entries::Entity",
        from = "Column::LedgerEntryId",
        to = "super::organization_wallet_ledger_entries::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    LedgerEntry,
    #[sea_orm(
        belongs_to = "super::organization_memberships::Entity",
        from = "Column::ExecutedByMembershipId",
        to = "super::organization_memberships::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    ExecutedByMembership,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_approval_requests::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Request.def()
    }
}

impl Related<super::organization_approval_decisions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Decision.def()
    }
}

impl Related<super::organization_wallet_ledger_entries::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::LedgerEntry.def()
    }
}

impl Related<super::organization_memberships::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExecutedByMembership.def()
    }
}
