//! 本地用户与自定义 OAuth2 subject 的独立唯一绑定实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "custom_oauth2_identities")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub provider_key: String,
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
        on_delete = "Cascade"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::custom_oauth2_providers::Entity",
        from = "Column::ProviderKey",
        to = "super::custom_oauth2_providers::Column::ProviderKey",
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

impl Related<super::custom_oauth2_providers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Provider.def()
    }
}
