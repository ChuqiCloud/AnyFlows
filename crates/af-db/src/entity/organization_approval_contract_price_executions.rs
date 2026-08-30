//! 企业合同价审批执行事实；只记录已批准的合同价版本创建快照。

use sea_orm::entity::prelude::*;

use super::{SensitiveDecimal, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_approval_contract_price_executions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub request_id: i64,
    pub decision_id: i64,
    pub contract_price_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub model: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub protocol: String,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub input_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub output_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub cache_read_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub cache_creation_5m_price: SensitiveDecimal,
    #[sea_orm(column_type = "Decimal(Some((38, 28)))")]
    pub cache_creation_1h_price: SensitiveDecimal,
    pub effective_from: TimeDateTimeWithTimeZone,
    pub effective_until: Option<TimeDateTimeWithTimeZone>,
    pub expected_version: Option<i64>,
    pub applied_version: i64,
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
        belongs_to = "super::organization_contract_prices::Entity",
        from = "Column::ContractPriceId",
        to = "super::organization_contract_prices::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    ContractPrice,
    #[sea_orm(
        belongs_to = "super::organization_memberships::Entity",
        from = "Column::ExecutedByMembershipId",
        to = "super::organization_memberships::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    ExecutedBy,
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

impl Related<super::organization_contract_prices::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ContractPrice.def()
    }
}

impl Related<super::organization_memberships::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ExecutedBy.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {}
