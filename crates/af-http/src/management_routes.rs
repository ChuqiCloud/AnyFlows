use af_admin::{
    AdminRoute, AdminRouteChannel, AdminRouteListQuery, AdminRoutePage, AdminRouteReadError,
};
use af_domain::RouteId;
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

/// 管理端返回的路由候选及其运行时健康统计。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRouteChannelResponse)]
pub(crate) struct AdminRouteChannelResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    channel_id: i64,
    #[schema(minimum = 1)]
    credential_id: i64,
    #[schema(minimum = 0)]
    priority: i32,
    #[schema(minimum = 0)]
    weight: i32,
    enabled: bool,
    #[schema(minimum = 0)]
    success_count: i64,
    #[schema(minimum = 0)]
    fail_count: i64,
    #[schema(minimum = 0)]
    total_latency_ms: i64,
    #[schema(minimum = 0, maximum = 3)]
    cooldown_level: i16,
    /// Unix 秒时间戳；为空表示当前没有冷却截止时间。
    cooldown_until: Option<i64>,
    /// Unix 秒时间戳；为空表示该候选尚未被选中。
    last_selected_at: Option<i64>,
    /// Unix 秒时间戳；为空表示该候选尚未失败。
    last_failure_at: Option<i64>,
}

impl AdminRouteChannelResponse {
    fn from_channel(channel: &AdminRouteChannel) -> Self {
        Self {
            id: channel.id().get(),
            channel_id: channel.channel_id().get(),
            credential_id: channel.credential_id().get(),
            priority: channel.priority(),
            weight: channel.weight(),
            enabled: channel.enabled(),
            success_count: channel.success_count(),
            fail_count: channel.fail_count(),
            total_latency_ms: channel.total_latency(),
            cooldown_level: channel.cooldown_level(),
            cooldown_until: channel.cooldown_until_epoch_seconds(),
            last_selected_at: channel.last_selected_at_epoch_seconds(),
            last_failure_at: channel.last_failure_at_epoch_seconds(),
        }
    }
}

/// 管理端返回的智能路由规则快照。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRoute)]
pub(crate) struct AdminRouteResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(min_length = 1, max_length = 255)]
    model_pattern: String,
    #[schema(value_type = crate::openapi::schema::AdminRouteModeSchema)]
    mode: String,
    #[schema(value_type = crate::openapi::schema::AdminRouteStrategySchema)]
    strategy: String,
    /// Canonical 模型到上游模型的可选映射对象。
    #[schema(schema_with = crate::openapi::schema::free_form_object_schema)]
    model_mapping: serde_json::Value,
    enabled: bool,
    #[schema(max_items = 64)]
    channels: Vec<AdminRouteChannelResponse>,
}

impl AdminRouteResponse {
    pub(crate) fn from_route(route: &AdminRoute) -> Self {
        Self {
            id: route.route_id().get(),
            name: route.name().to_owned(),
            model_pattern: route.model_pattern().to_owned(),
            mode: route.mode().as_str().to_owned(),
            strategy: route.strategy().as_str().to_owned(),
            model_mapping: route.model_mapping().clone(),
            enabled: route.enabled(),
            channels: route
                .channels()
                .iter()
                .map(AdminRouteChannelResponse::from_channel)
                .collect(),
        }
    }
}

/// 管理端返回的路由列表与下一页游标。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRouteListResponse)]
pub(crate) struct AdminRouteListResponse {
    #[schema(max_items = 100)]
    routes: Vec<AdminRouteResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

impl AdminRouteListResponse {
    fn from_page(page: AdminRoutePage) -> Self {
        Self {
            routes: page
                .routes()
                .iter()
                .map(AdminRouteResponse::from_route)
                .collect(),
            next_cursor: page.next_cursor().map(RouteId::get),
        }
    }
}

/// 返回管理员可见的智能路由列表，按稳定路由 ID 游标分页。
pub(crate) async fn list_admin_routes(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let reader = state
        .admin_route_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = reader
        .list(authentication.principal(), query)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminRouteListResponse::from_page(page)))
}

/// 返回单个未软删除路由及候选运行统计。
pub(crate) async fn get_admin_route(
    State(state): State<HttpState>,
    Path(route_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let route_id = parse_route_id(&route_id)?;
    let reader = state
        .admin_route_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let route = reader
        .get(authentication.principal(), route_id)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminRouteResponse::from_route(&route)))
}

pub(crate) fn parse_route_id(value: &str) -> Result<RouteId, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<i64>()
        .ok()
        .and_then(|value| RouteId::new(value).ok())
        .ok_or(ManagementError::InvalidRequest)
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminRouteListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminRouteListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminRouteListQuery::default());
    }
    validate_percent_encoding(raw_query)?;
    let mut seen_after = false;
    let mut seen_limit = false;
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" => {
                if seen_after {
                    return Err(ManagementError::InvalidRequest);
                }
                seen_after = true;
                after = Some(parse_route_id(&value)?);
            }
            "limit" => {
                if seen_limit {
                    return Err(ManagementError::InvalidRequest);
                }
                seen_limit = true;
                limit = Some(parse_limit(&value)?);
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminRouteListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_ROUTE_PAGE_SIZE),
    )
    .map_err(map_read_error)
}

fn parse_limit(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn map_read_error(error: AdminRouteReadError) -> ManagementError {
    match error {
        AdminRouteReadError::InvalidPagination => ManagementError::InvalidRequest,
        AdminRouteReadError::Forbidden => ManagementError::Forbidden,
        AdminRouteReadError::NotFound => ManagementError::RouteNotFound,
        AdminRouteReadError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_id_parser_requires_positive_decimal_text() {
        assert_eq!(parse_route_id("42").unwrap().get(), 42);
        for value in ["", "0", "+1", "-1", "1.0", "abc"] {
            assert_eq!(
                parse_route_id(value),
                Err(ManagementError::InvalidRequest),
                "{value}"
            );
        }
    }

    #[test]
    fn list_query_parser_rejects_duplicates_and_unknown_keys() {
        let query = parse_list_query(Some("after=3&limit=7")).unwrap();
        assert_eq!(query.after().unwrap().get(), 3);
        assert_eq!(query.limit(), 7);
        for raw_query in ["limit=1&limit=2", "after=1&unknown=x", "limit=%ZZ"] {
            assert_eq!(
                parse_list_query(Some(raw_query)),
                Err(ManagementError::InvalidRequest),
                "{raw_query}"
            );
        }
    }
}
