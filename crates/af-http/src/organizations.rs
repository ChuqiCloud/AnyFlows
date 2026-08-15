use std::sync::Arc;

use af_admin::{
    OrganizationAccessSummary, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationEntitlementSummary, SessionAuthentication,
};
use af_domain::{OrganizationId, OrganizationPermission};
use axum::{
    Extension, Router,
    extract::{Path, State},
    middleware,
    response::Response,
    routing::get,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
pub(crate) struct OrganizationHttpState {
    service: Arc<dyn OrganizationAuthorizationService>,
}

/// 构建企业目录与企业概览只读入口；所有入口都要求有效管理会话。
pub(crate) fn build_organization_router(
    service: Arc<dyn OrganizationAuthorizationService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/account/organizations", get(list_organizations))
        .route(
            "/api/organizations/{organization_id}",
            get(get_organization),
        )
        .layer(authentication)
        .with_state(OrganizationHttpState { service })
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

async fn list_organizations(
    State(state): State<OrganizationHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let organizations = state
        .service
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
        .service
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
