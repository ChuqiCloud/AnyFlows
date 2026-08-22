use std::sync::Arc;

use af_admin::{
    AdminSubscriptionPageQuery, AdminSubscriptionPlan, AdminSubscriptionPlanCreateCommand,
    AdminSubscriptionPlanDisableCommand, AdminUserSubscription, AdminUserSubscriptionBindCommand,
    AdminUserSubscriptionLifecycleAction, AdminUserSubscriptionLifecycleCommand,
    AdminUserSubscriptionLifecycleResult, SessionAuthentication, SessionAuthenticator,
    SubscriptionCatalog, SubscriptionCatalogPlan, SubscriptionOrder,
    SubscriptionOrderCreateCommand, SubscriptionOrderPaymentCommand, SubscriptionService,
    SubscriptionServiceError, UserTopupConfiguration, UserTopupError, UserTopupService,
};
use af_domain::{
    SubscriptionCycle, SubscriptionOrderId, SubscriptionOrderRequestId, SubscriptionOrderStatus,
    SubscriptionPlanId, SubscriptionPlanStatus, UserId, UserSubscriptionId, UserSubscriptionStatus,
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
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
    wallet_query::parse_wallet_list_query,
};

#[derive(Clone)]
struct SubscriptionHttpState {
    service: Arc<dyn SubscriptionService>,
    topup_service: Option<Arc<dyn UserTopupService>>,
}

/// HTTP 契约使用的闭合订阅周期。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SubscriptionCycle, rename_all = "snake_case")]
pub(crate) enum SubscriptionCycleDto {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl From<SubscriptionCycleDto> for SubscriptionCycle {
    fn from(value: SubscriptionCycleDto) -> Self {
        match value {
            SubscriptionCycleDto::Daily => Self::Daily,
            SubscriptionCycleDto::Weekly => Self::Weekly,
            SubscriptionCycleDto::Monthly => Self::Monthly,
            SubscriptionCycleDto::Yearly => Self::Yearly,
        }
    }
}

impl From<SubscriptionCycle> for SubscriptionCycleDto {
    fn from(value: SubscriptionCycle) -> Self {
        match value {
            SubscriptionCycle::Daily => Self::Daily,
            SubscriptionCycle::Weekly => Self::Weekly,
            SubscriptionCycle::Monthly => Self::Monthly,
            SubscriptionCycle::Yearly => Self::Yearly,
        }
    }
}

/// 订阅计划的闭合管理状态。
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SubscriptionPlanStatus, rename_all = "snake_case")]
pub(crate) enum SubscriptionPlanStatusDto {
    Active,
    Disabled,
}

impl From<SubscriptionPlanStatus> for SubscriptionPlanStatusDto {
    fn from(value: SubscriptionPlanStatus) -> Self {
        match value {
            SubscriptionPlanStatus::Active => Self::Active,
            SubscriptionPlanStatus::Disabled => Self::Disabled,
        }
    }
}

/// 用户订阅的闭合生命周期状态。
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = UserSubscriptionStatus, rename_all = "snake_case")]
pub(crate) enum UserSubscriptionStatusDto {
    Active,
    Suspended,
    Canceled,
    Expired,
}

/// 订阅购买订单的稳定状态。
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = SubscriptionOrderStatus, rename_all = "snake_case")]
pub(crate) enum SubscriptionOrderStatusDto {
    Created,
    Pending,
    Paid,
    Failed,
    Canceled,
    Expired,
}

impl From<SubscriptionOrderStatus> for SubscriptionOrderStatusDto {
    fn from(value: SubscriptionOrderStatus) -> Self {
        match value {
            SubscriptionOrderStatus::Created => Self::Created,
            SubscriptionOrderStatus::Pending => Self::Pending,
            SubscriptionOrderStatus::Paid => Self::Paid,
            SubscriptionOrderStatus::Failed => Self::Failed,
            SubscriptionOrderStatus::Canceled => Self::Canceled,
            SubscriptionOrderStatus::Expired => Self::Expired,
        }
    }
}

