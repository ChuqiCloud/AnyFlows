use std::sync::Arc;

use af_admin::{
    PlatformOrganizationDirectoryError, PlatformOrganizationDirectoryQuery,
    PlatformOrganizationDirectoryService, PlatformOrganizationSummary,
    PlatformUserOrganizationQuery, PlatformUserOrganizationSummary, SessionAuthentication,
};
use af_domain::{OrganizationId, OrganizationMembershipId, UserId};
use af_telemetry::RequestId;
use axum::{
    Extension, Router,
    extract::{Path, RawQuery, State},
    middleware,
    response::Response,
    routing::get,
};
use http::HeaderMap;
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
    middleware::ensure_request_id,
    organization_activity::platform_reason,
};

#[derive(Clone)]
struct PlatformOrganizationHttpState {
    service: Arc<dyn PlatformOrganizationDirectoryService>,
}

/// 构建受平台权限和只追加审计保护的企业目录路由。
pub(crate) fn build_platform_organization_router(
    service: Arc<dyn PlatformOrganizationDirectoryService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/platform/organizations",
            get(list_platform_organizations),
        )
        .route(
            "/api/platform/users/{user_id}/organizations",
            get(list_platform_user_organizations),
        )
        .layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(PlatformOrganizationHttpState { service })
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformOrganizationSummaryResponse {
    #[schema(minimum = 1)]
    id: i64,
    name: String,
    slug: String,
    organization_status: &'static str,
    entitlement_status: &'static str,
    #[schema(minimum = 1)]
    owner_membership_id: i64,
    #[schema(minimum = 1)]
    owner_user_id: i64,
    owner_username: String,
    #[schema(minimum = 1)]
    active_member_count: i64,
    #[schema(minimum = 1)]
    seat_limit: i64,
    valid_until: i64,
    #[schema(required = true)]
    grace_until: Option<i64>,
    #[schema(minimum = 1)]
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformOrganizationDirectoryResponse {
    organizations: Vec<PlatformOrganizationSummaryResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformUserOrganizationSummaryResponse {
    #[schema(minimum = 1)]
    organization_id: i64,
    organization_name: String,
    organization_slug: String,
    organization_status: &'static str,
    entitlement_status: &'static str,
    #[schema(minimum = 1)]
    membership_id: i64,
    role: &'static str,
    membership_status: &'static str,
    joined_at: i64,
    #[schema(minimum = 1)]
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformUserOrganizationListResponse {
    organizations: Vec<PlatformUserOrganizationSummaryResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

async fn list_platform_organizations(
    State(state): State<PlatformOrganizationHttpState>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    let query = parse_organization_query(raw_query.as_deref())?;
    let page = state
        .service
        .list(
            authentication.principal(),
            query,
            platform_reason(&headers)?,
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_directory_error)?;
    Ok(no_store_json(PlatformOrganizationDirectoryResponse {
        organizations: page
            .organizations()
            .iter()
            .map(PlatformOrganizationSummaryResponse::from_summary)
            .collect(),
        next_cursor: page.next_cursor().map(OrganizationId::get),
    }))
}

async fn list_platform_user_organizations(
    State(state): State<PlatformOrganizationHttpState>,
    Path(user_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
    headers: HeaderMap,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    let user_id = UserId::new(user_id).map_err(|_| ManagementError::InvalidRequest)?;
    let query = parse_user_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_for_user(
            authentication.principal(),
            user_id,
            query,
            platform_reason(&headers)?,
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_directory_error)?;
    Ok(no_store_json(PlatformUserOrganizationListResponse {
        organizations: page
            .organizations()
            .iter()
            .map(PlatformUserOrganizationSummaryResponse::from_summary)
            .collect(),
        next_cursor: page.next_cursor().map(OrganizationMembershipId::get),
    }))
}

impl PlatformOrganizationSummaryResponse {
    fn from_summary(summary: &PlatformOrganizationSummary) -> Self {
        Self {
            id: summary.organization_id().get(),
            name: summary.name().to_owned(),
            slug: summary.slug().to_owned(),
            organization_status: organization_status_code(summary.organization_status()),
            entitlement_status: summary.entitlement_status().code(),
            owner_membership_id: summary.owner_membership_id().get(),
            owner_user_id: summary.owner_user_id().get(),
            owner_username: summary.owner_username().to_owned(),
            active_member_count: summary.active_member_count(),
            seat_limit: summary.seat_limit(),
            valid_until: summary.valid_until(),
            grace_until: summary.grace_until(),
            version: summary.version(),
        }
    }
}

impl PlatformUserOrganizationSummaryResponse {
    fn from_summary(summary: &PlatformUserOrganizationSummary) -> Self {
        Self {
            organization_id: summary.organization_id().get(),
            organization_name: summary.organization_name().to_owned(),
            organization_slug: summary.organization_slug().to_owned(),
            organization_status: organization_status_code(summary.organization_status()),
            entitlement_status: summary.entitlement_status().code(),
            membership_id: summary.membership_id().get(),
            role: organization_role_code(summary.role()),
            membership_status: membership_status_code(summary.membership_status()),
            joined_at: summary.joined_at(),
            version: summary.version(),
        }
    }
}

fn parse_organization_query(
    raw_query: Option<&str>,
) -> Result<PlatformOrganizationDirectoryQuery, ManagementError> {
    let (after, limit) = parse_query(raw_query)?;
    let after = after
        .map(OrganizationId::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    PlatformOrganizationDirectoryQuery::new(after, limit).map_err(map_directory_error)
}

fn parse_user_query(
    raw_query: Option<&str>,
) -> Result<PlatformUserOrganizationQuery, ManagementError> {
    let (after, limit) = parse_query(raw_query)?;
    let after = after
        .map(OrganizationMembershipId::new)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?;
    PlatformUserOrganizationQuery::new(after, limit).map_err(map_directory_error)
}

fn parse_query(raw_query: Option<&str>) -> Result<(Option<i64>, usize), ManagementError> {
    let mut after = None;
    let mut limit = None;
    if let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) {
        for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
            let target = match key.as_ref() {
                "after" if after.is_none() => &mut after,
                "limit" if limit.is_none() => &mut limit,
                _ => return Err(ManagementError::InvalidRequest),
            };
            if value.is_empty() || value.chars().any(|character| !character.is_ascii_digit()) {
                return Err(ManagementError::InvalidRequest);
            }
            *target = Some(
                value
                    .parse::<i64>()
                    .ok()
                    .filter(|value| *value > 0)
                    .ok_or(ManagementError::InvalidRequest)?,
            );
        }
    }
    let limit = limit
        .map(usize::try_from)
        .transpose()
        .map_err(|_| ManagementError::InvalidRequest)?
        .unwrap_or(af_admin::DEFAULT_PLATFORM_ORGANIZATION_DIRECTORY_PAGE_SIZE);
    Ok((after, limit))
}

const fn organization_status_code(status: af_domain::OrganizationStatus) -> &'static str {
    match status {
        af_domain::OrganizationStatus::Active => "active",
        af_domain::OrganizationStatus::Suspended => "suspended",
        af_domain::OrganizationStatus::Closed => "closed",
    }
}

const fn organization_role_code(role: af_domain::OrganizationRole) -> &'static str {
    match role {
        af_domain::OrganizationRole::Owner => "owner",
        af_domain::OrganizationRole::Admin => "admin",
        af_domain::OrganizationRole::Billing => "billing",
        af_domain::OrganizationRole::Member => "member",
        af_domain::OrganizationRole::Viewer => "viewer",
    }
}

const fn membership_status_code(status: af_domain::OrganizationMembershipStatus) -> &'static str {
    match status {
        af_domain::OrganizationMembershipStatus::Active => "active",
        af_domain::OrganizationMembershipStatus::Suspended => "suspended",
        af_domain::OrganizationMembershipStatus::Removed => "removed",
    }
}

const fn map_directory_error(error: PlatformOrganizationDirectoryError) -> ManagementError {
    match error {
        PlatformOrganizationDirectoryError::InvalidInput => ManagementError::InvalidRequest,
        PlatformOrganizationDirectoryError::Forbidden => ManagementError::Forbidden,
        PlatformOrganizationDirectoryError::NotFound => ManagementError::UserNotFound,
        PlatformOrganizationDirectoryError::Internal => ManagementError::Internal,
    }
}
