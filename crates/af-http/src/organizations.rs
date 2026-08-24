use std::sync::Arc;

use af_admin::{
    DEFAULT_ORGANIZATION_INVITATION_TTL_SECONDS, IssuedOrganizationInvitation,
    OrganizationAccessSummary, OrganizationActivityService, OrganizationAuditOutcome,
    OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationCustomRoleBindingCommand, OrganizationCustomRoleBindingService,
    OrganizationCustomRoleBindingServiceError, OrganizationCustomRoleChangeCommand,
    OrganizationCustomRoleCreateCommand, OrganizationCustomRoleDisableCommand,
    OrganizationCustomRoleListQuery, OrganizationCustomRoleService,
    OrganizationCustomRoleServiceError, OrganizationEntitlementSummary,
    OrganizationInvitationBatchCommand, OrganizationInvitationBatchResult,
    OrganizationInvitationCommand, OrganizationInvitationError,
    OrganizationInvitationPreviewCommand, OrganizationInvitationPreviewResult,
    OrganizationInvitationRecord, OrganizationInvitationService, OrganizationManagementError,
    OrganizationManagementService, OrganizationMembershipChangeCommand,
    OrganizationMembershipListQuery, OrganizationMembershipRecord,
    OrganizationOwnershipTransferCommand, OrganizationSsoProvider, OrganizationSsoProviderCommand,
    OrganizationSsoProviderService, OrganizationSsoProviderServiceError,
    OrganizationTeamChangeCommand, OrganizationTeamCreateCommand, OrganizationTeamDisableCommand,
    OrganizationTeamListQuery, OrganizationTeamRecord, SessionAuthentication,
};
use af_domain::{
    OrganizationCustomRoleKey, OrganizationId, OrganizationInvitationKey,
    OrganizationInvitationStatus, OrganizationMembershipId, OrganizationMembershipStatus,
    OrganizationPermission, OrganizationRole, OrganizationSsoProtocol,
    OrganizationSsoProviderStatus, OrganizationTeamId,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, patch, post, put},
};
use http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

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

fn organization_sso_protocol_code(protocol: OrganizationSsoProtocol) -> &'static str {
    match protocol {
        OrganizationSsoProtocol::Oidc => "oidc",
        OrganizationSsoProtocol::Saml => "saml",
    }
}

fn organization_sso_provider_status_code(status: OrganizationSsoProviderStatus) -> &'static str {
    match status {
        OrganizationSsoProviderStatus::Draft => "draft",
        OrganizationSsoProviderStatus::Active => "active",
        OrganizationSsoProviderStatus::Disabled => "disabled",
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

fn custom_role_status_code(status: af_domain::OrganizationCustomRoleStatus) -> &'static str {
    match status {
        af_domain::OrganizationCustomRoleStatus::Active => "active",
        af_domain::OrganizationCustomRoleStatus::Disabled => "disabled",
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
        let provider = OrganizationSsoProvider {
            id: 1,
            organization_id: OrganizationId::new(7).unwrap(),
            provider_key: "sso_0123456789abcdef0123456789abcdef".to_owned(),
            protocol: OrganizationSsoProtocol::Oidc,
            status: OrganizationSsoProviderStatus::Active,
            display_name: "企业登录".to_owned(),
            metadata_projection: Some(json!({"issuer": "https://id.example.com"})),
            secret_configured: true,
            version: 2,
            created_at: 10,
            updated_at: 11,
        };
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
