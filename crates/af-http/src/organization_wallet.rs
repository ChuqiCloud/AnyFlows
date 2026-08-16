use std::sync::Arc;

use af_admin::{
    OrganizationAuthorizationError, OrganizationAuthorizationService, OrganizationContext,
    OrganizationWalletAdjustmentCommand, OrganizationWalletEntry,
    OrganizationWalletLedgerEntryType, OrganizationWalletListQuery, OrganizationWalletPage,
    OrganizationWalletService, OrganizationWalletServiceError, OrganizationWalletSummary,
    SessionAuthentication, SessionAuthenticator, UserTopupOrderCreateCommand, UserTopupService,
};
use af_billing::PaymentCheckoutAction;
use af_domain::{
    OrganizationId, OrganizationPermission, QuotaDelta, TopupOrderStatus, WalletEventId,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    wallet_query::parse_wallet_list_query,
};

#[derive(Clone)]
struct OrganizationWalletHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    wallet_service: Arc<dyn OrganizationWalletService>,
    topup_service: Option<Arc<dyn UserTopupService>>,
}

/// 构建企业钱包、账本、受审计调账和企业充值入口。
pub fn build_organization_wallet_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    wallet_service: Arc<dyn OrganizationWalletService>,
    topup_service: Option<Arc<dyn UserTopupService>>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/wallet",
            get(get_organization_wallet),
        )
        .route(
            "/api/organizations/{organization_id}/wallet/entries",
            get(list_organization_wallet_entries),
        )
        .route(
            "/api/organizations/{organization_id}/wallet/adjustments",
            post(adjust_organization_wallet),
        )
        .route(
            "/api/organizations/{organization_id}/topups",
            post(create_organization_topup),
        )
        .route_layer(authentication)
        .with_state(OrganizationWalletHttpState {
            authorization_service,
            wallet_service,
            topup_service,
        })
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationWalletSummaryResponse {
    #[schema(minimum = 1)]
    organization_id: i64,
    #[schema(minimum = 0)]
    balance: i64,
    #[schema(minimum = 0)]
    used_quota: i64,
    #[schema(minimum = 0)]
    frozen_quota: i64,
    #[schema(minimum = 1)]
    version: i64,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrganizationWalletEntryTypeResponse {
    OpeningBalance,
    AdminAdjustment,
    Topup,
    UsageSettlement,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationWalletEntryResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    event_id: String,
    #[schema(minimum = 1, required = true)]
    actor_user_id: Option<i64>,
    entry_type: OrganizationWalletEntryTypeResponse,
    quota_delta: i64,
    #[schema(minimum = 0)]
    balance_before: i64,
    #[schema(minimum = 0)]
    balance_after: i64,
    #[schema(min_length = 1, max_length = 64)]
    source: String,
    #[schema(min_length = 1, max_length = 500, required = true)]
    reason: Option<String>,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationWalletListResponse {
    #[schema(max_items = 100)]
    entries: Vec<OrganizationWalletEntryResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationWalletAdjustmentRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    event_id: String,
    quota_delta: i64,
    #[schema(min_length = 1, max_length = 500)]
    reason: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationWalletAdjustmentResponse {
    entry: OrganizationWalletEntryResponse,
    replayed: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationTopupOrderCreateRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    idempotency_key: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(min_length = 1, max_length = 64)]
    payment_method: String,
    #[schema(minimum = 1, maximum = 99_999_999)]
    amount_minor: i64,
}

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrganizationTopupOrderStatusResponse {
    Created,
    Pending,
    Paid,
    Failed,
    Canceled,
    Expired,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum OrganizationTopupPaymentSessionResponse {
    Stripe {
        #[schema(min_length = 4, max_length = 128, pattern = "^pi_[A-Za-z0-9_]+$")]
        payment_intent_id: String,
        #[schema(min_length = 1, max_length = 4096, read_only = true)]
        client_secret: String,
    },
    Redirect {
        #[schema(min_length = 1, max_length = 16384)]
        redirect_url: String,
    },
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationTopupOrderResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    order_id: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(min_length = 1, max_length = 64)]
    payment_method: String,
    status: OrganizationTopupOrderStatusResponse,
    #[schema(minimum = 1, maximum = 99_999_999)]
    amount_minor: i64,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    currency: String,
    #[schema(minimum = 1)]
    quota_amount: i64,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(minimum = 1)]
    created_at: i64,
    replayed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    payment: Option<OrganizationTopupPaymentSessionResponse>,
}

async fn get_organization_wallet(
    State(state): State<OrganizationWalletHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::WalletRead,
    )
    .await?;
    let summary = state
        .wallet_service
        .summary(context)
        .await
        .map_err(map_wallet_error)?;
    Ok(no_store_status_json(
        StatusCode::OK,
        OrganizationWalletSummaryResponse::from(summary),
    ))
}

