//! 登录用户及钱包状态实体。

use sea_orm::entity::prelude::*;

use super::{EncryptedJson, PasswordHash};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub username: String,
    #[sea_orm(column_type = "String(StringLen::N(320))", nullable)]
    pub email: Option<String>,
    #[sea_orm(column_type = "String(StringLen::N(255))", nullable)]
    pub password_hash: Option<PasswordHash>,
    pub session_version: i64,
    pub role: i16,
    pub status: i16,
    pub default_group_id: i64,
    pub quota: i64,
    pub used_quota: i64,
    pub frozen_quota: i64,
    pub request_count: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub aff_code: String,
    pub inviter_id: Option<i64>,
    pub aff_quota: i64,
    pub aff_history_quota: i64,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub totp_secret: Option<EncryptedJson>,
    pub rpm_limit: Option<i32>,
    pub concurrency: Option<i32>,
    /// 是否接收产品更新邮件；默认关闭，避免未经选择的营销触达。
    pub email_product_updates: bool,
    /// 是否接收用量提醒邮件；默认开启，便于用户及时发现额度风险。
    pub email_usage_alerts: bool,
    /// 个人余额预警阈值；空值表示继承系统默认值。
    pub balance_alert_threshold: Option<i64>,
    #[sea_orm(column_type = "JsonBinary")]
    pub settings: Json,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::DefaultGroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    DefaultGroup,
    #[sea_orm(
        belongs_to = "Entity",
        from = "Column::InviterId",
        to = "Column::Id",
        on_update = "Cascade",
        on_delete = "SetNull"
    )]
    Inviter,
}

impl Related<super::groups::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DefaultGroup.def()
    }
}
