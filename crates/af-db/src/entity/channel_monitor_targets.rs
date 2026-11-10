//! Explicitly opted-in active channel/model/credential probe targets.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "channel_monitor_targets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub channel_id: i64,
    pub credential_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub model: String,
    pub enabled: bool,
    pub interval_secs: i32,
    pub daily_budget: i32,
    pub timeout_secs: i32,
    pub alert_after_secs: i32,
    pub daily_used: i32,
    pub budget_window_started_at: Option<TimeDateTimeWithTimeZone>,
    pub last_attempt_at: Option<TimeDateTimeWithTimeZone>,
    pub last_success_at: Option<TimeDateTimeWithTimeZone>,
    pub failure_since: Option<TimeDateTimeWithTimeZone>,
    pub alert_active: bool,
    pub muted_until: Option<TimeDateTimeWithTimeZone>,
    pub delete_after_lease: bool,
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub lease_token: Option<String>,
    pub lease_until: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