async fn list_organization_wallet_entries(
    State(state): State<OrganizationWalletHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::WalletRead,
    )
    .await?;
    let (before, limit) = parse_wallet_list_query(
        raw_query.as_deref(),
        af_admin::DEFAULT_ORGANIZATION_WALLET_PAGE_SIZE,
    )?;
    let query = OrganizationWalletListQuery::new(before, limit).map_err(map_wallet_error)?;
    let page = state
        .wallet_service
        .list(context, query)
        .await
        .map_err(map_wallet_error)?;
    Ok(no_store_status_json(
        StatusCode::OK,
        OrganizationWalletListResponse::from_page(&page),
    ))
}

async fn adjust_organization_wallet(
    State(state): State<OrganizationWalletHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    request: Result<Json<OrganizationWalletAdjustmentRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::WalletManage,
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = OrganizationWalletAdjustmentCommand::new(
        WalletEventId::from_persistence_key(&request.event_id)
            .map_err(|_| ManagementError::InvalidRequest)?,
        QuotaDelta::new(request.quota_delta).map_err(|_| ManagementError::InvalidRequest)?,
        request.reason,
    )
    .map_err(map_wallet_error)?;
    let adjustment = state
        .wallet_service
        .adjust(context, command)
        .await
        .map_err(map_wallet_error)?;
    let status = if adjustment.replayed() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(no_store_status_json(
        status,
        OrganizationWalletAdjustmentResponse {
            entry: OrganizationWalletEntryResponse::from_entry(adjustment.entry()),
            replayed: adjustment.replayed(),
        },
    ))
}

async fn create_organization_topup(
    State(state): State<OrganizationWalletHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    request: Result<Json<OrganizationTopupOrderCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    authorize_id(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::WalletManage,
    )
    .await?;
    let service = state
        .topup_service
        .as_ref()
        .ok_or(ManagementError::TopupUnavailable)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = UserTopupOrderCreateCommand::new_for_organization(
        organization_id,
        &request.idempotency_key,
        request.provider,
        request.payment_method,
        request.amount_minor,
    )
    .map_err(map_topup_error)?;
    let order = service
        .create(authentication.principal(), command)
        .await
        .map_err(map_topup_error)?;
    if order.organization_id() != Some(organization_id) {
        return Err(ManagementError::Internal);
    }
    let status = if order.replayed() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(no_store_status_json(
        status,
        OrganizationTopupOrderResponse::from_order(&order)?,
    ))
}

impl From<OrganizationWalletSummary> for OrganizationWalletSummaryResponse {
    fn from(summary: OrganizationWalletSummary) -> Self {
        Self {
            organization_id: summary.organization_id().get(),
            balance: summary.balance().units(),
            used_quota: summary.used_quota().units(),
            frozen_quota: summary.frozen_quota().units(),
            version: summary.version(),
        }
    }
}

