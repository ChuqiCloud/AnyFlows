//! 分组、模型与渠道的派生选路索引实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "abilities")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub group_id: i64,
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "String(StringLen::N(255))"
    )]
    pub model: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: i64,
    pub enabled: bool,
    pub priority: i32,
    pub weight: i32,
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub tag: Option<String>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::GroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Group,
    #[sea_orm(
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Channel,
}

impl Related<super::groups::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}

impl Related<super::channels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}
