//! 密码登录与公开注册能力设置实体。

use sea_orm::entity::prelude::*;

use super::groups;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "authentication_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub password_login_enabled: bool,
    pub registration_enabled: bool,
    pub registration_default_group_id: Option<i64>,
    pub registration_initial_quota: i64,
    pub invitation_rebate_quota: i64,
    pub registration_email_required: bool,
    pub registration_rate_limit_attempts: i32,
    pub registration_rate_limit_window_seconds: i64,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "groups::Entity",
        from = "Column::RegistrationDefaultGroupId",
        to = "groups::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    RegistrationDefaultGroup,
}

impl Related<groups::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RegistrationDefaultGroup.def()
    }
}
