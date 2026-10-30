//! 用户 TOTP 待绑定状态；密钥材料始终以加密封套保存。

use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_two_factor_enrollments")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: i64,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub pending_configuration: Option<EncryptedJson>,
    pub pending_created_at: Option<TimeDateTimeWithTimeZone>,
    pub pending_expires_at: Option<TimeDateTimeWithTimeZone>,
    pub bound_at: Option<TimeDateTimeWithTimeZone>,
    pub last_verified_at: Option<TimeDateTimeWithTimeZone>,
    pub pending_session_version: Option<i64>,
    pub pending_attempts: i32,
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
