use std::sync::Arc;

use af_admin::{
    MerchantFinanceError, MerchantFinanceService, SessionAuthentication, SessionAuthenticator,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::{OpenApi, ToSchema};

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_session::no_store_json,
};

#[derive(Clone)]
struct FinanceState {
    service: Arc<dyn MerchantFinanceService>,
}

#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantFinanceBalance)]
struct BalanceSchema {
    merchant_id: i64,
    pending_quota: i64,
    available_quota: i64,
    withdrawing_quota: i64,
    frozen_quota: i64,
    debt_quota: i64,
    total_gross_quota: i64,
    total_commission_quota: i64,
    total_paid_quota: i64,
    quota_per_usd: i64,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantFinanceEntry)]
struct EntrySchema {
    id: i64,
    kind: String,
    reference: String,
    gross_quota: i64,
    commission_quota: i64,
    net_quota: i64,
    created_at: i64,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantWithdrawal)]
struct WithdrawalSchema {
    id: i64,
    event_key: String,
    merchant_id: i64,
    amount_quota: i64,
    status: String,
    reference: String,
    reason: Option<String>,
    created_at: i64,
    updated_at: i64,
    payout: Option<crate::merchant_payout::PayoutSchema>,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantFinanceEntryPage)]
struct EntryPageSchema {
    items: Vec<EntrySchema>,
    next_cursor: Option<i64>,
}
#[allow(dead_code)]
#[derive(ToSchema)]
#[schema(as=MerchantWithdrawalPage)]
struct WithdrawalPageSchema {
    items: Vec<WithdrawalSchema>,
    next_cursor: Option<i64>,
}

pub(crate) fn document() -> utoipa::openapi::OpenApi {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            balance,
            entries,
            withdrawals,
            request_withdrawal,
            admin_withdrawals,
            decide_withdrawal,
            reverse_settlement,
            hold_balance,
            admin_balance,
            admin_entries
        ),
        components(schemas(
            BalanceSchema,
            EntrySchema,
            WithdrawalSchema,
            EntryPageSchema,
            WithdrawalPageSchema,
            MerchantWithdrawalRequest,
            MerchantWithdrawalDecision,
            MerchantSettlementReversal,
            MerchantFinanceHold
        ))
    )]
    struct FinanceApi;
    FinanceApi::openapi()
}

