//! 支付 Provider webhook 的只追加审计实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "topup_payment_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub event_key: String,
    pub order_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub provider: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub provider_event_id: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub trade_no: Option<SensitiveString>,
    pub amount_minor: Option<i64>,
    #[sea_orm(column_type = "Char(Some(3))", nullable)]
    pub currency: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(32))", nullable)]
    pub payment_method: Option<String>,
    pub event_type: i16,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub signature_key_fingerprint: SensitiveString,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub payload_sha256: SensitiveString,
    pub received_at: TimeDateTimeWithTimeZone,
    pub processed_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::topup_orders::Entity",
        from = "Column::OrderId",
        to = "super::topup_orders::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Order,
}

impl Related<super::topup_orders::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Order.def()
    }
}
