//! 退款成功后的只追加负向现金对账事实。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "refund_reconciliation_entries")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub request_key: String,
    pub provider_event_id: Option<i64>,
    pub manual_completion_id: Option<i64>,
    pub user_id: i64,
    pub organization_id: Option<i64>,
    pub approval_actor_id: i64,
    pub order_kind: i16,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub order_key: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub provider: String,
    pub amount_delta_minor: i64,
    #[sea_orm(column_type = "Char(Some(3))")]
    pub currency: String,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::refund_requests::Entity",
        from = "Column::RequestKey",
        to = "super::refund_requests::Column::RequestKey",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Request,
    #[sea_orm(
        belongs_to = "super::refund_provider_events::Entity",
        from = "Column::ProviderEventId",
        to = "super::refund_provider_events::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    ProviderEvent,
    #[sea_orm(
        belongs_to = "super::refund_manual_completions::Entity",
        from = "Column::ManualCompletionId",
        to = "super::refund_manual_completions::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    ManualCompletion,
}
