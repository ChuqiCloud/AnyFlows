use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_session::{no_store_empty, no_store_json},
};
use af_admin::{MerchantPoolService, SessionAuthentication, SessionAuthenticator};
use af_domain::{
    MerchantApplication, MerchantListQuery, MerchantPoolError, MerchantPoolSettings,
    MerchantPoolTerms, MerchantPoolWrite, MerchantReview, MerchantServiceWrite,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::limit::RequestBodyLimitLayer;

#[derive(Clone)]
struct MerchantHttpState {
    service: Arc<dyn MerchantPoolService>,
}

/// Every operation is authenticated; role and ownership are checked in the service.
pub fn build_merchant_pool_router(
    service: Arc<dyn MerchantPoolService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/merchant-pools", get(list_pools))
        .route("/api/merchant-pools/{pool}/services", get(catalog))
        .route(
            "/api/merchant-pools/{pool}/services/{id}",
            get(catalog_service),
        )
        .route("/api/merchant/profile", get(profile))
        .route("/api/merchant/apply", axum::routing::post(apply))
        .route("/api/merchant/quote", axum::routing::post(quote))
        .route("/api/merchant/available-pools", get(available_pools))
        .route(
            "/api/merchant/services",
            get(owned_services).post(create_service),
        )
        .route(
            "/api/merchant/services/{id}",
            get(owned_service).put(update_service),
        )
        .route(
            "/api/merchant/services/{id}/submit",
            axum::routing::post(submit),
        )
        .route(
            "/api/merchant/services/{id}/pause",
            axum::routing::post(pause),
        )
        .route(
            "/api/merchant/services/{id}/key",
            axum::routing::post(rotate_key),
        )
        .route(
            "/api/admin/merchant-pools",
            get(admin_pools).post(create_pool),
        )
        .route(
            "/api/admin/merchant-pools/{id}",
            axum::routing::put(update_pool).delete(delete_pool),
        )
        .route("/api/admin/merchants", get(admin_merchants))
        .route(
            "/api/admin/merchants/{id}",
            axum::routing::delete(delete_merchant),
        )
        .route(
            "/api/admin/merchants/{id}/review",
            axum::routing::post(review_merchant),
        )
        .route("/api/admin/merchant-services", get(admin_services))
        .route("/api/admin/merchant-services/{id}", get(admin_service))
        .route(
            "/api/admin/merchant-services/{id}/resume",
            axum::routing::post(resume_service),
        )
        .route(
            "/api/admin/merchant-services/{id}/review",
            axum::routing::post(review_service),
        )
        .route(
            "/api/admin/merchant-pool-settings",
            get(settings).put(write_settings),
        )
        .route(
            "/api/admin/merchant-pools/{pool}/merchants/{merchant}/terms",
            get(terms).put(write_terms),
        )
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(RequestBodyLimitLayer::new(256 * 1024))
        .layer(authentication)
        .with_state(MerchantHttpState { service })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PauseWrite {
    paused: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyWrite {
    api_key: String,
}

pub(crate) struct MerchantHttpError(MerchantPoolError);
impl From<MerchantPoolError> for MerchantHttpError {
    fn from(value: MerchantPoolError) -> Self {
        Self(value)
    }
}
impl IntoResponse for MerchantHttpError {
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            MerchantPoolError::InvalidInput => {
                (StatusCode::BAD_REQUEST, "merchant_pool_invalid_request")
            }
            MerchantPoolError::Forbidden => (StatusCode::FORBIDDEN, "merchant_pool_forbidden"),
            MerchantPoolError::NotFound => (StatusCode::NOT_FOUND, "merchant_pool_not_found"),
            MerchantPoolError::Conflict => (StatusCode::CONFLICT, "merchant_pool_conflict"),
            MerchantPoolError::InUse => (StatusCode::CONFLICT, "merchant_pool_in_use"),
            MerchantPoolError::LimitReached => {
                (StatusCode::CONFLICT, "merchant_pool_limit_reached")
            }
            MerchantPoolError::Unavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, "merchant_pool_unavailable")
            }
        };
        let mut response =
            no_store_json(serde_json::json!({"error":{"code":code,"message":self.0.to_string()}}));
        *response.status_mut() = status;
        response
    }
}
pub(crate) fn body<T>(request: Result<Json<T>, JsonRejection>) -> Result<T, MerchantHttpError> {
    request
        .map(|Json(value)| value)
        .map_err(|_| MerchantPoolError::InvalidInput.into())
}
pub(crate) fn id(value: i64) -> Result<i64, MerchantHttpError> {
    if value > 0 {
        Ok(value)
    } else {
        Err(MerchantPoolError::InvalidInput.into())
    }
}
pub(crate) fn created<T: Serialize>(value: T) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = StatusCode::CREATED;
    response
}

