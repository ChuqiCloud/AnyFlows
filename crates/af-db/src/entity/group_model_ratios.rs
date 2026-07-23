//! 用户所属分组与实际计费分组之间的附加倍率。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "group_model_ratios")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_group_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub target_group_id: i64,
    /// 百万分比定点倍率，`1_000_000` 表示 `1.0`。
    pub ratio_micros: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::SourceGroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    SourceGroup,
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::TargetGroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    TargetGroup,
}
