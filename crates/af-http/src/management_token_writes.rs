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
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_tokens::{AdminTokenResponse, parse_token_id},
};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminTokenWriteRequest)]
/// 管理端签发或完整更新令牌时使用的配置正文。
pub(crate) struct AdminTokenWriteRequest {
    /// 令牌所属用户 ID；更新时必须与现有归属一致。
    #[schema(minimum = 1)]
    user_id: i64,
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(value_type = crate::openapi::schema::AdminTokenStatusSchema)]
    status: af_admin::AdminTokenStatus,
    /// 强制绑定分组；null 表示使用所属用户的默认分组。
    #[schema(minimum = 1, required = true)]
    group_id: Option<i64>,
    #[schema(minimum = 0)]
    remain_quota: i64,
    unlimited_quota: bool,
    /// Unix 秒时间戳；null 表示永不过期。
    #[schema(minimum = 0, required = true)]
    expired_at: Option<i64>,
    #[schema(
        schema_with = crate::openapi::schema::token_model_limits_write_schema,
        required = true
    )]
    model_limits: Option<Vec<String>>,
    #[schema(
        schema_with = crate::openapi::schema::token_allow_ips_write_schema,
        required = true
    )]
    allow_ips: Option<Vec<String>>,
    cross_group_retry: bool,
    #[schema(minimum = 0, required = true)]
    rate_limit_5h: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_1d: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_7d: Option<i64>,
    #[schema(minimum = 0, required = true)]
    max_requests: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = IssuedAdminToken)]
pub(crate) struct IssuedAdminTokenResponse<'a> {
    /// 只在本次签发响应中出现的完整 API Key。
    #[schema(
        min_length = 49,
        max_length = 49,
        pattern = "^sk-af-[A-Za-z0-9_-]{43}$"
    )]
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
        AdminTokenWriteError::LimitReached => ManagementError::TokenLimitReached,
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
