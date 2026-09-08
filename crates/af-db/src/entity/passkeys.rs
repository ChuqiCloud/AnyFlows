//! Passkey 公钥凭证实体；私钥永远不进入服务端数据库。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "passkeys")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(2048))")]
    pub credential_id: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub credential_id_digest: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub passkey: Json,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub display_name: String,
    pub created_at: TimeDateTimeWithTimeZone,
    pub last_used_at: Option<TimeDateTimeWithTimeZone>,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
    pub sign_count: i64,
    pub anomaly_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    User,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
