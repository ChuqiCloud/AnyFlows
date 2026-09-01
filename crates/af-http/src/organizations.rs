use std::sync::Arc;

use af_admin::{
    DEFAULT_ORGANIZATION_INVITATION_TTL_SECONDS, IssuedOrganizationInvitation,
    OrganizationAccessSummary, OrganizationActivityService, OrganizationAuditOutcome,
    OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationCustomRoleBindingCommand, OrganizationCustomRoleBindingService,
    OrganizationCustomRoleBindingServiceError, OrganizationCustomRoleChangeCommand,
    OrganizationCustomRoleCreateCommand, OrganizationCustomRoleDisableCommand,
    OrganizationCustomRoleListQuery, OrganizationCustomRoleService,
    OrganizationCustomRoleServiceError, OrganizationDepartmentCreateCommand,
    OrganizationDepartmentCursor, OrganizationDepartmentDisableCommand,
    OrganizationDepartmentListQuery, OrganizationDepartmentMembershipAddCommand,
    OrganizationDepartmentMembershipRecord, OrganizationDepartmentMoveCommand,
    OrganizationDepartmentRecord, OrganizationEntitlementSummary,
    OrganizationInvitationBatchCommand, OrganizationInvitationBatchResult,
    OrganizationInvitationCommand, OrganizationInvitationError,
    OrganizationInvitationPreviewCommand, OrganizationInvitationPreviewResult,
    OrganizationInvitationRecord, OrganizationInvitationService, OrganizationManagementError,
    OrganizationManagementService, OrganizationMembershipChangeCommand,
    OrganizationMembershipListQuery, OrganizationMembershipRecord,
    OrganizationOwnershipTransferCommand, OrganizationServiceAccountBudgetBindingCommand,
    OrganizationServiceAccountCreateCommand, OrganizationServiceAccountIssued,
    OrganizationServiceAccountListQuery, OrganizationServiceAccountRotateCommand,
    OrganizationServiceAccountService, OrganizationServiceAccountServiceError,
    OrganizationServiceAccountStatusCommand, OrganizationSsoDomain as AdminOrganizationSsoDomain,
    OrganizationSsoDomainCreateCommand, OrganizationSsoDomainCreated, OrganizationSsoDomainService,
    OrganizationSsoDomainServiceError, OrganizationSsoDomainVerifyCommand,
    OrganizationSsoIdentityBindingService, OrganizationSsoIdentityBindingServiceError,
    OrganizationSsoIdentityBindingStepUp, OrganizationSsoLoginTransactionService,
    OrganizationSsoLoginTransactionServiceError, OrganizationSsoPolicyCommand,
    OrganizationSsoPolicyService, OrganizationSsoPolicyServiceError, OrganizationSsoProvider,
    OrganizationSsoProviderCommand, OrganizationSsoProviderService,
    OrganizationSsoProviderServiceError, OrganizationSsoRecoveryCommand,
    OrganizationSsoRecoveryService, OrganizationSsoRecoveryServiceError,
    OrganizationTeamChangeCommand, OrganizationTeamCreateCommand, OrganizationTeamDisableCommand,
    OrganizationTeamListQuery, OrganizationTeamRecord, SessionAuthentication,
};
use af_domain::{
    OrganizationBudgetPolicyId, OrganizationCustomRoleKey, OrganizationDepartmentId,
    OrganizationEntitlementStatus, OrganizationId, OrganizationInvitationKey,
    OrganizationInvitationStatus, OrganizationMembershipId, OrganizationMembershipStatus,
    OrganizationPermission, OrganizationRole, OrganizationServiceAccountBudgetScope,
    OrganizationServiceAccountKey, OrganizationServiceAccountStatus,
    OrganizationSsoEnforcementPolicy, OrganizationSsoProtocol, OrganizationSsoProviderStatus,
    OrganizationTeamId,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{delete, get, patch, post},
};
use http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::{
    ToSchema,
    openapi::{
        RefOr,
        schema::{ObjectBuilder, Schema, Type},
    },
};

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
    middleware::ensure_request_id,
    organization_activity::{
        audit_organization_action_result, authorize_organization_action, record_organization_action,
    },
};

#[derive(Clone)]
pub(crate) struct OrganizationHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    invitation_service: Option<Arc<dyn OrganizationInvitationService>>,
    management_service: Option<Arc<dyn OrganizationManagementService>>,
    custom_role_service: Option<Arc<dyn OrganizationCustomRoleService>>,
    custom_role_binding_service: Option<Arc<dyn OrganizationCustomRoleBindingService>>,
    sso_provider_service: Option<Arc<dyn OrganizationSsoProviderService>>,
    sso_domain_service: Option<Arc<dyn OrganizationSsoDomainService>>,
    sso_login_service: Option<Arc<dyn OrganizationSsoLoginTransactionService>>,
    sso_identity_binding_service: Option<Arc<dyn OrganizationSsoIdentityBindingService>>,
    sso_policy_service: Option<Arc<dyn OrganizationSsoPolicyService>>,
    sso_recovery_service: Option<Arc<dyn OrganizationSsoRecoveryService>>,
    service_account_service: Option<Arc<dyn OrganizationServiceAccountService>>,
    activity_service: Arc<dyn OrganizationActivityService>,
}

