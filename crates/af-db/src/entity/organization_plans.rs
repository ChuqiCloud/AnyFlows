//! 平台维护的企业套餐目录事实；套餐内容创建后不可改写。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_plans")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub plan_key: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(80))")]
    pub name: String,
    #[sea_orm(column_type = "String(StringLen::N(240))", nullable)]
    pub description: Option<String>,
    pub status: i16,
    pub team_management_enabled: bool,
    pub seat_limit: i64,
    pub team_limit: i64,
    pub organization_key_limit: i64,
    pub audit_retention_days: i64,
    pub sso_enabled: bool,
    pub custom_roles_enabled: bool,
    pub validity_days: i64,
    pub created_by_user_id: i64,
    pub version: i64,
    pub disabled_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::CreatedByUserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    CreatedByUser,
    #[sea_orm(has_one = "super::organization_plan_prices::Entity")]
    Price,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CreatedByUser.def()
    }
}

impl Related<super::organization_plan_prices::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Price.def()
    }
}
