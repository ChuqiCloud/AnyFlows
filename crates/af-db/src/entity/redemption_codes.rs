//! 只保存摘要的一次性兑换码消费实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "redemption_codes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub code_key: SensitiveString,
    pub batch_id: i64,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub code_sha256: SensitiveString,
    pub status: i16,
    pub used_by_user_id: Option<i64>,
    pub redeemed_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::redemption_batches::Entity",
        from = "Column::BatchId",
        to = "super::redemption_batches::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Batch,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UsedByUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    UsedByUser,
}

impl Related<super::redemption_batches::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Batch.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::UsedByUser.def()
    }
}