/// 构建企业目录与企业概览只读入口；所有入口都要求有效管理会话。
pub fn build_organization_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    invitation_service: Option<Arc<dyn OrganizationInvitationService>>,
    management_service: Option<Arc<dyn OrganizationManagementService>>,
    custom_role_service: Option<Arc<dyn OrganizationCustomRoleService>>,
    custom_role_binding_service: Option<Arc<dyn OrganizationCustomRoleBindingService>>,
    activity_service: Arc<dyn OrganizationActivityService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
    sso_provider_service: Option<Arc<dyn OrganizationSsoProviderService>>,
) -> Router {
    build_organization_router_with_domain(
        authorization_service,
        invitation_service,
        management_service,
        custom_role_service,
        custom_role_binding_service,
        activity_service,
        session_authenticator,
        sso_provider_service,
        None,
        None,
        None,
        None,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_organization_router_with_domain(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    invitation_service: Option<Arc<dyn OrganizationInvitationService>>,
    management_service: Option<Arc<dyn OrganizationManagementService>>,
    custom_role_service: Option<Arc<dyn OrganizationCustomRoleService>>,
    custom_role_binding_service: Option<Arc<dyn OrganizationCustomRoleBindingService>>,
    activity_service: Arc<dyn OrganizationActivityService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
    sso_provider_service: Option<Arc<dyn OrganizationSsoProviderService>>,
    sso_policy_service: Option<Arc<dyn OrganizationSsoPolicyService>>,
    sso_domain_service: Option<Arc<dyn OrganizationSsoDomainService>>,
    sso_login_service: Option<Arc<dyn OrganizationSsoLoginTransactionService>>,
    sso_identity_binding_service: Option<Arc<dyn OrganizationSsoIdentityBindingService>>,
    sso_recovery_service: Option<Arc<dyn OrganizationSsoRecoveryService>>,
    service_account_service: Option<Arc<dyn OrganizationServiceAccountService>>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    let router = Router::new()
        .route("/api/account/organizations", get(list_organizations))
        .route(
            "/api/organizations/{organization_id}",
            get(get_organization),
        );
    let router = if invitation_service.is_some() {
        router
            .route(
                "/api/organizations/{organization_id}/invitations",
                get(list_organization_invitations).post(create_organization_invitation),
            )
            .route(
                "/api/organizations/{organization_id}/invitations/preview",
                post(preview_organization_invitations),
            )
            .route(
                "/api/organizations/{organization_id}/invitations/batches",
                post(create_organization_invitation_batch),
            )
            .route(
                "/api/organizations/{organization_id}/invitations/{invitation_key}/revoke",
                post(revoke_organization_invitation),
            )
            .route(
                "/api/organizations/{organization_id}/invitations/{invitation_key}/resend",
                post(resend_organization_invitation),
            )
            .route(
                "/api/account/organization-invitations/accept",
                post(accept_organization_invitation),
            )
    } else {
        router
    };
    let router = if management_service.is_some() {
        router
            .route(
                "/api/organizations/{organization_id}/members",
                get(list_organization_members),
            )
            .route(
                "/api/organizations/{organization_id}/members/{membership_id}",
                patch(change_organization_member),
            )
            .route(
                "/api/organizations/{organization_id}/ownership-transfer",
                post(transfer_organization_ownership),
            )
            .route(
                "/api/organizations/{organization_id}/teams",
                get(list_organization_teams).post(create_organization_team),
            )
            .route(
                "/api/organizations/{organization_id}/teams/{team_id}",
                patch(change_organization_team),
            )
            .route(
                "/api/organizations/{organization_id}/teams/{team_id}/disable",
                post(disable_organization_team),
            )
            .route(
                "/api/organizations/{organization_id}/departments",
                get(list_organization_departments).post(create_organization_department),
            )
            .route(
                "/api/organizations/{organization_id}/departments/{department_id}",
                get(get_organization_department),
            )
            .route(
                "/api/organizations/{organization_id}/departments/{department_id}/move",
                post(move_organization_department),
            )
            .route(
                "/api/organizations/{organization_id}/departments/{department_id}/disable",
                post(disable_organization_department),
            )
            .route(
                "/api/organizations/{organization_id}/members/{membership_id}/department-memberships",
                get(list_organization_department_memberships)
                    .post(add_organization_department_membership),
            )
            .route(
                "/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}/primary",
                post(set_primary_organization_department_membership),
            )
            .route(
                "/api/organizations/{organization_id}/members/{membership_id}/department-memberships/{department_id}",
                delete(remove_organization_department_membership),
            )
    } else {
        router
    };
    let router = if custom_role_service.is_some() {
        router
            .route(
                "/api/organizations/{organization_id}/roles",
                get(list_organization_custom_roles).post(create_organization_custom_role),
            )
            .route(
                "/api/organizations/{organization_id}/roles/{role_key}",
                get(get_organization_custom_role).patch(update_organization_custom_role),
            )
            .route(
                "/api/organizations/{organization_id}/roles/{role_key}/disable",
                post(disable_organization_custom_role),
            )
    } else {
        router
    };
    let router = if custom_role_binding_service.is_some() {
        router.route(
            "/api/organizations/{organization_id}/members/{membership_id}/custom-role",
            patch(set_organization_member_custom_role),
        )
    } else {
        router
    };
    let router = if sso_provider_service.is_some() {
        router
            .route(
                "/api/organizations/{organization_id}/sso/providers",
                get(list_organization_sso_providers).post(create_organization_sso_provider),
            )
            .route(
                "/api/organizations/{organization_id}/sso/providers/{provider_key}",
                get(get_organization_sso_provider).put(update_organization_sso_provider),
            )
    } else {
        router
    };
    let router = if sso_domain_service.is_some() {
        router
            .route(
                "/api/organizations/{organization_id}/sso/domains",
                get(list_organization_sso_domains).post(create_organization_sso_domain),
            )
            .route(
                "/api/organizations/{organization_id}/sso/domains/{domain_id}/verify",
                post(verify_organization_sso_domain),
            )
    } else {
        router
    };
    let router = if sso_login_service.is_some() {
        router.route(
            "/api/organizations/{organization_id}/sso/login/start",
            post(start_organization_sso_login),
        )
    } else {
        router
    };
    let router = if sso_login_service.is_some() && sso_identity_binding_service.is_some() {
        router.route(
            "/api/organizations/{organization_id}/sso/identities/bind",
            post(bind_organization_sso_identity),
        )
    } else {
        router
    };
    let router = if sso_recovery_service.is_some() {
        router
            .route(
                "/api/organizations/{organization_id}/sso/recovery/establish",
                post(establish_organization_sso_recovery),
            )
            .route(
                "/api/organizations/{organization_id}/sso/recovery/suspend-required",
                post(suspend_organization_sso_required),
            )
    } else {
        router
    };
    let router = if sso_policy_service.is_some() {
        router.route(
            "/api/organizations/{organization_id}/sso/policy",
            get(get_organization_sso_policy).put(update_organization_sso_policy),
        )
    } else {
        router
    };
    let router = if service_account_service.is_some() {
        router
            .route(
                "/api/organizations/{organization_id}/service-accounts",
                get(list_organization_service_accounts).post(create_organization_service_account),
            )
            .route(
                "/api/organizations/{organization_id}/service-accounts/{service_account_key}",
                get(get_organization_service_account),
            )
            .route(
                "/api/organizations/{organization_id}/service-accounts/{service_account_key}/status",
                post(change_organization_service_account_status),
            )
            .route(
                "/api/organizations/{organization_id}/service-accounts/{service_account_key}/rotate-key",
                post(rotate_organization_service_account_key),
            )
            .route(
                "/api/organizations/{organization_id}/service-accounts/{service_account_key}/budget-binding",
                get(get_organization_service_account_budget_binding)
                    .put(set_organization_service_account_budget_binding),
            )
    } else {
        router
    };
    router
        .layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(OrganizationHttpState {
            authorization_service,
            invitation_service,
            management_service,
            custom_role_service,
            custom_role_binding_service,
            sso_provider_service,
            sso_domain_service,
            sso_login_service,
            sso_identity_binding_service,
            sso_policy_service,
            sso_recovery_service,
            service_account_service,
            activity_service,
        })
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationSummaryResponse {
    id: i64,
    name: String,
    slug: String,
    role: &'static str,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationListResponse {
    organizations: Vec<OrganizationSummaryResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationOverviewResponse {
    id: i64,
    name: String,
    slug: String,
    role: &'static str,
    membership_id: i64,
    permissions: Vec<&'static str>,
    entitlement: OrganizationEntitlementResponse,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationEntitlementResponse {
    status: &'static str,
    sso_enabled: bool,
    team_management_enabled: bool,
    custom_roles_enabled: bool,
    seat_limit: i64,
    active_seats: i64,
    remaining_seats: i64,
    team_limit: i64,
    active_teams: i64,
    remaining_teams: i64,
    organization_key_limit: i64,
    audit_retention_days: i64,
    valid_until: i64,
    grace_until: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationCreateRequest {
    email: String,
    role: String,
    team_id: Option<i64>,
    expires_at: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationPreviewRequest {
    items: Vec<OrganizationInvitationCreateRequest>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationBatchRequest {
    batch_key: String,
    items: Vec<OrganizationInvitationCreateRequest>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationAcceptRequest {
    token: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationResponse {
    invitation_key: String,
    organization_id: i64,
    role: &'static str,
    team_id: Option<i64>,
    status: &'static str,
    invited_by_user_id: i64,
    accepted_by_user_id: Option<i64>,
    batch_key: Option<String>,
    version: i64,
    created_at: i64,
    expires_at: i64,
    finalized_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationIssuedResponse {
    invitation: OrganizationInvitationResponse,
    token: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationListResponse {
    invitations: Vec<OrganizationInvitationResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationBatchResponse {
    replayed: bool,
    invitations: Vec<OrganizationInvitationResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationPreviewItemResponse {
    index: usize,
    ready: bool,
    issues: Vec<&'static str>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationPreviewResponse {
    ready: bool,
    available_seats: i64,
    items: Vec<OrganizationInvitationPreviewItemResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationInvitationAcceptResponse {
    organization_id: i64,
    membership_id: i64,
    user_id: i64,
    role: &'static str,
    team_id: Option<i64>,
    status: &'static str,
    version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationMembershipChangeRequest {
    role: String,
    status: String,
    team_id: Option<i64>,
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentMembershipAddRequest {
    #[schema(minimum = 1)]
    department_id: i64,
    is_primary: bool,
    #[schema(minimum = 1, required = true)]
    expected_membership_version: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentMembershipPrimaryRequest {
    #[schema(minimum = 1)]
    expected_membership_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentMembershipRemoveRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationOwnershipTransferRequest {
    target_membership_id: i64,
    current_owner_version: i64,
    target_expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationTeamCreateRequest {
    name: String,
    description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationTeamChangeRequest {
    name: String,
    description: Option<String>,
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationTeamDisableRequest {
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationCustomRoleCreateRequest {
    name: String,
    description: Option<String>,
    permissions: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationCustomRoleChangeRequest {
    name: String,
    description: Option<String>,
    permissions: Vec<String>,
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationCustomRoleDisableRequest {
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationMemberCustomRoleRequest {
    #[schema(required = true)]
    role_key: Option<String>,
    expected_version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationMemberCustomRoleResponse {
    membership_id: i64,
    #[schema(required = true)]
    custom_role_key: Option<String>,
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationCustomRoleResponse {
    role_key: String,
    name: String,
    description: Option<String>,
    status: &'static str,
    permissions: Vec<&'static str>,
    version: i64,
    created_by_user_id: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountCreateRequest {
    name: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountStatusRequest {
    status: String,
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountRotateRequest {
    expected_version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountResponse {
    service_account_key: String,
    organization_id: i64,
    name: String,
    status: &'static str,
    version: i64,
    created_at: i64,
    updated_at: i64,
    revoked_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountIssuedResponse {
    service_account: OrganizationServiceAccountResponse,
    key_key: String,
    key_prefix: String,
    secret: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountListResponse {
    service_accounts: Vec<OrganizationServiceAccountResponse>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountBudgetBindingRequest {
    policy_id: i64,
    expected_version: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationServiceAccountBudgetBindingResponse {
    service_account_key: String,
    organization_id: i64,
    policy_id: i64,
    policy_version: i64,
    scope: &'static str,
    team_id: Option<i64>,
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationCustomRoleListResponse {
    roles: Vec<OrganizationCustomRoleResponse>,
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationMembershipResponse {
    id: i64,
    organization_id: i64,
    user_id: i64,
    role: &'static str,
    team_id: Option<i64>,
    #[schema(required = true)]
    custom_role_key: Option<String>,
    status: &'static str,
    joined_at: i64,
    suspended_at: Option<i64>,
    removed_at: Option<i64>,
    invited_by_user_id: Option<i64>,
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationMembershipListResponse {
    members: Vec<OrganizationMembershipResponse>,
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentMembershipResponse {
    id: i64,
    organization_id: i64,
    membership_id: i64,
    department_id: i64,
    is_primary: bool,
    status: &'static str,
    version: i64,
    created_at: i64,
    updated_at: i64,
    removed_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentMembershipListResponse {
    department_memberships: Vec<OrganizationDepartmentMembershipResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationTeamResponse {
    id: i64,
    organization_id: i64,
    name: String,
    description: Option<String>,
    status: &'static str,
    version: i64,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationTeamListResponse {
    teams: Vec<OrganizationTeamResponse>,
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentResponse {
    id: i64,
    organization_id: i64,
    department_key: String,
    parent_department_id: Option<i64>,
    name: String,
    sort_order: i64,
    depth: u8,
    status: &'static str,
    version: i64,
    created_at: i64,
    updated_at: i64,
    disabled_at: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentListResponse {
    departments: Vec<OrganizationDepartmentResponse>,
    next_cursor: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentCreateRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(minimum = 1, required = true)]
    parent_department_id: Option<i64>,
    sort_order: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentMoveRequest {
    #[schema(minimum = 1, required = true)]
    parent_department_id: Option<i64>,
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationDepartmentDisableRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationOwnershipTransferResponse {
    previous_owner: OrganizationMembershipResponse,
    current_owner: OrganizationMembershipResponse,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoProvider)]
pub struct OrganizationSsoProviderResponse {
    id: i64,
    organization_id: i64,
    provider_key: String,
    protocol: &'static str,
    status: &'static str,
    display_name: String,
    metadata_projection: Option<serde_json::Value>,
    secret_configured: bool,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoProviderList)]
pub struct OrganizationSsoProviderListResponse {
    providers: Vec<OrganizationSsoProviderResponse>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoProviderRequest)]
pub struct OrganizationSsoProviderRequest {
    #[schema(minimum = 0)]
    expected_version: i64,
    #[schema(example = "oidc")]
    protocol: String,
    #[schema(example = "draft")]
    status: String,
    #[schema(max_length = 128)]
    display_name: String,
    metadata_projection: Option<serde_json::Value>,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    client_secret: Option<String>,
    clear_client_secret: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDomain)]
pub struct OrganizationSsoDomainResponse {
    id: i64,
    organization_id: i64,
    provider_key: String,
    domain: String,
    status: &'static str,
    proof_expires_at: Option<i64>,
    verified_at: Option<i64>,
    recheck_at: Option<i64>,
    revoked_at: Option<i64>,
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDomainList)]
pub struct OrganizationSsoDomainListResponse {
    domains: Vec<OrganizationSsoDomainResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDomainCreated)]
pub struct OrganizationSsoDomainCreatedResponse {
    domain: OrganizationSsoDomainResponse,
    #[schema(min_length = 1)]
    txt_record_name: String,
    #[schema(min_length = 1)]
    txt_record_value: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDomainCreateRequest)]
pub struct OrganizationSsoDomainCreateRequest {
    provider_key: String,
    domain: String,
    #[schema(maximum = 604_800, minimum = 900)]
    ttl_seconds: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDomainVerifyRequest)]
pub struct OrganizationSsoDomainVerifyRequest {
    #[schema(max_length = 64, min_length = 64, format = Password, write_only)]
    proof_token: String,
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoLoginStartRequest)]
pub struct OrganizationSsoLoginStartRequest {
    #[schema(min_length = 1, max_length = 128)]
    provider_key: String,
    #[schema(min_length = 64, max_length = 64)]
    request_digest: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoLoginStartResponse)]
pub struct OrganizationSsoLoginStartResponse {
    protocol: &'static str,
    provider_version: i64,
    #[schema(min_length = 64, max_length = 64)]
    state_digest: String,
    #[schema(min_length = 64, max_length = 64)]
    request_digest: String,
    expires_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    saml_authn_request: Option<OrganizationSsoSamlAuthnRequestResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationSsoSamlAuthnRequestResponse {
    action: String,
    fields: Vec<OrganizationSsoSamlAuthnRequestFieldResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationSsoSamlAuthnRequestFieldResponse {
    name: String,
    value: String,
}

async fn start_organization_sso_login(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoLoginStartRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoRead,
        &request_id,
        &headers,
        "organization.sso.login.start",
        "organization",
        organization_id.get().to_string(),
    )
    .await?;
    if !matches!(
        context.role(),
        OrganizationRole::Owner | OrganizationRole::Admin
    ) {
        return Err(ManagementError::Forbidden);
    }
    if !context.entitlement().sso_enabled()
        || context.entitlement().status() != OrganizationEntitlementStatus::Active
    {
        return Err(ManagementError::OrganizationEntitlementRestricted);
    }
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let provider_key = request.provider_key.clone();
    let service = state
        .sso_login_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let start = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoRead,
        "organization.sso.login.start",
        "sso_provider",
        provider_key.clone(),
        service
            .begin(organization_id, provider_key, request.request_digest)
            .await
            .map_err(map_sso_login_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoRead.code(),
        "organization.sso.login.start",
        "sso_provider",
        request.provider_key,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "protocol": organization_sso_protocol_code(start.protocol()),
            "provider_version": start.provider_version(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoLoginStartResponse {
        protocol: organization_sso_protocol_code(start.protocol()),
        provider_version: start.provider_version(),
        state_digest: start.state_digest().to_owned(),
        request_digest: start.request_digest().to_owned(),
        expires_at: start.expires_at(),
        saml_authn_request: start.saml_authn_request().map(|request| {
            OrganizationSsoSamlAuthnRequestResponse {
                action: request.action().to_owned(),
                fields: request
                    .fields()
                    .iter()
                    .map(
                        |(name, value)| OrganizationSsoSamlAuthnRequestFieldResponse {
                            name: name.clone(),
                            value: value.clone(),
                        },
                    )
                    .collect(),
            }
        }),
    }))
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoIdentityBindingRequest)]
pub struct OrganizationSsoIdentityBindingRequest {
    #[schema(min_length = 1, max_length = 512, format = Password, write_only)]
    ticket: String,
    #[schema(min_length = 1, max_length = 32, pattern = "^(password|passkey)$")]
    method: String,
    #[schema(max_length = 4096, format = Password, write_only, required = false)]
    current_password: Option<String>,
    #[schema(max_length = 128, format = Password, write_only, required = false)]
    totp_code: Option<String>,
    credential: Option<Value>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoIdentityBindingResponse)]
pub struct OrganizationSsoIdentityBindingResponse {
    identity_id: i64,
    organization_id: i64,
    membership_id: i64,
    status: &'static str,
    binding_method: &'static str,
    version: i64,
}

/// 交换一次性 ticket 并完成当前成员的显式企业 SSO 身份绑定。
async fn bind_organization_sso_identity(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoIdentityBindingRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::OverviewRead,
        &request_id,
        &headers,
        "organization.sso.identity.bind",
        "organization",
        organization_id.get().to_string(),
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let step_up = parse_identity_binding_step_up(&request)?;
    let login_service = state
        .sso_login_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let binding_service = state
        .sso_identity_binding_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let ticket = request.ticket;
    let binding = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::OverviewRead,
        "organization.sso.identity.bind",
        "sso_identity",
        "new".to_owned(),
        async {
            let exchanged = login_service
                .exchange_ticket(organization_id, ticket)
                .await
                .map_err(map_sso_login_error)?;
            binding_service
                .bind_explicit_identity_with_step_up(
                    authentication.principal(),
                    organization_id,
                    exchanged,
                    step_up,
                )
                .await
                .map_err(map_sso_identity_binding_error)
        }
        .await,
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::OverviewRead.code(),
        "organization.sso.identity.bind",
        "sso_identity",
        binding.id().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "membership_id": binding.membership_id().get(),
            "status": organization_sso_identity_status_code(binding.status()),
            "binding_method": organization_sso_identity_binding_method_code(binding.binding_method()),
            "version": binding.version(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoIdentityBindingResponse {
        identity_id: binding.id(),
        organization_id: binding.organization_id().get(),
        membership_id: binding.membership_id().get(),
        status: organization_sso_identity_status_code(binding.status()),
        binding_method: organization_sso_identity_binding_method_code(binding.binding_method()),
        version: binding.version(),
    }))
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoRecoveryRequest)]
pub struct OrganizationSsoRecoveryRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
    #[schema(min_length = 1, max_length = 4096, format = Password, write_only)]
    current_password: String,
    #[schema(max_length = 128, format = Password, write_only, required = false)]
    totp_code: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoRecoveryResponse)]
pub struct OrganizationSsoRecoveryResponse {
    #[schema(schema_with = organization_sso_recovery_policy_schema)]
    policy: &'static str,
    recovery_ready: bool,
    #[schema(minimum = 1)]
    version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoPolicyRequest)]
pub struct OrganizationSsoPolicyRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
    #[schema(schema_with = organization_sso_recovery_policy_schema)]
    policy: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoPolicyResponse)]
pub struct OrganizationSsoPolicyResponse {
    #[schema(schema_with = organization_sso_recovery_policy_schema)]
    policy: &'static str,
    recovery_ready: bool,
    #[schema(minimum = 1)]
    version: i64,
}

/// 保持策略响应为字符串枚举，并兼容 utoipa 的字段级 Schema 构造器。
fn organization_sso_recovery_policy_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::String)
        .enum_values(Some(["optional", "required"]))
        .build()
        .into()
}

async fn get_organization_sso_policy(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoRead,
        &request_id,
        &headers,
        "organization.sso.policy.read",
        "sso_policy",
        organization_id.get().to_string(),
    )
    .await?;
    let service = state
        .sso_policy_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let policy = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoRead,
        "organization.sso.policy.read",
        "sso_policy",
        organization_id.get().to_string(),
        service
            .get(context.clone())
            .await
            .map_err(map_sso_policy_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoRead.code(),
        "organization.sso.policy.read",
        "sso_policy",
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "policy": organization_sso_recovery_policy_code(policy.policy()),
            "recovery_ready": policy.recovery_ready(),
            "version": policy.version(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoPolicyResponse {
        policy: organization_sso_recovery_policy_code(policy.policy()),
        recovery_ready: policy.recovery_ready(),
        version: policy.version(),
    }))
}

async fn update_organization_sso_policy(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoPolicyRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoRecovery,
        &request_id,
        &headers,
        "organization.sso.policy.update",
        "sso_policy",
        organization_id.get().to_string(),
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let policy = parse_sso_enforcement_policy(&request.policy)?;
    let command = OrganizationSsoPolicyCommand::new(policy, request.expected_version)
        .map_err(map_sso_policy_error)?;
    let service = state
        .sso_policy_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let policy = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoRecovery,
        "organization.sso.policy.update",
        "sso_policy",
        organization_id.get().to_string(),
        service
            .update(context.clone(), command)
            .await
            .map_err(map_sso_policy_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoRecovery.code(),
        "organization.sso.policy.update",
        "sso_policy",
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "policy": organization_sso_recovery_policy_code(policy.policy()),
            "recovery_ready": policy.recovery_ready(),
            "version": policy.version(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoPolicyResponse {
        policy: organization_sso_recovery_policy_code(policy.policy()),
        recovery_ready: policy.recovery_ready(),
        version: policy.version(),
    }))
}

/// 完成 Owner break-glass 恢复登记，并仅返回脱敏后的策略快照。
async fn establish_organization_sso_recovery(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoRecoveryRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    execute_organization_sso_recovery(
        state,
        authentication,
        request_id,
        headers,
        organization_id,
        request,
        false,
    )
    .await
}

/// 暂停 Required 策略，保留恢复事实并返回脱敏后的策略快照。
async fn suspend_organization_sso_required(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoRecoveryRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    execute_organization_sso_recovery(
        state,
        authentication,
        request_id,
        headers,
        organization_id,
        request,
        true,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_organization_sso_recovery(
    state: OrganizationHttpState,
    authentication: SessionAuthentication,
    request_id: RequestId,
    headers: HeaderMap,
    organization_id: i64,
    request: Result<Json<OrganizationSsoRecoveryRequest>, JsonRejection>,
    suspend_required: bool,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let operation = if suspend_required {
        "organization.sso.recovery.suspend_required"
    } else {
        "organization.sso.recovery.establish"
    };
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoRecovery,
        &request_id,
        &headers,
        operation,
        "sso_recovery",
        organization_id.get().to_string(),
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = recovery_command(request)?;
    let service = state
        .sso_recovery_service
        .as_ref()
        .ok_or(ManagementError::OrganizationSsoRecoveryUnavailable)?;
    let result = if suspend_required {
        service.suspend_required(authentication.principal(), context.clone(), command)
    } else {
        service.establish(authentication.principal(), context.clone(), command)
    }
    .await
    .map_err(map_sso_recovery_error);
    let recovery = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoRecovery,
        operation,
        "sso_recovery",
        organization_id.get().to_string(),
        result,
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoRecovery.code(),
        operation,
        "sso_recovery",
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "policy": organization_sso_recovery_policy_code(recovery.policy()),
            "recovery_ready": recovery.recovery_ready(),
            "version": recovery.version(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoRecoveryResponse {
        policy: organization_sso_recovery_policy_code(recovery.policy()),
        recovery_ready: recovery.recovery_ready(),
        version: recovery.version(),
    }))
}

fn recovery_command(
    request: OrganizationSsoRecoveryRequest,
) -> Result<OrganizationSsoRecoveryCommand, ManagementError> {
    let step_up =
        af_admin::UserSecurityStepUpCommand::new(request.current_password, request.totp_code)
            .map_err(|_| ManagementError::InvalidRequest)?;
    OrganizationSsoRecoveryCommand::new(step_up, request.expected_version)
        .map_err(map_sso_recovery_error)
}

fn parse_identity_binding_step_up(
    request: &OrganizationSsoIdentityBindingRequest,
) -> Result<OrganizationSsoIdentityBindingStepUp, ManagementError> {
    match request.method.as_str() {
        "password" => {
            if request.credential.is_some() {
                return Err(ManagementError::InvalidRequest);
            }
            let current_password = request
                .current_password
                .clone()
                .ok_or(ManagementError::InvalidRequest)?;
            let command = af_admin::UserSecurityStepUpCommand::new(
                current_password,
                request.totp_code.clone(),
            )
            .map_err(|_| ManagementError::InvalidRequest)?;
            Ok(OrganizationSsoIdentityBindingStepUp::PasswordAndSecondFactor(command))
        }
        "passkey" => {
            if request.current_password.is_some() || request.totp_code.is_some() {
                return Err(ManagementError::InvalidRequest);
            }
            let credential = request
                .credential
                .clone()
                .ok_or(ManagementError::InvalidRequest)?;
            let command = af_admin::PasskeyAuthenticationCommand::new(credential)
                .map_err(|_| ManagementError::InvalidRequest)?;
            Ok(OrganizationSsoIdentityBindingStepUp::Passkey(command))
        }
        _ => Err(ManagementError::InvalidRequest),
    }
}

async fn list_organization_sso_domains(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoRead,
        &request_id,
        &headers,
        "organization.sso.domain.list",
        "organization",
        organization_id.get().to_string(),
    )
    .await?;
    let service = state
        .sso_domain_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let domains = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoRead,
        "organization.sso.domain.list",
        "organization",
        organization_id.get().to_string(),
        service
            .list(context.clone())
            .await
            .map_err(map_sso_domain_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoRead.code(),
        "organization.sso.domain.list",
        "organization",
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"result_count": domains.len()})),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoDomainListResponse {
        domains: domains
            .into_iter()
            .map(OrganizationSsoDomainResponse::from)
            .collect(),
    }))
}

async fn create_organization_sso_domain(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoDomainCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoManage,
        &request_id,
        &headers,
        "organization.sso.domain.create",
        "sso_domain",
        "new".to_owned(),
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = OrganizationSsoDomainCreateCommand::new(
        request.provider_key,
        request.domain,
        request.ttl_seconds,
    )
    .map_err(map_sso_domain_error)?;
    let service = state
        .sso_domain_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let created = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.domain.create",
        "sso_domain",
        "new".to_owned(),
        service
            .create(context.clone(), command)
            .await
            .map_err(map_sso_domain_error),
    )
    .await?;
    record_sso_domain_success(
        &state,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.domain.create",
        created.domain(),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoDomainCreatedResponse::from(
        created,
    )))
}

async fn verify_organization_sso_domain(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, domain_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoDomainVerifyRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoManage,
        &request_id,
        &headers,
        "organization.sso.domain.verify",
        "sso_domain",
        domain_id.to_string(),
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command =
        OrganizationSsoDomainVerifyCommand::new(request.proof_token, request.expected_version)
            .map_err(map_sso_domain_error)?;
    let service = state
        .sso_domain_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let domain = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.domain.verify",
        "sso_domain",
        domain_id.to_string(),
        service
            .verify(context.clone(), domain_id, command)
            .await
            .map_err(map_sso_domain_error),
    )
    .await?;
    record_sso_domain_success(
        &state,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.domain.verify",
        &domain,
    )
    .await?;
    Ok(no_store_json(OrganizationSsoDomainResponse::from(domain)))
}

async fn list_organization_sso_providers(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoRead,
        &request_id,
        &headers,
        "organization.sso.provider.list",
        "organization",
        organization_id.get().to_string(),
    )
    .await?;
    let service = state
        .sso_provider_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let providers = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoRead,
        "organization.sso.provider.list",
        "organization",
        organization_id.get().to_string(),
        service
            .list(context.clone())
            .await
            .map_err(map_sso_provider_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoRead.code(),
        "organization.sso.provider.list",
        "organization",
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"result_count": providers.len()})),
    )
    .await?;
    Ok(no_store_json(OrganizationSsoProviderListResponse {
        providers: providers
            .into_iter()
            .map(OrganizationSsoProviderResponse::from)
            .collect(),
    }))
}

async fn get_organization_sso_provider(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, provider_key)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoRead,
        &request_id,
        &headers,
        "organization.sso.provider.read",
        "sso_provider",
        provider_key.clone(),
    )
    .await?;
    let service = state
        .sso_provider_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let provider = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoRead,
        "organization.sso.provider.read",
        "sso_provider",
        provider_key.clone(),
        service
            .get(context.clone(), provider_key.clone())
            .await
            .map_err(map_sso_provider_error),
    )
    .await?;
    record_sso_provider_success(
        &state,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoRead,
        "organization.sso.provider.read",
        &provider,
    )
    .await?;
    Ok(no_store_json(OrganizationSsoProviderResponse::from(
        provider,
    )))
}

async fn create_organization_sso_provider(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoProviderRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoManage,
        &request_id,
        &headers,
        "organization.sso.provider.create",
        "sso_provider",
        "new".to_owned(),
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = request.into_command()?;
    let service = state
        .sso_provider_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let provider = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.provider.create",
        "sso_provider",
        "new".to_owned(),
        service
            .create(context.clone(), command)
            .await
            .map_err(map_sso_provider_error),
    )
    .await?;
    record_sso_provider_success(
        &state,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.provider.create",
        &provider,
    )
    .await?;
    Ok(no_store_json(OrganizationSsoProviderResponse::from(
        provider,
    )))
}

async fn update_organization_sso_provider(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, provider_key)): Path<(i64, String)>,
    headers: HeaderMap,
    request: Result<Json<OrganizationSsoProviderRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::SsoManage,
        &request_id,
        &headers,
        "organization.sso.provider.update",
        "sso_provider",
        provider_key.clone(),
    )
    .await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = request.into_command()?;
    let service = state
        .sso_provider_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let provider = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.provider.update",
        "sso_provider",
        provider_key.clone(),
        service
            .update(context.clone(), provider_key, command)
            .await
            .map_err(map_sso_provider_error),
    )
    .await?;
    record_sso_provider_success(
        &state,
        context,
        &request_id,
        &headers,
        OrganizationPermission::SsoManage,
        "organization.sso.provider.update",
        &provider,
    )
    .await?;
    Ok(no_store_json(OrganizationSsoProviderResponse::from(
        provider,
    )))
}

async fn record_sso_provider_success(
    state: &OrganizationHttpState,
    context: af_admin::OrganizationContext,
    request_id: &RequestId,
    headers: &HeaderMap,
    permission: OrganizationPermission,
    operation: &'static str,
    provider: &OrganizationSsoProvider,
) -> Result<(), ManagementError> {
    record_organization_action(
        &state.activity_service,
        context,
        request_id,
        headers,
        permission.code(),
        operation,
        "sso_provider",
        provider.provider_key().to_owned(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "protocol": organization_sso_protocol_code(provider.protocol()),
            "status": organization_sso_provider_status_code(provider.status()),
            "secret_configured": provider.secret_configured(),
            "version": provider.version(),
        })),
    )
    .await
}

async fn record_sso_domain_success(
    state: &OrganizationHttpState,
    context: af_admin::OrganizationContext,
    request_id: &RequestId,
    headers: &HeaderMap,
    permission: OrganizationPermission,
    operation: &'static str,
    domain: &AdminOrganizationSsoDomain,
) -> Result<(), ManagementError> {
    record_organization_action(
        &state.activity_service,
        context,
        request_id,
        headers,
        permission.code(),
        operation,
        "sso_domain",
        domain.id().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "status": organization_sso_domain_status_code(domain.status()),
            "version": domain.version(),
        })),
    )
    .await
}

impl OrganizationSsoProviderRequest {
    fn into_command(self) -> Result<OrganizationSsoProviderCommand, ManagementError> {
        OrganizationSsoProviderCommand::new(
            self.expected_version,
            parse_sso_protocol(&self.protocol)?,
            parse_sso_provider_status(&self.status)?,
            self.display_name,
            self.metadata_projection,
            self.client_secret,
            self.clear_client_secret,
        )
        .map_err(map_sso_provider_error)
    }
}

async fn list_organization_custom_roles(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::RolesRead,
        &request_id,
        &headers,
        "organization.role.list",
        "organization",
        organization_id.get().to_string(),
    )
    .await?;
    let query = parse_custom_role_list_query(raw_query.as_deref())?;
    let service = state
        .custom_role_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::RolesRead,
        "organization.role.list",
        "organization",
        organization_id.get().to_string(),
        service
            .list(context.clone(), query)
            .await
            .map_err(map_custom_role_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::RolesRead.code(),
        "organization.role.list",
        "organization",
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "result_count": page.records().len(),
            "has_next_cursor": page.next_cursor().is_some(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationCustomRoleListResponse {
        roles: page
            .records()
            .iter()
            .map(OrganizationCustomRoleResponse::from)
            .collect(),
        next_cursor: page
            .next_cursor()
            .map(af_domain::OrganizationCustomRoleId::get),
    }))
}

