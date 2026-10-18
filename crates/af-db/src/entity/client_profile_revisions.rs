use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "client_profile_revisions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub profile_key: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub revision: i64,
    #[sea_orm(column_type = "JsonBinary")]
    pub definition: Json,
    pub created_by: Option<i64>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
