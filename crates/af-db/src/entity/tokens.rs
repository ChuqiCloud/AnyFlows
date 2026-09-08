//! 下游 API Key 的鉴权、额度与窗口状态实体。

use sea_orm::entity::prelude::*;

use super::{TokenHash, TokenIpAllowlist, TokenModelAllowlist};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub key_hash: TokenHash,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub key_prefix: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub name: String,
    pub status: i16,
    pub group_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub organization_membership_id: Option<i64>,
    pub organization_team_id: Option<i64>,
    /// 创建/认证时固化的部门归属；空值表示迁移前的旧 Key。
    pub organization_department_id: Option<i64>,
    pub remain_quota: i64,
    pub unlimited_quota: bool,
    pub used_quota: i64,
    pub expired_at: Option<TimeDateTimeWithTimeZone>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub model_limits: Option<TokenModelAllowlist>,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    pub allow_ips: Option<TokenIpAllowlist>,
    pub cross_group_retry: bool,
    pub rate_limit_5h: Option<i64>,
    pub rate_limit_1d: Option<i64>,
    pub rate_limit_7d: Option<i64>,
    pub usage_5h: i64,
    pub usage_1d: i64,
    pub usage_7d: i64,
    pub window_5h_start: TimeDateTimeWithTimeZone,
    pub window_1d_start: TimeDateTimeWithTimeZone,
    pub window_7d_start: TimeDateTimeWithTimeZone,
    pub max_requests: Option<i64>,
    pub used_requests: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub deleted_at: Option<TimeDateTimeWithTimeZone>,
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

impl Related<super::groups::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}
