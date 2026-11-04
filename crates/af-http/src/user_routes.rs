use std::sync::Arc;

use af_admin::{
    SessionAuthentication, SessionAuthenticator, UserRouteError, UserRouteMemberWrite,
    UserRouteRecord, UserRouteService, UserRouteWrite,
};
use af_domain::{GroupId, UserRouteStrategy};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

/// 用户路由可选策略的稳定 API 枚举。
#[derive(ToSchema)]
#[schema(rename_all = "snake_case")]
pub(crate) enum UserRouteStrategySchema {
    PriceFirst,
    StableFirst,
    Ordered,
    RoundRobin,
    Weighted,
}

#[derive(Clone)]
struct UserRouteHttpState {
    service: Arc<dyn UserRouteService>,
}

/// 自建路由只要求用户登录，所有权由应用服务从会话读取。
pub fn build_user_route_router(
    service: Arc<dyn UserRouteService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/user-routes", get(list_routes).post(create_route))
        .route(
            "/api/user-routes/{id}",
            get(get_route).put(update_route).delete(delete_route),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(RequestBodyLimitLayer::new(16 * 1024))
        .layer(authentication)
        .with_state(UserRouteHttpState { service })
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserRouteMember)]
pub(crate) struct UserRouteMemberDto {
    #[schema(minimum = 1)]
    group_id: i64,
    #[schema(minimum = 0)]
    priority: i32,
    #[schema(minimum = 1, maximum = 10000)]
    weight: u32,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserRouteWriteRequest)]
pub(crate) struct UserRouteWriteRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(value_type = UserRouteStrategySchema)]
    strategy: UserRouteStrategy,
    enabled: bool,
    #[schema(minimum = 1, maximum = 16)]
    max_group_attempts: u32,
    #[schema(minimum = 0, required = true)]
    max_price_ratio_micros: Option<i64>,
    #[schema(min_items = 1, max_items = 16)]
    members: Vec<UserRouteMemberDto>,
}

impl UserRouteWriteRequest {
    fn into_write(self) -> Result<UserRouteWrite, ManagementError> {
        let members = self
            .members
            .into_iter()
            .map(|member| {
                Ok(UserRouteMemberWrite {
                    group_id: GroupId::new(member.group_id)
                        .map_err(|_| ManagementError::InvalidRequest)?,
                    priority: member.priority,
                    weight: member.weight,
                })
            })
            .collect::<Result<Vec<_>, ManagementError>>()?;
        let write = UserRouteWrite {
            name: self.name,
            strategy: self.strategy,
            enabled: self.enabled,
            max_group_attempts: self.max_group_attempts,
            max_price_ratio_micros: self.max_price_ratio_micros,
            members,
        };
        write.validate().map_err(map_error)?;
        Ok(write)
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserRoute)]
pub(crate) struct UserRouteResponse {
    #[schema(minimum = 1)]
    id: i64,
    name: String,
    #[schema(value_type = UserRouteStrategySchema)]
    strategy: UserRouteStrategy,
    enabled: bool,
    #[schema(minimum = 1, maximum = 16)]
    max_group_attempts: u32,
    #[schema(minimum = 0, required = true)]
    max_price_ratio_micros: Option<i64>,
    #[schema(min_items = 1, max_items = 16)]
    members: Vec<UserRouteMemberDto>,
}

impl From<UserRouteRecord> for UserRouteResponse {
    fn from(route: UserRouteRecord) -> Self {
        Self {
            id: route.id,
            name: route.name,
            strategy: route.strategy,
            enabled: route.enabled,
            max_group_attempts: route.max_group_attempts,
            max_price_ratio_micros: route.max_price_ratio_micros,
            members: route
                .members
                .into_iter()
                .map(|member| UserRouteMemberDto {
                    group_id: member.group_id.get(),
                    priority: member.priority,
                    weight: member.weight,
                })
                .collect(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserRouteListResponse)]
pub(crate) struct UserRouteListResponse {
    #[schema(max_items = 32)]
    items: Vec<UserRouteResponse>,
    #[schema(minimum = 32, maximum = 32)]
    capacity: usize,
}

async fn list_routes(
    State(state): State<UserRouteHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let routes = state
        .service
        .list(auth.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(UserRouteListResponse {
        items: routes.into_iter().map(Into::into).collect(),
        capacity: af_domain::MAX_USER_ROUTES,
    }))
}

async fn get_route(
    State(state): State<UserRouteHttpState>,
    Path(id): Path<String>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let route = state
        .service
        .get(auth.principal(), parse_id(&id)?)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(UserRouteResponse::from(route)))
}

async fn create_route(
    State(state): State<UserRouteHttpState>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<UserRouteWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(body) = body.map_err(|_| ManagementError::InvalidRequest)?;
    let route = state
        .service
        .create(auth.principal(), body.into_write()?)
        .await
        .map_err(map_error)?;
    let mut response = no_store_json(UserRouteResponse::from(route));
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

async fn update_route(
    State(state): State<UserRouteHttpState>,
    Path(id): Path<String>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<UserRouteWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(body) = body.map_err(|_| ManagementError::InvalidRequest)?;
    let route = state
        .service
        .update(auth.principal(), parse_id(&id)?, body.into_write()?)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(UserRouteResponse::from(route)))
}

async fn delete_route(
    State(state): State<UserRouteHttpState>,
    Path(id): Path<String>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    state
        .service
        .delete(auth.principal(), parse_id(&id)?)
        .await
        .map_err(map_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn parse_id(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or(ManagementError::InvalidRequest)
}

fn map_error(error: UserRouteError) -> ManagementError {
    match error {
        UserRouteError::InvalidInput => ManagementError::InvalidRequest,
        UserRouteError::InvalidSession => ManagementError::InvalidSession,
        UserRouteError::NotFound => ManagementError::UserRouteNotFound,
        UserRouteError::LimitReached => ManagementError::UserRouteLimitReached,
        UserRouteError::Unavailable => ManagementError::UserRouteUnavailable,
        UserRouteError::Internal => ManagementError::Internal,
    }
}