impl From<UserSubscriptionStatus> for UserSubscriptionStatusDto {
    fn from(value: UserSubscriptionStatus) -> Self {
        match value {
            UserSubscriptionStatus::Active => Self::Active,
            UserSubscriptionStatus::Suspended => Self::Suspended,
            UserSubscriptionStatus::Canceled => Self::Canceled,
            UserSubscriptionStatus::Expired => Self::Expired,
        }
    }
}

/// 管理员可见的订阅计划。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminSubscriptionPlan)]
pub(crate) struct AdminSubscriptionPlanResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(minimum = 1)]
    created_by_user_id: i64,
    status: SubscriptionPlanStatusDto,
    #[schema(minimum = 1)]
    quota_amount: i64,
    cycle: SubscriptionCycleDto,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(minimum = 1, required = true)]
    disabled_at: Option<i64>,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    updated_at: i64,
}

/// 订阅计划列表响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminSubscriptionPlanListResponse)]
pub(crate) struct AdminSubscriptionPlanListResponse {
    #[schema(max_items = 100)]
    plans: Vec<AdminSubscriptionPlanResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 当前用户可购买的计划与不可变价格快照。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SubscriptionCatalogResponse)]
pub(crate) struct SubscriptionCatalogResponse {
    #[schema(max_items = 100)]
    plans: Vec<SubscriptionCatalogPlanResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SubscriptionCatalogPlan)]
pub(crate) struct SubscriptionCatalogPlanResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(minimum = 1)]
    quota_amount: i64,
    cycle: SubscriptionCycleDto,
    #[schema(minimum = 1)]
    plan_version: u64,
    #[schema(min_length = 1, max_length = 32)]
    price_provider: String,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    price_currency: String,
    #[schema(minimum = 1)]
    price_amount_minor: i64,
}

impl SubscriptionCatalogResponse {
    fn from_catalog(
        catalog: &SubscriptionCatalog,
        configuration: Option<&UserTopupConfiguration>,
    ) -> Self {
        let plans = catalog
            .plans()
            .iter()
            .filter(|plan| {
                configuration.is_some_and(|config| {
                    config.methods().iter().any(|method| {
                        method.provider() == plan.provider()
                            && method.currency() == plan.currency()
                            && plan.amount_minor() >= method.min_amount_minor()
                            && plan.amount_minor() <= method.max_amount_minor()
                    })
                })
            })
            .map(SubscriptionCatalogPlanResponse::from_plan)
            .collect();
        Self { plans }
    }
}

impl SubscriptionCatalogPlanResponse {
    fn from_plan(plan: &SubscriptionCatalogPlan) -> Self {
        Self {
            plan_id: plan.plan_id().persistence_key(),
            name: plan.name().to_owned(),
            quota_amount: plan.quota_amount().units(),
            cycle: plan.cycle().into(),
            plan_version: plan.plan_version(),
            price_provider: plan.provider().to_owned(),
            price_currency: plan.currency().to_owned(),
            price_amount_minor: plan.amount_minor(),
        }
    }
}

/// 当前用户创建待支付订阅订单的请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SubscriptionOrderCreateRequest)]
pub(crate) struct SubscriptionOrderCreateRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    idempotency_key: String,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(minimum = 1)]
    plan_version: i64,
    #[schema(min_length = 1, max_length = 32)]
    price_provider: String,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    price_currency: String,
    #[schema(minimum = 1)]
    price_amount_minor: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SubscriptionOrderPaymentRequest)]
pub(crate) struct SubscriptionOrderPaymentRequest {
    #[schema(min_length = 1, max_length = 32)]
    payment_method: String,
}

/// 当前用户创建订阅订单后的服务端快照。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SubscriptionOrder)]
pub(crate) struct SubscriptionOrderResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    order_id: String,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(minimum = 1)]
    plan_version: i64,
    #[schema(min_length = 1, max_length = 32)]
    price_provider: String,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    price_currency: String,
    #[schema(minimum = 1)]
    price_amount_minor: i64,
    #[schema(minimum = 1)]
    quota_amount: i64,
    status: SubscriptionOrderStatusDto,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(minimum = 1, required = true)]
    expires_at: Option<i64>,
    #[schema(minimum = 1, required = true)]
    paid_at: Option<i64>,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    updated_at: i64,
    replayed: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SubscriptionPaymentResponse)]
