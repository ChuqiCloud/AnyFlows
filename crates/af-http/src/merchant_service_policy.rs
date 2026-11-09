use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_session::no_store_json,
    merchant_pool::{MerchantHttpError, body, created, id},
};
use af_admin::{MerchantServicePolicy, SessionAuthentication, SessionAuthenticator};
use af_domain::{MerchantListQuery, MerchantMaintenanceWrite, MerchantServicePriceWrite};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post, put},
};
use std::sync::Arc;
use tower_http::limit::RequestBodyLimitLayer;

#[derive(Clone)]
struct PolicyState {
    service: Arc<dyn MerchantServicePolicy>,
    administrative: bool,
}

pub fn build_merchant_service_policy_router(
    service: Arc<dyn MerchantServicePolicy>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(authenticator),
        authenticate_management_session,
    );
    policy_routes("/api/merchant/services", service.clone(), false)
        .merge(policy_routes("/api/admin/merchant-services", service, true))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(RequestBodyLimitLayer::new(64 * 1024))
        .layer(authentication)
}

fn policy_routes(
    prefix: &str,
    service: Arc<dyn MerchantServicePolicy>,
    administrative: bool,
) -> Router {
    Router::new()
        .route(
            &format!("{prefix}/{{id}}/prices"),
            get(prices).post(schedule_price),
        )
        .route(
            &format!("{prefix}/{{id}}/prices/{{price}}/cancel"),
            post(cancel_price),
        )
        .route(
            &format!("{prefix}/{{id}}/maintenance"),
            get(maintenance).post(create_maintenance),
        )
        .route(
            &format!("{prefix}/{{id}}/maintenance/{{notice}}"),
            put(update_maintenance),
        )
        .route(
            &format!("{prefix}/{{id}}/maintenance/{{notice}}/cancel"),
            post(cancel_maintenance),
        )
        .with_state(PolicyState {
            service,
            administrative,
        })
}

async fn prices(
    State(s): State<PolicyState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    Query(query): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .prices(auth.principal(), s.administrative, id(service)?, query)
            .await?,
    ))
}
async fn schedule_price(
    State(s): State<PolicyState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    request: Result<Json<MerchantServicePriceWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(created(
        s.service
            .schedule_price(
                auth.principal(),
                s.administrative,
                id(service)?,
                body(request)?,
            )
            .await?,
    ))
}
async fn cancel_price(
    State(s): State<PolicyState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((service, price)): Path<(i64, i64)>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .cancel_price(auth.principal(), s.administrative, id(service)?, id(price)?)
            .await?,
    ))
}
async fn maintenance(
    State(s): State<PolicyState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    Query(query): Query<MerchantListQuery>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .maintenance(auth.principal(), s.administrative, id(service)?, query)
            .await?,
    ))
}
async fn create_maintenance(
    State(s): State<PolicyState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(service): Path<i64>,
    request: Result<Json<MerchantMaintenanceWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(created(
        s.service
            .write_maintenance(
                auth.principal(),
                s.administrative,
                id(service)?,
                None,
                body(request)?,
            )
            .await?,
    ))
}
async fn update_maintenance(
    State(s): State<PolicyState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((service, notice)): Path<(i64, i64)>,
    request: Result<Json<MerchantMaintenanceWrite>, JsonRejection>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .write_maintenance(
                auth.principal(),
                s.administrative,
                id(service)?,
                Some(id(notice)?),
                body(request)?,
            )
            .await?,
    ))
}
async fn cancel_maintenance(
    State(s): State<PolicyState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path((service, notice)): Path<(i64, i64)>,
) -> Result<Response, MerchantHttpError> {
    Ok(no_store_json(
        s.service
            .cancel_maintenance(
                auth.principal(),
                s.administrative,
                id(service)?,
                id(notice)?,
            )
            .await?,
    ))
}
