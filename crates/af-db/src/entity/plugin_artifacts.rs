//! Immutable, content-addressed plugin artifact metadata.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_artifacts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub plugin_id: String,
    pub plugin_version: String,
    pub digest: String,
    pub manifest: Json,
    pub source_kind: String,
    pub source_reference: String,
    pub signer_key_id: String,
    pub signature: String,
    pub created_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "super::plugin_artifact_approvals::Entity")]
    Approval,
}

impl Related<super::plugin_artifact_approvals::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Approval.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