async fn list_pools(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(q): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(s.service.pools(auth.principal(), q).await?))
}
async fn catalog(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(pool): Path<i64>,
    Query(q): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.catalog(auth.principal(), id(pool)?, q).await?,
    ))
}
async fn catalog_service(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((pool, service)): Path<(i64, i64)>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .catalog_service(auth.principal(), id(pool)?, id(service)?)
            .await?,
    ))
}
async fn profile(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(s.service.profile(auth.principal()).await?))
}
async fn apply(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    request: Result<Json<MerchantApplication>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(created(
        s.service.apply(auth.principal(), body(request)?).await?,
    ))
}
async fn quote(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    request: Result<Json<af_domain::MerchantQuoteWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.quote(auth.principal(), body(request)?).await?,
    ))
}
async fn available_pools(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(q): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.available_pools(auth.principal(), q).await?,
    ))
}
async fn owned_services(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(q): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.services(auth.principal(), q).await?,
    ))
}
async fn owned_service(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.service(auth.principal(), id(service)?).await?,
    ))
}
async fn create_service(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    request: Result<Json<MerchantServiceWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(created(
        s.service
            .write_service(auth.principal(), None, body(request)?)
            .await?,
    ))
}
async fn update_service(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    request: Result<Json<MerchantServiceWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .write_service(auth.principal(), Some(id(service)?), body(request)?)
            .await?,
    ))
}
async fn submit(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .submit_service(auth.principal(), id(service)?)
            .await?,
    ))
}
async fn pause(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    request: Result<Json<PauseWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .pause_service(auth.principal(), id(service)?, body(request)?.paused)
            .await?,
    ))
}
async fn rotate_key(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    request: Result<Json<KeyWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .rotate_service_key(auth.principal(), id(service)?, body(request)?.api_key)
            .await?,
    ))
}
async fn admin_pools(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(q): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.admin_pools(auth.principal(), q).await?,
    ))
}
async fn create_pool(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    request: Result<Json<MerchantPoolWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(created(
        s.service
            .write_pool(auth.principal(), None, body(request)?)
            .await?,
    ))
}
async fn update_pool(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(pool): Path<i64>,
    request: Result<Json<MerchantPoolWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .write_pool(auth.principal(), Some(id(pool)?), body(request)?)
            .await?,
    ))
}
async fn delete_pool(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(pool): Path<i64>,
) -> Result<Response, MerchantHttpError> {
    s.service.delete_pool(auth.principal(), id(pool)?).await?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}
async fn delete_merchant(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(merchant): Path<i64>,
) -> Result<Response, MerchantHttpError> {
    s.service
        .delete_merchant(auth.principal(), id(merchant)?)
        .await?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}
async fn admin_merchants(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(q): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.admin_merchants(auth.principal(), q).await?,
    ))
}
async fn review_merchant(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(merchant): Path<i64>,
    request: Result<Json<MerchantReview>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .review_merchant(auth.principal(), id(merchant)?, body(request)?)
            .await?,
    ))
}
async fn admin_services(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(q): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service.admin_services(auth.principal(), q).await?,
    ))
}
async fn admin_service(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .admin_service(auth.principal(), id(service)?)
            .await?,
    ))
}
async fn review_service(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    request: Result<Json<MerchantReview>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .review_service(auth.principal(), id(service)?, body(request)?)
            .await?,
    ))
}
async fn resume_service(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .resume_service(auth.principal(), id(service)?)
            .await?,
    ))
}
async fn settings(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(s.service.settings(auth.principal()).await?))
}
async fn write_settings(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    request: Result<Json<MerchantPoolSettings>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .write_settings(auth.principal(), body(request)?)
            .await?,
    ))
}
async fn terms(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((pool, merchant)): Path<(i64, i64)>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .terms(auth.principal(), id(pool)?, id(merchant)?)
            .await?,
    ))
}
async fn write_terms(
    State(s): State<MerchantHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((pool, merchant)): Path<(i64, i64)>,
    request: Result<Json<MerchantPoolTerms>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .write_terms(auth.principal(), id(pool)?, id(merchant)?, body(request)?)
            .await?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn merchant_deletion_in_use_is_a_distinct_conflict() {
        let response = MerchantHttpError(MerchantPoolError::InUse).into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["error"]["code"], "merchant_pool_in_use");
    }
}
