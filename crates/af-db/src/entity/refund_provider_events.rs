//! 退款 Provider 回执的只追加审计事实。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "refund_provider_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub event_key: String,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub request_key: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub provider: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub provider_event_id: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub provider_refund_id: SensitiveString,
    pub event_type: i16,
    pub amount_minor: i64,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
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
        belongs_to = "super::refund_requests::Entity",
        from = "Column::RequestKey",
        to = "super::refund_requests::Column::RequestKey",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Request,
}

impl Related<super::refund_requests::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Request.def()
    }
}
