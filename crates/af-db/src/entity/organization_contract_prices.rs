//! 企业合同价版本事实实体；金额只作为后续计费解析输入，不产生资金流水。
use sea_orm::entity::prelude::*;

use super::{SensitiveDecimal, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_contract_prices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
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
    pub version: i64,
    pub created_by_user_id: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
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
        belongs_to = "super::users::Entity",
        from = "Column::CreatedByUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    CreatedBy,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CreatedBy.def()
    }
}
