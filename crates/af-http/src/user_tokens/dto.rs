use af_admin::{IssuedUserToken, UserToken, UserTokenPage, UserTokenWriteCommand};
use af_domain::TokenId;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::management_error::ManagementError;

/// 普通用户创建或更新 API Key 的正文。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTokenWriteRequest)]
pub(crate) struct UserTokenWriteRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(value_type = crate::openapi::schema::UserTokenStatusSchema)]
    status: af_admin::UserTokenStatus,
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
}

impl UserTokenWriteRequest {
    pub(super) fn into_command(self) -> Result<UserTokenWriteCommand, ManagementError> {
        UserTokenWriteCommand::new(
            self.name,
            self.status,
            self.remain_quota,
            self.unlimited_quota,
            self.expired_at,
            self.model_limits,
            self.allow_ips,
        )
        .map_err(|_| ManagementError::InvalidRequest)
    }
}

/// 普通用户可读取的非敏感 API Key。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserToken)]
pub(crate) struct UserTokenResponse {
    #[schema(minimum = 1)]
    id: i64,
    /// 不可用于鉴权的 18 字符展示前缀。
    #[schema(
        min_length = 18,
        max_length = 18,
        pattern = "^sk-af-[A-Za-z0-9_-]{12}$"
    )]
    key_prefix: String,
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(value_type = crate::openapi::schema::UserTokenStatusSchema)]
    status: af_admin::UserTokenStatus,
    #[schema(minimum = 0)]
    remain_quota: i64,
    unlimited_quota: bool,
    #[schema(minimum = 0)]
    used_quota: i64,
    #[schema(minimum = 0, required = true)]
    expired_at: Option<i64>,
    #[schema(
        schema_with = crate::openapi::schema::token_model_limits_schema,
        required = true
    )]
    model_limits: Option<Vec<String>>,
    #[schema(
        schema_with = crate::openapi::schema::token_allow_ips_schema,
        required = true
    )]
    allow_ips: Option<Vec<String>>,
    #[schema(minimum = 0)]
    created_at: i64,
    #[schema(minimum = 0)]
    updated_at: i64,
}

impl UserTokenResponse {
    pub(crate) fn from_application(token: &UserToken) -> Self {
        Self {
            id: token.token_id().get(),
            key_prefix: token.key_prefix().to_owned(),
            name: token.name().to_owned(),
            status: token.status(),
            remain_quota: token.remain_quota(),
            unlimited_quota: token.unlimited_quota(),
            used_quota: token.used_quota(),
            expired_at: token.expired_at(),
            model_limits: token.model_limits().map(|values| values.to_vec()),
            allow_ips: token.allow_ips().map(|values| values.to_vec()),
            created_at: token.created_at(),
            updated_at: token.updated_at(),
        }
    }
}

/// 当前用户的一页 API Key。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserTokenListResponse)]
pub(crate) struct UserTokenListResponse {
    #[schema(max_items = 100)]
    tokens: Vec<UserTokenResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
    #[schema(minimum = 32, maximum = 32)]
    capacity: usize,
}

impl UserTokenListResponse {
    pub(crate) fn from_application(page: UserTokenPage) -> Self {
        Self {
            tokens: page
                .tokens()
                .iter()
                .map(UserTokenResponse::from_application)
                .collect(),
            next_cursor: page.next_cursor().map(TokenId::get),
            capacity: af_admin::MAX_USER_TOKENS_PER_USER,
        }
    }
}

/// 一次性用户 API Key 签发响应。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = IssuedUserToken)]
pub(crate) struct IssuedUserTokenResponse<'a> {
    /// 只在本次签发响应中出现的完整 API Key。
    #[schema(
        min_length = 49,
        max_length = 49,
        pattern = "^sk-af-[A-Za-z0-9_-]{43}$"
    )]
    api_key: &'a str,
    token: UserTokenResponse,
}

impl<'a> IssuedUserTokenResponse<'a> {
    pub(crate) fn from_application(issued: &'a IssuedUserToken) -> Self {
        Self {
            api_key: issued.api_key().expose_secret(),
            token: UserTokenResponse::from_application(issued.token()),
        }
    }
}