pub(crate) struct SubscriptionPaymentResponse {
    #[schema(max_length = 256, required = true)]
    payment_intent_id: Option<String>,
    #[schema(max_length = 4096, required = true)]
    client_secret: Option<String>,
    #[schema(max_length = 16384, required = true)]
    redirect_url: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SubscriptionOrderPaymentResponse)]
pub(crate) struct SubscriptionOrderPaymentResponse {
    order: SubscriptionOrderResponse,
    payment: SubscriptionPaymentResponse,
}

impl SubscriptionOrderResponse {
    fn from_order(order: &SubscriptionOrder) -> Result<Self, ManagementError> {
        Ok(Self {
            order_id: order.order_id().persistence_key(),
            plan_id: order.plan_id().persistence_key(),
            plan_version: i64::try_from(order.plan_version())
                .map_err(|_| ManagementError::Internal)?,
            price_provider: order.provider().to_owned(),
            price_currency: order.currency().to_owned(),
            price_amount_minor: order.amount_minor(),
            quota_amount: order.quota_amount().units(),
            status: order.status().into(),
            version: i64::try_from(order.version()).map_err(|_| ManagementError::Internal)?,
            expires_at: optional_i64(order.expires_at())?,
            paid_at: optional_i64(order.paid_at())?,
            created_at: i64::try_from(order.created_at()).map_err(|_| ManagementError::Internal)?,
            updated_at: i64::try_from(order.updated_at()).map_err(|_| ManagementError::Internal)?,
            replayed: order.replayed(),
        })
    }
}

/// 管理员创建订阅计划的结构化请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminSubscriptionPlanCreateRequest)]
pub(crate) struct AdminSubscriptionPlanCreateRequest {
    #[schema(min_length = 1, max_length = 80)]
    name: String,
    #[schema(minimum = 1)]
    quota_amount: i64,
    cycle: SubscriptionCycleDto,
    #[schema(min_length = 1, max_length = 32)]
    price_provider: String,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    price_currency: String,
    #[schema(minimum = 1)]
    price_amount_minor: i64,
}

/// 管理员停用订阅计划的 CAS 请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminSubscriptionPlanDisableRequest)]
pub(crate) struct AdminSubscriptionPlanDisableRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

/// 当前用户或管理员可读取的用户订阅窗口。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserSubscription)]
pub(crate) struct UserSubscriptionResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    subscription_id: String,
    #[schema(minimum = 1)]
    user_id: i64,
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
    #[schema(min_length = 1, max_length = 80)]
    plan_name: String,
    #[schema(minimum = 1)]
    plan_version: i64,
    status: UserSubscriptionStatusDto,
    #[schema(minimum = 1)]
    quota_amount: i64,
    #[schema(minimum = 0)]
    quota_used: i64,
    cycle: SubscriptionCycleDto,
    #[schema(minimum = 1)]
    window_started_at: i64,
    #[schema(minimum = 1)]
    window_ends_at: i64,
    #[schema(minimum = 1)]
    version: i64,
    #[schema(minimum = 1)]
    bound_at: i64,
    #[schema(minimum = 1)]
    status_changed_at: i64,
    #[schema(minimum = 1)]
    updated_at: i64,
}

/// 用户订阅列表响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserSubscriptionListResponse)]
pub(crate) struct UserSubscriptionListResponse {
    #[schema(max_items = 100)]
    subscriptions: Vec<UserSubscriptionResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 管理员给路径指定用户绑定计划的请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserSubscriptionBindRequest)]
pub(crate) struct AdminUserSubscriptionBindRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    plan_id: String,
}

/// 管理员可提交的闭合订阅生命周期动作。
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminUserSubscriptionLifecycleAction, rename_all = "snake_case")]
pub(crate) enum AdminUserSubscriptionLifecycleActionDto {
    Suspend,
    Resume,
    Cancel,
}

