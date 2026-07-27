use af_admin::{AdminToken, AdminTokenListQuery, AdminTokenPage, AdminTokenReadError};
use af_domain::{GroupId, TokenId};
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminToken)]
pub(crate) struct AdminTokenResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    user_id: i64,
    /// 不可用于鉴权的 18 字符展示前缀。
    #[schema(
        min_length = 18,
        max_length = 18,
        pattern = "^sk-af-[A-Za-z0-9_-]{12}$"
    )]
    key_prefix: String,
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(value_type = crate::openapi::schema::AdminTokenStatusSchema)]
    status: af_admin::AdminTokenStatus,
    #[schema(minimum = 1, required = true)]
    group_id: Option<i64>,
    #[schema(minimum = 0)]
    remain_quota: i64,
    unlimited_quota: bool,
    #[schema(minimum = 0)]
    used_quota: i64,
    /// Unix 秒时间戳；null 表示永不过期。
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
    cross_group_retry: bool,
    #[schema(minimum = 0, required = true)]
    rate_limit_5h: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_1d: Option<i64>,
    #[schema(minimum = 0, required = true)]
    rate_limit_7d: Option<i64>,
    #[schema(minimum = 0)]
    usage_5h: i64,
    #[schema(minimum = 0)]
    usage_1d: i64,
    #[schema(minimum = 0)]
    usage_7d: i64,
    #[schema(minimum = 0)]
    window_5h_start: i64,
    #[schema(minimum = 0)]
    window_1d_start: i64,
    #[schema(minimum = 0)]
    window_7d_start: i64,
    #[schema(minimum = 0, required = true)]
    max_requests: Option<i64>,
    #[schema(minimum = 0)]
    used_requests: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminTokenListResponse)]
pub(crate) struct AdminTokenListResponse {
    #[schema(max_items = 100)]
    tokens: Vec<AdminTokenResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 返回管理员可见的令牌列表，只包含非敏感字段和不可鉴权展示前缀。
pub(crate) async fn list_admin_tokens(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let page = state
        .admin_token_reader
        .list(authentication.principal(), query)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminTokenListResponse::from_page(page)))
}

/// 返回单个未软删除令牌的非敏感管理快照。
pub(crate) async fn get_admin_token(
    State(state): State<HttpState>,
    Path(token_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let token_id = parse_token_id(&token_id)?;
    let token = state
        .admin_token_reader
        .get(authentication.principal(), token_id)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminTokenResponse::from_token(&token)))
}

impl AdminTokenListResponse {
    fn from_page(page: AdminTokenPage) -> Self {
        Self {
            tokens: page
                .tokens()
                .iter()
                .map(AdminTokenResponse::from_token)
                .collect(),
            next_cursor: page.next_cursor().map(TokenId::get),
        }
    }
}

impl AdminTokenResponse {
    pub(crate) fn from_token(token: &AdminToken) -> Self {
        Self {
            id: token.token_id().get(),
            user_id: token.user_id().get(),
            key_prefix: token.key_prefix().to_owned(),
            name: token.name().to_owned(),
            status: token.status(),
            group_id: token.group_id().map(GroupId::get),
            remain_quota: token.remain_quota(),
            unlimited_quota: token.unlimited_quota(),
            used_quota: token.used_quota(),
            expired_at: token.expired_at(),
            model_limits: token.model_limits().map(|values| values.to_vec()),
            allow_ips: token.allow_ips().map(|values| values.to_vec()),
            cross_group_retry: token.cross_group_retry(),
            rate_limit_5h: token.rate_limit_5h(),
            rate_limit_1d: token.rate_limit_1d(),
            rate_limit_7d: token.rate_limit_7d(),
            usage_5h: token.usage_5h(),
            usage_1d: token.usage_1d(),
            usage_7d: token.usage_7d(),
            window_5h_start: token.window_5h_start(),
            window_1d_start: token.window_1d_start(),
            window_7d_start: token.window_7d_start(),
            max_requests: token.max_requests(),
            used_requests: token.used_requests(),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminTokenListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminTokenListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminTokenListQuery::default());
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
                after = Some(parse_token_id(&value)?);
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
    AdminTokenListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_TOKEN_PAGE_SIZE),
    )
    .map_err(map_read_error)
}

pub(crate) fn parse_token_id(value: &str) -> Result<TokenId, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    let parsed = value
        .parse::<i64>()
        .map_err(|_| ManagementError::InvalidRequest)?;
    TokenId::new(parsed).map_err(|_| ManagementError::InvalidRequest)
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

fn map_read_error(error: AdminTokenReadError) -> ManagementError {
    match error {
        AdminTokenReadError::InvalidPagination => ManagementError::InvalidRequest,
        AdminTokenReadError::Forbidden => ManagementError::Forbidden,
        AdminTokenReadError::NotFound => ManagementError::TokenNotFound,
        AdminTokenReadError::Internal => ManagementError::Internal,
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
    fn list_query_parser_rejects_unstable_inputs() {
        assert_eq!(parse_list_query(None).unwrap().limit(), 50);
        assert_eq!(
            parse_list_query(Some("after=1&limit=100")).unwrap().limit(),
            100
        );

        for raw_query in [
            "after=0",
            "after=-1",
            "after=1&after=2",
            "limit=0",
            "limit=101",
            "limit=1&limit=2",
            "unknown=1",
            "after=%",
            "after=%ff",
        ] {
            assert_eq!(
                parse_list_query(Some(raw_query)),
                Err(ManagementError::InvalidRequest),
                "{raw_query}"
            );
        }
    }
}
