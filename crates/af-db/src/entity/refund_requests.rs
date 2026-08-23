//! 退款请求事实实体；仅保存可对账的订单与金额快照。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "refund_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub request_key: String,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub idempotency_key: String,
    pub user_id: i64,
    pub order_kind: i16,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub order_key: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub provider: String,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub payment_reference: Option<String>,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
    pub original_amount_minor: i64,
    pub refund_amount_minor: i64,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub provider_refund_id: Option<String>,
    pub status: i16,
    pub approval_status: i16,
    pub approval_actor_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(512))", nullable)]
    pub approval_reason: Option<String>,
    pub version: i64,
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
