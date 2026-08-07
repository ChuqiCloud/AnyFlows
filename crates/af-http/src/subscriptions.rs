use std::sync::Arc;

use af_admin::{
    AdminSubscriptionPageQuery, AdminSubscriptionPlan, AdminSubscriptionPlanCreateCommand,
    AdminSubscriptionPlanDisableCommand, AdminUserSubscription, AdminUserSubscriptionBindCommand,
    SessionAuthentication, SessionAuthenticator, SubscriptionService, SubscriptionServiceError,
};
use af_domain::{
    SubscriptionCycle, SubscriptionPlanId, SubscriptionPlanStatus, UserId, UserSubscriptionStatus,
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

/// 构建管理员订阅管理和当前用户订阅读取路由。
pub(crate) fn build_subscription_router(
    service: Arc<dyn SubscriptionService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let state = SubscriptionHttpState { service };
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
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(admin_authentication)
        .with_state(state.clone());
    let user_routes = Router::new()
        .route(
            "/api/account/subscriptions",
            get(list_current_user_subscriptions),
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
        SubscriptionServiceError::Conflict => ManagementError::SubscriptionConflict,
        SubscriptionServiceError::OutcomeUnknown => ManagementError::SubscriptionOutcomeUnknown,
        SubscriptionServiceError::Internal => ManagementError::Internal,
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