pub fn build_merchant_finance_router(
    service: Arc<dyn MerchantFinanceService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/merchant/finance", get(balance))
        .route("/api/merchant/finance/entries", get(entries))
        .route(
            "/api/merchant/withdrawals",
            get(withdrawals).post(request_withdrawal),
        )
        .route("/api/admin/merchant-withdrawals", get(admin_withdrawals))
        .route(
            "/api/admin/merchant-withdrawals/{id}/decision",
            post(decide_withdrawal),
        )
        .route(
            "/api/admin/merchant-settlements/{key}/reverse",
            post(reverse_settlement),
        )
        .route(
            "/api/admin/merchant-finance/{merchant_id}/hold",
            post(hold_balance),
        )
        .route(
            "/api/admin/merchant-finance/{merchant_id}",
            get(admin_balance),
        )
        .route(
            "/api/admin/merchant-finance/{merchant_id}/entries",
            get(admin_entries),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(RequestBodyLimitLayer::new(16 * 1024))
        .layer(authentication)
        .with_state(FinanceState { service })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageQuery {
    #[serde(default = "default_limit")]
    limit: u32,
    before_id: Option<i64>,
    status: Option<String>,
}
const fn default_limit() -> u32 {
    20
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MerchantWithdrawalRequest {
    #[schema(min_length = 32, max_length = 32)]
    event_key: String,
    #[schema(minimum = 1)]
    amount_quota: i64,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MerchantWithdrawalDecision {
    status: String,
    #[schema(max_length = 256)]
    reference: String,
    #[schema(max_length = 512)]
    reason: Option<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MerchantSettlementReversal {
    #[schema(min_length = 1, max_length = 500)]
    reason: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MerchantFinanceHold {
    event_key: String,
    amount_quota: i64,
    freeze: bool,
    reason: String,
}

#[utoipa::path(get,path="/api/merchant/finance",operation_id="getMerchantFinance",tag="Merchant pool",security(("bearerAuth"=[])),responses((status=200,body=BalanceSchema)))]
async fn balance(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        state
            .service
            .balance(auth.principal())
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/merchant/finance/entries",operation_id="listMerchantFinanceEntries",tag="Merchant pool",params(("limit"=Option<u32>,Query,minimum=1,maximum=100),("before_id"=Option<i64>,Query)),security(("bearerAuth"=[])),responses((status=200,body=EntryPageSchema)))]
async fn entries(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(query): Query<PageQuery>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        state
            .service
            .entries(auth.principal(), query.limit, query.before_id)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/merchant/withdrawals",operation_id="listMerchantWithdrawals",tag="Merchant pool",params(("limit"=Option<u32>,Query,minimum=1,maximum=100),("before_id"=Option<i64>,Query)),security(("bearerAuth"=[])),responses((status=200,body=WithdrawalPageSchema)))]
async fn withdrawals(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(query): Query<PageQuery>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        state
            .service
            .withdrawals(auth.principal(), query.limit, query.before_id)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(post,path="/api/merchant/withdrawals",operation_id="createMerchantWithdrawal",tag="Merchant pool",request_body=MerchantWithdrawalRequest,security(("bearerAuth"=[])),responses((status=200,body=WithdrawalSchema)))]
async fn request_withdrawal(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<MerchantWithdrawalRequest>, JsonRejection>,
) -> Result<Response, FinanceHttpError> {
    let Json(body) = body.map_err(|_| FinanceHttpError(MerchantFinanceError::InvalidInput))?;
    Ok(no_store_json(
        state
            .service
            .request_withdrawal(auth.principal(), body.event_key, body.amount_quota)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/admin/merchant-withdrawals",operation_id="listAdminMerchantWithdrawals",tag="Merchant pool",params(("limit"=Option<u32>,Query,minimum=1,maximum=100),("before_id"=Option<i64>,Query),("status"=Option<String>,Query)),security(("bearerAuth"=[])),responses((status=200,body=WithdrawalPageSchema)))]
async fn admin_withdrawals(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Query(query): Query<PageQuery>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        state
            .service
            .admin_withdrawals(auth.principal(), query.status, query.limit, query.before_id)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(post,path="/api/admin/merchant-withdrawals/{id}/decision",operation_id="decideAdminMerchantWithdrawal",tag="Merchant pool",params(("id"=i64,Path)),request_body=MerchantWithdrawalDecision,security(("bearerAuth"=[])),responses((status=200,body=WithdrawalSchema)))]
async fn decide_withdrawal(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
    body: Result<Json<MerchantWithdrawalDecision>, JsonRejection>,
) -> Result<Response, FinanceHttpError> {
    let Json(body) = body.map_err(|_| FinanceHttpError(MerchantFinanceError::InvalidInput))?;
    Ok(no_store_json(
        state
            .service
            .decide_withdrawal(
                auth.principal(),
                id,
                body.status,
                body.reference,
                body.reason,
            )
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(post,path="/api/admin/merchant-settlements/{key}/reverse",operation_id="reverseAdminMerchantSettlement",tag="Merchant pool",params(("key"=String,Path)),request_body=MerchantSettlementReversal,security(("bearerAuth"=[])),responses((status=200,body=serde_json::Value)))]
async fn reverse_settlement(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(key): Path<String>,
    body: Result<Json<MerchantSettlementReversal>, JsonRejection>,
) -> Result<Response, FinanceHttpError> {
    let Json(body) = body.map_err(|_| FinanceHttpError(MerchantFinanceError::InvalidInput))?;
    state
        .service
        .reverse_settlement(auth.principal(), key, body.reason)
        .await
        .map_err(FinanceHttpError)?;
    Ok(no_store_json(serde_json::json!({"reversed":true})))
}
#[utoipa::path(post,path="/api/admin/merchant-finance/{merchant_id}/hold",operation_id="holdAdminMerchantFinance",tag="Merchant pool",params(("merchant_id"=i64,Path)),request_body=MerchantFinanceHold,security(("bearerAuth"=[])),responses((status=200,body=BalanceSchema)))]
async fn hold_balance(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(merchant_id): Path<i64>,
    body: Result<Json<MerchantFinanceHold>, JsonRejection>,
) -> Result<Response, FinanceHttpError> {
    let Json(body) = body.map_err(|_| FinanceHttpError(MerchantFinanceError::InvalidInput))?;
    Ok(no_store_json(
        state
            .service
            .hold_balance(
                auth.principal(),
                merchant_id,
                body.event_key,
                body.amount_quota,
                body.freeze,
                body.reason,
            )
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/admin/merchant-finance/{merchant_id}",operation_id="getAdminMerchantFinance",tag="Merchant pool",params(("merchant_id"=i64,Path)),security(("bearerAuth"=[])),responses((status=200,body=BalanceSchema)))]
async fn admin_balance(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(merchant_id): Path<i64>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        state
            .service
            .admin_balance(auth.principal(), merchant_id)
            .await
            .map_err(FinanceHttpError)?,
    ))
}
#[utoipa::path(get,path="/api/admin/merchant-finance/{merchant_id}/entries",operation_id="listAdminMerchantFinanceEntries",tag="Merchant pool",params(("merchant_id"=i64,Path),("limit"=Option<u32>,Query,minimum=1,maximum=100),("before_id"=Option<i64>,Query)),security(("bearerAuth"=[])),responses((status=200,body=EntryPageSchema)))]
async fn admin_entries(
    State(state): State<FinanceState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(merchant_id): Path<i64>,
    Query(query): Query<PageQuery>,
) -> Result<Response, FinanceHttpError> {
    Ok(no_store_json(
        state
            .service
            .admin_entries(auth.principal(), merchant_id, query.limit, query.before_id)
            .await
            .map_err(FinanceHttpError)?,
    ))
}

pub(crate) struct FinanceHttpError(pub(crate) MerchantFinanceError);
impl IntoResponse for FinanceHttpError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self.0 {
            MerchantFinanceError::InvalidInput => (
                StatusCode::BAD_REQUEST,
                "merchant_finance_invalid_request",
                "商户资金请求无效",
            ),
            MerchantFinanceError::NotFound => (
                StatusCode::NOT_FOUND,
                "merchant_finance_not_found",
                "商户资金记录不存在",
            ),
            MerchantFinanceError::Conflict => (
                StatusCode::CONFLICT,
                "merchant_finance_conflict",
                "资金记录状态已改变，请刷新后重试",
            ),
            MerchantFinanceError::InsufficientBalance => (
                StatusCode::CONFLICT,
                "merchant_finance_insufficient_balance",
                "可提现余额不足或存在冻结、待补金额",
            ),
            MerchantFinanceError::Forbidden => {
                (StatusCode::FORBIDDEN, "forbidden", "无权访问商户资金")
            }
            MerchantFinanceError::Unavailable | MerchantFinanceError::Invariant => (
                StatusCode::SERVICE_UNAVAILABLE,
                "merchant_finance_unavailable",
                "商户资金服务暂不可用",
            ),
        };
        #[derive(Serialize)]
        struct Body {
            error: ErrorBody,
        }
        #[derive(Serialize)]
        struct ErrorBody {
            code: &'static str,
            message: &'static str,
        }
        let mut response = no_store_json(Body {
            error: ErrorBody { code, message },
        });
        *response.status_mut() = status;
        response
    }
}
