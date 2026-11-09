use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "merchant_profiles")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub owner_user_id: i64,
    pub name: String,
    pub description: String,
    pub status: String,
    pub review_reason: Option<String>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
