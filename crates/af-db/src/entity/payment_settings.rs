//! 在线支付 Provider 的固定系统设置实体。

use sea_orm::entity::prelude::*;

use super::{EncryptedJson, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "payment_settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i16,
    pub initialized: bool,
    pub stripe_enabled: bool,
    #[sea_orm(column_type = "String(StringLen::N(512))", nullable)]
    pub stripe_publishable_key: Option<SensitiveString>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub stripe_secret_key: Option<EncryptedJson>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub stripe_webhook_secret: Option<EncryptedJson>,
    pub stripe_signature_tolerance_seconds: i32,
    pub epay_enabled: bool,
    #[sea_orm(column_type = "String(StringLen::N(2048))", nullable)]
    pub epay_gateway_url: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub epay_merchant_id: Option<SensitiveString>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub epay_merchant_key: Option<EncryptedJson>,
    pub epay_alipay_enabled: bool,
    pub epay_wxpay_enabled: bool,
    pub epay_qr_enabled: bool,
    pub epay_refund_enabled: bool,
    pub refund_auto_submit_enabled: bool,
    pub epay_quota_per_cny: i64,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