impl From<AdminUserSubscriptionLifecycleActionDto> for AdminUserSubscriptionLifecycleAction {
    fn from(value: AdminUserSubscriptionLifecycleActionDto) -> Self {
        match value {
            AdminUserSubscriptionLifecycleActionDto::Suspend => Self::Suspend,
            AdminUserSubscriptionLifecycleActionDto::Resume => Self::Resume,
            AdminUserSubscriptionLifecycleActionDto::Cancel => Self::Cancel,
        }
    }
}

/// 管理员迁移用户订阅生命周期的 CAS 请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserSubscriptionLifecycleRequest)]
pub(crate) struct AdminUserSubscriptionLifecycleRequest {
    action: AdminUserSubscriptionLifecycleActionDto,
    #[schema(minimum = 1)]
    expected_version: i64,
}

/// 生命周期迁移后的订阅事实和窗口推进信息。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserSubscriptionLifecycleResponse)]
pub(crate) struct AdminUserSubscriptionLifecycleResponse {
    subscription: UserSubscriptionResponse,
    #[schema(minimum = 0)]
    periods_elapsed: u32,
}

/// 构建管理员订阅管理和当前用户订阅读取路由。
pub(crate) fn build_subscription_router(
    service: Arc<dyn SubscriptionService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    topup_service: Option<Arc<dyn UserTopupService>>,
) -> Router {
    let state = SubscriptionHttpState {
        service,
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
            "/api/admin/subscription-plans",
            get(list_admin_subscription_plans).post(create_admin_subscription_plan),
        )
        .route(
            "/api/admin/subscription-plans/{plan_id}/disable",
            post(disable_admin_subscription_plan),
        )
        .route(
            "/api/admin/users/{user_id}/subscriptions",
            get(list_admin_user_subscriptions).post(bind_admin_user_subscription),
        )
        .route(
            "/api/admin/users/{user_id}/subscriptions/{subscription_id}/lifecycle",
            post(transition_admin_user_subscription_lifecycle),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(admin_authentication)
        .with_state(state.clone());
    let user_routes = Router::new()
        .route(
            "/api/account/subscriptions",
            get(list_current_user_subscriptions),
        )
        .route(
            "/api/account/subscription-catalog",
            get(list_current_subscription_catalog),
        )
        .route(
            "/api/account/subscription-orders",
            post(create_current_subscription_order),
        )
        .route(
            "/api/account/subscription-orders/{order_id}",
            get(get_current_subscription_order),
        )
        .route(
            "/api/account/subscription-orders/{order_id}/payment",
            post(submit_current_subscription_order_payment),
        )
        .route_layer(user_authentication)
        .with_state(state);
    admin_routes.merge(user_routes)
}

async fn list_admin_subscription_plans(
    State(state): State<SubscriptionHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_page_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_plans(authentication.principal(), query)
        .await
        .map_err(map_service_error)?;
    let plans = page
        .plans()
        .iter()
        .map(AdminSubscriptionPlanResponse::from_plan)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_json(AdminSubscriptionPlanListResponse {
        plans,
        next_cursor: page.next_cursor(),
    }))
}

async fn create_admin_subscription_plan(
    State(state): State<SubscriptionHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminSubscriptionPlanCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminSubscriptionPlanCreateCommand::new(
        request.name,
        request.quota_amount,
        request.cycle.into(),
        request.price_provider,
        request.price_currency,
        request.price_amount_minor,
    )
    .map_err(map_service_error)?;
    let plan = state
        .service
        .create_plan(authentication.principal(), command)
        .await
        .map_err(map_service_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminSubscriptionPlanResponse::from_plan(&plan)?,
    ))
}

