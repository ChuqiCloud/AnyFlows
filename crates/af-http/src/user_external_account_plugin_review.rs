//! User-selected plugin review of already stored external account balance snapshots.

use std::sync::Arc;

use af_admin::{
    ExternalAccountSnapshotPluginReview, ExternalAccountSnapshotPluginReviewCheck,
    ExternalAccountSnapshotPluginReviewError, ExternalAccountSnapshotPluginReviewService,
    ExternalAccountSnapshotPluginReviewStatus, PluginUiView, SessionAuthentication,
    SessionAuthenticator,
};
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Path, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::{OpenApi, ToSchema};

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct StateData {
    service: Arc<dyn ExternalAccountSnapshotPluginReviewService>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub plugin_id: String,
    /// Explicit, per-request permission for this plugin to read the linked
    /// upstream account through the host's fixed, read-only connector endpoints.
    /// PAT requests also need the separate `external_accounts:live_read` scope.
    #[serde(default)]
    pub allow_external_reads: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerResponse {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub supports_external_reads: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewerListResponse {
    pub plugins: Vec<ReviewerResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatusResponse {
    Pass,
    Info,
    Warning,
    Fail,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCheckResponse {
    pub code: String,
    pub title: String,
    pub status: ReviewStatusResponse,
    pub message: String,
    pub recommendation: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResponse {
    pub plugin_id: String,
    pub display_name: String,
    pub plugin_version: String,
    pub snapshot_count: u32,
    pub first_observed_at: i64,
    pub last_observed_at: i64,
    pub summary: String,
    pub checks: Vec<ReviewCheckResponse>,
    #[schema(value_type = Vec<serde_json::Value>, max_items = 8)]
    pub ui_views: Vec<PluginUiView>,
}

impl From<ExternalAccountSnapshotPluginReviewCheck> for ReviewCheckResponse {
    fn from(value: ExternalAccountSnapshotPluginReviewCheck) -> Self {
        Self {
            code: value.code,
            title: value.title,
            status: match value.status {
                ExternalAccountSnapshotPluginReviewStatus::Pass => ReviewStatusResponse::Pass,
                ExternalAccountSnapshotPluginReviewStatus::Info => ReviewStatusResponse::Info,
                ExternalAccountSnapshotPluginReviewStatus::Warning => ReviewStatusResponse::Warning,
                ExternalAccountSnapshotPluginReviewStatus::Fail => ReviewStatusResponse::Fail,
            },
            message: value.message,
            recommendation: value.recommendation,
        }
    }
}

impl From<ExternalAccountSnapshotPluginReview> for ReviewResponse {
    fn from(value: ExternalAccountSnapshotPluginReview) -> Self {
        Self {
            plugin_id: value.plugin_id,
            display_name: value.display_name,
            plugin_version: value.plugin_version,
            snapshot_count: value.snapshot_count,
            first_observed_at: value.first_observed_at,
            last_observed_at: value.last_observed_at,
            summary: value.summary,
            checks: value.checks.into_iter().map(Into::into).collect(),
            ui_views: value.ui_views,
        }
    }
}

pub fn build_user_external_account_plugin_review_router(
    service: Arc<dyn ExternalAccountSnapshotPluginReviewService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/account/external-accounts/plugin-reviewers",
            get(list_reviewers),
        )
        .route(
            "/api/account/external-accounts/{id}/plugin-review",
            post(review_snapshots),
        )
        .layer(DefaultBodyLimit::max(4 * 1024))
        .layer(RequestBodyLimitLayer::new(4 * 1024))
        .route_layer(authentication)
        .with_state(StateData { service })
}

fn map_error(error: ExternalAccountSnapshotPluginReviewError) -> ManagementError {
    match error {
        ExternalAccountSnapshotPluginReviewError::InvalidInput => ManagementError::InvalidRequest,
        ExternalAccountSnapshotPluginReviewError::NotFound
        | ExternalAccountSnapshotPluginReviewError::NoSnapshots => {
            ManagementError::ExternalAccountNotFound
        }
        ExternalAccountSnapshotPluginReviewError::Forbidden => ManagementError::Forbidden,
        ExternalAccountSnapshotPluginReviewError::Conflict => {
            ManagementError::ExternalAccountConflict
        }
        ExternalAccountSnapshotPluginReviewError::Unavailable => {
            ManagementError::ExternalAccountUnavailable
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/account/external-accounts/plugin-reviewers",
    operation_id = "listUserExternalAccountSnapshotReviewers",
    tag = "个人资料与安全",
    summary = "列出可用于余额快照分析的插件",
    responses((status = 200, body = ReviewerListResponse), (status = 401, description = "会话无效")),
    security(("bearerAuth" = []))
)]
async fn list_reviewers(
    State(state): State<StateData>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let result = state
        .service
        .reviewers(authentication.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ReviewerListResponse {
        plugins: result
            .plugins
            .into_iter()
            .map(|value| ReviewerResponse {
                plugin_id: value.plugin_id,
                display_name: value.display_name,
                plugin_version: value.plugin_version,
                supports_external_reads: value.supports_external_reads,
            })
            .collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/account/external-accounts/{id}/plugin-review",
    operation_id = "reviewUserExternalAccountBalanceSnapshots",
    tag = "个人资料与安全",
    summary = "使用所选插件分析已存余额快照",
    params(("id" = i64, Path, minimum = 1, description = "当前用户外部账户连接 ID")),
    request_body = ReviewRequest,
    responses((status = 200, body = ReviewResponse), (status = 401, description = "会话无效"), (status = 404, description = "账户或余额快照不存在")),
    security(("bearerAuth" = []))
)]
async fn review_snapshots(
    State(state): State<StateData>,
    Path(connection_id): Path<i64>,
    Extension(request_id): Extension<af_telemetry::RequestId>,
    Extension(authentication): Extension<SessionAuthentication>,
    access_token: Option<Extension<af_admin::PersonalAccessTokenAuthentication>>,
    request: Result<Json<ReviewRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    if connection_id <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    if request.allow_external_reads
        && access_token
            .as_ref()
            .is_some_and(|Extension(token)| !token_has_live_external_read_scope(token.scopes()))
    {
        return Err(ManagementError::Forbidden);
    }
    let review = state
        .service
        .review_snapshots(
            authentication.principal(),
            connection_id,
            request.plugin_id,
            request_id.as_str().to_owned(),
            request.allow_external_reads,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ReviewResponse::from(review)))
}

fn token_has_live_external_read_scope(scopes: &[String]) -> bool {
    scopes
        .iter()
        .any(|scope| scope == "external_accounts:live_read")
}

#[derive(OpenApi)]
#[openapi(
    paths(list_reviewers, review_snapshots),
    components(schemas(
        ReviewRequest,
        ReviewerResponse,
        ReviewerListResponse,
        ReviewStatusResponse,
        ReviewCheckResponse,
        ReviewResponse
    ))
)]
struct UserExternalAccountPluginReviewApi;

pub fn user_external_account_plugin_review_openapi_document() -> utoipa::openapi::OpenApi {
    UserExternalAccountPluginReviewApi::openapi()
}

#[cfg(test)]
mod tests {
    use super::{
        token_has_live_external_read_scope, user_external_account_plugin_review_openapi_document,
    };

    #[test]
    fn external_account_plugin_review_contract_exposes_bounded_static_views() {
        let document =
            serde_json::to_value(user_external_account_plugin_review_openapi_document()).unwrap();
        assert_eq!(
            document["components"]["schemas"]["ReviewResponse"]["properties"]["uiViews"]["maxItems"],
            8
        );
        let request = &document["components"]["schemas"]["ReviewRequest"];
        assert_eq!(
            request["properties"]["allow_external_reads"]["type"],
            "boolean"
        );
        assert!(
            request["properties"]["allow_external_reads"]["description"]
                .as_str()
                .is_some_and(|description| { description.contains("external_accounts:live_read") })
        );
    }

    #[test]
    fn live_external_reads_need_the_separate_pat_scope() {
        assert!(token_has_live_external_read_scope(&[
            "external_accounts:read".to_owned(),
            "external_accounts:live_read".to_owned(),
        ]));
        assert!(!token_has_live_external_read_scope(&[
            "external_accounts:read".to_owned(),
        ]));
    }
}
