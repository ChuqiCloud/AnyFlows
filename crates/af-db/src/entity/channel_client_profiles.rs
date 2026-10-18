use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "channel_client_profiles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: i64,
    pub profile_key: String,
    pub revision: i64,
    pub version: i64,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
