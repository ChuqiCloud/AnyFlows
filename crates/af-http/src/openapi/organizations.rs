#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organizations::{
        OrganizationEntitlementResponse, OrganizationInvitationAcceptRequest,
        OrganizationInvitationAcceptResponse, OrganizationInvitationBatchRequest,
        OrganizationInvitationBatchResponse, OrganizationInvitationCreateRequest,
        OrganizationInvitationIssuedResponse, OrganizationInvitationListResponse,
        OrganizationInvitationPreviewItemResponse, OrganizationInvitationPreviewRequest,
        OrganizationInvitationPreviewResponse, OrganizationInvitationResponse,
        OrganizationListResponse, OrganizationOverviewResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/account/organizations",
    operation_id = "listAccountOrganizations",
    tag = "企业空间",
    summary = "列出当前用户可进入的企业",
    responses(
        (status = 200, description = "活动企业目录", body = OrganizationListResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_account_organizations() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}",
    operation_id = "getOrganizationOverview",
    tag = "企业空间",
    summary = "读取企业概览与当前成员权限",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业概览", body = OrganizationOverviewResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "企业角色无权访问", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_overview() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/invitations",
    operation_id = "listOrganizationInvitations",
    tag = "企业空间",
    summary = "列出企业邀请安全投影",
    params(("organization_id" = i64, Path, minimum = 1)),
    responses(
        (status = 200, body = OrganizationInvitationListResponse),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_invitations() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/invitations",
    operation_id = "createOrganizationInvitation",
    tag = "企业空间",
    summary = "创建企业邀请",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationInvitationCreateRequest,
    responses(
        (status = 200, body = OrganizationInvitationIssuedResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_invitation() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/invitations/preview",
    operation_id = "previewOrganizationInvitations",
    tag = "企业空间",
    summary = "预览最多 200 条企业邀请",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationInvitationPreviewRequest,
    responses(
        (status = 200, body = OrganizationInvitationPreviewResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn preview_organization_invitations() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/invitations/batches",
    operation_id = "createOrganizationInvitationBatch",
    tag = "企业空间",
    summary = "幂等提交最多 200 条企业邀请",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationInvitationBatchRequest,
    responses(
        (status = 200, body = OrganizationInvitationBatchResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_invitation_batch() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/invitations/{invitation_key}/revoke",
    operation_id = "revokeOrganizationInvitation",
    tag = "企业空间",
    summary = "撤销待接受企业邀请",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("invitation_key" = String, Path, description = "企业邀请公开标识")
    ),
    responses(
        (status = 200, body = OrganizationInvitationResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn revoke_organization_invitation() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/invitations/{invitation_key}/resend",
    operation_id = "resendOrganizationInvitation",
    tag = "企业空间",
    summary = "重发企业邀请并使旧令牌失效",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("invitation_key" = String, Path, description = "企业邀请公开标识")
    ),
    responses(
        (status = 200, body = OrganizationInvitationIssuedResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn resend_organization_invitation() {}

#[utoipa::path(
    post,
    path = "/api/account/organization-invitations/accept",
    operation_id = "acceptOrganizationInvitation",
    tag = "企业空间",
    summary = "当前登录用户单次消费企业邀请",
    request_body = OrganizationInvitationAcceptRequest,
    responses(
        (status = 200, body = OrganizationInvitationAcceptResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn accept_organization_invitation() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_account_organizations,
        get_organization_overview,
        list_organization_invitations,
        create_organization_invitation,
        preview_organization_invitations,
        create_organization_invitation_batch,
        revoke_organization_invitation,
        resend_organization_invitation,
        accept_organization_invitation
    ),
    components(schemas(
        OrganizationListResponse,
        OrganizationOverviewResponse,
        OrganizationEntitlementResponse,
        OrganizationInvitationCreateRequest,
        OrganizationInvitationPreviewRequest,
        OrganizationInvitationBatchRequest,
        OrganizationInvitationAcceptRequest,
        OrganizationInvitationResponse,
        OrganizationInvitationIssuedResponse,
        OrganizationInvitationListResponse,
        OrganizationInvitationBatchResponse,
        OrganizationInvitationPreviewItemResponse,
        OrganizationInvitationPreviewResponse,
        OrganizationInvitationAcceptResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationApi::openapi()
}
