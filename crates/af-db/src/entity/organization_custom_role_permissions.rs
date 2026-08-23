//! 企业自定义角色逐条权限事实实体。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_custom_role_permissions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub custom_role_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub permission_code: String,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organization_custom_roles::Entity",
        from = "Column::CustomRoleId",
        to = "super::organization_custom_roles::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    CustomRole,
}

impl Related<super::organization_custom_roles::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::CustomRole.def()
    }
}
