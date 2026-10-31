//! 指定来源分组可选择目标分组的授权。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "group_group_grants")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub target_group_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_group_id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::TargetGroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    TargetGroup,
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::SourceGroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    SourceGroup,
}

impl ActiveModelBehavior for ActiveModel {}
