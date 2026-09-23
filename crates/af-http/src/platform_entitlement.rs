use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
    middleware::ensure_request_id,
};
use af_admin::{PlatformEntitlementService, SessionAuthentication, SessionAuthenticator};
use af_domain::{OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    middleware,
    response::Response,
    routing::patch,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlatformEntitlementUpdateRequest {
    expected_version: u64,
    reason: String,
    team_management_enabled: bool,
    sso_enabled: bool,
    custom_roles_enabled: bool,
    seat_limit: i64,
    team_limit: i64,
    organization_key_limit: i64,
    audit_retention_days: i64,
    valid_from: i64,
    valid_until: i64,
    grace_until: Option<i64>,
}
#[derive(Serialize, ToSchema)]
pub(crate) struct PlatformEntitlementUpdateResponse {
    version: i64,
}

pub fn build_platform_entitlement_router(
    service: Arc<PlatformEntitlementService>,
    auth: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route(
            "/api/platform/organizations/{organization_id}/entitlement",
            patch(update),
        )
        .route_layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(auth),
            authenticate_management_session,
        ))
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(service)
}
async fn update(
    State(s): State<Arc<PlatformEntitlementService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    Path(id): Path<i64>,
    Json(b): Json<PlatformEntitlementUpdateRequest>,
) -> Result<Response, ManagementError> {
    let invalid = || ManagementError::InvalidRequest;
    let capacity =
        OrganizationEntitlementCapacity::new(b.seat_limit, b.team_limit, b.organization_key_limit)
            .map_err(|_| invalid())?;
    let snapshot = OrganizationEntitlementSnapshot::new(
        capacity,
        b.team_management_enabled,
        b.sso_enabled,
        b.custom_roles_enabled,
        b.audit_retention_days,
        b.valid_from,
        b.valid_until,
        b.grace_until,
    )
    .map_err(|_| invalid())?;
    let version = s
        .update(
            a.principal(),
            OrganizationId::new(id).map_err(|_| invalid())?,
            snapshot,
            b.expected_version,
            b.reason,
            r.as_str().to_owned(),
        )
        .await
        .map_err(crate::account_verification::map_error)?;
    Ok(no_store_json(PlatformEntitlementUpdateResponse { version }))
}