async fn get_organization_custom_role(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, role_key)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let role_key = parse_custom_role_key(&role_key)?;
    let role_identifier = role_key.persistence_key();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::RolesRead,
        &request_id,
        &headers,
        "organization.role.read",
        "custom_role",
        role_identifier.clone(),
    )
    .await?;
    let service = state
        .custom_role_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let role = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::RolesRead,
        "organization.role.read",
        "custom_role",
        role_identifier,
        service
            .get(context.clone(), role_key)
            .await
            .map_err(map_custom_role_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::RolesRead.code(),
        "organization.role.read",
        "custom_role",
        role.role_key().persistence_key(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": role.version()})),
    )
    .await?;
    Ok(no_store_json(OrganizationCustomRoleResponse::from(&role)))
}

async fn create_organization_custom_role(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OrganizationCustomRoleCreateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::RolesManage,
        &request_id,
        &headers,
        "organization.role.create",
        "custom_role",
        "new".to_owned(),
    )
    .await?;
    let permissions = parse_custom_role_permissions(request.permissions)?;
    let service = state
        .custom_role_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let role = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::RolesManage,
        "organization.role.create",
        "custom_role",
        "new".to_owned(),
        service
            .create(
                context.clone(),
                OrganizationCustomRoleCreateCommand::new(
                    request.name,
                    request.description,
                    permissions,
                ),
            )
            .await
            .map_err(map_custom_role_error),
    )
    .await?;
    record_custom_role_success(
        &state,
        context,
        &request_id,
        &headers,
        "organization.role.create",
        &role,
    )
    .await?;
    Ok(no_store_json(OrganizationCustomRoleResponse::from(&role)))
}

