use std::sync::Arc;

use af_admin::{
    MerchantServiceWatch, MerchantServiceWatchError, MerchantServiceWatchPage,
    MerchantServiceWatchService, SessionAuthentication, SessionAuthenticator,
};
use axum::{
    Extension, Router,
    extract::{Path, State},
    middleware,
    response::Response,
    routing::{get, put},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct StateData {
    service: Arc<dyn MerchantServiceWatchService>,
}

/// Build authenticated operations for watching public merchant services.
pub fn build_merchant_service_watch_router(
    service: Arc<dyn MerchantServiceWatchService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/merchant-pool/watches", get(list_watches))
        .route(
            "/api/merchant-pools/{pool}/services/{service}/watch",
            put(watch_service).delete(unwatch_service),
        )
        .layer(authentication)
        .with_state(StateData { service })
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MerchantServiceWatchResponse {
    pool_id: i64,
    pool_name: String,
    service_id: i64,
    merchant_id: i64,
    service_name: String,
    merchant_name: String,
    active: bool,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MerchantServiceWatchListResponse {
    items: Vec<MerchantServiceWatchResponse>,
    total: u64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MerchantServiceWatchChangeResponse {
    changed: bool,
}

async fn list_watches(
    State(state): State<StateData>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let page = state
        .service
        .list(authentication.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(MerchantServiceWatchListResponse::from_page(
        page,
    )))
}

async fn watch_service(
    State(state): State<StateData>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((pool, service)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    if pool <= 0 || service <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    let watch = state
        .service
        .watch(authentication.principal(), pool, service)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(MerchantServiceWatchResponse::from_watch(
        watch,
    )))
}

async fn unwatch_service(
    State(state): State<StateData>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((pool, service)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    if pool <= 0 || service <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    let changed = state
        .service
        .unwatch(authentication.principal(), pool, service)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(MerchantServiceWatchChangeResponse {
        changed,
    }))
}

impl MerchantServiceWatchResponse {
    fn from_watch(watch: MerchantServiceWatch) -> Self {
        Self {
            pool_id: watch.pool_id,
            pool_name: watch.pool_name,
            service_id: watch.service_id,
            merchant_id: watch.merchant_id,
            service_name: watch.service_name,
            merchant_name: watch.merchant_name,
            active: watch.active,
            created_at: watch.created_at,
        }
    }
}

impl MerchantServiceWatchListResponse {
    fn from_page(page: MerchantServiceWatchPage) -> Self {
        Self {
            items: page
                .items
                .into_iter()
                .map(MerchantServiceWatchResponse::from_watch)
                .collect(),
            total: page.total,
        }
    }
}

fn map_error(error: MerchantServiceWatchError) -> ManagementError {
    match error {
        MerchantServiceWatchError::InvalidInput => ManagementError::InvalidRequest,
        MerchantServiceWatchError::NotFound => ManagementError::MerchantServiceWatchNotFound,
        MerchantServiceWatchError::LimitReached => {
            ManagementError::MerchantServiceWatchLimitReached
        }
        MerchantServiceWatchError::Internal => ManagementError::MerchantServiceWatchUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_response_contains_only_public_service_identity() {
        let response = MerchantServiceWatchResponse::from_watch(MerchantServiceWatch {
            pool_id: 2,
            pool_name: "Public pool".to_owned(),
            service_id: 7,
            merchant_id: 4,
            service_name: "Service".to_owned(),
            merchant_name: "Merchant".to_owned(),
            active: true,
            created_at: 100,
        });
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["pool_id"], 2);
        assert_eq!(value["service_id"], 7);
        assert!(value.get("api_key").is_none());
        assert!(value.get("base_url").is_none());
    }
}
