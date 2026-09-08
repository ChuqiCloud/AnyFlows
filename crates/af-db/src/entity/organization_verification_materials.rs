//! 企业认证材料元数据实体；正文只存在受控对象存储。

use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_verification_materials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub case_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub kind: String,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub object_reference: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(255))")]
    pub file_name: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(128))")]
    pub content_type: String,
    pub size_bytes: i64,
    /// 文件正文；旧版本仅保存元数据时允许为空，新的上传必须写入正文。
    #[sea_orm(column_type = "Blob")]
    pub content_bytes: Option<Vec<u8>>,
    pub created_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organization_verification_cases::Entity",
        from = "Column::CaseId",
        to = "super::organization_verification_cases::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Case,
}

impl Related<super::organization_verification_cases::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Case.def()
    }
}
