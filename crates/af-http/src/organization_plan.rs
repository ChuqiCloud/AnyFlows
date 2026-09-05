use std::sync::Arc;

use af_admin::{
    OrganizationPlanCreateCommand, OrganizationPlanDisableCommand, OrganizationPlanListQuery,
    OrganizationPlanOrderCreateCommand, OrganizationPlanOrderService,
    OrganizationPlanOrderServiceError, OrganizationPlanOrderView, OrganizationPlanService,
    OrganizationPlanServiceError, OrganizationPlanView, SessionAuthentication,
    SessionAuthenticator, UserTopupService,
};
use af_domain::{
    OrganizationId, OrganizationPlanFeatures, OrganizationPlanId, OrganizationPlanOrderId,
    OrganizationPlanOrderRequestId, OrganizationPlanOrderStatus, OrganizationPlanStatus,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post},
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
    middleware::ensure_request_id,
};

#[derive(Clone)]
struct OrganizationPlanHttpState {
    service: Option<Arc<dyn OrganizationPlanService>>,
    order_service: Option<Arc<dyn OrganizationPlanOrderService>>,
    topup_service: Option<Arc<dyn UserTopupService>>,
}

/// 构建企业套餐目录和平台管理接口；所有入口均要求有效管理会话。
pub fn build_organization_plan_router(
    service: Option<Arc<dyn OrganizationPlanService>>,
    order_service: Option<Arc<dyn OrganizationPlanOrderService>>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    topup_service: Option<Arc<dyn UserTopupService>>,
) -> Router {
    let state = OrganizationPlanHttpState {
        service,
        order_service,
        topup_service,
    };
    let admin_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let user_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    let admin_routes = Router::new()
        .route(
            "/api/admin/organization-plans",
            get(list_admin_organization_plans).post(create_admin_organization_plan),
        )
        .route(
            "/api/admin/organization-plans/{plan_id}/disable",
            post(disable_admin_organization_plan),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(admin_authentication)
        .with_state(state.clone());
    let user_routes = Router::new()
        .route(
            "/api/account/organization-plan-catalog",
            get(list_organization_plan_catalog),
        )
        .route(
            "/api/organizations/{organization_id}/plan-orders",
            post(create_organization_plan_order),
        )
        .route(
            "/api/organizations/{organization_id}/plan-orders/{order_id}",
            get(get_organization_plan_order),
        )
        .route_layer(user_authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(state);
    admin_routes.merge(user_routes)
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationPlanCreateRequest {
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(max_length = 240, required = true)]
    description: Option<String>,
    team_management_enabled: bool,
    sso_enabled: bool,
    custom_roles_enabled: bool,
    #[schema(minimum = 1)]
    seat_limit: i64,
    #[schema(minimum = 1)]
    team_limit: i64,
    #[schema(minimum = 1)]
    organization_key_limit: i64,
    #[schema(minimum = 1, maximum = 3650)]
    audit_retention_days: i64,
    #[schema(minimum = 1, maximum = 3650)]
    validity_days: i64,
    #[schema(min_length = 1, max_length = 32)]
    price_provider: String,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    price_currency: String,
    #[schema(minimum = 1)]
    price_amount_minor: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationPlanDisableRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrganizationPlanStatusDto {
    Active,
    Disabled,
}

impl From<OrganizationPlanStatus> for OrganizationPlanStatusDto {
    fn from(value: OrganizationPlanStatus) -> Self {
        match value {
            OrganizationPlanStatus::Active => Self::Active,
            OrganizationPlanStatus::Disabled => Self::Disabled,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationPlanResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(max_length = 240, required = true)]
    description: Option<String>,
    status: OrganizationPlanStatusDto,
    team_management_enabled: bool,
    sso_enabled: bool,
    custom_roles_enabled: bool,
    #[schema(minimum = 1)]
    seat_limit: i64,
    #[schema(minimum = 1)]
    team_limit: i64,
    #[schema(minimum = 1)]
    organization_key_limit: i64,
    #[schema(minimum = 1, maximum = 3650)]
    audit_retention_days: i64,
    #[schema(minimum = 1, maximum = 3650)]
    validity_days: i64,
    #[schema(min_length = 1, max_length = 32)]
    price_provider: String,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    price_currency: String,
    #[schema(minimum = 1)]
    price_amount_minor: i64,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(minimum = 1)]
    created_by_user_id: i64,
    #[schema(minimum = 0, required = true)]
    disabled_at: Option<i64>,
    #[schema(minimum = 0)]
    created_at: i64,
    #[schema(minimum = 0)]
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationPlanListResponse {
    #[schema(max_items = 100)]
    plans: Vec<OrganizationPlanResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationPlanCatalogResponse {
    #[schema(max_items = 100)]
    plans: Vec<OrganizationPlanResponse>,
    /// 当前响应仅表示可售价格快照，不代表已支付或已发放企业授权。
    purchase_notice: &'static str,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationPlanOrderCreateRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    idempotency_key: String,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrganizationPlanOrderStatusDto {
    Created,
    Pending,
    Paid,
    Failed,
    Canceled,
    Expired,
}

impl From<OrganizationPlanOrderStatus> for OrganizationPlanOrderStatusDto {
    fn from(value: OrganizationPlanOrderStatus) -> Self {
        match value {
            OrganizationPlanOrderStatus::Created => Self::Created,
            OrganizationPlanOrderStatus::Pending => Self::Pending,
            OrganizationPlanOrderStatus::Paid => Self::Paid,
            OrganizationPlanOrderStatus::Failed => Self::Failed,
            OrganizationPlanOrderStatus::Canceled => Self::Canceled,
            OrganizationPlanOrderStatus::Expired => Self::Expired,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationPlanOrderResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    order_id: String,
    #[schema(minimum = 1)]
    organization_id: i64,
    #[schema(minimum = 1)]
    payer_user_id: i64,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(minimum = 1)]
    plan_version: u64,
    #[schema(min_length = 1, max_length = 32)]
    provider: String,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    currency: String,
    #[schema(minimum = 1)]
    amount_minor: i64,
    status: OrganizationPlanOrderStatusDto,
    #[schema(minimum = 1)]
    version: u64,
    #[schema(min_length = 1, max_length = 32, required = true)]
    payment_method: Option<String>,
    #[schema(minimum = 0, required = true)]
    expires_at: Option<u64>,
    #[schema(minimum = 0, required = true)]
    paid_at: Option<u64>,
    #[schema(minimum = 0, required = true)]
    closed_at: Option<u64>,
    #[schema(minimum = 0)]
    created_at: u64,
    #[schema(minimum = 0)]
    updated_at: u64,
    replayed: bool,
}

async fn list_admin_organization_plans(
    State(state): State<OrganizationPlanHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state.service.as_ref().ok_or(ManagementError::Internal)?;
    let query = parse_query(raw_query.as_deref())?;
    let page = service
        .list(authentication.principal(), query)
        .await
        .map_err(map_error)?;
    let plans = page
        .plans()
        .iter()
        .map(OrganizationPlanResponse::from_view)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_json(OrganizationPlanListResponse {
        plans,
        next_cursor: page.next_cursor(),
    }))
}

async fn create_admin_organization_plan(
    State(state): State<OrganizationPlanHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<OrganizationPlanCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let service = state.service.as_ref().ok_or(ManagementError::Internal)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = OrganizationPlanCreateCommand::new(
        request.name,
        request.description,
        request.team_management_enabled,
        request.sso_enabled,
        request.custom_roles_enabled,
        request.seat_limit,
        request.team_limit,
        request.organization_key_limit,
        request.audit_retention_days,
        request.validity_days,
        request.price_provider,
        request.price_currency,
        request.price_amount_minor,
    )
    .map_err(map_error)?;
    let plan = service
        .create(authentication.principal(), command)
        .await
        .map_err(map_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        OrganizationPlanResponse::from_view(&plan)?,
    ))
}

async fn disable_admin_organization_plan(
    State(state): State<OrganizationPlanHttpState>,
    Path(plan_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<OrganizationPlanDisableRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let service = state.service.as_ref().ok_or(ManagementError::Internal)?;
    let plan_id = OrganizationPlanId::from_persistence_key(&plan_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        OrganizationPlanDisableCommand::new(request.expected_version).map_err(map_error)?;
    let plan = service
        .disable(authentication.principal(), plan_id, command)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(OrganizationPlanResponse::from_view(&plan)?))
}

async fn list_organization_plan_catalog(
    State(state): State<OrganizationPlanHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state.service.as_ref().ok_or(ManagementError::Internal)?;
    let catalog = service
        .list_catalog(authentication.principal())
        .await
        .map_err(map_error)?;
    let configuration = state
        .topup_service
        .as_ref()
        .and_then(|service| service.configuration().ok());
    let plans = catalog
        .plans()
        .iter()
        .filter(|plan| {
            configuration.as_ref().is_some_and(|config| {
                config.methods().iter().any(|method| {
                    method.provider() == plan.provider()
                        && method.currency() == plan.currency()
                        && plan.amount_minor() >= method.min_amount_minor()
                        && plan.amount_minor() <= method.max_amount_minor()
                })
            })
        })
        .map(OrganizationPlanResponse::from_view)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_json(OrganizationPlanCatalogResponse {
        plans,
        purchase_notice: "当前仅展示已配置支付 Provider 的企业套餐价格快照，支付和授权发放尚未完成。",
    }))
}

async fn create_organization_plan_order(
    State(state): State<OrganizationPlanHttpState>,
    Path(organization_id): Path<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<OrganizationPlanOrderCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let service = state
        .order_service
        .as_ref()
        .ok_or(ManagementError::OrganizationPlanOrderUnavailable)?;
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let plan_id = OrganizationPlanId::from_persistence_key(&request.plan_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let request_id = OrganizationPlanOrderRequestId::from_persistence_key(&request.idempotency_key)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let order = service
        .create(
            authentication.principal(),
            organization_id,
            OrganizationPlanOrderCreateCommand::new(plan_id, request_id),
        )
        .await
        .map_err(map_order_error)?;
    let status = if order.replayed() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(status_json(
        status,
        OrganizationPlanOrderResponse::from_view(&order),
    ))
}

async fn get_organization_plan_order(
    State(state): State<OrganizationPlanHttpState>,
    Path((organization_id, order_id)): Path<(i64, String)>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .order_service
        .as_ref()
        .ok_or(ManagementError::OrganizationPlanOrderUnavailable)?;
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    let order_id = OrganizationPlanOrderId::from_persistence_key(&order_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let order = service
        .get(authentication.principal(), organization_id, order_id)
        .await
        .map_err(map_order_error)?;
    Ok(no_store_json(OrganizationPlanOrderResponse::from_view(
        &order,
    )))
}

impl OrganizationPlanResponse {
    fn from_view(view: &OrganizationPlanView) -> Result<Self, ManagementError> {
        let features: OrganizationPlanFeatures = view.snapshot().features();
        Ok(Self {
            plan_id: view.plan_id().persistence_key(),
            name: view.name().to_owned(),
            description: view.description().map(str::to_owned),
            status: view.status().into(),
            team_management_enabled: features.team_management_enabled(),
            sso_enabled: features.sso_enabled(),
            custom_roles_enabled: features.custom_roles_enabled(),
            seat_limit: view.snapshot().seat_limit(),
            team_limit: view.snapshot().team_limit(),
            organization_key_limit: view.snapshot().organization_key_limit(),
            audit_retention_days: view.snapshot().audit_retention_days(),
            validity_days: view.snapshot().validity_days(),
            price_provider: view.provider().to_owned(),
            price_currency: view.currency().to_owned(),
            price_amount_minor: view.amount_minor(),
            version: view.version(),
            created_by_user_id: view.created_by_user_id().get(),
            disabled_at: view
                .disabled_at()
                .map(i64::try_from)
                .transpose()
                .map_err(|_| ManagementError::Internal)?,
            created_at: i64::try_from(view.created_at()).map_err(|_| ManagementError::Internal)?,
            updated_at: i64::try_from(view.updated_at()).map_err(|_| ManagementError::Internal)?,
        })
    }
}

impl OrganizationPlanOrderResponse {
    fn from_view(view: &OrganizationPlanOrderView) -> Self {
        Self {
            order_id: view.order_id().persistence_key(),
            organization_id: view.organization_id().get(),
            payer_user_id: view.payer_user_id().get(),
            plan_id: view.plan_id().persistence_key(),
            plan_version: view.plan_version(),
            provider: view.provider().to_owned(),
            currency: view.currency().to_owned(),
            amount_minor: view.amount_minor(),
            status: view.status().into(),
            version: view.version(),
            payment_method: view.payment_method().map(str::to_owned),
            expires_at: view.expires_at(),
            paid_at: view.paid_at(),
            closed_at: view.closed_at(),
            created_at: view.created_at(),
            updated_at: view.updated_at(),
            replayed: view.replayed(),
        }
    }
}

fn parse_query(raw_query: Option<&str>) -> Result<OrganizationPlanListQuery, ManagementError> {
    let mut before = None;
    let mut limit = af_admin::DEFAULT_ORGANIZATION_PLAN_PAGE_SIZE;
    if let Some(raw_query) = raw_query {
        for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            match key.as_ref() {
                "before" if before.is_none() => {
                    before = Some(
                        value
                            .parse::<i64>()
                            .map_err(|_| ManagementError::InvalidRequest)?,
                    )
                }
                "limit" if limit == af_admin::DEFAULT_ORGANIZATION_PLAN_PAGE_SIZE => {
                    limit = value
                        .parse::<usize>()
                        .map_err(|_| ManagementError::InvalidRequest)?
                }
                _ => return Err(ManagementError::InvalidRequest),
            }
        }
    }
    OrganizationPlanListQuery::new(before, limit).map_err(map_error)
}

fn map_error(error: OrganizationPlanServiceError) -> ManagementError {
    match error {
        OrganizationPlanServiceError::InvalidRequest => ManagementError::InvalidRequest,
        OrganizationPlanServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationPlanServiceError::NotFound => ManagementError::OrganizationPlanNotFound,
        OrganizationPlanServiceError::Conflict => ManagementError::OrganizationPlanConflict,
        OrganizationPlanServiceError::Internal => ManagementError::Internal,
    }
}

fn map_order_error(error: OrganizationPlanOrderServiceError) -> ManagementError {
    match error {
        OrganizationPlanOrderServiceError::InvalidRequest => ManagementError::InvalidRequest,
        OrganizationPlanOrderServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationPlanOrderServiceError::NotFound => {
            ManagementError::OrganizationPlanOrderNotFound
        }
        OrganizationPlanOrderServiceError::Conflict => {
            ManagementError::OrganizationPlanOrderConflict
        }
        OrganizationPlanOrderServiceError::OutcomeUnknown => {
            ManagementError::OrganizationPlanOrderOutcomeUnknown
        }
        OrganizationPlanOrderServiceError::Unavailable => {
            ManagementError::OrganizationPlanOrderUnavailable
        }
        OrganizationPlanOrderServiceError::Internal => ManagementError::Internal,
    }
}

fn status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = status;
    response
}
