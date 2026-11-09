use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "merchant_services")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub merchant_id: i64,
    pub pool_id: i64,
    pub name: String,
    pub description: String,
    pub status: String,
    pub merchant_paused: bool,
    pub platform_suspended: bool,
    pub published_revision_id: Option<i64>,
    pub pending_revision_id: Option<i64>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
