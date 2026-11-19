//! Namespaced JSON state owned by one plugin deployment identity.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_storage")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub plugin_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub storage_key: String,
    pub value_json: Json,
    pub expires_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
