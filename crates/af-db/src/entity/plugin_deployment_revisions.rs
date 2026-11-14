//! Immutable deployment generations retained for rollback and audit linkage.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_deployment_revisions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub plugin_id: String,
    pub generation: i64,
    pub artifact_id: i64,
    pub enabled: bool,
    pub capability_grants: Json,
    pub config: Json,
    pub updated_by: i64,
    pub updated_at: i64,
    pub request_id: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::plugin_artifacts::Entity",
        from = "Column::ArtifactId",
        to = "super::plugin_artifacts::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Artifact,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UpdatedBy",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Operator,
}

impl Related<super::plugin_artifacts::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Artifact.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Operator.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
