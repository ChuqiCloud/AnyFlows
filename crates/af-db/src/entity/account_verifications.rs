use super::SensitiveString;
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "account_verifications")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub kind: String,
    pub provider: String,
    pub provider_reference: Option<String>,
    pub provider_action_url: Option<String>,
    pub provider_status: Option<String>,
    pub document_country: String,
    pub document_type: String,
    pub document_number_masked: Option<String>,
    pub subject_name: SensitiveString,
    pub summary: SensitiveString,
    pub status: i16,
    pub version: i64,
    pub reviewer_user_id: Option<i64>,
    pub review_reason: Option<SensitiveString>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