async fn update_organization_custom_role(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, role_key)): Path<(i64, String)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationCustomRoleChangeRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let role_key = parse_custom_role_key(&role_key)?;
    let role_identifier = role_key.persistence_key();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::RolesManage,
        &request_id,
        &headers,
        "organization.role.update",
        "custom_role",
        role_identifier.clone(),
    )
    .await?;
    let command = OrganizationCustomRoleChangeCommand::new(
        role_key,
        request.name,
        request.description,
        parse_custom_role_permissions(request.permissions)?,
        request.expected_version,
    )
    .map_err(map_custom_role_error)?;
    let service = state
        .custom_role_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let role = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::RolesManage,
        "organization.role.update",
        "custom_role",
        role_identifier,
        service
            .update(context.clone(), command)
            .await
            .map_err(map_custom_role_error),
    )
    .await?;
    record_custom_role_success(
        &state,
        context,
        &request_id,
        &headers,
        "organization.role.update",
        &role,
    )
    .await?;
    Ok(no_store_json(OrganizationCustomRoleResponse::from(&role)))
}

async fn disable_organization_custom_role(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, role_key)): Path<(i64, String)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationCustomRoleDisableRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let role_key = parse_custom_role_key(&role_key)?;
    let role_identifier = role_key.persistence_key();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::RolesManage,
        &request_id,
        &headers,
        "organization.role.disable",
        "custom_role",
        role_identifier.clone(),
    )
    .await?;
    let command = OrganizationCustomRoleDisableCommand::new(role_key, request.expected_version)
        .map_err(map_custom_role_error)?;
    let service = state
        .custom_role_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let role = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::RolesManage,
        "organization.role.disable",
        "custom_role",
        role_identifier,
        service
            .disable(context.clone(), command)
            .await
            .map_err(map_custom_role_error),
    )
    .await?;
    record_custom_role_success(
        &state,
        context,
        &request_id,
        &headers,
        "organization.role.disable",
        &role,
    )
    .await?;
    Ok(no_store_json(OrganizationCustomRoleResponse::from(&role)))
}

async fn list_organization_service_accounts(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ServiceAccountsRead,
        &request_id,
        &headers,
        "organization.service_account.list",
        "organization",
        organization_id.get().to_string(),
    )
    .await?;
    let query = parse_service_account_list_query(raw_query.as_deref())?;
    let service = state
        .service_account_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsRead,
        "organization.service_account.list",
        "organization",
        organization_id.get().to_string(),
        service
            .list(context.clone(), query)
            .await
            .map_err(map_service_account_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsRead.code(),
        "organization.service_account.list",
        "organization",
        organization_id.get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "result_count": page.records().len(),
            "has_next_cursor": page.next_cursor().is_some(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationServiceAccountListResponse {
        service_accounts: page
            .records()
            .iter()
            .map(OrganizationServiceAccountResponse::from)
            .collect(),
        next_cursor: page
            .next_cursor()
            .map(OrganizationServiceAccountKey::persistence_key),
    }))
}

async fn get_organization_service_account(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, service_account_key)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let service_account_key = parse_service_account_key(&service_account_key)?;
    let identifier = service_account_key.persistence_key();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ServiceAccountsRead,
        &request_id,
        &headers,
        "organization.service_account.read",
        "service_account",
        identifier.clone(),
    )
    .await?;
    let service = state
        .service_account_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let account = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsRead,
        "organization.service_account.read",
        "service_account",
        identifier.clone(),
        service
            .get(context.clone(), service_account_key)
            .await
            .map_err(map_service_account_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsRead.code(),
        "organization.service_account.read",
        "service_account",
        identifier,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": account.version()})),
    )
    .await?;
    Ok(no_store_json(OrganizationServiceAccountResponse::from(
        &account,
    )))
}

async fn create_organization_service_account(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OrganizationServiceAccountCreateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ServiceAccountsManage,
        &request_id,
        &headers,
        "organization.service_account.create",
        "service_account",
        "new".to_owned(),
    )
    .await?;
    let service = state
        .service_account_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let issued = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage,
        "organization.service_account.create",
        "service_account",
        "new".to_owned(),
        service
            .create(
                context.clone(),
                OrganizationServiceAccountCreateCommand::new(request.name),
            )
            .await
            .map_err(map_service_account_error),
    )
    .await?;
    let identifier = issued.account().account().public_key().persistence_key();
    let response = OrganizationServiceAccountIssuedResponse::from_issued(issued);
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage.code(),
        "organization.service_account.create",
        "service_account",
        identifier,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": response.service_account.version})),
    )
    .await?;
    Ok(no_store_json(response))
}

async fn change_organization_service_account_status(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, service_account_key)): Path<(i64, String)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationServiceAccountStatusRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let service_account_key = parse_service_account_key(&service_account_key)?;
    let identifier = service_account_key.persistence_key();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ServiceAccountsManage,
        &request_id,
        &headers,
        "organization.service_account.status",
        "service_account",
        identifier.clone(),
    )
    .await?;
    let target = parse_service_account_status(&request.status)?;
    let command = OrganizationServiceAccountStatusCommand::new(
        service_account_key,
        request.expected_version,
        target,
    )
    .map_err(map_service_account_error)?;
    let service = state
        .service_account_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let account = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage,
        "organization.service_account.status",
        "service_account",
        identifier.clone(),
        service
            .set_status(context.clone(), command)
            .await
            .map_err(map_service_account_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage.code(),
        "organization.service_account.status",
        "service_account",
        identifier,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"status": service_account_status_code(account.account().status()), "version": account.version()})),
    )
    .await?;
    Ok(no_store_json(OrganizationServiceAccountResponse::from(
        &account,
    )))
}

async fn rotate_organization_service_account_key(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, service_account_key)): Path<(i64, String)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationServiceAccountRotateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let service_account_key = parse_service_account_key(&service_account_key)?;
    let identifier = service_account_key.persistence_key();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ServiceAccountsManage,
        &request_id,
        &headers,
        "organization.service_account.rotate_key",
        "service_account",
        identifier.clone(),
    )
    .await?;
    let command =
        OrganizationServiceAccountRotateCommand::new(service_account_key, request.expected_version)
            .map_err(map_service_account_error)?;
    let service = state
        .service_account_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let issued = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage,
        "organization.service_account.rotate_key",
        "service_account",
        identifier.clone(),
        service
            .rotate_key(context.clone(), command)
            .await
            .map_err(map_service_account_error),
    )
    .await?;
    let response = OrganizationServiceAccountIssuedResponse::from_issued(issued);
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage.code(),
        "organization.service_account.rotate_key",
        "service_account",
        identifier,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": response.service_account.version})),
    )
    .await?;
    Ok(no_store_json(response))
}

async fn get_organization_service_account_budget_binding(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, service_account_key)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let service_account_key = parse_service_account_key(&service_account_key)?;
    let identifier = service_account_key.persistence_key();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ServiceAccountsRead,
        &request_id,
        &headers,
        "organization.service_account.budget_binding.read",
        "service_account",
        identifier.clone(),
    )
    .await?;
    let service = state
        .service_account_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let binding = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsRead,
        "organization.service_account.budget_binding.read",
        "service_account",
        identifier.clone(),
        service
            .get_budget_binding(context.clone(), service_account_key)
            .await
            .map_err(map_service_account_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsRead.code(),
        "organization.service_account.budget_binding.read",
        "service_account",
        identifier,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "policy_id": binding.policy_id().get(),
            "policy_version": binding.policy_version(),
            "version": binding.version(),
        })),
    )
    .await?;
    Ok(no_store_json(
        OrganizationServiceAccountBudgetBindingResponse::from_binding(
            service_account_key,
            &binding,
        ),
    ))
}

async fn set_organization_service_account_budget_binding(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, service_account_key)): Path<(i64, String)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationServiceAccountBudgetBindingRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let service_account_key = parse_service_account_key(&service_account_key)?;
    let identifier = service_account_key.persistence_key();
    let policy_id = OrganizationBudgetPolicyId::new(request.policy_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let command = OrganizationServiceAccountBudgetBindingCommand::new(
        service_account_key,
        policy_id,
        request.expected_version,
    )
    .map_err(map_service_account_error)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ServiceAccountsManage,
        &request_id,
        &headers,
        "organization.service_account.budget_binding.set",
        "service_account",
        identifier.clone(),
    )
    .await?;
    let service = state
        .service_account_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let binding = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage,
        "organization.service_account.budget_binding.set",
        "service_account",
        identifier.clone(),
        service
            .set_budget_binding(context.clone(), command)
            .await
            .map_err(map_service_account_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ServiceAccountsManage.code(),
        "organization.service_account.budget_binding.set",
        "service_account",
        identifier,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "policy_id": binding.policy_id().get(),
            "policy_version": binding.policy_version(),
            "version": binding.version(),
        })),
    )
    .await?;
    Ok(no_store_json(
        OrganizationServiceAccountBudgetBindingResponse::from_binding(
            service_account_key,
            &binding,
        ),
    ))
}

async fn record_custom_role_success(
    state: &OrganizationHttpState,
    context: af_admin::OrganizationContext,
    request_id: &RequestId,
    headers: &HeaderMap,
    operation: &'static str,
    role: &af_admin::OrganizationCustomRoleRecord,
) -> Result<(), ManagementError> {
    record_organization_action(
        &state.activity_service,
        context,
        request_id,
        headers,
        OrganizationPermission::RolesManage.code(),
        operation,
        "custom_role",
        role.role_key().persistence_key(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "status": custom_role_status_code(role.status()),
            "permission_count": role.permissions().len(),
            "version": role.version(),
        })),
    )
    .await
}

/// 按成员版本绑定或清除活动 Member 的自定义角色。
async fn set_organization_member_custom_role(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, membership_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationMemberCustomRoleRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let membership_id = OrganizationMembershipId::new(membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let role_key = request
        .role_key
        .as_deref()
        .map(parse_custom_role_key)
        .transpose()?;
    let operation = if role_key.is_some() {
        "organization.member.custom_role.bind"
    } else {
        "organization.member.custom_role.clear"
    };
    let target_identifier = membership_id.get().to_string();
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::RolesAssign,
        &request_id,
        &headers,
        operation,
        "membership",
        target_identifier.clone(),
    )
    .await?;
    let service = state
        .custom_role_binding_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let result = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::RolesAssign,
        operation,
        "membership",
        target_identifier.clone(),
        service
            .set(
                context.clone(),
                OrganizationCustomRoleBindingCommand::new(
                    membership_id,
                    role_key,
                    request.expected_version,
                )
                .map_err(map_custom_role_binding_error)?,
            )
            .await
            .map_err(map_custom_role_binding_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::RolesAssign.code(),
        operation,
        "membership",
        target_identifier,
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "custom_role_key": result.role_key().map(|key| key.persistence_key()),
            "version": result.membership_version(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationMemberCustomRoleResponse {
        membership_id: result.membership_id().get(),
        custom_role_key: result.role_key().map(|key| key.persistence_key()),
        version: result.membership_version(),
    }))
}

async fn list_organization_members(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::OverviewRead,
    )
    .await?;
    let query = parse_membership_list_query(raw_query.as_deref())?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let include_custom_role_key = context.allows(OrganizationPermission::RolesRead);
    let page = service
        .list_members(context, query)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationMembershipListResponse {
        members: page
            .records()
            .iter()
            .map(|record| {
                OrganizationMembershipResponse::from_record(record, include_custom_role_key)
            })
            .collect(),
        next_cursor: page.next_cursor().map(OrganizationMembershipId::get),
    }))
}

