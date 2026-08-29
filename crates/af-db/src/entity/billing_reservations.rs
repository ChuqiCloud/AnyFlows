//! 单次请求的额度预留与终态结算实体。

use sea_orm::entity::prelude::*;

use super::{BillingReservationKey, SensitiveDecimal};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "billing_reservations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub idempotency_key: BillingReservationKey,
    pub user_id: i64,
    pub token_id: i64,
    pub group_id: i64,
    pub organization_id: Option<i64>,
    pub contract_price_id: Option<i64>,
    pub contract_price_version: Option<i64>,
    pub contract_input_price: Option<SensitiveDecimal>,
    pub contract_output_price: Option<SensitiveDecimal>,
    pub contract_cache_read_price: Option<SensitiveDecimal>,
    pub contract_cache_creation_5m_price: Option<SensitiveDecimal>,
    pub contract_cache_creation_1h_price: Option<SensitiveDecimal>,
    pub status: i16,
    pub reservation_kind: i16,
    pub funding_source: i16,
    pub reserved_quota: i64,
    pub token_reserved_quota: i64,
    pub actual_quota: Option<i64>,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub finalized_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::tokens::Entity",
        from = "Column::TokenId",
        to = "super::tokens::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Token,
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::GroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Group,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::tokens::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Token.def()
    }
}

impl Related<super::groups::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}
