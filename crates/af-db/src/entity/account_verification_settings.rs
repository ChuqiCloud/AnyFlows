use sea_orm::entity::prelude::*;

use super::{EncryptedJson, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "account_verification_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub initialized: bool,
    pub manual_enabled: bool,
    pub individual_manual_enabled: bool,
    pub enterprise_manual_enabled: bool,
    pub individual_reason_required: bool,
    pub enterprise_reason_required: bool,
    pub alipay_enabled: bool,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub alipay_app_id: Option<SensitiveString>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub alipay_credentials: Option<EncryptedJson>,
    #[sea_orm(column_type = "String(StringLen::N(2048))")]
    pub alipay_gateway_url: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub alipay_biz_code: String,
    pub alipay_timeout_secs: i32,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
