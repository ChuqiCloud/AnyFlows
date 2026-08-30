//! 企业服务账号审批执行事实；只记录主体状态或预算绑定变更，不保存密钥材料。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_approval_service_account_executions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub request_id: i64,
    pub decision_id: i64,
    pub service_account_id: i64,
    pub service_account_department_id: Option<i64>,
    pub action_kind: i16,
    pub target_status: Option<i16>,
    pub policy_id: Option<i64>,
    pub expected_account_version: i64,
    pub applied_account_version: i64,
    pub expected_policy_version: Option<i64>,
    pub applied_policy_version: Option<i64>,
    pub expected_binding_version: Option<i64>,
    pub applied_binding_version: Option<i64>,
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
        belongs_to = "super::organization_service_accounts::Entity",
        from = "Column::ServiceAccountId",
        to = "super::organization_service_accounts::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    ServiceAccount,
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

impl Related<super::organization_service_accounts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ServiceAccount.def()
    }
}
