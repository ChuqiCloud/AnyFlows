use af_admin::{AdminUser, AdminUserListQuery, AdminUserPage, AdminUserReadError};
use af_domain::UserId;
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize)]
struct AdminUserResponse {
    id: i64,
    username: String,
    email: Option<String>,
    role: af_admin::SessionRole,
    status: af_admin::AdminUserStatus,
    default_group_id: i64,
    quota: i64,
    used_quota: i64,
    frozen_quota: i64,
    request_count: i64,
    rpm_limit: Option<i32>,
    concurrency: Option<i32>,
}

#[derive(Serialize)]
struct AdminUserListResponse {
    users: Vec<AdminUserResponse>,
    next_cursor: Option<i64>,
}

/// 返回管理员可见的用户只读列表，查询参数必须保持稳定且可预测。
pub(crate) async fn list_admin_users(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let page = state
        .admin_user_reader
        .list(authentication.principal(), query)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminUserListResponse::from_page(page)))
}

/// 返回单个未软删除用户的非敏感管理快照。
pub(crate) async fn get_admin_user(
    State(state): State<HttpState>,
    Path(user_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let user_id = parse_user_id(&user_id)?;
    let user = state
        .admin_user_reader
        .get(authentication.principal(), user_id)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminUserResponse::from_user(&user)))
}

impl AdminUserListResponse {
    fn from_page(page: AdminUserPage) -> Self {
        Self {
            users: page
                .users()
                .iter()
                .map(AdminUserResponse::from_user)
                .collect(),
            next_cursor: page.next_cursor().map(UserId::get),
        }
    }
}

impl AdminUserResponse {
    fn from_user(user: &AdminUser) -> Self {
        Self {
            id: user.user_id().get(),
            username: user.username().to_owned(),
            email: user.email().map(str::to_owned),
            role: user.role(),
            status: user.status(),
            default_group_id: user.default_group_id().get(),
            quota: user.quota(),
            used_quota: user.used_quota(),
            frozen_quota: user.frozen_quota(),
            request_count: user.request_count(),
            rpm_limit: user.rpm_limit(),
            concurrency: user.concurrency(),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminUserListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminUserListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminUserListQuery::default());
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
                after = Some(parse_user_id(&value)?);
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
    AdminUserListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_USER_PAGE_SIZE),
    )
    .map_err(map_read_error)
}

fn parse_user_id(value: &str) -> Result<UserId, ManagementError> {
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
    UserId::new(parsed).map_err(|_| ManagementError::InvalidRequest)
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

fn map_read_error(error: AdminUserReadError) -> ManagementError {
    match error {
        AdminUserReadError::InvalidPagination => ManagementError::InvalidRequest,
        AdminUserReadError::Forbidden => ManagementError::Forbidden,
        AdminUserReadError::NotFound => ManagementError::UserNotFound,
        AdminUserReadError::Internal => ManagementError::Internal,
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
