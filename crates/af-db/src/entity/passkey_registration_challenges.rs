//! Passkey 注册挑战实体；注册状态以系统密钥封套形式保存。

use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "passkey_registration_challenges")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub challenge_digest: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub registration_state: EncryptedJson,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub consumed_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
