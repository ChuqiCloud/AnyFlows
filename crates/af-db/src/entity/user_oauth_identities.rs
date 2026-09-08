//! 本地用户与外部 OAuth subject 的唯一绑定实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_oauth_identities")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub provider: String,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub subject: String,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::oauth_login_providers::Entity",
        from = "Column::Provider",
        to = "super::oauth_login_providers::Column::Provider",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Provider,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::oauth_login_providers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Provider.def()
    }
}
