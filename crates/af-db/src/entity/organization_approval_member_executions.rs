//! 企业成员审批执行事实实体；每个申请只能绑定一个完整成员目标快照。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_approval_member_executions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub request_id: i64,
    pub decision_id: i64,
    pub target_membership_id: i64,
    pub target_role: i16,
    pub target_status: i16,
    pub target_team_id: Option<i64>,
    pub expected_membership_version: i64,
    pub applied_membership_version: i64,
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