async fn disable_admin_subscription_plan(
    State(state): State<SubscriptionHttpState>,
    Path(plan_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminSubscriptionPlanDisableRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let plan_id = parse_plan_id(&plan_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminSubscriptionPlanDisableCommand::new(request.expected_version)
        .map_err(map_service_error)?;
    let plan = state
        .service
        .disable_plan(authentication.principal(), plan_id, command)
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(AdminSubscriptionPlanResponse::from_plan(
        &plan,
    )?))
}

async fn list_admin_user_subscriptions(
    State(state): State<SubscriptionHttpState>,
    Path(user_id): Path<String>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let user_id = parse_user_id(&user_id)?;
    let query = parse_page_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_user_subscriptions(authentication.principal(), user_id, query)
        .await
        .map_err(map_service_error)?;
    user_subscription_page_response(page)
}

async fn list_current_user_subscriptions(
    State(state): State<SubscriptionHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_page_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_current_subscriptions(authentication.principal(), query)
        .await
        .map_err(map_service_error)?;
    user_subscription_page_response(page)
}

async fn list_current_subscription_catalog(
    State(state): State<SubscriptionHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let catalog = state
        .service
        .list_catalog(authentication.principal())
        .await
        .map_err(map_service_error)?;
    let configured = state
        .topup_service
        .as_ref()
        .and_then(|service| service.configuration().ok());
    Ok(no_store_json(SubscriptionCatalogResponse::from_catalog(
        &catalog,
        configured.as_ref(),
    )))
}

async fn create_current_subscription_order(
    State(state): State<SubscriptionHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<SubscriptionOrderCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let request_id = SubscriptionOrderRequestId::from_persistence_key(&request.idempotency_key)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let plan_id = parse_plan_id(&request.plan_id)?;
    let configured = state
        .topup_service
        .as_ref()
        .and_then(|service| service.configuration().ok());
    let available = configured.is_some_and(|config| {
        config.methods().iter().any(|method| {
            method.provider() == request.price_provider
                && method.currency() == request.price_currency
                && request.price_amount_minor >= method.min_amount_minor()
                && request.price_amount_minor <= method.max_amount_minor()
        })
    });
    if !available {
        return Err(ManagementError::SubscriptionConflict);
    }
    let command = SubscriptionOrderCreateCommand::new(
        request_id,
        plan_id,
        request.plan_version,
        request.price_provider,
        request.price_currency,
        request.price_amount_minor,
    )
    .map_err(map_service_error)?;
    let order = state
        .service
        .create_order(authentication.principal(), command)
        .await
        .map_err(map_service_error)?;
    let status = if order.replayed() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(status_json(
        status,
        SubscriptionOrderResponse::from_order(&order)?,
    ))
}

/// 读取当前用户的订阅订单结果；跨用户订单不泄露存在性。
async fn get_current_subscription_order(
    State(state): State<SubscriptionHttpState>,
    Path(order_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let order_id = parse_order_id(&order_id)?;
    let order = state
        .service
        .get_order(authentication.principal(), order_id)
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(SubscriptionOrderResponse::from_order(
        &order,
    )?))
}

async fn submit_current_subscription_order_payment(
    State(state): State<SubscriptionHttpState>,
    Path(order_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<SubscriptionOrderPaymentRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let order_id = parse_order_id(&order_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        SubscriptionOrderPaymentCommand::new(request.payment_method).map_err(map_service_error)?;
    let order = state
        .service
        .get_order(authentication.principal(), order_id)
        .await
        .map_err(map_service_error)?;
    let provider = state
        .topup_service
        .as_ref()
        .ok_or(ManagementError::TopupUnavailable)?
        .provider_for(order.provider(), command.payment_method())
        .map_err(map_topup_provider_error)?;
    let result = state
        .service
        .submit_order(authentication.principal(), order_id, command, provider)
        .await
        .map_err(map_service_error)?;
    let payment = result.payment();
    Ok(no_store_json(SubscriptionOrderPaymentResponse {
        order: SubscriptionOrderResponse::from_order(result.order())?,
        payment: SubscriptionPaymentResponse {
            payment_intent_id: payment
                .client_secret()
                .map(|_| payment.payment_intent_id().to_owned()),
            client_secret: payment.client_secret().map(str::to_owned),
            redirect_url: payment.redirect_url().map(str::to_owned),
        },
    }))
}

async fn bind_admin_user_subscription(
    State(state): State<SubscriptionHttpState>,
    Path(user_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminUserSubscriptionBindRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let user_id = parse_user_id(&user_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminUserSubscriptionBindCommand::new(parse_plan_id(&request.plan_id)?);
    let subscription = state
        .service
        .bind_user(authentication.principal(), user_id, command)
        .await
        .map_err(map_service_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        UserSubscriptionResponse::from_subscription(&subscription)?,
    ))
}

async fn transition_admin_user_subscription_lifecycle(
    State(state): State<SubscriptionHttpState>,
    Path((user_id, subscription_id)): Path<(String, String)>,
    Extension(authentication): Extension<SessionAuthentication>,
    request: Result<Json<AdminUserSubscriptionLifecycleRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let user_id = parse_user_id(&user_id)?;
    let subscription_id = parse_subscription_id(&subscription_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        AdminUserSubscriptionLifecycleCommand::new(request.action.into(), request.expected_version)
            .map_err(map_service_error)?;
    let result = state
        .service
        .transition_user_lifecycle(
            authentication.principal(),
            user_id,
            subscription_id,
            command,
        )
        .await
        .map_err(map_service_error)?;
    Ok(no_store_json(
        AdminUserSubscriptionLifecycleResponse::from_result(&result)?,
    ))
}

impl AdminSubscriptionPlanResponse {
    fn from_plan(plan: &AdminSubscriptionPlan) -> Result<Self, ManagementError> {
        Ok(Self {
            plan_id: plan.plan_id().persistence_key(),
            name: plan.name().to_owned(),
            created_by_user_id: plan.created_by_user_id().get(),
            status: plan.status().into(),
            quota_amount: plan.quota_amount().units(),
            cycle: plan.cycle().into(),
            version: i64::try_from(plan.version()).map_err(|_| ManagementError::Internal)?,
            disabled_at: optional_i64(plan.disabled_at())?,
            created_at: i64::try_from(plan.created_at()).map_err(|_| ManagementError::Internal)?,
            updated_at: i64::try_from(plan.updated_at()).map_err(|_| ManagementError::Internal)?,
        })
    }
}

impl UserSubscriptionResponse {
    fn from_subscription(subscription: &AdminUserSubscription) -> Result<Self, ManagementError> {
        Ok(Self {
            subscription_id: subscription.subscription_id().persistence_key(),
            user_id: subscription.user_id().get(),
            plan_id: subscription.plan_id().persistence_key(),
            plan_name: subscription.plan_name().to_owned(),
            plan_version: i64::try_from(subscription.plan_version())
                .map_err(|_| ManagementError::Internal)?,
            status: subscription.status().into(),
            quota_amount: subscription.quota_amount().units(),
            quota_used: subscription.quota_used().units(),
            cycle: subscription.cycle().into(),
            window_started_at: i64::try_from(subscription.window_started_at())
                .map_err(|_| ManagementError::Internal)?,
            window_ends_at: i64::try_from(subscription.window_ends_at())
                .map_err(|_| ManagementError::Internal)?,
            version: i64::try_from(subscription.version())
                .map_err(|_| ManagementError::Internal)?,
            bound_at: i64::try_from(subscription.bound_at())
                .map_err(|_| ManagementError::Internal)?,
            status_changed_at: i64::try_from(subscription.status_changed_at())
                .map_err(|_| ManagementError::Internal)?,
            updated_at: i64::try_from(subscription.updated_at())
                .map_err(|_| ManagementError::Internal)?,
        })
    }
}

impl AdminUserSubscriptionLifecycleResponse {
    fn from_result(result: &AdminUserSubscriptionLifecycleResult) -> Result<Self, ManagementError> {
        Ok(Self {
            subscription: UserSubscriptionResponse::from_subscription(result.subscription())?,
            periods_elapsed: result.periods_elapsed(),
        })
    }
}

fn user_subscription_page_response(
    page: af_admin::AdminUserSubscriptionPage,
) -> Result<Response, ManagementError> {
    let subscriptions = page
        .subscriptions()
        .iter()
        .map(UserSubscriptionResponse::from_subscription)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_json(UserSubscriptionListResponse {
        subscriptions,
        next_cursor: page.next_cursor(),
    }))
}

fn parse_page_query(
    raw_query: Option<&str>,
) -> Result<AdminSubscriptionPageQuery, ManagementError> {
    let (before, limit) =
        parse_wallet_list_query(raw_query, af_admin::DEFAULT_ADMIN_SUBSCRIPTION_PAGE_SIZE)?;
    AdminSubscriptionPageQuery::new(before, limit).map_err(map_service_error)
}

fn parse_plan_id(value: &str) -> Result<SubscriptionPlanId, ManagementError> {
    SubscriptionPlanId::from_persistence_key(value).map_err(|_| ManagementError::InvalidRequest)
}

fn parse_user_id(value: &str) -> Result<UserId, ManagementError> {
    if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| UserId::new(value).ok())
        .ok_or(ManagementError::InvalidRequest)
}

fn parse_subscription_id(value: &str) -> Result<UserSubscriptionId, ManagementError> {
    UserSubscriptionId::from_persistence_key(value).map_err(|_| ManagementError::InvalidRequest)
}

fn parse_order_id(value: &str) -> Result<SubscriptionOrderId, ManagementError> {
    SubscriptionOrderId::from_persistence_key(value).map_err(|_| ManagementError::InvalidRequest)
}

fn optional_i64(value: Option<u64>) -> Result<Option<i64>, ManagementError> {
    value
        .map(i64::try_from)
        .transpose()
        .map_err(|_| ManagementError::Internal)
}

fn map_service_error(error: SubscriptionServiceError) -> ManagementError {
    match error {
        SubscriptionServiceError::InvalidInput => ManagementError::InvalidRequest,
        SubscriptionServiceError::Forbidden => ManagementError::Forbidden,
        SubscriptionServiceError::InvalidSession => ManagementError::InvalidSession,
        SubscriptionServiceError::PlanNotFound => ManagementError::SubscriptionPlanNotFound,
        SubscriptionServiceError::PlanDisabled => ManagementError::SubscriptionPlanDisabled,
        SubscriptionServiceError::UserNotFound => ManagementError::UserNotFound,
        SubscriptionServiceError::SubscriptionNotFound => ManagementError::SubscriptionNotFound,
        SubscriptionServiceError::InvalidTransition => {
            ManagementError::SubscriptionTransitionInvalid
        }
        SubscriptionServiceError::InUse => ManagementError::SubscriptionInUse,
        SubscriptionServiceError::Conflict => ManagementError::SubscriptionConflict,
        SubscriptionServiceError::OutcomeUnknown => ManagementError::SubscriptionOutcomeUnknown,
        SubscriptionServiceError::PaymentUnavailable => ManagementError::TopupUnavailable,
        SubscriptionServiceError::PaymentRejected => ManagementError::TopupProviderRejected,
        SubscriptionServiceError::Internal => ManagementError::Internal,
    }
}

fn map_topup_provider_error(error: UserTopupError) -> ManagementError {
    match error {
        UserTopupError::Unavailable => ManagementError::TopupUnavailable,
        UserTopupError::Conflict => ManagementError::SubscriptionConflict,
        UserTopupError::OutcomeUnknown => ManagementError::SubscriptionOutcomeUnknown,
        UserTopupError::ProviderRejected => ManagementError::TopupProviderRejected,
        UserTopupError::InvalidInput
        | UserTopupError::InvalidSession
        | UserTopupError::Internal => ManagementError::Internal,
    }
}

fn status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_and_pagination_reject_ambiguous_inputs() {
        assert!(parse_plan_id("11111111111111111111111111111111").is_ok());
        assert!(parse_subscription_id("22222222222222222222222222222222").is_ok());
        assert_eq!(
            parse_subscription_id("2222"),
            Err(ManagementError::InvalidRequest)
        );
        assert_eq!(parse_user_id("42").unwrap().get(), 42);
        for value in ["", "0", "+1", "-1", "1.0", "abc"] {
            assert_eq!(parse_user_id(value), Err(ManagementError::InvalidRequest));
        }
        for raw_query in ["before=0", "limit=0", "limit=101", "limit=1&limit=2"] {
            assert_eq!(
                parse_page_query(Some(raw_query)),
                Err(ManagementError::InvalidRequest),
                "{raw_query}"
            );
        }
    }
}
