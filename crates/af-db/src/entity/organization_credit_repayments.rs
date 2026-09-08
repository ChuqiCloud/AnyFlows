//! 企业已确认还款事实实体；只保存来源摘要，不写入钱包或支付材料。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_credit_repayments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub repayment_key: String,
    pub organization_id: i64,
    pub source_kind: i16,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub source_key: String,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
    pub amount_minor: i64,
    pub received_at: TimeDateTimeWithTimeZone,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
