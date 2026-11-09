use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_session::no_store_json,
    merchant_finance::FinanceHttpError,
};
use af_admin::{
    DatabaseMerchantPayoutService, MerchantPayoutAccountCommand, MerchantPayoutSettingsCommand,
    SessionAuthentication, SessionAuthenticator,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantPayout)]
pub(crate) struct PayoutSchema {
    withdrawal_id: i64,
    merchant_id: i64,
    event_key: String,
    status: String,
    amount_minor: i64,
    currency: String,
    reference: String,
    reason: Option<String>,
    created_at: i64,
    updated_at: i64,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantPayoutPage)]
struct PayoutPageSchema {
    items: Vec<PayoutSchema>,
    next_cursor: Option<i64>,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantPayoutSettings)]
struct SettingsSchema {
    version: i64,
    enabled: bool,
    auto_approve_enabled: bool,
    gateway_url: String,
    currency: String,
    quota_per_unit: i64,
    min_amount_minor: i64,
    max_amount_minor: i64,
    daily_limit_minor: i64,
    signing_key_configured: bool,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantPayoutSettingsCommand)]
struct SettingsCommandSchema {
    expected_version: i64,
    enabled: bool,
    auto_approve_enabled: bool,
    gateway_url: String,
    currency: String,
    quota_per_unit: i64,
    min_amount_minor: i64,
    max_amount_minor: i64,
    daily_limit_minor: i64,
    #[schema(write_only)]
    signing_key: Option<String>,
    clear_signing_key: Option<bool>,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantPayoutAccount)]
struct AccountSchema {
    configured: bool,
    label: String,
    version: i64,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantPayoutAccountCommand)]
struct AccountCommandSchema {
    expected_version: i64,
    label: String,
    #[schema(write_only)]
    account: Option<String>,
    clear: Option<bool>,
}

pub(crate) fn document() -> utoipa::openapi::OpenApi {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            settings,
            save_settings,
            account,
            save_account,
            payouts,
            admin_payouts,
            enqueue,
            reconcile
        ),
        components(schemas(
            PayoutSchema,
            PayoutPageSchema,
            SettingsSchema,
            SettingsCommandSchema,
            AccountSchema,
            AccountCommandSchema
        ))
    )]
    struct Api;
    Api::openapi()
}
pub fn build_merchant_payout_router(
    service: Arc<DatabaseMerchantPayoutService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route(
            "/api/admin/merchant-payout-settings",
            get(settings).put(save_settings),
        )
        .route(
            "/api/merchant/payout-account",
            get(account).put(save_account),
        )
        .route("/api/merchant/payouts", get(payouts))
        .route("/api/admin/merchant-payouts", get(admin_payouts))
        .route("/api/admin/merchant-withdrawals/{id}/payout", post(enqueue))
        .route(
            "/api/admin/merchant-withdrawals/{id}/reconcile",
            post(reconcile),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(service)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default = "default_limit")]
    limit: u32,
    before_id: Option<i64>,
    merchant_id: Option<i64>,
}
const fn default_limit() -> u32 {
    20
}
#[utoipa::path(get,path="/api/admin/merchant-payout-settings",operation_id="getMerchantPayoutSettings",tag="Merchant pool",security(("bearerAuth"=[])),responses((status=200,body=SettingsSchema)))]
async fn settings(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        service
            .settings(auth.principal())
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(put,path="/api/admin/merchant-payout-settings",operation_id="saveMerchantPayoutSettings",tag="Merchant pool",request_body=SettingsCommandSchema,security(("bearerAuth"=[])),responses((status=200,body=SettingsSchema)))]
async fn save_settings(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<MerchantPayoutSettingsCommand>, JsonRejection>,
) -> Result<Response, FinanceHttpError> {
    let Json(body) =
        body.map_err(|_| FinanceHttpError(af_admin::MerchantFinanceError::InvalidInput))?;
    Ok(no_store_json(
        service
            .save_settings(auth.principal(), body)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/merchant/payout-account",operation_id="getMerchantPayoutAccount",tag="Merchant pool",security(("bearerAuth"=[])),responses((status=200,body=AccountSchema)))]
async fn account(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        service
            .account(auth.principal())
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(put,path="/api/merchant/payout-account",operation_id="saveMerchantPayoutAccount",tag="Merchant pool",request_body=AccountCommandSchema,security(("bearerAuth"=[])),responses((status=200,body=AccountSchema)))]
async fn save_account(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<MerchantPayoutAccountCommand>, JsonRejection>,
) -> Result<Response, FinanceHttpError> {
    let Json(body) =
        body.map_err(|_| FinanceHttpError(af_admin::MerchantFinanceError::InvalidInput))?;
    Ok(no_store_json(
        service
            .save_account(auth.principal(), body)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/merchant/payouts",operation_id="listMerchantPayouts",tag="Merchant pool",params(("limit"=Option<u32>,Query),("before_id"=Option<i64>,Query)),security(("bearerAuth"=[])),responses((status=200,body=PayoutPageSchema)))]
async fn payouts(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(query): Query<Page>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        service
            .payouts(auth.principal(), false, None, query.limit, query.before_id)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/admin/merchant-payouts",operation_id="listAdminMerchantPayouts",tag="Merchant pool",params(("limit"=Option<u32>,Query),("before_id"=Option<i64>,Query),("merchant_id"=Option<i64>,Query)),security(("bearerAuth"=[])),responses((status=200,body=PayoutPageSchema)))]
async fn admin_payouts(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(query): Query<Page>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        service
            .payouts(
                auth.principal(),
                true,
                query.merchant_id,
                query.limit,
                query.before_id,
            )
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(post,path="/api/admin/merchant-withdrawals/{id}/payout",operation_id="submitMerchantPayout",tag="Merchant pool",params(("id"=i64,Path)),security(("bearerAuth"=[])),responses((status=200,body=PayoutSchema)))]
async fn enqueue(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        service
            .enqueue(auth.principal(), id)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(post,path="/api/admin/merchant-withdrawals/{id}/reconcile",operation_id="reconcileMerchantPayout",tag="Merchant pool",params(("id"=i64,Path)),security(("bearerAuth"=[])),responses((status=200,body=serde_json::Value)))]
async fn reconcile(
    State(service): State<Arc<DatabaseMerchantPayoutService>>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, FinanceHttpError> {
    service
        .reconcile(auth.principal(), id)
        .await
        .map_err(FinanceHttpError)?;
    Ok(no_store_json(serde_json::json!({"scheduled":true})))
}
