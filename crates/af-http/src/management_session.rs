use af_admin::{IssuedSession, LoginCredentials, SessionAuthenticationError};
use axum::{
    Json,
    extract::{Extension, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = LoginRequest)]
pub(crate) struct LoginRequest {
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(max_length = 4096, format = Password)]
    password: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SessionUser)]
pub(crate) struct SessionUser {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(
        value_type = crate::openapi::schema::AdminSessionRoleSchema,
        inline
    )]
    role: af_admin::SessionRole,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = LoginResponse)]
pub(crate) struct LoginResponse {
    #[schema(value_type = String)]
    access_token: af_admin::SessionToken,
    #[schema(
        value_type = crate::openapi::schema::BearerTokenTypeSchema,
        inline
    )]
    token_type: &'static str,
    #[schema(maximum = 86400)]
    expires_in: u64,
    #[schema(minimum = 1)]
    expires_at: u64,
    user: SessionUser,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SessionResponse)]
pub(crate) struct SessionResponse {
    user: SessionUser,
    #[schema(minimum = 1)]
    expires_at: u64,
}

/// 校验用户名密码并返回短期 Bearer JWT。
pub(crate) async fn login(
    State(state): State<HttpState>,
    request: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    login_with_credentials(&state, request.username, request.password).await
}

/// 使用受控凭据完成认证并返回统一的无缓存登录响应。
pub(crate) async fn login_with_credentials(
    state: &HttpState,
    username: String,
    password: String,
) -> Result<Response, ManagementError> {
    login_with_authenticator(
        state.session_authenticator.as_ref(),
        LoginCredentials::new(username, password),
    )
    .await
}

/// 使用统一认证器签发登录会话，并确保原始凭据在调用结束后立即释放。
pub(crate) async fn login_with_authenticator(
    authenticator: &dyn af_admin::SessionAuthenticator,
    credentials: LoginCredentials,
) -> Result<Response, ManagementError> {
    let result = authenticator.login(&credentials).await;
    drop(credentials);
    let session = result.map_err(map_session_error)?;
    Ok(no_store_json(login_response(session)))
}

/// 返回 JWT 对应的当前用户状态，角色来自请求期间的数据库回查。
pub(crate) async fn current_session(
    State(_state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Response {
    no_store_json(SessionResponse {
        user: session_user(authentication.principal()),
        expires_at: authentication.expires_at(),
    })
}

fn login_response(session: IssuedSession) -> LoginResponse {
    let principal = session.principal();
    let expires_at = session.expires_at();
    LoginResponse {
        access_token: session.into_token(),
        token_type: "Bearer",
        expires_in: expires_at.saturating_sub(current_timestamp()),
        expires_at,
        user: session_user(principal),
    }
}

fn session_user(principal: af_admin::SessionPrincipal) -> SessionUser {
    SessionUser {
        id: principal.user_id().get(),
        role: principal.role(),
    }
}

fn map_session_error(error: SessionAuthenticationError) -> ManagementError {
    match error {
        SessionAuthenticationError::InvalidCredentials => ManagementError::InvalidCredentials,
        SessionAuthenticationError::InvalidSession => ManagementError::InvalidSession,
        SessionAuthenticationError::Internal => ManagementError::Internal,
    }
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(crate) fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
