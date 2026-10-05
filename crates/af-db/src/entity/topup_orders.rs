//! 充值订单状态机的持久化实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "topup_orders")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub order_key: String,
    pub user_id: i64,
    pub organization_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub provider: String,
    #[sea_orm(column_type = "String(StringLen::N(32))", nullable)]
    pub payment_method: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub provider_order_id: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub trade_no: Option<SensitiveString>,
    pub status: i16,
    pub amount_minor: i64,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
    pub quota_amount: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub idempotency_key: String,
    pub version: i64,
    pub expires_at: Option<TimeDateTimeWithTimeZone>,
    pub paid_at: Option<TimeDateTimeWithTimeZone>,
    pub closed_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}