async fn change_organization_member(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, membership_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationMembershipChangeRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let membership_id = OrganizationMembershipId::new(membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::MembersManage,
        &request_id,
        &headers,
        "organization.member.change",
        "membership",
        membership_id.get().to_string(),
    )
    .await?;
    let command = OrganizationMembershipChangeCommand::new(
        membership_id,
        parse_organization_role(&request.role)?,
        parse_organization_membership_status(&request.status)?,
        request
            .team_id
            .map(OrganizationTeamId::new)
            .transpose()
            .map_err(|_| ManagementError::InvalidRequest)?,
        request.expected_version,
    )
    .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let member = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::MembersManage,
        "organization.member.change",
        "membership",
        membership_id.get().to_string(),
        service
            .change_member(context.clone(), command)
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::MembersManage.code(),
        "organization.member.change",
        "membership",
        member.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "role": organization_role_code(member.role()),
            "status": organization_membership_status_code(member.status()),
            "team": member.team_id().map(OrganizationTeamId::get),
            "version": member.version(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationMembershipResponse::from(&member)))
}

async fn list_organization_department_memberships(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, membership_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let membership_id = OrganizationMembershipId::new(membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsRead,
    )
    .await?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list_department_memberships(context, membership_id)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(
        OrganizationDepartmentMembershipListResponse {
            department_memberships: page
                .records()
                .iter()
                .map(OrganizationDepartmentMembershipResponse::from)
                .collect(),
        },
    ))
}

async fn add_organization_department_membership(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, membership_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationDepartmentMembershipAddRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let membership_id = OrganizationMembershipId::new(membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let department_id = OrganizationDepartmentId::new(request.department_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsManage,
        &request_id,
        &headers,
        "organization.member.department.add",
        "membership",
        membership_id.get().to_string(),
    )
    .await?;
    let command = OrganizationDepartmentMembershipAddCommand::new(
        department_id,
        request.is_primary,
        request.expected_membership_version,
    )
    .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let record = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage,
        "organization.member.department.add",
        "membership",
        membership_id.get().to_string(),
        service
            .add_department_membership(context.clone(), membership_id, command)
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage.code(),
        "organization.member.department.add",
        "department_membership",
        format!("{}:{}", membership_id.get(), department_id.get()),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "department_id": record.department_id().get(),
            "is_primary": record.is_primary(),
            "version": record.version(),
        })),
    )
    .await?;
    Ok(no_store_json(
        OrganizationDepartmentMembershipResponse::from(&record),
    ))
}

async fn set_primary_organization_department_membership(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, membership_id, department_id)): Path<(i64, i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationDepartmentMembershipPrimaryRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let membership_id = OrganizationMembershipId::new(membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let department_id = OrganizationDepartmentId::new(department_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsManage,
        &request_id,
        &headers,
        "organization.member.department.primary",
        "membership",
        membership_id.get().to_string(),
    )
    .await?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let record = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage,
        "organization.member.department.primary",
        "membership",
        membership_id.get().to_string(),
        service
            .set_primary_department_membership(
                context.clone(),
                membership_id,
                department_id,
                request.expected_membership_version,
            )
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage.code(),
        "organization.member.department.primary",
        "department_membership",
        format!("{}:{}", membership_id.get(), department_id.get()),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "department_id": record.department_id().get(),
            "is_primary": record.is_primary(),
            "version": record.version(),
        })),
    )
    .await?;
    Ok(no_store_json(
        OrganizationDepartmentMembershipResponse::from(&record),
    ))
}

async fn remove_organization_department_membership(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, membership_id, department_id)): Path<(i64, i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationDepartmentMembershipRemoveRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let membership_id = OrganizationMembershipId::new(membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let department_id = OrganizationDepartmentId::new(department_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsManage,
        &request_id,
        &headers,
        "organization.member.department.remove",
        "membership",
        membership_id.get().to_string(),
    )
    .await?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let record = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage,
        "organization.member.department.remove",
        "membership",
        membership_id.get().to_string(),
        service
            .remove_department_membership(
                context.clone(),
                membership_id,
                department_id,
                request.expected_version,
            )
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage.code(),
        "organization.member.department.remove",
        "department_membership",
        format!("{}:{}", membership_id.get(), department_id.get()),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "department_id": record.department_id().get(),
            "status": organization_department_membership_status_code(record.status()),
            "version": record.version(),
        })),
    )
    .await?;
    Ok(no_store_json(
        OrganizationDepartmentMembershipResponse::from(&record),
    ))
}

async fn transfer_organization_ownership(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OrganizationOwnershipTransferRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let target_membership_id = OrganizationMembershipId::new(request.target_membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::OwnershipTransfer,
        &request_id,
        &headers,
        "organization.ownership.transfer",
        "membership",
        target_membership_id.get().to_string(),
    )
    .await?;
    let command = OrganizationOwnershipTransferCommand::new(
        target_membership_id,
        request.current_owner_version,
        request.target_expected_version,
    )
    .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let result = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::OwnershipTransfer,
        "organization.ownership.transfer",
        "membership",
        target_membership_id.get().to_string(),
        service
            .transfer_ownership(context.clone(), command)
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::OwnershipTransfer.code(),
        "organization.ownership.transfer",
        "membership",
        result.current_owner().id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "previous_owner": result.previous_owner().id().get(),
            "current_owner": result.current_owner().id().get(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationOwnershipTransferResponse {
        previous_owner: OrganizationMembershipResponse::from(result.previous_owner()),
        current_owner: OrganizationMembershipResponse::from(result.current_owner()),
    }))
}

async fn list_organization_teams(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::OverviewRead,
    )
    .await?;
    let query = parse_team_list_query(raw_query.as_deref())?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list_teams(context, query)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationTeamListResponse {
        teams: page
            .records()
            .iter()
            .map(OrganizationTeamResponse::from)
            .collect(),
        next_cursor: page.next_cursor().map(OrganizationTeamId::get),
    }))
}

async fn create_organization_team(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OrganizationTeamCreateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::TeamsManage,
        &request_id,
        &headers,
        "organization.team.create",
        "team",
        "new".to_owned(),
    )
    .await?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let team = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::TeamsManage,
        "organization.team.create",
        "team",
        "new".to_owned(),
        service
            .create_team(
                context.clone(),
                OrganizationTeamCreateCommand::new(request.name, request.description),
            )
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::TeamsManage.code(),
        "organization.team.create",
        "team",
        team.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": team.version()})),
    )
    .await?;
    Ok(no_store_json(OrganizationTeamResponse::from(&team)))
}

async fn change_organization_team(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, team_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationTeamChangeRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let team_id = OrganizationTeamId::new(team_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::TeamsManage,
        &request_id,
        &headers,
        "organization.team.change",
        "team",
        team_id.get().to_string(),
    )
    .await?;
    let command = OrganizationTeamChangeCommand::new(
        team_id,
        request.name,
        request.description,
        request.expected_version,
    )
    .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let team = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::TeamsManage,
        "organization.team.change",
        "team",
        team_id.get().to_string(),
        service
            .change_team(context.clone(), command)
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::TeamsManage.code(),
        "organization.team.change",
        "team",
        team.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": team.version()})),
    )
    .await?;
    Ok(no_store_json(OrganizationTeamResponse::from(&team)))
}

async fn disable_organization_team(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, team_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationTeamDisableRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let team_id = OrganizationTeamId::new(team_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::TeamsManage,
        &request_id,
        &headers,
        "organization.team.disable",
        "team",
        team_id.get().to_string(),
    )
    .await?;
    let command = OrganizationTeamDisableCommand::new(team_id, request.expected_version)
        .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let team = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::TeamsManage,
        "organization.team.disable",
        "team",
        team_id.get().to_string(),
        service
            .disable_team(context.clone(), command)
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::TeamsManage.code(),
        "organization.team.disable",
        "team",
        team.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": team.version()})),
    )
    .await?;
    Ok(no_store_json(OrganizationTeamResponse::from(&team)))
}

async fn list_organization_departments(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsRead,
    )
    .await?;
    let query = parse_department_list_query(raw_query.as_deref())?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list_departments(context, query)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationDepartmentListResponse {
        departments: page
            .records()
            .iter()
            .map(OrganizationDepartmentResponse::from)
            .collect(),
        next_cursor: page.next_cursor().map(format_department_cursor),
    }))
}

async fn get_organization_department(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, department_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let department_id = OrganizationDepartmentId::new(department_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsRead,
    )
    .await?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let department = service
        .get_department(context, department_id)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationDepartmentResponse::from(
        &department,
    )))
}

async fn create_organization_department(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OrganizationDepartmentCreateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let parent_id = request
        .parent_department_id
        .map(OrganizationDepartmentId::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsManage,
        &request_id,
        &headers,
        "organization.department.create",
        "department",
        "new".to_owned(),
    )
    .await?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let department = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage,
        "organization.department.create",
        "department",
        "new".to_owned(),
        service
            .create_department(
                context.clone(),
                OrganizationDepartmentCreateCommand::new(
                    request.name,
                    parent_id,
                    request.sort_order,
                ),
            )
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage.code(),
        "organization.department.create",
        "department",
        department.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": department.version(), "parent_department_id": department.parent_id().map(OrganizationDepartmentId::get)})),
    )
    .await?;
    Ok(no_store_json(OrganizationDepartmentResponse::from(
        &department,
    )))
}

async fn move_organization_department(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, department_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationDepartmentMoveRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let department_id = OrganizationDepartmentId::new(department_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let parent_id = request
        .parent_department_id
        .map(OrganizationDepartmentId::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsManage,
        &request_id,
        &headers,
        "organization.department.move",
        "department",
        department_id.get().to_string(),
    )
    .await?;
    let command =
        OrganizationDepartmentMoveCommand::new(department_id, parent_id, request.expected_version)
            .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let department = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage,
        "organization.department.move",
        "department",
        department_id.get().to_string(),
        service
            .move_department(context.clone(), command)
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage.code(),
        "organization.department.move",
        "department",
        department.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": department.version(), "parent_department_id": department.parent_id().map(OrganizationDepartmentId::get)})),
    )
    .await?;
    Ok(no_store_json(OrganizationDepartmentResponse::from(
        &department,
    )))
}

async fn disable_organization_department(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, department_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<OrganizationDepartmentDisableRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let department_id = OrganizationDepartmentId::new(department_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::DepartmentsManage,
        &request_id,
        &headers,
        "organization.department.disable",
        "department",
        department_id.get().to_string(),
    )
    .await?;
    let command =
        OrganizationDepartmentDisableCommand::new(department_id, request.expected_version)
            .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let department = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage,
        "organization.department.disable",
        "department",
        department_id.get().to_string(),
        service
            .disable_department(context.clone(), command)
            .await
            .map_err(map_management_lifecycle_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::DepartmentsManage.code(),
        "organization.department.disable",
        "department",
        department.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": department.version(), "status": "disabled"})),
    )
    .await?;
    Ok(no_store_json(OrganizationDepartmentResponse::from(
        &department,
    )))
}

async fn list_organizations(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let organizations = state
        .authorization_service
        .list(authentication.principal())
        .await
        .map_err(map_authorization_error)?
        .into_iter()
        .map(OrganizationSummaryResponse::from)
        .collect();
    Ok(no_store_json(OrganizationListResponse { organizations }))
}

async fn get_organization(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
) -> Result<Response, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = state
        .authorization_service
        .context(
            authentication.principal(),
            organization_id,
            OrganizationPermission::OverviewRead,
        )
        .await
        .map_err(map_authorization_error)?;
    Ok(no_store_json(OrganizationOverviewResponse {
        id: context.organization_id().get(),
        name: context.name().to_owned(),
        slug: context.slug().to_owned(),
        role: organization_role_code(context.role()),
        membership_id: context.membership_id().get(),
        permissions: context
            .permissions()
            .iter()
            .map(|permission| permission.code())
            .collect(),
        entitlement: OrganizationEntitlementResponse::from(context.entitlement()),
    }))
}

async fn list_organization_invitations(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
) -> Result<Response, ManagementError> {
    let organization_id = authorize_invitation_management(&state, &authentication, organization_id)
        .await?
        .organization_id();
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let invitations = invitation_service
        .list(organization_id)
        .await
        .map_err(map_invitation_management_error)?
        .iter()
        .map(OrganizationInvitationResponse::from)
        .collect();
    Ok(no_store_json(OrganizationInvitationListResponse {
        invitations,
    }))
}

async fn create_organization_invitation(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OrganizationInvitationCreateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::MembersManage,
        &request_id,
        &headers,
        "organization.invitation.create",
        "invitation",
        "new".to_owned(),
    )
    .await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let command = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::MembersManage,
        "organization.invitation.create",
        "invitation",
        "new".to_owned(),
        request.into_command(),
    )
    .await?;
    let issued = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::MembersManage,
        "organization.invitation.create",
        "invitation",
        "new".to_owned(),
        invitation_service
            .create(authentication.principal(), organization_id, command)
            .await
            .map_err(map_invitation_management_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::MembersManage.code(),
        "organization.invitation.create",
        "invitation",
        issued.record().invitation_key().persistence_key(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "role": organization_role_code(issued.record().role()),
            "team": issued.record().team_id().map(OrganizationTeamId::get),
        })),
    )
    .await?;
    Ok(no_store_json(issued_invitation_response(issued)?))
}

