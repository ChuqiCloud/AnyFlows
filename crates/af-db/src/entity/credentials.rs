//! 上游凭据及订阅账号调度元数据实体。

use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "credentials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub channel_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub kind: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub secret: EncryptedJson,
    pub status: i16,
    pub multi_key_mode: Option<i16>,
    pub priority: i32,
    pub weight: i32,
    pub concurrency: Option<i32>,
    /// 百万分比定点负载系数。
    pub load_factor_micros: Option<i64>,
    /// 百万分比定点成本倍率，不参与用户扣费。
    pub rate_multiplier_micros: Option<i64>,
    pub schedulable: bool,
    pub rate_limited_at: Option<TimeDateTimeWithTimeZone>,
    pub rate_limit_reset_at: Option<TimeDateTimeWithTimeZone>,
    pub overload_until: Option<TimeDateTimeWithTimeZone>,
    pub temp_unschedulable_until: Option<TimeDateTimeWithTimeZone>,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub temp_unschedulable_reason: Option<String>,
    pub session_window_start: Option<TimeDateTimeWithTimeZone>,
    pub session_window_end: Option<TimeDateTimeWithTimeZone>,
    pub parent_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub quota_dimension: String,
    pub proxy_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub oauth_provider: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub oauth_account_key: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub oauth_project_id: Option<String>,
    pub last_used_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Channel,
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::ParentId",
        to = "Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Parent,
}

impl Related<super::channels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}
