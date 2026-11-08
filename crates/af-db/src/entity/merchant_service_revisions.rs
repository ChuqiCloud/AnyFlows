use super::ChannelBaseUrl;
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "merchant_service_revisions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub service_id: i64,
    pub revision: i64,
    pub status: String,
    pub name: String,
    pub description: String,
    pub base_url: ChannelBaseUrl,
    pub protocol: String,
    pub default_ratio_micros: i64,
    pub models: Json,
    pub channel_id: i64,
    pub credential_id: i64,
    pub concurrency: Option<i32>,
    pub timeout_secs: i32,
    pub review_reason: Option<String>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
