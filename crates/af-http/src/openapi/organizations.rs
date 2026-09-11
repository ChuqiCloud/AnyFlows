#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organizations::{
        OrganizationCustomRoleChangeRequest, OrganizationCustomRoleCreateRequest,
        OrganizationCustomRoleDisableRequest, OrganizationCustomRoleListResponse,
        OrganizationCustomRoleResponse, OrganizationDepartmentCreateRequest,
        OrganizationDepartmentDisableRequest, OrganizationDepartmentListResponse,
        OrganizationDepartmentMembershipAddRequest, OrganizationDepartmentMembershipListResponse,
        OrganizationDepartmentMembershipPrimaryRequest,
        OrganizationDepartmentMembershipRemoveRequest, OrganizationDepartmentMembershipResponse,
        OrganizationDepartmentMoveRequest, OrganizationDepartmentResponse,
        OrganizationEntitlementResponse, OrganizationInvitationAcceptRequest,
        OrganizationInvitationAcceptResponse, OrganizationInvitationBatchRequest,
        OrganizationInvitationBatchResponse, OrganizationInvitationCreateRequest,
        OrganizationInvitationIssuedResponse, OrganizationInvitationListResponse,
        OrganizationInvitationPreviewItemResponse, OrganizationInvitationPreviewRequest,
        OrganizationInvitationPreviewResponse, OrganizationInvitationResponse,
        OrganizationListResponse, OrganizationMemberCustomRoleRequest,
        OrganizationMemberCustomRoleResponse, OrganizationMembershipChangeRequest,
        OrganizationMembershipListResponse, OrganizationMembershipResponse,
        OrganizationOverviewResponse, OrganizationOwnershipTransferRequest,
        OrganizationOwnershipTransferResponse, OrganizationProfileChangeRequest,
        OrganizationProfileResponse, OrganizationServiceAccountBudgetBindingRequest,
        OrganizationServiceAccountBudgetBindingResponse, OrganizationServiceAccountCreateRequest,
        OrganizationServiceAccountIssuedResponse, OrganizationServiceAccountListResponse,
        OrganizationServiceAccountResponse, OrganizationServiceAccountRotateRequest,
        OrganizationServiceAccountStatusRequest, OrganizationTeamChangeRequest,
        OrganizationTeamCreateRequest, OrganizationTeamDisableRequest,
        OrganizationTeamListResponse, OrganizationTeamResponse,
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
    patch,
    path = "/api/organizations/{organization_id}",
    operation_id = "updateOrganizationProfile",
    tag = "企业空间",
    summary = "按版本更新企业基础资料",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationProfileChangeRequest,
    responses(
        (status = 200, description = "更新后的企业基础资料", body = OrganizationProfileResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_organization_profile() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/departments",
    operation_id = "listOrganizationDepartments",
    tag = "企业部门",
    summary = "按稳定游标列出企业部门",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("after" = Option<String>, Query, description = "排序键游标，格式为 sort_order:id"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)
    ),
    responses(
        (status = 200, body = OrganizationDepartmentListResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_departments() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/departments/{department_id}",
    operation_id = "getOrganizationDepartment",
    tag = "企业部门",
    summary = "读取企业部门详情",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("department_id" = i64, Path, minimum = 1)
    ),
    responses(
        (status = 200, body = OrganizationDepartmentResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_department() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/departments",
    operation_id = "createOrganizationDepartment",
    tag = "企业部门",
    summary = "创建企业部门",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationDepartmentCreateRequest,
    responses(
        (status = 200, body = OrganizationDepartmentResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_department() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/departments/{department_id}/move",
    operation_id = "moveOrganizationDepartment",
    tag = "企业部门",
    summary = "按版本 CAS 移动企业部门",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("department_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationDepartmentMoveRequest,
    responses(
        (status = 200, body = OrganizationDepartmentResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn move_organization_department() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/departments/{department_id}/disable",
    operation_id = "disableOrganizationDepartment",
    tag = "企业部门",
    summary = "按版本 CAS 不可逆禁用企业部门",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("department_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationDepartmentDisableRequest,
    responses(
        (status = 200, body = OrganizationDepartmentResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_organization_department() {}

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

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/members",
    operation_id = "listOrganizationMembers",
    tag = "企业空间",
    summary = "按稳定游标列出企业成员",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("after" = Option<i64>, Query, minimum = 1),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)
    ),
    responses(
        (status = 200, body = OrganizationMembershipListResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_members() {}

#[utoipa::path(
    patch,
    path = "/api/organizations/{organization_id}/members/{membership_id}",
    operation_id = "changeOrganizationMember",
    tag = "企业空间",
    summary = "按 CAS 更新成员角色、团队与状态",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("membership_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationMembershipChangeRequest,
    responses(
        (status = 200, body = OrganizationMembershipResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn change_organization_member() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/members/{membership_id}/department-memberships",
    operation_id = "listOrganizationMemberDepartmentMemberships",
    tag = "企业部门",
    summary = "读取成员的活动与历史部门归属",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("membership_id" = i64, Path, minimum = 1)
    ),
    responses(
        (status = 200, body = OrganizationDepartmentMembershipListResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_member_department_memberships() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/members/{membership_id}/department-memberships",
    operation_id = "addOrganizationMemberDepartmentMembership",
    tag = "企业部门",
    summary = "添加成员部门归属",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("membership_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationDepartmentMembershipAddRequest,
    responses(
        (status = 200, body = OrganizationDepartmentMembershipResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn add_organization_member_department_membership() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}/primary",
    operation_id = "setPrimaryOrganizationMemberDepartmentMembership",
    tag = "企业部门",
    summary = "按成员版本 CAS 设置主部门",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("membership_id" = i64, Path, minimum = 1),
        ("department_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationDepartmentMembershipPrimaryRequest,
    responses(
        (status = 200, body = OrganizationDepartmentMembershipResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn set_primary_organization_member_department_membership() {}

#[utoipa::path(
    delete,
    path = "/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}",
    operation_id = "removeOrganizationMemberDepartmentMembership",
    tag = "企业部门",
    summary = "解除非主部门归属并保留历史",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("membership_id" = i64, Path, minimum = 1),
        ("department_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationDepartmentMembershipRemoveRequest,
    responses(
        (status = 200, body = OrganizationDepartmentMembershipResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn remove_organization_member_department_membership() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/ownership-transfer",
    operation_id = "transferOrganizationOwnership",
    tag = "企业空间",
    summary = "由当前 Owner 原子转移企业所有权",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationOwnershipTransferRequest,
    responses(
        (status = 200, body = OrganizationOwnershipTransferResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn transfer_organization_ownership() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/teams",
    operation_id = "listOrganizationTeams",
    tag = "企业空间",
    summary = "按稳定游标列出企业团队",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("after" = Option<i64>, Query, minimum = 1),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)
    ),
    responses(
        (status = 200, body = OrganizationTeamListResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_teams() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/teams",
    operation_id = "createOrganizationTeam",
    tag = "企业空间",
    summary = "创建企业团队",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationTeamCreateRequest,
    responses(
        (status = 200, body = OrganizationTeamResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_team() {}

#[utoipa::path(
    patch,
    path = "/api/organizations/{organization_id}/teams/{team_id}",
    operation_id = "changeOrganizationTeam",
    tag = "企业空间",
    summary = "按 CAS 更新企业团队资料",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("team_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationTeamChangeRequest,
    responses(
        (status = 200, body = OrganizationTeamResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn change_organization_team() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/teams/{team_id}/disable",
    operation_id = "disableOrganizationTeam",
    tag = "企业空间",
    summary = "禁用没有活动成员的企业团队",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("team_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationTeamDisableRequest,
    responses(
        (status = 200, body = OrganizationTeamResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_organization_team() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/roles",
    operation_id = "listOrganizationCustomRoles",
    tag = "企业空间",
    summary = "按稳定游标列出企业自定义角色",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("after" = Option<i64>, Query, minimum = 1),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)
    ),
    responses(
        (status = 200, body = OrganizationCustomRoleListResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_custom_roles() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/roles/{role_key}",
    operation_id = "getOrganizationCustomRole",
    tag = "企业空间",
    summary = "读取企业自定义角色详情",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("role_key" = String, Path, description = "企业自定义角色公开标识")
    ),
    responses(
        (status = 200, body = OrganizationCustomRoleResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_custom_role() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/roles",
    operation_id = "createOrganizationCustomRole",
    tag = "企业空间",
    summary = "创建企业自定义角色",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationCustomRoleCreateRequest,
    responses(
        (status = 200, body = OrganizationCustomRoleResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_custom_role() {}

#[utoipa::path(
    patch,
    path = "/api/organizations/{organization_id}/roles/{role_key}",
    operation_id = "updateOrganizationCustomRole",
    tag = "企业空间",
    summary = "按 CAS 更新企业自定义角色",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("role_key" = String, Path, description = "企业自定义角色公开标识")
    ),
    request_body = OrganizationCustomRoleChangeRequest,
    responses(
        (status = 200, body = OrganizationCustomRoleResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_organization_custom_role() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/roles/{role_key}/disable",
    operation_id = "disableOrganizationCustomRole",
    tag = "企业空间",
    summary = "按 CAS 不可逆停用企业自定义角色",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("role_key" = String, Path, description = "企业自定义角色公开标识")
    ),
    request_body = OrganizationCustomRoleDisableRequest,
    responses(
        (status = 200, body = OrganizationCustomRoleResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn disable_organization_custom_role() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/service-accounts",
    operation_id = "listOrganizationServiceAccounts",
    tag = "企业服务账号",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("after" = Option<String>, Query, description = "上一页最后一项的服务账号随机公开标识"),
        ("limit" = Option<usize>, Query, minimum = 1, maximum = 100)
    ),
    responses((status = 200, body = OrganizationServiceAccountListResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn list_organization_service_accounts() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/service-accounts/{service_account_key}",
    operation_id = "getOrganizationServiceAccount",
    tag = "企业服务账号",
    params(("organization_id" = i64, Path, minimum = 1), ("service_account_key" = String, Path, description = "服务账号随机公开标识")),
    responses((status = 200, body = OrganizationServiceAccountResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_service_account() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/service-accounts",
    operation_id = "createOrganizationServiceAccount",
    tag = "企业服务账号",
    params(("organization_id" = i64, Path, minimum = 1)),
    request_body = OrganizationServiceAccountCreateRequest,
    responses((status = 200, body = OrganizationServiceAccountIssuedResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn create_organization_service_account() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/service-accounts/{service_account_key}/status",
    operation_id = "changeOrganizationServiceAccountStatus",
    tag = "企业服务账号",
    params(("organization_id" = i64, Path, minimum = 1), ("service_account_key" = String, Path, description = "服务账号随机公开标识")),
    request_body = OrganizationServiceAccountStatusRequest,
    responses((status = 200, body = OrganizationServiceAccountResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn change_organization_service_account_status() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/service-accounts/{service_account_key}/rotate-key",
    operation_id = "rotateOrganizationServiceAccountKey",
    tag = "企业服务账号",
    params(("organization_id" = i64, Path, minimum = 1), ("service_account_key" = String, Path, description = "服务账号随机公开标识")),
    request_body = OrganizationServiceAccountRotateRequest,
    responses((status = 200, body = OrganizationServiceAccountIssuedResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn rotate_organization_service_account_key() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/service-accounts/{service_account_key}/budget-binding",
    operation_id = "getOrganizationServiceAccountBudgetBinding",
    tag = "企业服务账号",
    summary = "读取服务账号预算绑定",
    params(("organization_id" = i64, Path, minimum = 1), ("service_account_key" = String, Path, description = "服务账号随机公开标识")),
    responses((status = 200, body = OrganizationServiceAccountBudgetBindingResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn get_organization_service_account_budget_binding() {}

#[utoipa::path(
    put,
    path = "/api/organizations/{organization_id}/service-accounts/{service_account_key}/budget-binding",
    operation_id = "setOrganizationServiceAccountBudgetBinding",
    tag = "企业服务账号",
    summary = "按 CAS 设置服务账号预算绑定",
    params(("organization_id" = i64, Path, minimum = 1), ("service_account_key" = String, Path, description = "服务账号随机公开标识")),
    request_body = OrganizationServiceAccountBudgetBindingRequest,
    responses((status = 200, body = OrganizationServiceAccountBudgetBindingResponse), (status = 400, body = ManagementErrorBody), (status = 401, body = ManagementErrorBody), (status = 403, body = ManagementErrorBody), (status = 404, body = ManagementErrorBody), (status = 409, body = ManagementErrorBody), (status = 500, body = ManagementErrorBody)),
    security(("bearerAuth" = []))
)]
fn set_organization_service_account_budget_binding() {}

#[utoipa::path(
    patch,
    path = "/api/organizations/{organization_id}/members/{membership_id}/custom-role",
    operation_id = "setOrganizationMemberCustomRole",
    tag = "企业空间",
    summary = "按成员版本绑定或清除自定义角色",
    params(
        ("organization_id" = i64, Path, minimum = 1),
        ("membership_id" = i64, Path, minimum = 1)
    ),
    request_body = OrganizationMemberCustomRoleRequest,
    responses(
        (status = 200, body = OrganizationMemberCustomRoleResponse),
        (status = 400, body = ManagementErrorBody),
        (status = 401, body = ManagementErrorBody),
        (status = 403, body = ManagementErrorBody),
        (status = 404, body = ManagementErrorBody),
        (status = 409, body = ManagementErrorBody),
        (status = 500, body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn set_organization_member_custom_role() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_account_organizations,
        get_organization_overview,
        update_organization_profile,
        list_organization_departments,
        get_organization_department,
        create_organization_department,
        move_organization_department,
        disable_organization_department,
        list_organization_invitations,
        create_organization_invitation,
        preview_organization_invitations,
        create_organization_invitation_batch,
        revoke_organization_invitation,
        resend_organization_invitation,
        accept_organization_invitation,
        list_organization_members,
        change_organization_member,
        list_organization_member_department_memberships,
        add_organization_member_department_membership,
        set_primary_organization_member_department_membership,
        remove_organization_member_department_membership,
        transfer_organization_ownership,
        list_organization_teams,
        create_organization_team,
        change_organization_team,
        disable_organization_team,
        list_organization_custom_roles,
        get_organization_custom_role,
        create_organization_custom_role,
        update_organization_custom_role,
        disable_organization_custom_role,
        list_organization_service_accounts,
        get_organization_service_account,
        create_organization_service_account,
        change_organization_service_account_status,
        rotate_organization_service_account_key,
        get_organization_service_account_budget_binding,
        set_organization_service_account_budget_binding,
        set_organization_member_custom_role
    ),
    components(schemas(
        OrganizationListResponse,
        OrganizationOverviewResponse,
        OrganizationProfileChangeRequest,
        OrganizationProfileResponse,
        OrganizationEntitlementResponse,
        OrganizationDepartmentResponse,
        OrganizationDepartmentListResponse,
        OrganizationDepartmentCreateRequest,
        OrganizationDepartmentMoveRequest,
        OrganizationDepartmentDisableRequest,
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
        OrganizationMembershipChangeRequest,
        OrganizationDepartmentMembershipAddRequest,
        OrganizationDepartmentMembershipPrimaryRequest,
        OrganizationDepartmentMembershipRemoveRequest,
        OrganizationDepartmentMembershipResponse,
        OrganizationDepartmentMembershipListResponse,
        OrganizationOwnershipTransferRequest,
        OrganizationTeamCreateRequest,
        OrganizationTeamChangeRequest,
        OrganizationTeamDisableRequest,
        OrganizationMembershipResponse,
        OrganizationMembershipListResponse,
        OrganizationTeamResponse,
        OrganizationTeamListResponse,
        OrganizationOwnershipTransferResponse,
        OrganizationCustomRoleCreateRequest,
        OrganizationCustomRoleChangeRequest,
        OrganizationCustomRoleDisableRequest,
        OrganizationCustomRoleResponse,
        OrganizationMemberCustomRoleRequest,
        OrganizationMemberCustomRoleResponse,
        OrganizationCustomRoleListResponse,
        OrganizationServiceAccountCreateRequest,
        OrganizationServiceAccountStatusRequest,
        OrganizationServiceAccountRotateRequest,
        OrganizationServiceAccountResponse,
        OrganizationServiceAccountIssuedResponse,
        OrganizationServiceAccountListResponse,
        OrganizationServiceAccountBudgetBindingRequest,
        OrganizationServiceAccountBudgetBindingResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationApi::openapi()
}
