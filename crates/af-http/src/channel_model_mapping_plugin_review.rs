//! Administrator-only static review of a channel's public model mapping.

use std::sync::Arc;

use af_admin::{
    ChannelModelMappingPluginReview, ChannelModelMappingPluginReviewCheck,
    ChannelModelMappingPluginReviewError, ChannelModelMappingPluginReviewService,
    ChannelModelMappingPluginReviewStatus, SessionAuthentication, SessionAuthenticator,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct ChannelModelMappingPluginReviewState {
    service: Arc<dyn ChannelModelMappingPluginReviewService>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChannelModelMappingPluginReviewRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct ChannelModelMappingPluginReviewerResponse {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
    #[schema(min_length = 1, max_length = 128)]
    pub display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    pub plugin_version: String,
}

#[derive(Serialize, ToSchema)]
pub struct ChannelModelMappingPluginReviewerListResponse {
    #[schema(max_items = 100)]
    pub plugins: Vec<ChannelModelMappingPluginReviewerResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChannelModelMappingPluginReviewStatusResponse {
    Pass,
    Info,
    Warning,
    Fail,
}

#[derive(Serialize, ToSchema)]
pub struct ChannelModelMappingPluginReviewCheckResponse {
    #[schema(min_length = 1, max_length = 64)]
    pub code: String,
    #[schema(min_length = 1, max_length = 128)]
    pub title: String,
    pub status: ChannelModelMappingPluginReviewStatusResponse,
    #[schema(min_length = 1, max_length = 2048)]
    pub message: String,
    #[schema(required = true, min_length = 1, max_length = 2048)]
    pub recommendation: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ChannelModelMappingPluginReviewResponse {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
    #[schema(min_length = 1, max_length = 128)]
    pub display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    pub plugin_version: String,
    #[schema(minimum = 0)]
    pub model_count: u32,
    #[schema(minimum = 0)]
    pub mapping_count: u32,
    #[schema(min_length = 1, max_length = 2048)]
    pub summary: String,
    #[schema(max_items = 64)]
    pub checks: Vec<ChannelModelMappingPluginReviewCheckResponse>,
}

impl From<ChannelModelMappingPluginReviewCheck> for ChannelModelMappingPluginReviewCheckResponse {
    fn from(value: ChannelModelMappingPluginReviewCheck) -> Self {
        Self {
            code: value.code,
            title: value.title,
            status: match value.status {
                ChannelModelMappingPluginReviewStatus::Pass => {
                    ChannelModelMappingPluginReviewStatusResponse::Pass
                }
                ChannelModelMappingPluginReviewStatus::Info => {
                    ChannelModelMappingPluginReviewStatusResponse::Info
                }
                ChannelModelMappingPluginReviewStatus::Warning => {
                    ChannelModelMappingPluginReviewStatusResponse::Warning
                }
                ChannelModelMappingPluginReviewStatus::Fail => {
                    ChannelModelMappingPluginReviewStatusResponse::Fail
                }
            },
            message: value.message,
            recommendation: value.recommendation,
        }
    }
}

impl From<ChannelModelMappingPluginReview> for ChannelModelMappingPluginReviewResponse {
    fn from(value: ChannelModelMappingPluginReview) -> Self {
        Self {
            plugin_id: value.plugin_id,
            display_name: value.display_name,
            plugin_version: value.plugin_version,
            model_count: value.model_count,
            mapping_count: value.mapping_count,
            summary: value.summary,
            checks: value.checks.into_iter().map(Into::into).collect(),
        }
    }
}

pub fn build_channel_model_mapping_plugin_review_router(
    auth: Arc<dyn SessionAuthenticator>,
    service: Arc<dyn ChannelModelMappingPluginReviewService>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(auth),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/channel-model-mapping/plugin-reviewers",
            get(list_channel_model_mapping_plugin_reviewers),
        )
        .route(
            "/api/admin/channels/{channel_id}/model-mapping/plugin-review",
            post(review_admin_channel_model_mapping_with_plugin),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(ChannelModelMappingPluginReviewState { service })
}

fn map_plugin_review_error(error: ChannelModelMappingPluginReviewError) -> ManagementError {
    match error {
        ChannelModelMappingPluginReviewError::InvalidInput => {
            ManagementError::PluginLifecycleInvalidRequest
        }
        ChannelModelMappingPluginReviewError::NotFound => ManagementError::PluginLifecycleNotFound,
        ChannelModelMappingPluginReviewError::Forbidden => ManagementError::Forbidden,
        ChannelModelMappingPluginReviewError::Conflict => ManagementError::PluginLifecycleConflict,
        ChannelModelMappingPluginReviewError::Unavailable => {
            ManagementError::PluginLifecycleUnavailable
        }
    }
}

#[utoipa::path(get,path="/api/admin/channel-model-mapping/plugin-reviewers",operation_id="listAdminChannelModelMappingPluginReviewers",summary="列出渠道模型映射检查插件",responses((status=200,body=ChannelModelMappingPluginReviewerListResponse)),security(("bearerAuth" = []))) ]
async fn list_channel_model_mapping_plugin_reviewers(
    State(state): State<ChannelModelMappingPluginReviewState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let plugins = state
        .service
        .reviewers(authentication.principal())
        .await
        .map_err(map_plugin_review_error)?
        .plugins
        .into_iter()
        .map(|value| ChannelModelMappingPluginReviewerResponse {
            plugin_id: value.plugin_id,
            display_name: value.display_name,
            plugin_version: value.plugin_version,
        })
        .collect();
    Ok(no_store_json(
        ChannelModelMappingPluginReviewerListResponse { plugins },
    ))
}

#[utoipa::path(post,path="/api/admin/channels/{channel_id}/model-mapping/plugin-review",operation_id="reviewAdminChannelModelMappingWithPlugin",summary="静态检查渠道模型映射",params(("channel_id"=i64,Path,minimum=1)),request_body=ChannelModelMappingPluginReviewRequest,responses((status=200,body=ChannelModelMappingPluginReviewResponse)),security(("bearerAuth" = []))) ]
async fn review_admin_channel_model_mapping_with_plugin(
    State(state): State<ChannelModelMappingPluginReviewState>,
    Path(channel_id): Path<i64>,
    Extension(request_id): Extension<af_telemetry::RequestId>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<ChannelModelMappingPluginReviewRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    if channel_id <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let review = state
        .service
        .review_mappings(
            authentication.principal(),
            channel_id,
            request.plugin_id,
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_plugin_review_error)?;
    Ok(no_store_json(
        ChannelModelMappingPluginReviewResponse::from(review),
    ))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_channel_model_mapping_plugin_reviewers,
        review_admin_channel_model_mapping_with_plugin
    ),
    components(schemas(
        ChannelModelMappingPluginReviewRequest,
        ChannelModelMappingPluginReviewerResponse,
        ChannelModelMappingPluginReviewerListResponse,
        ChannelModelMappingPluginReviewStatusResponse,
        ChannelModelMappingPluginReviewCheckResponse,
        ChannelModelMappingPluginReviewResponse
    ))
)]
pub struct ChannelModelMappingPluginReviewApiDoc;

pub fn channel_model_mapping_plugin_review_openapi_document() -> utoipa::openapi::OpenApi {
    ChannelModelMappingPluginReviewApiDoc::openapi()
}

#[cfg(test)]
mod tests {
    use super::channel_model_mapping_plugin_review_openapi_document;

    #[test]
    fn model_mapping_plugin_review_routes_are_explicitly_admin_only_and_typed() {
        let document =
            serde_json::to_value(channel_model_mapping_plugin_review_openapi_document()).unwrap();
        let list = &document["paths"]["/api/admin/channel-model-mapping/plugin-reviewers"]["get"];
        assert_eq!(
            list["operationId"],
            "listAdminChannelModelMappingPluginReviewers"
        );
        assert_eq!(list["security"][0]["bearerAuth"], serde_json::json!([]));

        let review = &document["paths"]["/api/admin/channels/{channel_id}/model-mapping/plugin-review"]
            ["post"];
        assert_eq!(
            review["operationId"],
            "reviewAdminChannelModelMappingWithPlugin"
        );
        assert_eq!(review["security"][0]["bearerAuth"], serde_json::json!([]));
        assert_eq!(
            review["requestBody"]["content"]["application/json"]["schema"]["$ref"],
            "#/components/schemas/ChannelModelMappingPluginReviewRequest"
        );
    }
}
