use std::sync::Arc;

use af_admin::{
    PasskeyAuthenticationCommand, PasskeyAuthenticationError, PasskeyAuthenticationService,
    SessionAuthenticator,
};
use af_config::ServerConfig;
use af_domain::TrustedClientIp;
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Request, State, rejection::JsonRejection},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;

use crate::{
    client_ip::{ClientIpResolutionError, ClientIpResolver},
    management_error::ManagementError,
    management_session::{issue_session_for_user, no_store_json},
};

const PASSKEY_AUTHENTICATION_BODY_LIMIT_BYTES: usize = 64 * 1024;

/// Passkey 登录路由独立持有的认证服务和会话签发器。
#[derive(Clone)]
struct PasskeyAuthenticationHttpState {
    service: Option<Arc<dyn PasskeyAuthenticationService>>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
}

/// 用户名优先的 Passkey 登录请求。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PasskeyAuthenticationOptionsRequest)]
pub(crate) struct PasskeyAuthenticationOptionsRequest {
    #[schema(min_length = 1, max_length = 64)]
    username: String,
}

/// 服务端生成的 WebAuthn 请求选项；挑战摘要不会暴露给客户端。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PasskeyAuthenticationOptionsResponse)]
pub(crate) struct PasskeyAuthenticationOptionsResponse {
    options: Value,
}

/// 浏览器返回的 WebAuthn 认证结果。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PasskeyAuthenticationVerifyRequest)]
pub(crate) struct PasskeyAuthenticationVerifyRequest {
    credential: Value,
}

/// 构建公开 Passkey 登录端点。未配置受信 HTTPS Origin 时仍保留统一失败响应。
pub(crate) fn build_passkey_authentication_router(
    service: Option<Arc<dyn PasskeyAuthenticationService>>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    server_config: &ServerConfig,
) -> Router {
    let client_ip = middleware::from_fn_with_state(
        ClientIpResolver::from_config(server_config),
        resolve_passkey_client_ip,
    );
    Router::new()
        .route(
            "/api/auth/passkey/options",
            post(start_passkey_authentication).route_layer(client_ip),
        )
        .route(
            "/api/auth/passkey/verify",
            post(finish_passkey_authentication),
        )
        .layer(DefaultBodyLimit::max(
            PASSKEY_AUTHENTICATION_BODY_LIMIT_BYTES,
        ))
        .layer(RequestBodyLimitLayer::new(
            PASSKEY_AUTHENTICATION_BODY_LIMIT_BYTES,
        ))
        .with_state(PasskeyAuthenticationHttpState {
            service,
            session_authenticator,
        })
}

/// 按用户名创建一次性 WebAuthn 认证挑战。
async fn start_passkey_authentication(
    State(state): State<PasskeyAuthenticationHttpState>,
    Extension(client_ip): Extension<TrustedClientIp>,
    request: Result<Json<PasskeyAuthenticationOptionsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let service = state
        .service
        .as_ref()
        .ok_or(ManagementError::InvalidCredentials)?;
    let options = service
        .start(client_ip, &request.username)
        .await
        .map_err(map_passkey_authentication_error)?;
    Ok(no_store_json(PasskeyAuthenticationOptionsResponse {
        options: options.options().clone(),
    }))
}

async fn resolve_passkey_client_ip(
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
                target: "af_http::passkey_auth",
                error_kind = "passkey_client_ip_missing_peer",
                "Passkey 登录入口缺少 TCP 对端信息"
            );
            ManagementError::Internal.into_response()
        }
        Err(ClientIpResolutionError::InvalidProxyChain) => {
            ManagementError::InvalidRequest.into_response()
        }
    }
}

/// 验证浏览器认证结果，并复用现有 bearer 会话签发流程。
async fn finish_passkey_authentication(
    State(state): State<PasskeyAuthenticationHttpState>,
    request: Result<Json<PasskeyAuthenticationVerifyRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = PasskeyAuthenticationCommand::new(request.credential)
        .map_err(map_passkey_authentication_error)?;
    let service = state
        .service
        .as_ref()
        .ok_or(ManagementError::InvalidCredentials)?;
    let user_id = service
        .finish(command)
        .await
        .map_err(map_passkey_authentication_error)?;
    issue_session_for_user(state.session_authenticator.as_ref(), user_id).await
}

fn map_passkey_authentication_error(error: PasskeyAuthenticationError) -> ManagementError {
    match error {
        // 用户、挑战、凭证和克隆状态都收敛为同一认证失败，避免泄露账户与设备事实。
        PasskeyAuthenticationError::InvalidInput | PasskeyAuthenticationError::Rejected => {
            ManagementError::InvalidCredentials
        }
        PasskeyAuthenticationError::LoginDisabled => ManagementError::LoginDisabled,
        PasskeyAuthenticationError::Internal => ManagementError::Internal,
    }
}
