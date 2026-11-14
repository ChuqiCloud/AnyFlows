//! Immutable administrator revocation facts for approved plugin artifacts.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_artifact_revocations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub artifact_id: i64,
    pub revoked_by: i64,
    pub revoked_at: i64,
    pub request_id: String,
    pub reason: String,
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
        from = "Column::RevokedBy",
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
