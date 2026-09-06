//! 公告事实实体；草稿、发布与撤回状态均保留在服务端。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "announcements")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub version: i64,
    pub status: i16,
    pub audience: i16,
    #[sea_orm(column_type = "String(StringLen::N(160))")]
    pub title_zh: String,
    #[sea_orm(column_type = "String(StringLen::N(160))")]
    pub title_en: String,
    #[sea_orm(column_type = "String(StringLen::N(8_192))")]
    pub body_zh: String,
    #[sea_orm(column_type = "String(StringLen::N(8_192))")]
    pub body_en: String,
    pub visible_from: Option<TimeDateTimeWithTimeZone>,
    pub visible_until: Option<TimeDateTimeWithTimeZone>,
    pub created_by: i64,
    pub published_at: Option<TimeDateTimeWithTimeZone>,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedBy",
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
