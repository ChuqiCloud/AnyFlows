use af_admin::{AdminModelProvider, AdminModelProviderCommand, AdminModelProviderError};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState, management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelProviderCatalogEntry)]
pub(crate) struct ModelProviderCatalogEntryResponse {
    #[schema(pattern = "^[a-z0-9][a-z0-9_-]{0,63}$")]
    provider_key: String,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    #[schema(max_length = 128, required = true)]
    logo: Option<String>,
    #[schema(max_items = 32)]
    aliases: Vec<String>,
    enabled: bool,
    #[schema(minimum = 0)]
    sort_order: i32,
    #[schema(minimum = 1)]
    version: i64,
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelProviderCatalogList)]
pub(crate) struct ModelProviderCatalogListResponse {
    providers: Vec<ModelProviderCatalogEntryResponse>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ModelProviderCatalogRequest)]
pub(crate) struct ModelProviderCatalogRequest {
    #[schema(minimum = 0)]
    expected_version: i64,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    #[schema(max_length = 128)]
    logo: Option<String>,
    #[schema(max_items = 32)]
    aliases: Vec<String>,
    enabled: bool,
    #[schema(minimum = 0)]
    sort_order: i32,
}

impl From<AdminModelProvider> for ModelProviderCatalogEntryResponse {
    fn from(provider: AdminModelProvider) -> Self {
        Self {
            provider_key: provider.provider_key().to_owned(),
            display_name: provider.display_name().to_owned(),
            logo: provider.logo().map(str::to_owned),
            aliases: provider.aliases().to_vec(),
            enabled: provider.enabled(),
            sort_order: provider.sort_order(),
            version: provider.version(),
        }
    }
}

pub(crate) async fn list_public_model_provider_catalog(
    State(state): State<HttpState>,
) -> Result<Response, ManagementError> {
    let service = state
        .model_provider_catalog_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let providers = service.list_public().await.map_err(map_error)?;
    Ok(no_store_json(ModelProviderCatalogListResponse {
        providers: providers.into_iter().map(Into::into).collect(),
    }))
}

pub(crate) async fn list_admin_model_provider_catalog(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .model_provider_catalog_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let providers = service
        .list(authentication.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ModelProviderCatalogListResponse {
        providers: providers.into_iter().map(Into::into).collect(),
    }))
}

pub(crate) async fn get_admin_model_provider(
    State(state): State<HttpState>,
    Path(provider_key): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .model_provider_catalog_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let provider = service
        .get(authentication.principal(), provider_key)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ModelProviderCatalogEntryResponse::from(
        provider,
    )))
}

pub(crate) async fn update_admin_model_provider(
    State(state): State<HttpState>,
    Path(provider_key): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<ModelProviderCatalogRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let service = state
        .model_provider_catalog_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let provider = service
        .save(
            authentication.principal(),
            provider_key,
            AdminModelProviderCommand {
                expected_version: request.expected_version,
                display_name: request.display_name,
                logo: request.logo,
                aliases: request.aliases,
                enabled: request.enabled,
                sort_order: request.sort_order,
            },
        )
        .await
        .map_err(map_error)?;
    let mut response = no_store_json(ModelProviderCatalogEntryResponse::from(provider));
    *response.status_mut() = StatusCode::OK;
    Ok(response)
}

pub(crate) async fn delete_admin_model_provider(
    State(state): State<HttpState>,
    Path(provider_key): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<DeleteModelProviderRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let service = state
        .model_provider_catalog_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    service
        .delete(
            authentication.principal(),
            provider_key,
            request.expected_version,
        )
        .await
        .map_err(map_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteModelProviderRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

fn map_error(error: AdminModelProviderError) -> ManagementError {
    match error {
        AdminModelProviderError::InvalidInput => ManagementError::InvalidRequest,
        AdminModelProviderError::Forbidden => ManagementError::Forbidden,
        AdminModelProviderError::NotFound => ManagementError::ModelProviderNotFound,
        AdminModelProviderError::Conflict => ManagementError::ModelProviderConflict,
        AdminModelProviderError::Internal => ManagementError::Internal,
    }
}
