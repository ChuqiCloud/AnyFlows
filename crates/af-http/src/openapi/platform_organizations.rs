#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    platform_organizations::{
        PlatformOrganizationDirectoryResponse, PlatformOrganizationInvitationIssuedResponse,
        PlatformOrganizationInvitationRequest, PlatformOrganizationMemberChangeRequest,
        PlatformOrganizationMemberListResponse, PlatformOrganizationMemberResponse,
        PlatformOrganizationSummaryResponse, PlatformUserOrganizationListResponse,
        PlatformUserOrganizationSummaryResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/platform/organizations",
    operation_id = "listPlatformOrganizations",
    tag = "平台企业目录",
    summary = "按稳定游标读取平台企业安全摘要",
    params(
        ("after" = Option<i64>, Query, minimum = 1),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业访问理由")
    ),
    responses(
        (status = 200, body = PlatformOrganizationDirectoryResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_platform_organizations() {}

#[utoipa::path(
    get,
    path = "/api/platform/users/{user_id}/organizations",
    operation_id = "listPlatformUserOrganizations",
    tag = "平台企业目录",
    summary = "读取指定用户的企业成员归属",
    params(
        ("user_id" = i64, Path, minimum = 1),
        ("after" = Option<i64>, Query, minimum = 1),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业访问理由")
    ),
    responses(
        (status = 200, body = PlatformUserOrganizationListResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_platform_user_organizations() {}

#[utoipa::path(
    get,
    path = "/api/platform/organizations/{organization_id}/members",
    operation_id = "listPlatformOrganizationMembers",
    tag = "平台企业目录",
    summary = "读取指定企业的成员身份与生命周期摘要",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("after" = Option<i64>, Query, minimum = 1),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业访问理由")
    ),
    responses(
        (status = 200, body = PlatformOrganizationMemberListResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_platform_organization_members() {}

#[utoipa::path(
    patch,
    path = "/api/platform/organizations/{organization_id}/members/{membership_id}",
    operation_id = "changePlatformOrganizationMember",
    tag = "平台企业目录",
    summary = "按版本调整企业成员固定角色与状态",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("membership_id" = i64, Path, minimum = 1),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业操作理由")
    ),
    request_body = PlatformOrganizationMemberChangeRequest,
    responses(
        (status = 200, body = PlatformOrganizationMemberResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn change_platform_organization_member() {}

#[utoipa::path(
    post,
    path = "/api/platform/organizations/{organization_id}/invitations",
    operation_id = "invitePlatformOrganizationMember",
    tag = "平台企业目录",
    summary = "按目标用户接受语义创建企业邀请",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("x-anyflows-platform-reason" = String, Header, min_length = 1, max_length = 255, description = "跨企业操作理由")
    ),
    request_body = PlatformOrganizationInvitationRequest,
    responses(
        (status = 200, body = PlatformOrganizationInvitationIssuedResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn invite_platform_organization_member() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_platform_organizations,
        list_platform_user_organizations,
        list_platform_organization_members,
        change_platform_organization_member,
        invite_platform_organization_member
    ),
    components(schemas(
        PlatformOrganizationSummaryResponse,
        PlatformOrganizationDirectoryResponse,
        PlatformUserOrganizationSummaryResponse,
        PlatformUserOrganizationListResponse,
        PlatformOrganizationMemberResponse,
        PlatformOrganizationMemberListResponse,
        PlatformOrganizationMemberChangeRequest,
        PlatformOrganizationInvitationRequest,
        PlatformOrganizationInvitationIssuedResponse,
        ManagementErrorBody
    ))
)]
struct PlatformOrganizationsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PlatformOrganizationsApi::openapi()
}
