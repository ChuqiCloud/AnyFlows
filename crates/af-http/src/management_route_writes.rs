use af_admin::{
    AdminRouteChannelCommand, AdminRouteCreateCommand, AdminRouteUpdateCommand,
    AdminRouteWriteError,
};
use af_domain::{ChannelId, CredentialId, RouteMode, RouteStrategy};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_routes::{AdminRouteResponse, parse_route_id},
};

/// 管理端写入路由候选的结构化字段。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRouteChannelWriteRequest)]
pub(crate) struct AdminRouteChannelWriteRequest {
    #[schema(minimum = 1)]
    channel_id: i64,
    #[schema(minimum = 1)]
    credential_id: i64,
    #[schema(minimum = 0)]
    priority: i32,
    #[schema(minimum = 0)]
    weight: i32,
    enabled: bool,
}

/// 管理端创建或完整更新路由的请求正文。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminRouteWriteRequest)]
pub(crate) struct AdminRouteWriteRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(min_length = 1, max_length = 255)]
    model_pattern: String,
    #[schema(value_type = crate::openapi::schema::AdminRouteModeSchema)]
    mode: String,
    #[schema(value_type = crate::openapi::schema::AdminRouteStrategySchema)]
    strategy: String,
    #[schema(schema_with = crate::openapi::schema::free_form_object_schema)]
    model_mapping: serde_json::Value,
    enabled: bool,
    #[schema(max_items = 64)]
    channels: Vec<AdminRouteChannelWriteRequest>,
}

/// 创建一个管理端智能路由。
pub(crate) async fn create_admin_route(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminRouteWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let writer = state
        .admin_route_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let route = writer
        .create(authentication.principal(), request.into_create_command()?)
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminRouteResponse::from_route(&route),
    ))
}

/// 完整更新一个未软删除智能路由及其候选。
pub(crate) async fn update_admin_route(
    State(state): State<HttpState>,
    Path(route_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminRouteWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let route_id = parse_route_id(&route_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let writer = state
        .admin_route_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let route = writer
        .update(
            authentication.principal(),
            route_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminRouteResponse::from_route(&route)))
}

/// 软删除一个智能路由及其候选配置。
pub(crate) async fn delete_admin_route(
    State(state): State<HttpState>,
    Path(route_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let route_id = parse_route_id(&route_id)?;
    let writer = state
        .admin_route_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    writer
        .delete(authentication.principal(), route_id)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminRouteWriteRequest {
    fn into_create_command(self) -> Result<AdminRouteCreateCommand, ManagementError> {
        let (name, model_pattern, mode, strategy, model_mapping, enabled, channels) =
            self.into_parts()?;
        AdminRouteCreateCommand::new(
            name,
            model_pattern,
            mode,
            strategy,
            model_mapping,
            enabled,
            channels,
        )
        .map_err(map_write_error)
    }

    fn into_update_command(self) -> Result<AdminRouteUpdateCommand, ManagementError> {
        let (name, model_pattern, mode, strategy, model_mapping, enabled, channels) =
            self.into_parts()?;
        AdminRouteUpdateCommand::new(
            name,
            model_pattern,
            mode,
            strategy,
            model_mapping,
            enabled,
            channels,
        )
        .map_err(map_write_error)
    }

    fn into_parts(
        self,
    ) -> Result<
        (
            String,
            String,
            RouteMode,
            RouteStrategy,
            serde_json::Value,
            bool,
            Vec<AdminRouteChannelCommand>,
        ),
        ManagementError,
    > {
        let mode = parse_mode(&self.mode)?;
        let strategy = parse_strategy(&self.strategy)?;
        let channels = self
            .channels
            .into_iter()
            .map(AdminRouteChannelWriteRequest::into_command)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((
            self.name,
            self.model_pattern,
            mode,
            strategy,
            self.model_mapping,
            self.enabled,
            channels,
        ))
    }
}

impl AdminRouteChannelWriteRequest {
    fn into_command(self) -> Result<AdminRouteChannelCommand, ManagementError> {
        let channel_id =
            ChannelId::new(self.channel_id).map_err(|_| ManagementError::InvalidRequest)?;
        let credential_id =
            CredentialId::new(self.credential_id).map_err(|_| ManagementError::InvalidRequest)?;
        AdminRouteChannelCommand::new(
            channel_id,
            credential_id,
            self.priority,
            self.weight,
            self.enabled,
        )
        .map_err(map_write_error)
    }
}

fn parse_mode(value: &str) -> Result<RouteMode, ManagementError> {
    match value {
        "pattern" => Ok(RouteMode::Pattern),
        "explicit_group" => Ok(RouteMode::ExplicitGroup),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_strategy(value: &str) -> Result<RouteStrategy, ManagementError> {
    match value {
        "weighted" => Ok(RouteStrategy::Weighted),
        "round_robin" => Ok(RouteStrategy::RoundRobin),
        "stable_first" => Ok(RouteStrategy::StableFirst),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn map_write_error(error: AdminRouteWriteError) -> ManagementError {
    match error {
        AdminRouteWriteError::InvalidInput => ManagementError::InvalidRequest,
        AdminRouteWriteError::Forbidden => ManagementError::Forbidden,
        AdminRouteWriteError::Conflict => ManagementError::RouteConflict,
        AdminRouteWriteError::NotFound => ManagementError::RouteNotFound,
        AdminRouteWriteError::InvalidReference => ManagementError::RouteInvalidReference,
        AdminRouteWriteError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl serde::Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn status_json(status: StatusCode, value: impl serde::Serialize) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = status;
    response
}

fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_mode_and_strategy_accept_only_persisted_values() {
        assert_eq!(parse_mode("pattern"), Ok(RouteMode::Pattern));
        assert_eq!(parse_mode("explicit_group"), Ok(RouteMode::ExplicitGroup));
        assert_eq!(parse_strategy("weighted"), Ok(RouteStrategy::Weighted));
        assert_eq!(parse_strategy("round_robin"), Ok(RouteStrategy::RoundRobin));
        assert_eq!(
            parse_strategy("stable_first"),
            Ok(RouteStrategy::StableFirst)
        );
        assert_eq!(parse_mode("regex"), Err(ManagementError::InvalidRequest));
        assert_eq!(
            parse_strategy("random"),
            Err(ManagementError::InvalidRequest)
        );
    }

    #[test]
    fn channel_command_rejects_negative_order_values() {
        let request = AdminRouteChannelWriteRequest {
            channel_id: 1,
            credential_id: 2,
            priority: -1,
            weight: 1,
            enabled: true,
        };
        assert_eq!(request.into_command(), Err(ManagementError::InvalidRequest));
    }
}
