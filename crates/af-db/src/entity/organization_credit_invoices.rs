//! 企业账期账单事实实体；仅保存来源摘要和账期快照，不产生资金流水。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_credit_invoices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub invoice_key: String,
    pub organization_id: i64,
    pub source_kind: i16,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub source_key: String,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
    pub amount_minor: i64,
    pub term_version: i64,
    pub period_kind: i16,
    pub period_days: Option<i64>,
    pub issued_at: TimeDateTimeWithTimeZone,
    pub due_at: TimeDateTimeWithTimeZone,
    pub status: i16,
    pub status_changed_at: TimeDateTimeWithTimeZone,
    pub version: i64,
    pub settled_at: Option<TimeDateTimeWithTimeZone>,
    pub voided_at: Option<TimeDateTimeWithTimeZone>,
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
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}
