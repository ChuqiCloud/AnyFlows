//! 企业 API Key 本人管理和管理员目录的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_tokens::{
        IssuedOrganizationTokenResponse, OrganizationTokenListResponse, OrganizationTokenResponse,
        OrganizationTokenStatusDto, OrganizationTokenWriteRequest,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/tokens",
    operation_id = "listOrganizationTokens",
    tag = "企业 API Key",
    summary = "列出本人企业 API Key",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页最后一条 Key 标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "返回数量")
    ),
    responses(
        (status = 200, description = "本人企业 API Key 列表", body = OrganizationTokenListResponse),
        (status = 400, description = "企业标识或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理本人企业 API Key", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_tokens() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/tokens",
    operation_id = "createOrganizationToken",
    tag = "企业 API Key",
    summary = "签发本人企业 API Key",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationTokenWriteRequest,
    responses(
        (status = 201, description = "企业 API Key 已签发；明文只在本次响应返回", body = IssuedOrganizationTokenResponse<'static>),
        (status = 400, description = "Key 参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权签发企业 API Key", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "企业 API Key 数量已达授权上限", body = ManagementErrorBody),
        (status = 503, description = "写入结果未知，调用方必须刷新列表确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_token() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/tokens/directory",
    operation_id = "listOrganizationTokenDirectory",
    tag = "企业 API Key",
    summary = "列出企业脱敏 API Key 目录",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("after" = Option<i64>, Query, minimum = 1, description = "上一页最后一条 Key 标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100, description = "返回数量")
    ),
    responses(
        (status = 200, description = "企业脱敏 API Key 目录", body = OrganizationTokenListResponse),
        (status = 400, description = "企业标识或分页参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业 API Key 目录", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_token_directory() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/tokens/{token_id}",
    operation_id = "getOrganizationToken",
    tag = "企业 API Key",
    summary = "读取本人企业 API Key",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("token_id" = i64, Path, minimum = 1, description = "API Key 标识")
    ),
    responses(
        (status = 200, description = "本人企业 API Key", body = OrganizationTokenResponse),
        (status = 400, description = "企业或 Key 标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理本人企业 API Key", body = ManagementErrorBody),
        (status = 404, description = "企业或本人 Key 不存在", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_token() {}

#[utoipa::path(
    put,
    path = "/api/organizations/{organization_id}/tokens/{token_id}",
    operation_id = "updateOrganizationToken",
    tag = "企业 API Key",
    summary = "更新本人企业 API Key",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("token_id" = i64, Path, minimum = 1, description = "API Key 标识")
    ),
    request_body = OrganizationTokenWriteRequest,
    responses(
        (status = 200, description = "企业 API Key 已更新", body = OrganizationTokenResponse),
        (status = 400, description = "Key 标识或参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理本人企业 API Key", body = ManagementErrorBody),
        (status = 404, description = "企业或本人 Key 不存在", body = ManagementErrorBody),
        (status = 503, description = "写入结果未知，调用方必须刷新详情确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_organization_token() {}

#[utoipa::path(
    delete,
    path = "/api/organizations/{organization_id}/tokens/{token_id}",
    operation_id = "disableOwnOrganizationToken",
    tag = "企业 API Key",
    summary = "停用本人企业 API Key",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("token_id" = i64, Path, minimum = 1, description = "API Key 标识")
    ),
    responses(
        (status = 200, description = "本人企业 API Key 已停用", body = OrganizationTokenResponse),
        (status = 400, description = "企业或 Key 标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理本人企业 API Key", body = ManagementErrorBody),
        (status = 404, description = "企业或本人 Key 不存在", body = ManagementErrorBody),
        (status = 503, description = "写入结果未知，调用方必须刷新详情确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_own_organization_token() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/tokens/{token_id}/disable",
    operation_id = "disableOrganizationToken",
    tag = "企业 API Key",
    summary = "管理员停用企业成员 API Key",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("token_id" = i64, Path, minimum = 1, description = "API Key 标识")
    ),
    responses(
        (status = 200, description = "企业 API Key 已停用", body = OrganizationTokenResponse),
        (status = 400, description = "企业或 Key 标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权停用企业成员 API Key", body = ManagementErrorBody),
        (status = 404, description = "企业或 Key 不存在", body = ManagementErrorBody),
        (status = 503, description = "写入结果未知，调用方必须刷新目录确认", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_organization_token() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_organization_tokens,
        create_organization_token,
        list_organization_token_directory,
        get_organization_token,
        update_organization_token,
        disable_own_organization_token,
        disable_organization_token
    ),
    components(schemas(
        OrganizationTokenStatusDto,
        OrganizationTokenWriteRequest,
        OrganizationTokenResponse,
        OrganizationTokenListResponse,
        IssuedOrganizationTokenResponse<'static>,
        ManagementErrorBody
    ))
)]
struct OrganizationTokenApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationTokenApi::openapi()
}