async fn preview_organization_invitations(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    Json(request): Json<OrganizationInvitationPreviewRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = authorize_invitation_management(&state, &authentication, organization_id)
        .await?
        .organization_id();
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let command = OrganizationInvitationPreviewCommand::new(
        request
            .items
            .into_iter()
            .map(OrganizationInvitationCreateRequest::into_command)
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(map_invitation_management_error)?;
    let result = invitation_service
        .preview_batch(organization_id, command)
        .await
        .map_err(map_invitation_management_error)?;
    Ok(no_store_json(OrganizationInvitationPreviewResponse::from(
        result,
    )))
}

async fn create_organization_invitation_batch(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    Json(request): Json<OrganizationInvitationBatchRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let batch_key = OrganizationInvitationKey::from_persistence_key(&request.batch_key)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::MembersManage,
        &request_id,
        &headers,
        "organization.invitation.batch_create",
        "invitation_batch",
        batch_key.persistence_key(),
    )
    .await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let command = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::MembersManage,
        "organization.invitation.batch_create",
        "invitation_batch",
        batch_key.persistence_key(),
        OrganizationInvitationBatchCommand::new(
            batch_key,
            request
                .items
                .into_iter()
                .map(OrganizationInvitationCreateRequest::into_command)
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(map_invitation_management_error),
    )
    .await?;
    let result = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::MembersManage,
        "organization.invitation.batch_create",
        "invitation_batch",
        batch_key.persistence_key(),
        invitation_service
            .create_batch(authentication.principal(), organization_id, command)
            .await
            .map_err(map_invitation_management_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::MembersManage.code(),
        "organization.invitation.batch_create",
        "invitation_batch",
        batch_key.persistence_key(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "count": result.invitations().len(),
            "replayed": result.replayed(),
        })),
    )
    .await?;
    Ok(no_store_json(OrganizationInvitationBatchResponse::from(
        result,
    )))
}

async fn revoke_organization_invitation(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, invitation_key)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let invitation_key = parse_invitation_key(&invitation_key)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::MembersManage,
        &request_id,
        &headers,
        "organization.invitation.revoke",
        "invitation",
        invitation_key.persistence_key(),
    )
    .await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let invitation = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::MembersManage,
        "organization.invitation.revoke",
        "invitation",
        invitation_key.persistence_key(),
        invitation_service
            .revoke(organization_id, invitation_key)
            .await
            .map_err(map_invitation_management_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::MembersManage.code(),
        "organization.invitation.revoke",
        "invitation",
        invitation.invitation_key().persistence_key(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"status": organization_invitation_status_code(invitation.status())})),
    )
    .await?;
    Ok(no_store_json(OrganizationInvitationResponse::from(
        &invitation,
    )))
}

async fn resend_organization_invitation(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, invitation_key)): Path<(i64, String)>,
    headers: HeaderMap,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let invitation_key = parse_invitation_key(&invitation_key)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::MembersManage,
        &request_id,
        &headers,
        "organization.invitation.resend",
        "invitation",
        invitation_key.persistence_key(),
    )
    .await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let issued = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::MembersManage,
        "organization.invitation.resend",
        "invitation",
        invitation_key.persistence_key(),
        invitation_service
            .resend(authentication.principal(), organization_id, invitation_key)
            .await
            .map_err(map_invitation_management_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::MembersManage.code(),
        "organization.invitation.resend",
        "invitation",
        issued.record().invitation_key().persistence_key(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": issued.record().version()})),
    )
    .await?;
    Ok(no_store_json(issued_invitation_response(issued)?))
}

async fn accept_organization_invitation(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    Json(request): Json<OrganizationInvitationAcceptRequest>,
) -> Result<Response, ManagementError> {
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let accepted = invitation_service
        .accept(authentication.principal(), request.token)
        .await
        .map_err(map_invitation_accept_error)?;
    let context = state
        .authorization_service
        .context(
            authentication.principal(),
            accepted.organization_id,
            OrganizationPermission::OverviewRead,
        )
        .await
        .map_err(map_authorization_error)?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        "organization.invitations.accept",
        "organization.invitation.accept",
        "membership",
        accepted.membership.id().get().to_string(),
        OrganizationAuditOutcome::Succeeded,
        Some(json!({
            "role": organization_role_code(accepted.membership.role()),
            "team": accepted.membership.team_id().map(OrganizationTeamId::get),
        })),
    )
    .await?;
    Ok(no_store_json(
        OrganizationInvitationAcceptResponse::from_membership(
            accepted.organization_id,
            &accepted.membership,
        ),
    ))
}

impl From<OrganizationEntitlementSummary> for OrganizationEntitlementResponse {
    fn from(summary: OrganizationEntitlementSummary) -> Self {
        Self {
            status: summary.status().code(),
            sso_enabled: summary.sso_enabled(),
            team_management_enabled: summary.team_management_enabled(),
            custom_roles_enabled: summary.custom_roles_enabled(),
            seat_limit: summary.seat_limit(),
            active_seats: summary.active_seats(),
            remaining_seats: summary.seat_limit().saturating_sub(summary.active_seats()),
            team_limit: summary.team_limit(),
            active_teams: summary.active_teams(),
            remaining_teams: summary.team_limit().saturating_sub(summary.active_teams()),
            organization_key_limit: summary.organization_key_limit(),
            audit_retention_days: summary.audit_retention_days(),
            valid_until: summary.valid_until(),
            grace_until: summary.grace_until(),
        }
    }
}

impl From<OrganizationAccessSummary> for OrganizationSummaryResponse {
    fn from(summary: OrganizationAccessSummary) -> Self {
        Self {
            id: summary.organization_id().get(),
            name: summary.name().to_owned(),
            slug: summary.slug().to_owned(),
            role: organization_role_code(summary.role()),
        }
    }
}

fn organization_role_code(role: af_domain::OrganizationRole) -> &'static str {
    match role {
        af_domain::OrganizationRole::Owner => "owner",
        af_domain::OrganizationRole::Admin => "admin",
        af_domain::OrganizationRole::Billing => "billing",
        af_domain::OrganizationRole::Member => "member",
        af_domain::OrganizationRole::Viewer => "viewer",
    }
}

fn map_authorization_error(error: OrganizationAuthorizationError) -> ManagementError {
    match error {
        OrganizationAuthorizationError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
        OrganizationAuthorizationError::Internal => ManagementError::Internal,
    }
}

impl OrganizationInvitationCreateRequest {
    fn into_command(self) -> Result<OrganizationInvitationCommand, ManagementError> {
        let role = parse_organization_role(&self.role)?;
        let team_id = self
            .team_id
            .map(OrganizationTeamId::new)
            .transpose()
            .map_err(|_| ManagementError::InvalidRequest)?;
        let expires_at = self.expires_at.unwrap_or_else(|| {
            current_unix_timestamp().saturating_add(DEFAULT_ORGANIZATION_INVITATION_TTL_SECONDS)
        });
        OrganizationInvitationCommand::new(self.email, role, team_id, expires_at)
            .map_err(map_invitation_management_error)
    }
}

impl From<&OrganizationInvitationRecord> for OrganizationInvitationResponse {
    fn from(record: &OrganizationInvitationRecord) -> Self {
        Self {
            invitation_key: record.invitation_key().persistence_key(),
            organization_id: record.organization_id().get(),
            role: organization_role_code(record.role()),
            team_id: record.team_id().map(OrganizationTeamId::get),
            status: organization_invitation_status_code(record.status()),
            invited_by_user_id: record.invited_by_user_id().get(),
            accepted_by_user_id: record.accepted_by_user_id().map(af_domain::UserId::get),
            batch_key: record
                .batch_key()
                .map(OrganizationInvitationKey::persistence_key),
            version: record.version(),
            created_at: record.created_at(),
            expires_at: record.expires_at(),
            finalized_at: record.finalized_at(),
        }
    }
}

fn issued_invitation_response(
    issued: IssuedOrganizationInvitation,
) -> Result<OrganizationInvitationIssuedResponse, ManagementError> {
    let token = issued.token().ok_or(ManagementError::Internal)?.to_owned();
    Ok(OrganizationInvitationIssuedResponse {
        invitation: OrganizationInvitationResponse::from(issued.record()),
        token,
    })
}

impl From<OrganizationInvitationBatchResult> for OrganizationInvitationBatchResponse {
    fn from(result: OrganizationInvitationBatchResult) -> Self {
        Self {
            replayed: result.replayed(),
            invitations: result
                .invitations()
                .iter()
                .map(|issued| OrganizationInvitationResponse::from(issued.record()))
                .collect(),
        }
    }
}

impl From<OrganizationInvitationPreviewResult> for OrganizationInvitationPreviewResponse {
    fn from(result: OrganizationInvitationPreviewResult) -> Self {
        Self {
            ready: result.ready(),
            available_seats: result.available_seats(),
            items: result
                .items()
                .iter()
                .map(|item| OrganizationInvitationPreviewItemResponse {
                    index: item.index(),
                    ready: item.ready(),
                    issues: item.issues().iter().map(|issue| issue.code()).collect(),
                })
                .collect(),
        }
    }
}

impl OrganizationInvitationAcceptResponse {
    fn from_membership(
        organization_id: OrganizationId,
        membership: &OrganizationMembershipRecord,
    ) -> Self {
        Self {
            organization_id: organization_id.get(),
            membership_id: membership.id().get(),
            user_id: membership.user_id().get(),
            role: organization_role_code(membership.role()),
            team_id: membership.team_id().map(OrganizationTeamId::get),
            status: organization_membership_status_code(membership.status()),
            version: membership.version(),
        }
    }
}

impl OrganizationMembershipResponse {
    fn from_record(record: &OrganizationMembershipRecord, include_custom_role_key: bool) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            user_id: record.user_id().get(),
            role: organization_role_code(record.role()),
            team_id: record.team_id().map(OrganizationTeamId::get),
            custom_role_key: include_custom_role_key
                .then(|| record.custom_role_key().map(|key| key.persistence_key()))
                .flatten(),
            status: organization_membership_status_code(record.status()),
            joined_at: record.joined_at(),
            suspended_at: record.suspended_at(),
            removed_at: record.removed_at(),
            invited_by_user_id: record.invited_by_user_id().map(af_domain::UserId::get),
            version: record.version(),
        }
    }
}

impl From<&OrganizationMembershipRecord> for OrganizationMembershipResponse {
    fn from(record: &OrganizationMembershipRecord) -> Self {
        Self::from_record(record, false)
    }
}

impl From<&OrganizationTeamRecord> for OrganizationTeamResponse {
    fn from(record: &OrganizationTeamRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            name: record.name().to_owned(),
            description: record.description().map(str::to_owned),
            status: organization_team_status_code(record.status()),
            version: record.version(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            deleted_at: record.deleted_at(),
        }
    }
}

impl From<&OrganizationDepartmentRecord> for OrganizationDepartmentResponse {
    fn from(record: &OrganizationDepartmentRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            department_key: record.department_key().persistence_key(),
            parent_department_id: record.parent_id().map(OrganizationDepartmentId::get),
            name: record.name().to_owned(),
            sort_order: record.sort_order(),
            depth: record.depth(),
            status: organization_department_status_code(record.status()),
            version: record.version(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            disabled_at: record.disabled_at(),
        }
    }
}

