//! 企业授权订单实体；订单内容由受信仓储创建后只允许状态机推进。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_plan_orders")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub order_key: String,
    pub organization_id: i64,
    pub payer_user_id: i64,
    pub plan_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub plan_key: String,
    pub plan_version: i64,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub provider: String,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
    pub amount_minor: i64,
    pub kind: i16,
    pub effective_at: Option<TimeDateTimeWithTimeZone>,
    pub applied_at: Option<TimeDateTimeWithTimeZone>,
    pub status: i16,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub idempotency_key: String,
    pub version: i64,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub provider_order_id: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub trade_no: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(32))", nullable)]
    pub payment_method: Option<String>,
    pub expires_at: Option<TimeDateTimeWithTimeZone>,
    pub paid_at: Option<TimeDateTimeWithTimeZone>,
    pub closed_at: Option<TimeDateTimeWithTimeZone>,
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
        from = "Column::PayerUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Payer,
    #[sea_orm(
        belongs_to = "super::organization_plans::Entity",
        from = "Column::PlanId",
        to = "super::organization_plans::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Plan,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Payer.def()
    }
}

impl Related<super::organization_plans::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Plan.def()
    }
}
