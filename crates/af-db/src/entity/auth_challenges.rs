//! 注册邮箱验证与密码重置共用的短生命周期认证挑战实体。

use sea_orm::entity::prelude::*;

use super::AuthChallengeHash;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "auth_challenges")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub purpose: i16,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub subject_fingerprint: AuthChallengeHash,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub secret_digest: AuthChallengeHash,
    pub target_user_id: Option<i64>,
    pub attempts: i32,
    pub max_attempts: i32,
    pub version: i64,
    pub issued_at: TimeDateTimeWithTimeZone,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub next_send_at: TimeDateTimeWithTimeZone,
    pub consumed_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::TargetUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    TargetUser,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::TargetUser.def()
    }
}
