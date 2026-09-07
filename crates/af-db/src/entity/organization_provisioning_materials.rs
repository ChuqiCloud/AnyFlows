//! 企业开通材料正文与脱敏元数据实体。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_provisioning_materials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub request_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub kind: String,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub object_reference: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub file_name: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub content_type: String,
    pub size_bytes: i64,
    /// 旧记录可为空；新提交必须写入正文。
    #[sea_orm(column_type = "Blob")]
    pub content_bytes: Option<Vec<u8>>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organization_provisioning_requests::Entity",
        from = "Column::RequestId",
        to = "super::organization_provisioning_requests::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Request,
}

impl Related<super::organization_provisioning_requests::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Request.def()
    }
}
