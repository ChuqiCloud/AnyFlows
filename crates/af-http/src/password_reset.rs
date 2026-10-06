use std::sync::Arc;

use af_admin::{
    PasswordResetConfirmCommand, PasswordResetError, PasswordResetRequestCommand,
    PasswordResetService,
};
use af_config::ServerConfig;
use af_domain::TrustedClientIp;
use axum::{
    Extension, Json, Router,
    extract::{Request, State, rejection::JsonRejection},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    client_ip::{ClientIpResolutionError, ClientIpResolver},
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct PasswordResetHttpState {
    service: Arc<dyn PasswordResetService>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PasswordResetRequest)]
pub(crate) struct PasswordResetRequestBody {
    #[schema(min_length = 3, max_length = 320, format = Email)]
    email: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PasswordResetRequestResponse)]
pub(crate) struct PasswordResetRequestResponse {
    accepted: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PasswordResetConfirmRequest)]
pub(crate) struct PasswordResetConfirmRequest {
    #[schema(min_length = 60, max_length = 100, write_only)]
    token: String,
    #[schema(min_length = 12, max_length = 128, format = Password, write_only)]
    password: String,
}

/// 构建游客可访问的忘记密码与密码重置路由。
pub(crate) fn build_password_reset_router(
    service: Arc<dyn PasswordResetService>,
    server_config: &ServerConfig,
) -> Router {
    let state = PasswordResetHttpState { service };
    let client_ip = middleware::from_fn_with_state(
        ClientIpResolver::from_config(server_config),
        resolve_password_reset_client_ip,
    );
    Router::new()
        .route(
            "/api/auth/password-reset/request",
            post(request_password_reset).route_layer(client_ip),
        )
        .route(
            "/api/auth/password-reset/confirm",
            post(confirm_password_reset),
        )
        .with_state(state)
}

/// 接受忘记密码请求，但不回显邮箱是否对应有效账户。
async fn request_password_reset(
    State(state): State<PasswordResetHttpState>,
    Extension(client_ip): Extension<TrustedClientIp>,
    request: Result<Json<PasswordResetRequestBody>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        PasswordResetRequestCommand::new(request.email).map_err(map_password_reset_error)?;
    let result = state
        .service
        .request(client_ip, &command)
        .await
        .map_err(map_password_reset_error)?;
    let mut response = no_store_json(PasswordResetRequestResponse {
        accepted: result.accepted_flag(),
    });
    *response.status_mut() = StatusCode::ACCEPTED;
    Ok(response)
}

/// 消费单次重置令牌并在成功后撤销已有会话。
async fn confirm_password_reset(
    State(state): State<PasswordResetHttpState>,
    request: Result<Json<PasswordResetConfirmRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = PasswordResetConfirmCommand::new(request.token, request.password)
        .map_err(map_password_reset_error)?;
    state
        .service
        .confirm(command)
        .await
        .map_err(map_password_reset_error)?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn resolve_password_reset_client_ip(
    State(resolver): State<ClientIpResolver>,
    mut request: Request,
    next: Next,
) -> Response {
    match resolver.resolve(&mut request) {
        Ok(client_ip) => {
            request.extensions_mut().insert(client_ip);
            next.run(request).await
        }
        Err(ClientIpResolutionError::MissingPeer) => {
            tracing::error!(
                target: "af_http::password_reset",
                error_kind = "password_reset_client_ip_missing_peer",
                "忘记密码入口缺少 TCP 对端信息"
            );
            ManagementError::Internal.into_response()
        }
        Err(ClientIpResolutionError::InvalidProxyChain) => {
            ManagementError::InvalidRequest.into_response()
        }
    }
}

fn map_password_reset_error(error: PasswordResetError) -> ManagementError {
    match error {
        PasswordResetError::InvalidInput => ManagementError::InvalidRequest,
        PasswordResetError::Rejected => ManagementError::PasswordResetRejected,
        PasswordResetError::EmailNotConfigured => ManagementError::EmailNotConfigured,
        PasswordResetError::EmailDeliveryFailed => ManagementError::EmailDeliveryFailed,
        PasswordResetError::Internal => ManagementError::Internal,
    }
}
