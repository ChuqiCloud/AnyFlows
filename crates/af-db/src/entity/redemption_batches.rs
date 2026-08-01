//! 同面额兑换码的不可变批次与启停状态实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_batches")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub batch_key: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(80))")]
    pub name: String,
    pub created_by_user_id: i64,
    pub status: i16,
    pub quota_amount: i64,
    pub code_count: i32,
    pub version: i64,
    pub expires_at: Option<TimeDateTimeWithTimeZone>,
    pub disabled_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedByUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    CreatedByUser,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CreatedByUser.def()
    }
}
