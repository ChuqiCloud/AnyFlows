use af_admin::{
    AdminUser, AdminUserCreateCommand, AdminUserListQuery, AdminUserPage, AdminUserReadError,
    AdminUserUpdateCommand, AdminUserWriteError, PlatformAuditEntry, PlatformAuditOutcome,
    PlatformAuditService, PlatformPolicy,
};
use af_domain::{GroupId, PlatformPermission, UserId};
use af_telemetry::RequestId;
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUser)]
pub(crate) struct AdminUserResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(max_length = 320, format = Email, required = true)]
    email: Option<String>,
    #[schema(
        value_type = crate::openapi::schema::AdminSessionRoleSchema,
        inline
    )]
    role: af_admin::SessionRole,
    #[schema(value_type = crate::openapi::schema::AdminUserStatusSchema)]
    status: af_admin::AdminUserStatus,
    #[schema(minimum = 1)]
    default_group_id: i64,
    #[schema(minimum = 0)]
    quota: i64,
    #[schema(minimum = 0)]
    used_quota: i64,
    #[schema(minimum = 0)]
    frozen_quota: i64,
    #[schema(minimum = 0)]
    request_count: i64,
    #[schema(minimum = 0, required = true)]
    rpm_limit: Option<i32>,
    #[schema(minimum = 0, required = true)]
    concurrency: Option<i32>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserListResponse)]
pub(crate) struct AdminUserListResponse {
    #[schema(max_items = 100)]
    users: Vec<AdminUserResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserCreateRequest)]
/// 管理端创建用户时使用的写入正文；初始额度会同步写入 opening 账本。
pub(crate) struct AdminUserCreateRequest {
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(min_length = 1, max_length = 320)]
    email: Option<String>,
    #[schema(min_length = 1, max_length = 4096, format = Password)]
    password: Option<String>,
    #[schema(
        value_type = crate::openapi::schema::AdminSessionRoleSchema,
        inline
    )]
    role: af_admin::SessionRole,
    #[schema(value_type = crate::openapi::schema::AdminUserStatusSchema)]
    status: af_admin::AdminUserStatus,
    #[schema(minimum = 1)]
    default_group_id: i64,
    #[schema(minimum = 0)]
    quota: i64,
    #[schema(minimum = 0)]
    rpm_limit: Option<i32>,
    #[schema(minimum = 0)]
    concurrency: Option<i32>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUserUpdateRequest)]
/// 管理端更新用户资料时使用的写入正文；额度只能通过独立调账接口变更。
pub(crate) struct AdminUserUpdateRequest {
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(min_length = 1, max_length = 320)]
    email: Option<String>,
    #[schema(min_length = 1, max_length = 4096, format = Password)]
    password: Option<String>,
    #[schema(
        value_type = crate::openapi::schema::AdminSessionRoleSchema,
        inline
    )]
    role: af_admin::SessionRole,
    #[schema(value_type = crate::openapi::schema::AdminUserStatusSchema)]
    status: af_admin::AdminUserStatus,
    #[schema(minimum = 1)]
    default_group_id: i64,
    #[schema(minimum = 0)]
    rpm_limit: Option<i32>,
    #[schema(minimum = 0)]
    concurrency: Option<i32>,
}

/// 返回管理员可见的用户只读列表，查询参数必须保持稳定且可预测。
pub(crate) async fn list_admin_users(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    let principal = authentication.principal();
    let audit_service = state
        .platform_audit_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    if !PlatformPolicy::allows(principal, PlatformPermission::UserDirectoryReadAll) {
        record_user_directory_audit(
            audit_service,
            principal,
            &request_id,
            PlatformAuditOutcome::Denied,
            json!({"reason": "forbidden"}),
        )
        .await?;
        return Err(ManagementError::Forbidden);
    }
    let query = match parse_list_query(raw_query.as_deref()) {
        Ok(query) => query,
        Err(error) => {
            record_user_directory_audit(
                audit_service,
                principal,
                &request_id,
                PlatformAuditOutcome::Failed,
                json!({"reason": "invalid_pagination"}),
            )
            .await?;
            return Err(error);
        }
    };
    let page = match state.admin_user_reader.list(principal, query).await {
        Ok(page) => page,
        Err(error) => {
            record_user_directory_audit(
                audit_service,
                principal,
                &request_id,
                PlatformAuditOutcome::Failed,
                json!({"reason": read_error_code(error)}),
            )
            .await?;
            return Err(map_read_error(error));
        }
    };
    record_user_directory_audit(
        audit_service,
        principal,
        &request_id,
        PlatformAuditOutcome::Succeeded,
        json!({
            "after": query.after().map(UserId::get),
            "limit": query.limit(),
        }),
    )
    .await?;
    Ok(no_store_json(AdminUserListResponse::from_page(page)))
}

