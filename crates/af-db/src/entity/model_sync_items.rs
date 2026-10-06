//! 上游模型同步预览中的脱敏候选证据实体。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "model_sync_items")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub run_id: i64,
    pub ordinal: i32,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub canonical_model: String,
    #[sea_orm(column_type = "String(StringLen::N(256))", nullable)]
    pub upstream_model: Option<String>,
    pub relation: i16,
    #[sea_orm(column_type = "String(StringLen::N(128))", nullable)]
    pub display_name_hint: Option<String>,
    #[sea_orm(column_type = "Text", nullable)]
    pub description_hint: Option<String>,
    pub context_window_hint: Option<i64>,
    pub input_token_limit_hint: Option<i64>,
    pub output_token_limit_hint: Option<i64>,
    #[sea_orm(column_type = "JsonBinary")]
    pub supported_methods: Json,
    pub applied_model_id: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::model_sync_runs::Entity",
        from = "Column::RunId",
        to = "super::model_sync_runs::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Run,
    #[sea_orm(
        belongs_to = "super::models::Entity",
        from = "Column::AppliedModelId",
        to = "super::models::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    AppliedModel,
}

impl Related<super::model_sync_runs::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Run.def()
    }
}

impl Related<super::models::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AppliedModel.def()
    }
}
