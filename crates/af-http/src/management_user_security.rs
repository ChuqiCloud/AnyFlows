use af_admin::{AdminUserSecurityError, AdminUserSecuritySnapshot, SessionAuthentication};
use af_domain::UserId;
use axum::{
    Json,
    extract::{Extension, Path, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserSecurityTwoFactorResponse)]
pub(crate) struct AdminUserSecurityTwoFactorResponse {
    enabled: bool,
    state: &'static str,
    #[schema(required = true)]
    pending_created_at: Option<u64>,
    #[schema(required = true)]
    pending_expires_at: Option<u64>,
    #[schema(required = true)]
    bound_at: Option<u64>,
    #[schema(required = true)]
    last_verified_at: Option<u64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserSecurityPasskeyResponse)]
pub(crate) struct AdminUserSecurityPasskeyResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    created_at: i64,
    #[schema(required = true)]
    last_used_at: Option<i64>,
    #[schema(required = true)]
    revoked_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserSecurityResponse)]
pub(crate) struct AdminUserSecurityResponse {
    user_id: i64,
    two_factor: AdminUserSecurityTwoFactorResponse,
    #[schema(max_items = 100)]
    passkeys: Vec<AdminUserSecurityPasskeyResponse>,
}

/// 返回管理员可见的用户安全设置脱敏快照。
pub(crate) async fn get_admin_user_security(
    State(state): State<HttpState>,
    Path(user_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_user_security_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let snapshot = service
        .get(authentication.principal(), parse_user_id(&user_id)?)
        .await
        .map_err(map_security_error)?;
    Ok(no_store_json(AdminUserSecurityResponse::from_snapshot(
        &snapshot,
    )))
}

/// 重置目标用户 TOTP，并使其旧会话失效；不会生成或返回新的密钥材料。
pub(crate) async fn reset_admin_user_two_factor(
    State(state): State<HttpState>,
    Path(user_id): Path<String>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_user_security_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let snapshot = service
        .reset_two_factor(authentication.principal(), parse_user_id(&user_id)?)
        .await
        .map_err(map_security_error)?;
    Ok(no_store_json(AdminUserSecurityResponse::from_snapshot(
        &snapshot,
    )))
}

/// 撤销目标用户指定 Passkey，并使其旧会话失效。
pub(crate) async fn revoke_admin_user_passkey(
    State(state): State<HttpState>,
    Path((user_id, passkey_id)): Path<(String, String)>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_user_security_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let passkey_id = passkey_id
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or(ManagementError::InvalidRequest)?;
    service
        .revoke_passkey(
            authentication.principal(),
            parse_user_id(&user_id)?,
            passkey_id,
        )
        .await
        .map_err(map_security_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminUserSecurityResponse {
    fn from_snapshot(snapshot: &AdminUserSecuritySnapshot) -> Self {
        let two_factor = snapshot.two_factor();
        Self {
            user_id: snapshot.user_id().get(),
            two_factor: AdminUserSecurityTwoFactorResponse {
                enabled: two_factor.enabled(),
                state: if two_factor.enabled() {
                    "enabled"
                } else if two_factor.pending() {
                    "pending"
                } else {
                    "disabled"
                },
                pending_created_at: two_factor.pending_created_at(),
                pending_expires_at: two_factor.pending_expires_at(),
                bound_at: two_factor.bound_at(),
                last_verified_at: two_factor.last_verified_at(),
            },
            passkeys: snapshot
                .passkeys()
                .iter()
                .map(|passkey| AdminUserSecurityPasskeyResponse {
                    id: passkey.id(),
                    display_name: passkey.display_name().to_owned(),
                    created_at: passkey.created_at(),
                    last_used_at: passkey.last_used_at(),
                    revoked_at: passkey.revoked_at(),
                })
                .collect(),
        }
    }
}

fn parse_user_id(value: &str) -> Result<UserId, ManagementError> {
    value
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .and_then(|id| UserId::new(id).ok())
        .ok_or(ManagementError::InvalidRequest)
}

fn map_security_error(error: AdminUserSecurityError) -> ManagementError {
    match error {
        AdminUserSecurityError::Forbidden => ManagementError::Forbidden,
        AdminUserSecurityError::NotFound => ManagementError::UserNotFound,
        AdminUserSecurityError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