impl OrganizationWalletListResponse {
    fn from_page(page: &OrganizationWalletPage) -> Self {
        Self {
            entries: page
                .entries()
                .iter()
                .map(OrganizationWalletEntryResponse::from_entry)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl OrganizationWalletEntryResponse {
    fn from_entry(entry: &OrganizationWalletEntry) -> Self {
        Self {
            id: entry.id(),
            event_id: entry.event_id().persistence_key(),
            actor_user_id: entry.actor_user_id().map(|value| value.get()),
            entry_type: match entry.entry_type() {
                OrganizationWalletLedgerEntryType::OpeningBalance => {
                    OrganizationWalletEntryTypeResponse::OpeningBalance
                }
                OrganizationWalletLedgerEntryType::AdminAdjustment => {
                    OrganizationWalletEntryTypeResponse::AdminAdjustment
                }
                OrganizationWalletLedgerEntryType::Topup => {
                    OrganizationWalletEntryTypeResponse::Topup
                }
                OrganizationWalletLedgerEntryType::UsageSettlement => {
                    OrganizationWalletEntryTypeResponse::UsageSettlement
                }
            },
            quota_delta: entry.quota_delta().units(),
            balance_before: entry.balance_before().units(),
            balance_after: entry.balance_after().units(),
            source: entry.source().to_owned(),
            reason: entry.reason().map(str::to_owned),
            created_at: entry.created_at(),
        }
    }
}

impl OrganizationTopupOrderResponse {
    fn from_order(order: &af_admin::UserTopupOrder) -> Result<Self, ManagementError> {
        Ok(Self {
            order_id: order.order_id().persistence_key(),
            provider: order.provider().to_owned(),
            payment_method: order.payment_method().to_owned(),
            status: match order.status() {
                TopupOrderStatus::Created => OrganizationTopupOrderStatusResponse::Created,
                TopupOrderStatus::Pending => OrganizationTopupOrderStatusResponse::Pending,
                TopupOrderStatus::Paid => OrganizationTopupOrderStatusResponse::Paid,
                TopupOrderStatus::Failed => OrganizationTopupOrderStatusResponse::Failed,
                TopupOrderStatus::Canceled => OrganizationTopupOrderStatusResponse::Canceled,
                TopupOrderStatus::Expired => OrganizationTopupOrderStatusResponse::Expired,
            },
            amount_minor: i64::try_from(order.amount_minor())
                .map_err(|_| ManagementError::Internal)?,
            currency: order.currency().to_owned(),
            quota_amount: order.quota_amount().units(),
            version: i64::try_from(order.version()).map_err(|_| ManagementError::Internal)?,
            created_at: i64::try_from(order.created_at()).map_err(|_| ManagementError::Internal)?,
            replayed: order.replayed(),
            payment: order
                .payment()
                .map(OrganizationTopupPaymentSessionResponse::from_session)
                .transpose()?,
        })
    }
}

impl OrganizationTopupPaymentSessionResponse {
    fn from_session(session: &af_admin::UserTopupPaymentSession) -> Result<Self, ManagementError> {
        match session.action() {
            PaymentCheckoutAction::ClientSecret(_) => Ok(Self::Stripe {
                payment_intent_id: session.payment_intent_id().to_owned(),
                client_secret: session
                    .client_secret()
                    .ok_or(ManagementError::Internal)?
                    .to_owned(),
            }),
            PaymentCheckoutAction::RedirectUrl(_) => Ok(Self::Redirect {
                redirect_url: session
                    .redirect_url()
                    .ok_or(ManagementError::Internal)?
                    .to_owned(),
            }),
        }
    }
}

async fn authorize(
    state: &OrganizationWalletHttpState,
    authentication: &SessionAuthentication,
    organization_id: i64,
    permission: OrganizationPermission,
) -> Result<OrganizationContext, ManagementError> {
    authorize_id(
        state,
        authentication,
        parse_organization_id(organization_id)?,
        permission,
    )
    .await
}

async fn authorize_id(
    state: &OrganizationWalletHttpState,
    authentication: &SessionAuthentication,
    organization_id: OrganizationId,
    permission: OrganizationPermission,
) -> Result<OrganizationContext, ManagementError> {
    state
        .authorization_service
        .context(authentication.principal(), organization_id, permission)
        .await
        .map_err(map_authorization_error)
}

fn parse_organization_id(value: i64) -> Result<OrganizationId, ManagementError> {
    OrganizationId::new(value).map_err(|_| ManagementError::InvalidRequest)
}

fn map_authorization_error(error: OrganizationAuthorizationError) -> ManagementError {
    match error {
        OrganizationAuthorizationError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
        OrganizationAuthorizationError::Internal => ManagementError::Internal,
    }
}

fn map_wallet_error(error: OrganizationWalletServiceError) -> ManagementError {
    match error {
        OrganizationWalletServiceError::InvalidRequest => ManagementError::InvalidRequest,
        OrganizationWalletServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationWalletServiceError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationWalletServiceError::Conflict => ManagementError::WalletEventConflict,
        OrganizationWalletServiceError::InsufficientQuota => {
            ManagementError::WalletInsufficientQuota
        }
        OrganizationWalletServiceError::Overflow => ManagementError::WalletOverflow,
        OrganizationWalletServiceError::OutcomeUnknown => ManagementError::WalletOutcomeUnknown,
        OrganizationWalletServiceError::Internal => ManagementError::Internal,
    }
}

fn map_topup_error(error: af_admin::UserTopupError) -> ManagementError {
    match error {
        af_admin::UserTopupError::InvalidInput => ManagementError::InvalidRequest,
        af_admin::UserTopupError::InvalidSession => ManagementError::InvalidSession,
        af_admin::UserTopupError::Conflict => ManagementError::TopupOrderConflict,
        af_admin::UserTopupError::OutcomeUnknown => ManagementError::TopupOrderOutcomeUnknown,
        af_admin::UserTopupError::Unavailable => ManagementError::TopupUnavailable,
        af_admin::UserTopupError::ProviderRejected => ManagementError::TopupProviderRejected,
        af_admin::UserTopupError::Internal => ManagementError::Internal,
    }
}

fn no_store_status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
