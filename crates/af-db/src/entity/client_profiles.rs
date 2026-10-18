use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "client_profiles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub profile_key: String,
    pub name: String,
    pub service: String,
    pub protocols: String,
    pub credential_kinds: String,
    pub built_in: bool,
    pub enabled: bool,
    pub latest_revision: i64,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