async fn record_user_directory_audit(
    service: &std::sync::Arc<dyn PlatformAuditService>,
    principal: af_admin::SessionPrincipal,
    request_id: &RequestId,
    outcome: PlatformAuditOutcome,
    audit_info: Value,
) -> Result<(), ManagementError> {
    let entry = PlatformAuditEntry::new(
        principal,
        PlatformPermission::UserDirectoryReadAll,
        "/api/admin/users",
        "platform.user_directory.list",
        "user_directory",
        None,
        outcome,
        None,
        None,
        Some(audit_info),
        request_id.as_str(),
    )
    .map_err(|_| ManagementError::Internal)?;
    service
        .record(entry)
        .await
        .map_err(|_| ManagementError::Internal)
}

const fn read_error_code(error: AdminUserReadError) -> &'static str {
    match error {
        AdminUserReadError::InvalidPagination => "invalid_pagination",
        AdminUserReadError::Forbidden => "forbidden",
        AdminUserReadError::NotFound => "not_found",
        AdminUserReadError::Internal => "internal",
    }
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

/// 创建一个管理端用户，并只返回非敏感快照。
pub(crate) async fn create_admin_user(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminUserCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let user = state
        .admin_user_writer
        .create(authentication.principal(), request.into_create_command()?)
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminUserResponse::from_user(&user),
    ))
}

/// 完整更新一个未软删除用户的基础账户字段。
pub(crate) async fn update_admin_user(
    State(state): State<HttpState>,
    Path(user_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminUserUpdateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let user_id = parse_user_id(&user_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let user = state
        .admin_user_writer
        .update(
            authentication.principal(),
            user_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminUserResponse::from_user(&user)))
}

/// 软删除一个用户；已删除或不存在的用户统一返回 404。
pub(crate) async fn delete_admin_user(
    State(state): State<HttpState>,
    Path(user_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let user_id = parse_user_id(&user_id)?;
    state
        .admin_user_writer
        .delete(authentication.principal(), user_id)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
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

impl AdminUserCreateRequest {
    fn into_create_command(self) -> Result<AdminUserCreateCommand, ManagementError> {
        AdminUserCreateCommand::new(
            self.username,
            self.email,
            self.password,
            self.role,
            self.status,
            parse_group_id(self.default_group_id)?,
            self.quota,
            self.rpm_limit,
            self.concurrency,
        )
        .map_err(map_write_error)
    }
}

impl AdminUserUpdateRequest {
    fn into_update_command(self) -> Result<AdminUserUpdateCommand, ManagementError> {
        AdminUserUpdateCommand::new(
            self.username,
            self.email,
            self.password,
            self.role,
            self.status,
            parse_group_id(self.default_group_id)?,
            self.rpm_limit,
            self.concurrency,
        )
        .map_err(map_write_error)
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

fn parse_group_id(value: i64) -> Result<GroupId, ManagementError> {
    GroupId::new(value).map_err(|_| ManagementError::InvalidRequest)
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

fn map_write_error(error: AdminUserWriteError) -> ManagementError {
    match error {
        AdminUserWriteError::InvalidInput => ManagementError::InvalidRequest,
        AdminUserWriteError::Forbidden => ManagementError::Forbidden,
        AdminUserWriteError::Conflict => ManagementError::UserConflict,
        AdminUserWriteError::NotFound => ManagementError::UserNotFound,
        AdminUserWriteError::Internal => ManagementError::Internal,
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
