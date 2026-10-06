use af_admin::TokenAuthentication;
use af_domain::AfError;
use axum::{
    extract::{Extension, State},
    response::Response,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{OpenAiHttpError, chat_completions::HttpState, management_session::no_store_json};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OpenAiModel)]
pub(crate) struct OpenAiModelResponse {
    id: String,
    #[schema(example = "model")]
    object: &'static str,
    created: i64,
    owned_by: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OpenAiModelList)]
pub(crate) struct OpenAiModelListResponse {
    #[schema(example = "list")]
    object: &'static str,
    data: Vec<OpenAiModelResponse>,
}

pub(crate) async fn list_openai_models(
    State(state): State<HttpState>,
    Extension(authentication): Extension<TokenAuthentication>,
) -> Result<Response, OpenAiHttpError> {
    let reader = state
        .model_catalog_reader
        .as_deref()
        .ok_or(AfError::Internal)?;
    let models = reader
        .list_for_token(authentication)
        .await
        .map_err(|_| AfError::Internal)?;
    Ok(no_store_json(OpenAiModelListResponse {
        object: "list",
        data: models
            .into_iter()
            .map(|model| OpenAiModelResponse {
                id: model.id().to_owned(),
                object: "model",
                created: model.created(),
                owned_by: model.owned_by().to_owned(),
            })
            .collect(),
    }))
}
