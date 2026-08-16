use std::sync::Arc;

use af_admin::{
    DEFAULT_ORGANIZATION_INVITATION_TTL_SECONDS, IssuedOrganizationInvitation,
    OrganizationAccessSummary, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationEntitlementSummary, OrganizationInvitationBatchCommand,
    OrganizationInvitationBatchResult, OrganizationInvitationCommand, OrganizationInvitationError,
    OrganizationInvitationPreviewCommand, OrganizationInvitationPreviewResult,
    OrganizationInvitationRecord, OrganizationInvitationService, OrganizationManagementError,
    OrganizationManagementService, OrganizationMembershipChangeCommand,
    OrganizationMembershipListQuery, OrganizationMembershipRecord,
    OrganizationOwnershipTransferCommand, OrganizationTeamChangeCommand,
    OrganizationTeamCreateCommand, OrganizationTeamDisableCommand, OrganizationTeamListQuery,
    OrganizationTeamRecord, SessionAuthentication,
};
use af_domain::{
    OrganizationId, OrganizationInvitationKey, OrganizationInvitationStatus,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationPermission,
    OrganizationRole, OrganizationTeamId,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State},
    middleware,
    response::Response,
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
pub(crate) struct OrganizationHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    invitation_service: Option<Arc<dyn OrganizationInvitationService>>,
    management_service: Option<Arc<dyn OrganizationManagementService>>,
}

/// 构建企业目录与企业概览只读入口；所有入口都要求有效管理会话。
pub fn build_organization_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    invitation_service: Option<Arc<dyn OrganizationInvitationService>>,
    management_service: Option<Arc<dyn OrganizationManagementService>>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
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
    router
        .layer(authentication)
        .with_state(OrganizationHttpState {
            authorization_service,
            invitation_service,
            management_service,
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
    team_management_enabled: bool,
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

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OrganizationMembershipResponse {
    id: i64,
    organization_id: i64,
    user_id: i64,
    role: &'static str,
    team_id: Option<i64>,
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
    let page = service
        .list_members(context, query)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationMembershipListResponse {
        members: page
            .records()
            .iter()
            .map(OrganizationMembershipResponse::from)
            .collect(),
        next_cursor: page.next_cursor().map(OrganizationMembershipId::get),
    }))
}

async fn change_organization_member(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, membership_id)): Path<(i64, i64)>,
    Json(request): Json<OrganizationMembershipChangeRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let membership_id = OrganizationMembershipId::new(membership_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::MembersManage,
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
    let member = service
        .change_member(context, command)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationMembershipResponse::from(&member)))
}

async fn transfer_organization_ownership(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    Json(request): Json<OrganizationOwnershipTransferRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::OwnershipTransfer,
    )
    .await?;
    let command = OrganizationOwnershipTransferCommand::new(
        OrganizationMembershipId::new(request.target_membership_id)
            .map_err(|_| ManagementError::InvalidRequest)?,
        request.current_owner_version,
        request.target_expected_version,
    )
    .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let result = service
        .transfer_ownership(context, command)
        .await
        .map_err(map_management_lifecycle_error)?;
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
    Path(organization_id): Path<i64>,
    Json(request): Json<OrganizationTeamCreateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::TeamsManage,
    )
    .await?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let team = service
        .create_team(
            context,
            OrganizationTeamCreateCommand::new(request.name, request.description),
        )
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationTeamResponse::from(&team)))
}

async fn change_organization_team(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, team_id)): Path<(i64, i64)>,
    Json(request): Json<OrganizationTeamChangeRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let team_id = OrganizationTeamId::new(team_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::TeamsManage,
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
    let team = service
        .change_team(context, command)
        .await
        .map_err(map_management_lifecycle_error)?;
    Ok(no_store_json(OrganizationTeamResponse::from(&team)))
}