impl From<&OrganizationDepartmentMembershipRecord> for OrganizationDepartmentMembershipResponse {
    fn from(record: &OrganizationDepartmentMembershipRecord) -> Self {
        Self {
            id: record.id(),
            organization_id: record.organization_id().get(),
            membership_id: record.membership_id().get(),
            department_id: record.department_id().get(),
            is_primary: record.is_primary(),
            status: organization_department_membership_status_code(record.status()),
            version: record.version(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            removed_at: record.removed_at(),
        }
    }
}

impl From<&af_admin::OrganizationCustomRoleRecord> for OrganizationCustomRoleResponse {
    fn from(record: &af_admin::OrganizationCustomRoleRecord) -> Self {
        Self {
            role_key: record.role_key().persistence_key(),
            name: record.name().to_owned(),
            description: record.description().map(str::to_owned),
            status: custom_role_status_code(record.status()),
            permissions: record
                .permissions()
                .iter()
                .map(|permission| permission.code())
                .collect(),
            version: record.version(),
            created_by_user_id: record.created_by_user_id().get(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }
}

impl From<OrganizationSsoProvider> for OrganizationSsoProviderResponse {
    fn from(provider: OrganizationSsoProvider) -> Self {
        Self {
            id: provider.id(),
            organization_id: provider.organization_id().get(),
            provider_key: provider.provider_key().to_owned(),
            protocol: organization_sso_protocol_code(provider.protocol()),
            status: organization_sso_provider_status_code(provider.status()),
            display_name: provider.display_name().to_owned(),
            metadata_projection: provider.metadata_projection().cloned(),
            secret_configured: provider.secret_configured(),
            version: provider.version(),
            created_at: provider.created_at(),
            updated_at: provider.updated_at(),
        }
    }
}

impl From<AdminOrganizationSsoDomain> for OrganizationSsoDomainResponse {
    fn from(domain: AdminOrganizationSsoDomain) -> Self {
        Self {
            id: domain.id(),
            organization_id: domain.organization_id().get(),
            provider_key: domain.provider_key().to_owned(),
            domain: domain.domain().to_owned(),
            status: organization_sso_domain_status_code(domain.status()),
            proof_expires_at: domain.proof_expires_at(),
            verified_at: domain.verified_at(),
            recheck_at: domain.recheck_at(),
            revoked_at: domain.revoked_at(),
            version: domain.version(),
        }
    }
}

impl From<OrganizationSsoDomainCreated> for OrganizationSsoDomainCreatedResponse {
    fn from(created: OrganizationSsoDomainCreated) -> Self {
        Self {
            domain: OrganizationSsoDomainResponse::from(created.domain().clone()),
            txt_record_name: created.txt_record_name().to_owned(),
            txt_record_value: created.txt_record_value().to_owned(),
        }
    }
}

fn organization_sso_protocol_code(protocol: OrganizationSsoProtocol) -> &'static str {
    match protocol {
        OrganizationSsoProtocol::Oidc => "oidc",
        OrganizationSsoProtocol::Saml => "saml",
    }
}

fn organization_sso_recovery_policy_code(policy: OrganizationSsoEnforcementPolicy) -> &'static str {
    match policy {
        OrganizationSsoEnforcementPolicy::Optional => "optional",
        OrganizationSsoEnforcementPolicy::Required => "required",
    }
}

fn parse_sso_enforcement_policy(
    value: &str,
) -> Result<OrganizationSsoEnforcementPolicy, ManagementError> {
    match value {
        "optional" => Ok(OrganizationSsoEnforcementPolicy::Optional),
        "required" => Ok(OrganizationSsoEnforcementPolicy::Required),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn organization_sso_provider_status_code(status: OrganizationSsoProviderStatus) -> &'static str {
    match status {
        OrganizationSsoProviderStatus::Draft => "draft",
        OrganizationSsoProviderStatus::Active => "active",
        OrganizationSsoProviderStatus::Disabled => "disabled",
    }
}

fn organization_sso_domain_status_code(
    status: af_domain::OrganizationSsoDomainStatus,
) -> &'static str {
    match status {
        af_domain::OrganizationSsoDomainStatus::Pending => "pending",
        af_domain::OrganizationSsoDomainStatus::Verified => "verified",
        af_domain::OrganizationSsoDomainStatus::Revoked => "revoked",
    }
}

fn parse_sso_protocol(value: &str) -> Result<OrganizationSsoProtocol, ManagementError> {
    match value {
        "oidc" => Ok(OrganizationSsoProtocol::Oidc),
        "saml" => Ok(OrganizationSsoProtocol::Saml),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_sso_provider_status(
    value: &str,
) -> Result<OrganizationSsoProviderStatus, ManagementError> {
    match value {
        "draft" => Ok(OrganizationSsoProviderStatus::Draft),
        "active" => Ok(OrganizationSsoProviderStatus::Active),
        "disabled" => Ok(OrganizationSsoProviderStatus::Disabled),
        _ => Err(ManagementError::InvalidRequest),
    }
}

async fn authorize_invitation_management(
    state: &OrganizationHttpState,
    authentication: &SessionAuthentication,
    organization_id: i64,
) -> Result<af_admin::OrganizationContext, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    state
        .authorization_service
        .context(
            authentication.principal(),
            organization_id,
            OrganizationPermission::MembersManage,
        )
        .await
        .map_err(map_authorization_error)
}

async fn authorize_lifecycle_context(
    state: &OrganizationHttpState,
    authentication: &SessionAuthentication,
    organization_id: OrganizationId,
    permission: OrganizationPermission,
) -> Result<af_admin::OrganizationContext, ManagementError> {
    state
        .authorization_service
        .context(authentication.principal(), organization_id, permission)
        .await
        .map_err(map_authorization_error)
}

fn parse_organization_id(value: i64) -> Result<OrganizationId, ManagementError> {
    OrganizationId::new(value).map_err(|_| ManagementError::InvalidRequest)
}

fn parse_membership_list_query(
    raw_query: Option<&str>,
) -> Result<OrganizationMembershipListQuery, ManagementError> {
    let (after, limit) = parse_directory_query(raw_query)?;
    let after = after
        .map(OrganizationMembershipId::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    OrganizationMembershipListQuery::new(after, limit).map_err(map_management_lifecycle_error)
}

fn parse_team_list_query(
    raw_query: Option<&str>,
) -> Result<OrganizationTeamListQuery, ManagementError> {
    let (after, limit) = parse_directory_query(raw_query)?;
    let after = after
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    OrganizationTeamListQuery::new(after, limit).map_err(map_management_lifecycle_error)
}

fn parse_department_list_query(
    raw_query: Option<&str>,
) -> Result<OrganizationDepartmentListQuery, ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok(OrganizationDepartmentListQuery::default());
    };
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if after.is_none() => {
                let mut parts = value.split(':');
                let sort_order = parts
                    .next()
                    .ok_or(ManagementError::InvalidRequest)
                    .and_then(parse_signed_i64)?;
                let id = parts
                    .next()
                    .ok_or(ManagementError::InvalidRequest)
                    .and_then(parse_positive_i64)?;
                if parts.next().is_some() {
                    return Err(ManagementError::InvalidRequest);
                }
                after = Some(OrganizationDepartmentCursor::new(
                    sort_order,
                    OrganizationDepartmentId::new(id)
                        .map_err(|_| ManagementError::InvalidRequest)?,
                ));
            }
            "limit" if limit.is_none() => {
                limit = Some(
                    parse_positive_i64(&value)?
                        .try_into()
                        .map_err(|_| ManagementError::InvalidRequest)?,
                );
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    OrganizationDepartmentListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ORGANIZATION_DIRECTORY_PAGE_SIZE),
    )
    .map_err(map_management_lifecycle_error)
}

