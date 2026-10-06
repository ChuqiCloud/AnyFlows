//! 自定义 OAuth2 state 与短期登录票据的单次事务实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "custom_oauth2_login_transactions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub provider_key: String,
    pub configuration_version: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub state_digest: SensitiveString,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub claimed_at: Option<TimeDateTimeWithTimeZone>,
    pub user_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub ticket_digest: Option<SensitiveString>,
    pub ticket_expires_at: Option<TimeDateTimeWithTimeZone>,
    pub exchanged_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::custom_oauth2_providers::Entity",
        from = "Column::ProviderKey",
        to = "super::custom_oauth2_providers::Column::ProviderKey",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Provider,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
}

impl Related<super::custom_oauth2_providers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Provider.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}
