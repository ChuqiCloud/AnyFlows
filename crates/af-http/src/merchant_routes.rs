use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};
use af_admin::{
    MerchantRouteService, MerchantRouteWrite, SessionAuthentication, SessionAuthenticator,
    UserRouteError,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone)]
struct MerchantRouteHttpState {
    service: Arc<dyn MerchantRouteService>,
}
pub fn build_merchant_route_router(
    service: Arc<dyn MerchantRouteService>,
    auth: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route("/api/merchant-pool/routes", get(list).post(create))
        .route(
            "/api/merchant-pool/routes/preview",
            axum::routing::post(preview),
        )
        .route(
            "/api/merchant-pool/routes/{id}",
            get(read).put(update).delete(remove),
        )
        .route("/api/merchant-pool/attempts", get(attempts))
        .route("/api/merchant/statistics", get(own_statistics))
        .route("/api/merchant-pools/{pool_id}/statistics", get(statistics))
        .route("/api/merchant-pools/{pool_id}/timeline", get(timeline))
        .layer(DefaultBodyLimit::max(32 * 1024))
        .layer(tower_http::limit::RequestBodyLimitLayer::new(32 * 1024))
        .layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(auth),
            authenticate_management_session,
        ))
        .with_state(MerchantRouteHttpState { service })
}
#[derive(Serialize)]
struct RouteList {
    items: Vec<af_admin::MerchantRouteRecord>,
    capacity: usize,
}
async fn preview(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    body: Result<Json<af_admin::MerchantRoutePreviewRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = body.map_err(|_| ManagementError::InvalidRequest)?;
    Ok(no_store_json(
        s.service
            .preview(a.principal(), request)
            .await
            .map_err(map_error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AttemptQuery {
    request_id: String,
}
#[derive(Serialize)]
struct AttemptList {
    items: Vec<af_admin::MerchantConsumerAttempt>,
}
async fn attempts(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    Query(q): Query<AttemptQuery>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(AttemptList {
        items: s
            .service
            .attempts(a.principal(), q.request_id)
            .await
            .map_err(map_error)?,
    }))
}
async fn list(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(RouteList {
        items: s.service.list(a.principal()).await.map_err(map_error)?,
        capacity: af_domain::MAX_USER_ROUTES,
    }))
}
async fn read(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, ManagementError> {
    Ok(no_store_json(
        s.service.get(a.principal(), id).await.map_err(map_error)?,
    ))
}
async fn create(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    body: Result<Json<MerchantRouteWrite>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(write) = body.map_err(|_| ManagementError::InvalidRequest)?;
    let mut response = no_store_json(
        s.service
            .save(a.principal(), None, write)
            .await
            .map_err(map_error)?,
    );
    *response.status_mut() = http::StatusCode::CREATED;
    Ok(response)
}
async fn update(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
    body: Result<Json<MerchantRouteWrite>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(write) = body.map_err(|_| ManagementError::InvalidRequest)?;
    Ok(no_store_json(
        s.service
            .save(a.principal(), Some(id), write)
            .await
            .map_err(map_error)?,
    ))
}
async fn remove(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<http::StatusCode, ManagementError> {
    s.service
        .delete(a.principal(), id)
        .await
        .map_err(map_error)?;
    Ok(http::StatusCode::NO_CONTENT)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatisticsQuery {
    model: Option<String>,
    window_seconds: Option<u32>,
    protocol: Option<String>,
    streaming: Option<bool>,
}
#[derive(Serialize)]
struct StatisticsResponse {
    items: Vec<af_admin::MerchantServiceStatistics>,
    as_of: i64,
    window_seconds: u32,
}
async fn own_statistics(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    Query(q): Query<StatisticsQuery>,
) -> Result<Response, ManagementError> {
    let window_seconds = q.window_seconds.unwrap_or(86400);
    let items = s
        .service
        .own_statistics(
            a.principal(),
            q.model,
            window_seconds,
            q.protocol,
            q.streaming,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(StatisticsResponse {
        items,
        as_of: time::OffsetDateTime::now_utc().unix_timestamp(),
        window_seconds,
    }))
}
#[derive(Serialize)]
struct TimelineResponse {
    items: Vec<af_admin::MerchantServiceTimeline>,
    as_of: i64,
    window_seconds: u32,
}
async fn timeline(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    Path(pool_id): Path<i64>,
    Query(q): Query<StatisticsQuery>,
) -> Result<Response, ManagementError> {
    let window_seconds = q.window_seconds.unwrap_or(86400);
    let items = s
        .service
        .timeline(
            a.principal(),
            pool_id,
            q.model,
            window_seconds,
            q.protocol,
            q.streaming,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(TimelineResponse {
        items,
        as_of: time::OffsetDateTime::now_utc().unix_timestamp(),
        window_seconds,
    }))
}
async fn statistics(
    State(s): State<MerchantRouteHttpState>,
    Extension(a): Extension<SessionAuthentication>,
    Path(pool_id): Path<i64>,
    Query(q): Query<StatisticsQuery>,
) -> Result<Response, ManagementError> {
    let window_seconds = q.window_seconds.unwrap_or(86400);
    let items = s
        .service
        .statistics(
            a.principal(),
            pool_id,
            q.model,
            window_seconds,
            q.protocol,
            q.streaming,
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(StatisticsResponse {
        items,
        as_of: time::OffsetDateTime::now_utc().unix_timestamp(),
        window_seconds,
    }))
}
fn map_error(error: UserRouteError) -> ManagementError {
    match error {
        UserRouteError::InvalidInput => ManagementError::InvalidRequest,
        UserRouteError::NotFound => ManagementError::UserRouteNotFound,
        UserRouteError::InvalidSession => ManagementError::InvalidSession,
        UserRouteError::LimitReached => ManagementError::UserRouteLimitReached,
        UserRouteError::Unavailable => ManagementError::UserRouteUnavailable,
        UserRouteError::Internal => ManagementError::Internal,
    }
}
