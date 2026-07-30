use std::sync::Arc;

use af_admin::{
    RegistrationCommand, RegistrationError, RegistrationPolicy, RegistrationPolicyCommand,
    RegistrationService, SessionAuthenticator,
};
use af_config::ServerConfig;
use af_domain::{GroupId, TrustedClientIp};
use axum::{
    Extension, Json, Router,
    extract::{Request, State, rejection::JsonRejection},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    client_ip::{ClientIpResolutionError, ClientIpResolver},
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::{login_with_authenticator, no_store_json},
};

#[derive(Clone)]
struct RegistrationHttpState {
    service: Arc<dyn RegistrationService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RegistrationStatusResponse)]
pub(crate) struct RegistrationStatusResponse {
    enabled: bool,
    email_required: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RegistrationRequest)]
pub(crate) struct RegistrationRequest {
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(min_length = 3, max_length = 320, format = Email)]
    email: Option<String>,
    #[schema(min_length = 12, max_length = 128, format = Password, write_only)]
    password: String,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RegistrationPolicyResponse)]
pub(crate) struct RegistrationPolicyResponse {
    enabled: bool,
    #[schema(minimum = 1)]
    default_group_id: i64,
    #[schema(minimum = 0)]
    initial_quota: i64,
    email_required: bool,
    #[schema(minimum = 1, maximum = 100)]
    rate_limit_attempts: u32,
    #[schema(minimum = 60, maximum = 86400)]
    rate_limit_window_seconds: u64,
}

impl From<RegistrationPolicy> for RegistrationPolicyResponse {
    fn from(policy: RegistrationPolicy) -> Self {
        Self {
            enabled: policy.enabled(),
            default_group_id: policy.default_group_id().get(),
            initial_quota: policy.initial_quota(),
            email_required: policy.email_required(),
            rate_limit_attempts: policy.rate_limit_attempts(),
            rate_limit_window_seconds: policy.rate_limit_window_seconds(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RegistrationPolicyRequest)]
pub(crate) struct RegistrationPolicyRequest {
    enabled: bool,
    #[schema(minimum = 1)]
    default_group_id: i64,
    #[schema(minimum = 0)]
    initial_quota: i64,
    email_required: bool,
    #[schema(minimum = 1, maximum = 100)]
    rate_limit_attempts: u32,
    #[schema(minimum = 60, maximum = 86400)]
    rate_limit_window_seconds: u64,
}

/// 构建公开注册与管理员注册策略路由，并复用统一会话和客户端 IP 信任边界。
pub(crate) fn build_registration_router(
    service: Arc<dyn RegistrationService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    server_config: &ServerConfig,
) -> Router {
    let state = RegistrationHttpState {
        service,
        session_authenticator: Arc::clone(&session_authenticator),
    };
    let registration_ip = middleware::from_fn_with_state(
        ClientIpResolver::from_config(server_config),
        resolve_registration_client_ip,
    );
    let policy_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );

    Router::new()
        .route("/api/registration/status", get(registration_status))
        .route(
            "/api/registration",
            post(register).route_layer(registration_ip),
        )
        .route(
            "/api/admin/registration",
            get(get_registration_policy)
                .put(update_registration_policy)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(policy_authentication),
        )
        .with_state(state)
}

/// 返回失败关闭的公开注册状态。
async fn registration_status(
    State(state): State<RegistrationHttpState>,
) -> Result<Response, ManagementError> {
    let status = state
        .service
        .status()
        .await
        .map_err(map_registration_error)?;
    Ok(no_store_json(RegistrationStatusResponse {
        enabled: status.enabled(),
        email_required: status.email_required(),
    }))
}

/// 创建普通用户并由服务端立即签发统一登录会话。
async fn register(
    State(state): State<RegistrationHttpState>,
    Extension(client_ip): Extension<TrustedClientIp>,
    request: Result<Json<RegistrationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = RegistrationCommand::new(request.username, request.email, request.password)
        .map_err(map_registration_error)?;
    state
        .service
        .register(client_ip, &command)
        .await
        .map_err(map_registration_error)?;

    let mut response = login_with_authenticator(
        state.session_authenticator.as_ref(),
        command.into_login_credentials(),
    )
    .await
    .map_err(|error| match error {
        ManagementError::InvalidCredentials => ManagementError::Internal,
        other => other,
    })?;
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

/// 返回管理员完整注册策略。
async fn get_registration_policy(
    State(state): State<RegistrationHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let policy = state
        .service
        .policy(authentication.principal())
        .await
        .map_err(map_registration_error)?;
    Ok(no_store_json(RegistrationPolicyResponse::from(policy)))
}

/// 原子覆盖管理员完整注册策略。
async fn update_registration_policy(
    State(state): State<RegistrationHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<RegistrationPolicyRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let default_group_id =
        GroupId::new(request.default_group_id).map_err(|_| ManagementError::InvalidRequest)?;
    let command = RegistrationPolicyCommand::new(
        request.enabled,
        default_group_id,
        request.initial_quota,
        request.email_required,
        request.rate_limit_attempts,
        request.rate_limit_window_seconds,
    )
    .map_err(map_registration_error)?;
    let policy = state
        .service
        .update_policy(authentication.principal(), command)
        .await
        .map_err(map_registration_error)?;
    Ok(no_store_json(RegistrationPolicyResponse::from(policy)))
}

async fn resolve_registration_client_ip(
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
                target: "af_http::registration",
                error_kind = "registration_client_ip_missing_peer",
                "公开注册入口缺少 TCP 对端信息"
            );
            ManagementError::Internal.into_response()
        }
        Err(ClientIpResolutionError::InvalidProxyChain) => {
            ManagementError::InvalidRequest.into_response()
        }
    }
}

fn map_registration_error(error: RegistrationError) -> ManagementError {
    match error {
        RegistrationError::InvalidInput => ManagementError::InvalidRequest,
        RegistrationError::Forbidden => ManagementError::Forbidden,
        RegistrationError::Disabled => ManagementError::RegistrationDisabled,
        RegistrationError::Conflict => ManagementError::UserConflict,
        RegistrationError::RateLimited {
            retry_after_seconds,
        } => ManagementError::RegistrationRateLimited {
            retry_after_seconds,
        },
        RegistrationError::Internal => ManagementError::Internal,
    }
}
