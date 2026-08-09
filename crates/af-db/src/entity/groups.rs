//! 计费与渠道可见性分组实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "groups")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub name: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub display_name: String,
    /// 百万分比定点倍率，`1_000_000` 表示 `1.0`。
    pub ratio_micros: i64,
    /// 可选高峰倍率，采用与基础倍率相同的定点单位。
    pub peak_ratio_micros: Option<i64>,
    pub peak_start: Option<TimeTime>,
    pub peak_end: Option<TimeTime>,
    pub is_exclusive: bool,
    pub daily_limit: Option<i64>,
    pub weekly_limit: Option<i64>,
    pub monthly_limit: Option<i64>,
    pub daily_usage: i64,
    pub weekly_usage: i64,
    pub monthly_usage: i64,
    pub daily_window_start: TimeDateTimeWithTimeZone,
    pub weekly_window_start: TimeDateTimeWithTimeZone,
    pub monthly_window_start: TimeDateTimeWithTimeZone,
    pub rpm_limit: Option<i32>,
    pub fallback_group_id: Option<i64>,
    #[sea_orm(column_type = "JsonBinary")]
    pub flags: Json,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::FallbackGroupId",
        to = "Column::Id",
        on_update = "Cascade",
        on_delete = "SetNull"
    )]
    FallbackGroup,
}
