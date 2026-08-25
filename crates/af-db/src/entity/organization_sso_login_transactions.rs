//! 企业 SSO 单次登录事务实体；仅保存摘要、版本快照和短期票据状态。
use sea_orm::entity::prelude::*;

use super::{EncryptedJson, SensitiveString};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_sso_login_transactions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    pub provider_id: i64,
    pub protocol: i16,
    pub provider_version: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub state_digest: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub request_digest: SensitiveString,
    pub login_material_envelope: Option<EncryptedJson>,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub claimed_at: Option<TimeDateTimeWithTimeZone>,
    pub result_code: Option<i16>,
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub ticket_digest: Option<SensitiveString>,
    pub ticket_expires_at: Option<TimeDateTimeWithTimeZone>,
    pub exchanged_at: Option<TimeDateTimeWithTimeZone>,
    #[sea_orm(column_type = "String(StringLen::N(64))", nullable)]
    pub assertion_digest: Option<SensitiveString>,
    pub assertion_expires_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organization_sso_providers::Entity",
        from = "Column::ProviderId",
        to = "super::organization_sso_providers::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Provider,
}

impl Related<super::organization_sso_providers::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Provider.def()
    }
}
