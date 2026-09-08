//! 企业 SSO 过期登录事务清理任务的全局数据库租约。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_sso_login_transaction_cleanup_leases")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub lease_key: String,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub owner_token: String,
    pub lease_expires_at: TimeDateTimeWithTimeZone,
    pub version: i64,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