async fn disable_organization_team(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, team_id)): Path<(i64, i64)>,
    Json(request): Json<OrganizationTeamDisableRequest>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let team_id = OrganizationTeamId::new(team_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_lifecycle_context(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::TeamsManage,
    )
    .await?;
    let command = OrganizationTeamDisableCommand::new(team_id, request.expected_version)
        .map_err(map_management_lifecycle_error)?;
    let service = state
        .management_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let team = service
        .disable_team(context, command)
        .await
        .map_err(map_management_lifecycle_error)?;
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
            .role()
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
    let organization_id =
        authorize_invitation_management(&state, &authentication, organization_id).await?;
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
    Path(organization_id): Path<i64>,
    Json(request): Json<OrganizationInvitationCreateRequest>,
) -> Result<Response, ManagementError> {
    let organization_id =
        authorize_invitation_management(&state, &authentication, organization_id).await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let issued = invitation_service
        .create(
            authentication.principal(),
            organization_id,
            request.into_command()?,
        )
        .await
        .map_err(map_invitation_management_error)?;
    Ok(no_store_json(issued_invitation_response(issued)?))
}

async fn preview_organization_invitations(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    Json(request): Json<OrganizationInvitationPreviewRequest>,
) -> Result<Response, ManagementError> {
    let organization_id =
        authorize_invitation_management(&state, &authentication, organization_id).await?;
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
    Path(organization_id): Path<i64>,
    Json(request): Json<OrganizationInvitationBatchRequest>,
) -> Result<Response, ManagementError> {
    let organization_id =
        authorize_invitation_management(&state, &authentication, organization_id).await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let batch_key = OrganizationInvitationKey::from_persistence_key(&request.batch_key)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let command = OrganizationInvitationBatchCommand::new(
        batch_key,
        request
            .items
            .into_iter()
            .map(OrganizationInvitationCreateRequest::into_command)
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(map_invitation_management_error)?;
    let result = invitation_service
        .create_batch(authentication.principal(), organization_id, command)
        .await
        .map_err(map_invitation_management_error)?;
    Ok(no_store_json(OrganizationInvitationBatchResponse::from(
        result,
    )))
}

async fn revoke_organization_invitation(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, invitation_key)): Path<(i64, String)>,
) -> Result<Response, ManagementError> {
    let organization_id =
        authorize_invitation_management(&state, &authentication, organization_id).await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let invitation_key = parse_invitation_key(&invitation_key)?;
    let invitation = invitation_service
        .revoke(organization_id, invitation_key)
        .await
        .map_err(map_invitation_management_error)?;
    Ok(no_store_json(OrganizationInvitationResponse::from(
        &invitation,
    )))
}

async fn resend_organization_invitation(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, invitation_key)): Path<(i64, String)>,
) -> Result<Response, ManagementError> {
    let organization_id =
        authorize_invitation_management(&state, &authentication, organization_id).await?;
    let invitation_service = state
        .invitation_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let issued = invitation_service
        .resend(
            authentication.principal(),
            organization_id,
            parse_invitation_key(&invitation_key)?,
        )
        .await
        .map_err(map_invitation_management_error)?;
    Ok(no_store_json(issued_invitation_response(issued)?))
}

async fn accept_organization_invitation(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
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
            team_management_enabled: summary.team_management_enabled(),
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

impl From<&OrganizationMembershipRecord> for OrganizationMembershipResponse {
    fn from(record: &OrganizationMembershipRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            user_id: record.user_id().get(),
            role: organization_role_code(record.role()),
            team_id: record.team_id().map(OrganizationTeamId::get),
            status: organization_membership_status_code(record.status()),
            joined_at: record.joined_at(),
            suspended_at: record.suspended_at(),
            removed_at: record.removed_at(),
            invited_by_user_id: record.invited_by_user_id().map(af_domain::UserId::get),
            version: record.version(),
        }
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

async fn authorize_invitation_management(
    state: &OrganizationHttpState,
    authentication: &SessionAuthentication,
    organization_id: i64,
) -> Result<OrganizationId, ManagementError> {
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
        .map_err(map_authorization_error)?;
    Ok(organization_id)
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
