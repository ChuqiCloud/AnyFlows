use af_admin::{
    AdminTokenCreateCommand, AdminTokenUpdateCommand, AdminTokenWriteError, IssuedAdminToken,
};
use af_domain::{GroupId, UserId};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_tokens::{AdminTokenResponse, parse_token_id},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
/// 管理端签发或完整更新令牌时使用的配置正文。
pub(crate) struct AdminTokenWriteRequest {
    user_id: i64,
    name: String,
    status: af_admin::AdminTokenStatus,
    group_id: Option<i64>,
    remain_quota: i64,
    unlimited_quota: bool,
    expired_at: Option<i64>,
    model_limits: Option<Vec<String>>,
    allow_ips: Option<Vec<String>>,
    cross_group_retry: bool,
    rate_limit_5h: Option<i64>,
    rate_limit_1d: Option<i64>,
    rate_limit_7d: Option<i64>,
    max_requests: Option<i64>,
}

#[derive(Serialize)]
struct IssuedAdminTokenResponse<'a> {
    api_key: &'a str,
    token: AdminTokenResponse,
}

/// 签发一个令牌；完整 API Key 只在本次响应中返回。
pub(crate) async fn create_admin_token(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminTokenWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let issued = state
        .admin_token_writer
        .create(authentication.principal(), request.into_create_command()?)
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        IssuedAdminTokenResponse::from_issued(&issued),
    ))
}

/// 完整更新令牌配置，不轮换密钥或重置累计用量。
pub(crate) async fn update_admin_token(
    State(state): State<HttpState>,
    Path(token_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminTokenWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let token_id = parse_token_id(&token_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let token = state
        .admin_token_writer
        .update(
            authentication.principal(),
            token_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminTokenResponse::from_token(&token)))
}

/// 软删除令牌；不存在或已经删除时返回 404。
pub(crate) async fn delete_admin_token(
    State(state): State<HttpState>,
    Path(token_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let token_id = parse_token_id(&token_id)?;
    state
        .admin_token_writer
        .delete(authentication.principal(), token_id)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminTokenWriteRequest {
    fn into_create_command(self) -> Result<AdminTokenCreateCommand, ManagementError> {
        AdminTokenCreateCommand::new(
            parse_user_id(self.user_id)?,
            self.name,
            self.status,
            parse_optional_group_id(self.group_id)?,
            self.remain_quota,
            self.unlimited_quota,
            self.expired_at,
            self.model_limits,
            self.allow_ips,
            self.cross_group_retry,
            self.rate_limit_5h,
            self.rate_limit_1d,
            self.rate_limit_7d,
            self.max_requests,
        )
        .map_err(map_write_error)
    }

    fn into_update_command(self) -> Result<AdminTokenUpdateCommand, ManagementError> {
        AdminTokenUpdateCommand::new(
            parse_user_id(self.user_id)?,
            self.name,
            self.status,
            parse_optional_group_id(self.group_id)?,
            self.remain_quota,
            self.unlimited_quota,
            self.expired_at,
            self.model_limits,
            self.allow_ips,
            self.cross_group_retry,
            self.rate_limit_5h,
            self.rate_limit_1d,
            self.rate_limit_7d,
            self.max_requests,
        )
        .map_err(map_write_error)
    }
}

impl<'a> IssuedAdminTokenResponse<'a> {
    fn from_issued(issued: &'a IssuedAdminToken) -> Self {
        Self {
            api_key: issued.api_key().expose_secret(),
            token: AdminTokenResponse::from_token(issued.token()),
        }
    }
}

fn parse_user_id(value: i64) -> Result<UserId, ManagementError> {
    UserId::new(value).map_err(|_| ManagementError::InvalidRequest)
}

fn parse_optional_group_id(value: Option<i64>) -> Result<Option<GroupId>, ManagementError> {
    value
        .map(|value| GroupId::new(value).map_err(|_| ManagementError::InvalidRequest))
        .transpose()
}

fn map_write_error(error: AdminTokenWriteError) -> ManagementError {
    match error {
        AdminTokenWriteError::InvalidInput => ManagementError::InvalidRequest,
        AdminTokenWriteError::Forbidden => ManagementError::Forbidden,
        AdminTokenWriteError::NotFound => ManagementError::TokenNotFound,
        AdminTokenWriteError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn status_json(status: StatusCode, value: impl Serialize) -> Response {
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
