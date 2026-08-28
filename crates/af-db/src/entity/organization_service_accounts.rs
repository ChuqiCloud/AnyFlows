//! 企业服务账号主体实体；主体不绑定自然人、成员或钱包。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_service_accounts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub organization_id: i64,
    /// 创建/认证时固化的部门归属；空值表示迁移前的旧主体。
    pub organization_department_id: Option<i64>,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub public_key: String,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub name: String,
    pub status: i16,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
    pub revoked_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrganizationId",
        to = "super::organizations::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Organization,
    #[sea_orm(has_many = "super::organization_service_account_keys::Entity")]
    Keys,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_service_account_keys::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Keys.def()
    }
}
