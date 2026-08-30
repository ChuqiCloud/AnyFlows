//! 企业还款核销分配实体；每条记录不可变，只能追加。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_credit_repayment_allocations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub allocation_key: String,
    pub organization_id: i64,
    pub repayment_id: i64,
    pub invoice_id: i64,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
    pub amount_minor: i64,
    pub allocated_at: TimeDateTimeWithTimeZone,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
