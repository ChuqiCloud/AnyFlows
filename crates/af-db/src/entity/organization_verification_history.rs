use super::SensitiveString;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_verification_history")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub case_id: i64,
    pub version: i64,
    pub snapshot: SensitiveString,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