fn parse_signed_i64(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty()
        || (value.starts_with('-') && value.len() == 1)
        || !value
            .bytes()
            .enumerate()
            .all(|(index, byte)| byte.is_ascii_digit() || (index == 0 && byte == b'-'))
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<i64>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_custom_role_list_query(
    raw_query: Option<&str>,
) -> Result<OrganizationCustomRoleListQuery, ManagementError> {
    let (after, limit) = parse_directory_query(raw_query)?;
    let after = after
        .map(af_domain::OrganizationCustomRoleId::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    OrganizationCustomRoleListQuery::new(after, limit).map_err(map_custom_role_error)
}

fn parse_directory_query(raw_query: Option<&str>) -> Result<(Option<i64>, usize), ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok((None, af_admin::DEFAULT_ORGANIZATION_DIRECTORY_PAGE_SIZE));
    };
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if after.is_none() => after = Some(parse_positive_i64(&value)?),
            "limit" if limit.is_none() => {
                limit = Some(
                    parse_positive_i64(&value)?
                        .try_into()
                        .map_err(|_| ManagementError::InvalidRequest)?,
                )
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok((
        after,
        limit.unwrap_or(af_admin::DEFAULT_ORGANIZATION_DIRECTORY_PAGE_SIZE),
    ))
}

fn parse_positive_i64(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    let parsed = value
        .parse::<i64>()
        .map_err(|_| ManagementError::InvalidRequest)?;
    if parsed <= 0 {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(parsed)
}

fn parse_invitation_key(value: &str) -> Result<OrganizationInvitationKey, ManagementError> {
    OrganizationInvitationKey::from_persistence_key(value)
        .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_custom_role_key(value: &str) -> Result<OrganizationCustomRoleKey, ManagementError> {
    OrganizationCustomRoleKey::from_persistence_key(value)
        .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_custom_role_permissions(
    values: Vec<String>,
) -> Result<Vec<OrganizationPermission>, ManagementError> {
    values
        .into_iter()
        .map(|value| {
            OrganizationPermission::from_code(&value)
                .map_err(|_| ManagementError::InvalidRequest)
                .and_then(|permission| {
                    permission
                        .is_custom_role_grantable()
                        .then_some(permission)
                        .ok_or(ManagementError::InvalidRequest)
                })
        })
        .collect()
}

fn parse_organization_role(value: &str) -> Result<OrganizationRole, ManagementError> {
    match value {
        "owner" => Ok(OrganizationRole::Owner),
        "admin" => Ok(OrganizationRole::Admin),
        "billing" => Ok(OrganizationRole::Billing),
        "member" => Ok(OrganizationRole::Member),
        "viewer" => Ok(OrganizationRole::Viewer),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_organization_membership_status(
    value: &str,
) -> Result<OrganizationMembershipStatus, ManagementError> {
    match value {
        "active" => Ok(OrganizationMembershipStatus::Active),
        "suspended" => Ok(OrganizationMembershipStatus::Suspended),
        "removed" => Ok(OrganizationMembershipStatus::Removed),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn organization_invitation_status_code(status: OrganizationInvitationStatus) -> &'static str {
    match status {
        OrganizationInvitationStatus::Pending => "pending",
        OrganizationInvitationStatus::Accepted => "accepted",
        OrganizationInvitationStatus::Revoked => "revoked",
        OrganizationInvitationStatus::Expired => "expired",
    }
}

fn organization_membership_status_code(status: OrganizationMembershipStatus) -> &'static str {
    match status {
        OrganizationMembershipStatus::Active => "active",
        OrganizationMembershipStatus::Suspended => "suspended",
        OrganizationMembershipStatus::Removed => "removed",
    }
}

fn organization_team_status_code(status: af_domain::OrganizationTeamStatus) -> &'static str {
    match status {
        af_domain::OrganizationTeamStatus::Active => "active",
        af_domain::OrganizationTeamStatus::Disabled => "disabled",
    }
}

fn organization_department_status_code(
    status: af_domain::OrganizationDepartmentStatus,
) -> &'static str {
    match status {
        af_domain::OrganizationDepartmentStatus::Active => "active",
        af_domain::OrganizationDepartmentStatus::Disabled => "disabled",
    }
}

fn organization_department_membership_status_code(
    status: af_domain::OrganizationDepartmentMembershipStatus,
) -> &'static str {
    match status {
        af_domain::OrganizationDepartmentMembershipStatus::Active => "active",
        af_domain::OrganizationDepartmentMembershipStatus::Removed => "removed",
    }
}

fn format_department_cursor(cursor: OrganizationDepartmentCursor) -> String {
    format!("{}:{}", cursor.sort_order(), cursor.id().get())
}

fn custom_role_status_code(status: af_domain::OrganizationCustomRoleStatus) -> &'static str {
    match status {
        af_domain::OrganizationCustomRoleStatus::Active => "active",
        af_domain::OrganizationCustomRoleStatus::Disabled => "disabled",
    }
}

impl From<&af_admin::OrganizationServiceAccountRecord> for OrganizationServiceAccountResponse {
    fn from(record: &af_admin::OrganizationServiceAccountRecord) -> Self {
        let account = record.account();
        Self {
            service_account_key: account.public_key().persistence_key(),
            organization_id: record.locator().organization_id().get(),
            name: account.name().as_str().to_owned(),
            status: service_account_status_code(account.status()),
            version: record.version(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            revoked_at: record.revoked_at(),
        }
    }
}

impl OrganizationServiceAccountIssuedResponse {
    fn from_issued(issued: OrganizationServiceAccountIssued) -> Self {
        let (account, key_key, key_prefix, secret) = issued.into_parts();
        Self {
            service_account: OrganizationServiceAccountResponse::from(&account),
            key_key,
            key_prefix,
            secret,
        }
    }
}

impl OrganizationServiceAccountBudgetBindingResponse {
    fn from_binding(
        service_account_key: OrganizationServiceAccountKey,
        binding: &af_domain::OrganizationServiceAccountBudgetBinding,
    ) -> Self {
        let (scope, team_id) = match binding.scope() {
            OrganizationServiceAccountBudgetScope::Organization => ("organization", None),
            OrganizationServiceAccountBudgetScope::Team(team_id) => ("team", Some(team_id.get())),
        };
        Self {
            service_account_key: service_account_key.persistence_key(),
            organization_id: binding.organization_id().get(),
            policy_id: binding.policy_id().get(),
            policy_version: binding.policy_version(),
            scope,
            team_id,
            version: binding.version(),
        }
    }
}

fn service_account_status_code(status: OrganizationServiceAccountStatus) -> &'static str {
    match status {
        OrganizationServiceAccountStatus::Active => "active",
        OrganizationServiceAccountStatus::Suspended => "suspended",
        OrganizationServiceAccountStatus::Revoked => "revoked",
    }
}

fn parse_service_account_key(
    value: &str,
) -> Result<OrganizationServiceAccountKey, ManagementError> {
    OrganizationServiceAccountKey::from_persistence_key(value)
        .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_service_account_list_query(
    raw_query: Option<&str>,
) -> Result<OrganizationServiceAccountListQuery, ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok(OrganizationServiceAccountListQuery::default());
    };
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if after.is_none() => after = Some(parse_service_account_key(&value)?),
            "limit" if limit.is_none() => {
                limit = Some(
                    parse_positive_i64(&value)?
                        .try_into()
                        .map_err(|_| ManagementError::InvalidRequest)?,
                )
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    let after = after;
    let limit = limit.unwrap_or(af_admin::DEFAULT_ORGANIZATION_SERVICE_ACCOUNT_PAGE_SIZE);
    OrganizationServiceAccountListQuery::new(after, limit).map_err(map_service_account_error)
}

fn parse_service_account_status(
    value: &str,
) -> Result<OrganizationServiceAccountStatus, ManagementError> {
    match value {
        "active" => Ok(OrganizationServiceAccountStatus::Active),
        "suspended" => Ok(OrganizationServiceAccountStatus::Suspended),
        "revoked" => Ok(OrganizationServiceAccountStatus::Revoked),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn map_service_account_error(error: OrganizationServiceAccountServiceError) -> ManagementError {
    match error {
        OrganizationServiceAccountServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationServiceAccountServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationServiceAccountServiceError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationServiceAccountServiceError::Conflict => {
            ManagementError::OrganizationLifecycleConflict
        }
        OrganizationServiceAccountServiceError::InvalidTransition => {
            ManagementError::OrganizationLifecycleTransitionInvalid
        }
        OrganizationServiceAccountServiceError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationServiceAccountServiceError::Internal => ManagementError::Internal,
    }
}

fn map_custom_role_error(error: OrganizationCustomRoleServiceError) -> ManagementError {
    match error {
        OrganizationCustomRoleServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationCustomRoleServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationCustomRoleServiceError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationCustomRoleServiceError::Conflict => {
            ManagementError::OrganizationLifecycleConflict
        }
        OrganizationCustomRoleServiceError::InvalidTransition => {
            ManagementError::OrganizationLifecycleTransitionInvalid
        }
        OrganizationCustomRoleServiceError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationCustomRoleServiceError::Internal => ManagementError::Internal,
    }
}

fn map_sso_provider_error(error: OrganizationSsoProviderServiceError) -> ManagementError {
    match error {
        OrganizationSsoProviderServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationSsoProviderServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationSsoProviderServiceError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationSsoProviderServiceError::NotFound => {
            ManagementError::OrganizationSsoProviderNotFound
        }
        OrganizationSsoProviderServiceError::Conflict => {
            ManagementError::OrganizationSsoProviderConflict
        }
        OrganizationSsoProviderServiceError::InvalidTransition => {
            ManagementError::OrganizationSsoProviderTransitionInvalid
        }
        OrganizationSsoProviderServiceError::Internal => ManagementError::Internal,
    }
}

fn map_sso_login_error(error: OrganizationSsoLoginTransactionServiceError) -> ManagementError {
    match error {
        OrganizationSsoLoginTransactionServiceError::InvalidInput => {
            ManagementError::InvalidRequest
        }
        OrganizationSsoLoginTransactionServiceError::Unavailable
        | OrganizationSsoLoginTransactionServiceError::AlreadyProcessed
        | OrganizationSsoLoginTransactionServiceError::Expired
        | OrganizationSsoLoginTransactionServiceError::InvalidTransition => {
            ManagementError::OrganizationSsoProviderTransitionInvalid
        }
        OrganizationSsoLoginTransactionServiceError::Rejected => {
            ManagementError::OrganizationSsoProviderNotFound
        }
        OrganizationSsoLoginTransactionServiceError::Internal => ManagementError::Internal,
    }
}

fn map_sso_identity_binding_error(
    error: OrganizationSsoIdentityBindingServiceError,
) -> ManagementError {
    match error {
        OrganizationSsoIdentityBindingServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationSsoIdentityBindingServiceError::Rejected => {
            ManagementError::OrganizationSsoIdentityBindingRejected
        }
        OrganizationSsoIdentityBindingServiceError::Conflict => {
            ManagementError::OrganizationSsoIdentityBindingConflict
        }
        OrganizationSsoIdentityBindingServiceError::Internal => ManagementError::Internal,
    }
}

fn map_sso_recovery_error(error: OrganizationSsoRecoveryServiceError) -> ManagementError {
    match error {
        OrganizationSsoRecoveryServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationSsoRecoveryServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationSsoRecoveryServiceError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationSsoRecoveryServiceError::Rejected => {
            ManagementError::OrganizationSsoRecoveryRejected
        }
        OrganizationSsoRecoveryServiceError::Conflict => {
            ManagementError::OrganizationSsoRecoveryConflict
        }
        OrganizationSsoRecoveryServiceError::Unavailable => {
            ManagementError::OrganizationSsoRecoveryUnavailable
        }
        OrganizationSsoRecoveryServiceError::Internal => ManagementError::Internal,
    }
}

fn map_sso_policy_error(error: OrganizationSsoPolicyServiceError) -> ManagementError {
    match error {
        OrganizationSsoPolicyServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationSsoPolicyServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationSsoPolicyServiceError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationSsoPolicyServiceError::NotFound
        | OrganizationSsoPolicyServiceError::Unavailable => {
            ManagementError::OrganizationSsoRecoveryUnavailable
        }
        OrganizationSsoPolicyServiceError::Conflict => {
            ManagementError::OrganizationSsoRecoveryConflict
        }
        OrganizationSsoPolicyServiceError::Internal => ManagementError::Internal,
    }
}

fn organization_sso_identity_status_code(
    status: af_domain::OrganizationSsoIdentityStatus,
) -> &'static str {
    match status {
        af_domain::OrganizationSsoIdentityStatus::Active => "active",
        af_domain::OrganizationSsoIdentityStatus::Disabled => "disabled",
    }
}

fn organization_sso_identity_binding_method_code(
    method: af_domain::OrganizationSsoIdentityBindingMethod,
) -> &'static str {
    match method {
        af_domain::OrganizationSsoIdentityBindingMethod::Explicit => "explicit",
        af_domain::OrganizationSsoIdentityBindingMethod::Invited => "invited",
    }
}

fn map_sso_domain_error(error: OrganizationSsoDomainServiceError) -> ManagementError {
    match error {
        OrganizationSsoDomainServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationSsoDomainServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationSsoDomainServiceError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationSsoDomainServiceError::NotFound => {
            ManagementError::OrganizationSsoDomainNotFound
        }
        OrganizationSsoDomainServiceError::Conflict => {
            ManagementError::OrganizationSsoDomainConflict
        }
        OrganizationSsoDomainServiceError::InvalidTransition => {
            ManagementError::OrganizationSsoDomainTransitionInvalid
        }
        OrganizationSsoDomainServiceError::VerificationFailed => {
            ManagementError::OrganizationSsoDomainVerificationFailed
        }
        OrganizationSsoDomainServiceError::Internal => ManagementError::Internal,
    }
}

fn map_custom_role_binding_error(
    error: OrganizationCustomRoleBindingServiceError,
) -> ManagementError {
    match error {
        OrganizationCustomRoleBindingServiceError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationCustomRoleBindingServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationCustomRoleBindingServiceError::NotFound
        | OrganizationCustomRoleBindingServiceError::MembershipUnavailable
        | OrganizationCustomRoleBindingServiceError::RoleUnavailable => {
            ManagementError::OrganizationNotFound
        }
        OrganizationCustomRoleBindingServiceError::Conflict => {
            ManagementError::OrganizationLifecycleConflict
        }
        OrganizationCustomRoleBindingServiceError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationCustomRoleBindingServiceError::Internal => ManagementError::Internal,
    }
}

fn map_management_lifecycle_error(error: OrganizationManagementError) -> ManagementError {
    match error {
        OrganizationManagementError::InvalidInput => ManagementError::InvalidRequest,
        OrganizationManagementError::Forbidden => ManagementError::Forbidden,
        OrganizationManagementError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationManagementError::Conflict => ManagementError::OrganizationLifecycleConflict,
        OrganizationManagementError::InvalidTransition => {
            ManagementError::OrganizationLifecycleTransitionInvalid
        }
        OrganizationManagementError::LastOwner => ManagementError::OrganizationLastOwner,
        OrganizationManagementError::TeamHasActiveMembers => ManagementError::OrganizationTeamInUse,
        OrganizationManagementError::EntitlementRestricted => {
            ManagementError::OrganizationEntitlementRestricted
        }
        OrganizationManagementError::SeatLimitReached => {
            ManagementError::OrganizationSeatLimitReached
        }
        OrganizationManagementError::TeamLimitReached => {
            ManagementError::OrganizationTeamLimitReached
        }
        OrganizationManagementError::Internal => ManagementError::Internal,
    }
}

fn map_invitation_management_error(error: OrganizationInvitationError) -> ManagementError {
    match error {
        OrganizationInvitationError::InvalidInput | OrganizationInvitationError::BatchTooLarge => {
            ManagementError::InvalidRequest
        }
        OrganizationInvitationError::Forbidden => ManagementError::Forbidden,
        OrganizationInvitationError::NotFound => ManagementError::OrganizationInvitationNotFound,
        OrganizationInvitationError::Conflict => ManagementError::OrganizationInvitationConflict,
        OrganizationInvitationError::SeatLimitReached => {
            ManagementError::OrganizationSeatLimitReached
        }
        OrganizationInvitationError::Expired => ManagementError::OrganizationInvitationExpired,
        OrganizationInvitationError::EmailMismatch => {
            ManagementError::OrganizationInvitationRejected
        }
        OrganizationInvitationError::Internal => ManagementError::Internal,
    }
}

fn map_invitation_accept_error(error: OrganizationInvitationError) -> ManagementError {
    match error {
        OrganizationInvitationError::InvalidInput | OrganizationInvitationError::BatchTooLarge => {
            ManagementError::InvalidRequest
        }
        OrganizationInvitationError::SeatLimitReached => {
            ManagementError::OrganizationSeatLimitReached
        }
        OrganizationInvitationError::Forbidden => ManagementError::Forbidden,
        OrganizationInvitationError::NotFound
        | OrganizationInvitationError::Conflict
        | OrganizationInvitationError::Expired
        | OrganizationInvitationError::EmailMismatch => {
            ManagementError::OrganizationInvitationRejected
        }
        OrganizationInvitationError::Internal => ManagementError::Internal,
    }
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod sso_provider_tests {
    use super::*;

    #[test]
    fn sso_provider_response_never_serializes_client_secret() {
        let provider = OrganizationSsoProvider::new(
            1,
            OrganizationId::new(7).unwrap(),
            "sso_0123456789abcdef0123456789abcdef".to_owned(),
            OrganizationSsoProtocol::Oidc,
            OrganizationSsoProviderStatus::Active,
            "企业登录".to_owned(),
            Some(json!({"issuer": "https://id.example.com"})),
            true,
            2,
            10,
            11,
        );
        let rendered =
            serde_json::to_value(OrganizationSsoProviderResponse::from(provider)).unwrap();
        assert_eq!(rendered["secret_configured"], true);
        assert!(rendered.get("client_secret").is_none());
        assert!(!rendered.to_string().contains("client-secret"));
    }

    #[test]
    fn sso_provider_request_rejects_secret_and_clear_together() {
        let request = OrganizationSsoProviderRequest {
            expected_version: 1,
            protocol: "oidc".to_owned(),
            status: "active".to_owned(),
            display_name: "企业登录".to_owned(),
            metadata_projection: None,
            client_secret: Some("client-secret".to_owned()),
            clear_client_secret: true,
        };
        assert_eq!(
            request.into_command().unwrap_err(),
            ManagementError::InvalidRequest
        );
    }
}

#[cfg(test)]
mod sso_identity_binding_tests {
    use super::*;

    fn request(method: &str) -> OrganizationSsoIdentityBindingRequest {
        OrganizationSsoIdentityBindingRequest {
            ticket: "ticket-value".to_owned(),
            method: method.to_owned(),
            current_password: None,
            totp_code: None,
            credential: None,
        }
    }

    #[test]
    fn binding_request_rejects_unknown_fields() {
        let result = serde_json::from_value::<OrganizationSsoIdentityBindingRequest>(json!({
            "ticket": "ticket-value",
            "method": "password",
            "current_password": "current-password",
            "unexpected": true
        }));
        assert!(result.is_err());
    }

    #[test]
    fn binding_request_requires_matching_step_up_material() {
        let mut password = request("password");
        password.current_password = Some("current-password".to_owned());
        password.credential = Some(json!({"id": "credential"}));
        assert_eq!(
            parse_identity_binding_step_up(&password).unwrap_err(),
            ManagementError::InvalidRequest
        );

        let mut passkey = request("passkey");
        passkey.current_password = Some("current-password".to_owned());
        assert_eq!(
            parse_identity_binding_step_up(&passkey).unwrap_err(),
            ManagementError::InvalidRequest
        );
    }

    #[test]
    fn binding_response_excludes_authentication_material() {
        let response = serde_json::to_value(OrganizationSsoIdentityBindingResponse {
            identity_id: 1,
            organization_id: 2,
            membership_id: 3,
            status: "active",
            binding_method: "explicit",
            version: 4,
        })
        .unwrap();
        for field in [
            "ticket",
            "subject",
            "credential",
            "current_password",
            "totp_code",
        ] {
            assert!(response.get(field).is_none(), "发现敏感响应字段: {field}");
        }
        assert_eq!(response["identity_id"], 1);
        assert_eq!(response["binding_method"], "explicit");
    }
}

#[cfg(test)]
mod sso_recovery_tests {
    use super::*;

    #[test]
    fn recovery_request_rejects_unknown_fields_and_invalid_step_up() {
        let unknown = serde_json::from_value::<OrganizationSsoRecoveryRequest>(json!({
            "expected_version": 1,
            "current_password": "current-password",
            "unexpected": true
        }));
        assert!(unknown.is_err());

        let invalid = OrganizationSsoRecoveryRequest {
            expected_version: 0,
            current_password: String::new(),
            totp_code: None,
        };
        assert_eq!(
            recovery_command(invalid).unwrap_err(),
            ManagementError::InvalidRequest
        );
    }

    #[test]
    fn recovery_response_is_redacted_and_policy_is_stable() {
        let response = serde_json::to_value(OrganizationSsoRecoveryResponse {
            policy: "required",
            recovery_ready: true,
            version: 2,
        })
        .unwrap();
        assert_eq!(response["policy"], "required");
        assert_eq!(response["recovery_ready"], true);
        assert_eq!(response["version"], 2);
        for field in [
            "current_password",
            "totp_code",
            "recovery_material",
            "secret",
        ] {
            assert!(response.get(field).is_none(), "发现敏感响应字段: {field}");
        }
    }

    #[test]
    fn recovery_errors_map_to_stable_management_codes() {
        assert_eq!(
            map_sso_recovery_error(OrganizationSsoRecoveryServiceError::Rejected),
            ManagementError::OrganizationSsoRecoveryRejected
        );
        assert_eq!(
            map_sso_recovery_error(OrganizationSsoRecoveryServiceError::Conflict),
            ManagementError::OrganizationSsoRecoveryConflict
        );
        assert_eq!(
            map_sso_recovery_error(OrganizationSsoRecoveryServiceError::Unavailable),
            ManagementError::OrganizationSsoRecoveryUnavailable
        );
    }
}
