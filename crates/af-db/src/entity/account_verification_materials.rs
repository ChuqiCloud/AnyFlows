use super::SensitiveString;
use sea_orm::entity::prelude::*;

// Material bytes deliberately do not participate in Debug output.
#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "account_verification_materials")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub case_id: i64,
    pub kind: String,
    pub file_name: SensitiveString,
    pub content_type: String,
    pub size_bytes: i64,
    #[sea_orm(column_type = "Blob")]
    pub content_bytes: Vec<u8>,
}
impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountVerificationMaterial")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
