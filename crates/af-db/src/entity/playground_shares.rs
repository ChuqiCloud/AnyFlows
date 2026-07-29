//! Playground 只读分享的不可变快照实体。

use sea_orm::entity::prelude::*;

use super::{SensitiveJson, TokenHash};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "playground_shares")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub owner_user_id: i64,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub token_hash: TokenHash,
    #[sea_orm(column_type = "JsonBinary")]
    pub snapshot: SensitiveJson,
    pub created_at: TimeDateTimeWithTimeZone,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::OwnerUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Owner,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Owner.def()
    }
}
