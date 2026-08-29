//! 企业授信与账期策略事实实体；不与钱包、账本或账单建立关联。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_credit_terms")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(3))")]
    pub currency: String,
    pub credit_limit: i64,
    pub period_kind: i16,
    pub period_days: Option<i64>,
    pub status: i16,
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
