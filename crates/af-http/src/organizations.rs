use std::sync::Arc;

use af_admin::{
    DEFAULT_ORGANIZATION_INVITATION_TTL_SECONDS, IssuedOrganizationInvitation,
    OrganizationAccessSummary, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationEntitlementSummary, OrganizationInvitationBatchCommand,
    OrganizationInvitationBatchResult, OrganizationInvitationCommand, OrganizationInvitationError,
    OrganizationInvitationPreviewCommand, OrganizationInvitationPreviewResult,
    OrganizationInvitationRecord, OrganizationInvitationService, OrganizationMembershipRecord,
    SessionAuthentication,
};
use af_domain::{
    OrganizationId, OrganizationInvitationKey, OrganizationInvitationStatus,
    OrganizationMembershipStatus, OrganizationPermission, OrganizationRole, OrganizationTeamId,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    middleware,
    response::Response,
    routing::{get, post},
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
}

/// 构建企业目录与企业概览只读入口；所有入口都要求有效管理会话。
pub fn build_organization_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    invitation_service: Option<Arc<dyn OrganizationInvitationService>>,
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
    router
        .layer(authentication)
        .with_state(OrganizationHttpState {
            authorization_service,
            invitation_service,
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

fn parse_invitation_key(value: &str) -> Result<OrganizationInvitationKey, ManagementError> {
    OrganizationInvitationKey::from_persistence_key(value)
        .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_organization_role(value: &str) -> Result<OrganizationRole, ManagementError> {
    match value {
        "admin" => Ok(OrganizationRole::Admin),
        "billing" => Ok(OrganizationRole::Billing),
        "member" => Ok(OrganizationRole::Member),
        "viewer" => Ok(OrganizationRole::Viewer),
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
