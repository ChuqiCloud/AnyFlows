//! 管理员上游账号 OAuth 连接 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    oauth_connections::{
        AdminOAuthAuthorizationRequest, AdminOAuthAuthorizationResponse,
        AdminOAuthCompletionResponse, AdminOAuthCompletionStatus, AdminOAuthManualCallbackRequest,
        AdminOAuthProvider, AdminOAuthProviderListResponse, AdminOAuthProviderResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/admin/oauth/providers",
    operation_id = "listAdminOAuthProviders",
    tag = "OAuth 连接",
    summary = "读取已配置的上游 OAuth Provider",
    responses(
        (status = 200, description = "Provider 与回调能力列表", body = AdminOAuthProviderListResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_admin_oauth_providers() {}

#[utoipa::path(
    post,
    path = "/api/admin/channels/{channel_id}/credentials/{credential_id}/oauth-authorizations",
    operation_id = "beginAdminOAuthAuthorization",
    tag = "OAuth 连接",
    summary = "为已有 OAuth 凭据发起授权",
    params(
        ("channel_id" = i64, Path, minimum = 1, description = "渠道 ID"),
        ("credential_id" = i64, Path, minimum = 1, description = "OAuth 凭据 ID")
    ),
    request_body = AdminOAuthAuthorizationRequest,
    responses(
        (status = 201, description = "一次性授权地址与固定回调说明", body = AdminOAuthAuthorizationResponse),
        (status = 400, description = "路径、正文或凭据类型无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "渠道或凭据不存在", body = ManagementErrorBody),
        (status = 409, description = "Provider 未配置或凭据已绑定其他 Provider", body = ManagementErrorBody),
        (status = 429, description = "待完成授权已达到单实例容量", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn begin_admin_oauth_authorization() {}

#[utoipa::path(
    post,
    path = "/api/admin/oauth/authorizations/manual-callback",
    operation_id = "completeAdminOAuthManualCallback",
    tag = "OAuth 连接",
    summary = "提交完整回调地址完成授权",
    request_body = AdminOAuthManualCallbackRequest,
    responses(
        (status = 200, description = "OAuth token 已写入绑定凭据", body = AdminOAuthCompletionResponse),
        (status = 400, description = "回调地址或授权响应无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是管理员", body = ManagementErrorBody),
        (status = 404, description = "授权会话、授权发起人或目标凭据不匹配", body = ManagementErrorBody),
        (status = 409, description = "上游拒绝授权或凭据 Provider 冲突", body = ManagementErrorBody),
        (status = 410, description = "授权会话已过期", body = ManagementErrorBody),
        (status = 502, description = "上游拒绝 token 交换或响应无效", body = ManagementErrorBody),
        (status = 503, description = "OAuth 网络或存储依赖不可用", body = ManagementErrorBody),
        (status = 504, description = "OAuth token 交换超时", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn complete_admin_oauth_manual_callback() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_admin_oauth_providers,
        begin_admin_oauth_authorization,
        complete_admin_oauth_manual_callback
    ),
    components(schemas(
        AdminOAuthProvider,
        AdminOAuthProviderListResponse,
        AdminOAuthProviderResponse,
        AdminOAuthAuthorizationRequest,
        AdminOAuthAuthorizationResponse,
        AdminOAuthManualCallbackRequest,
        AdminOAuthCompletionStatus,
        AdminOAuthCompletionResponse,
        ManagementErrorBody
    ))
)]
struct OAuthConnectionsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OAuthConnectionsApi::openapi()
}
