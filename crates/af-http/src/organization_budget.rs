use std::sync::Arc;

use af_admin::{
    OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationBudgetPolicyCreateCommand, OrganizationBudgetService,
    OrganizationBudgetServiceError, OrganizationContext, SessionAuthentication,
    SessionAuthenticator,
};
use af_admin::{
    OrganizationBillingReservationRecord, OrganizationBillingReservationStatus,
    OrganizationBudgetAllocationRecord, OrganizationBudgetPolicyRecord, OrganizationBudgetScope,
    OrganizationBudgetWindowRecord,
};
use af_domain::{
    BillingReservationId, OrganizationBudgetLimit, OrganizationBudgetPeriod, OrganizationId,
    OrganizationMembershipId, OrganizationPermission, OrganizationTeamId, Quota,
};
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
};

#[derive(Clone)]
struct OrganizationBudgetHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    budget_service: Arc<dyn OrganizationBudgetService>,
}

/// 构建企业预算管理接口；路由层只负责会话认证，组织权限由服务端重新解析。
pub fn build_organization_budget_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    budget_service: Arc<dyn OrganizationBudgetService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/budgets",
            get(get_organization_budgets).post(create_organization_budget_policy),
        )
        .route(
            "/api/organizations/{organization_id}/budgets/reservations/{reservation_id}",
            get(get_organization_budget_reservation),
        )
        .route_layer(authentication)
        .with_state(OrganizationBudgetHttpState {
            authorization_service,
            budget_service,
        })
}

#[derive(Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OrganizationBudgetScopeRequest {
    Organization,
    Team { team_id: i64 },
    Member { membership_id: i64 },
}

#[derive(Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum OrganizationBudgetPeriodRequest {
    UtcDay,
    UtcWeek,
    UtcMonth,
    Fixed { seconds: u32 },
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationBudgetPolicyCreateRequest {
    scope: OrganizationBudgetScopeRequest,
    period: OrganizationBudgetPeriodRequest,
    #[schema(minimum = 1, required = true)]
    quota_limit: Option<i64>,
    #[schema(minimum = 0)]
    effective_from: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum OrganizationBudgetScopeResponse {
    Organization,
    Team { team_id: i64 },
    Member { membership_id: i64 },
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum OrganizationBudgetPeriodResponse {
    UtcDay,
    UtcWeek,
    UtcMonth,
    Fixed { seconds: u32 },
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationBudgetPolicyResponse {
    id: i64,
    organization_id: i64,
    scope: OrganizationBudgetScopeResponse,
    period: OrganizationBudgetPeriodResponse,
    #[schema(minimum = 1, required = true)]
    quota_limit: Option<i64>,
    effective_from: i64,
    #[schema(required = true)]
    effective_until: Option<i64>,
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationBudgetWindowResponse {
    id: i64,
    policy_id: i64,
    organization_id: i64,
    scope: OrganizationBudgetScopeResponse,
    policy_version: i64,
    limit_snapshot: i64,
    used_quota: i64,
    frozen_quota: i64,
    remaining_quota: i64,
    window_starts_at: i64,
    window_ends_at: i64,
    version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationBudgetOverviewResponse {
    policies: Vec<OrganizationBudgetPolicyResponse>,
    windows: Vec<OrganizationBudgetWindowResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationBudgetAllocationResponse {
    policy_id: i64,
    window_id: i64,
    policy_version: i64,
    window_version: i64,
    reserved_quota: i64,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrganizationBudgetReservationStatusResponse {
    Reserved,
    SettlementPending,
    Settled,
    Released,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationBudgetReservationResponse {
    reservation_id: String,
    organization_id: i64,
    membership_id: i64,
    #[schema(required = true)]
    team_id: Option<i64>,
    user_id: i64,
    status: OrganizationBudgetReservationStatusResponse,
    reserved_quota: i64,
    #[schema(required = true)]
    actual_quota: Option<i64>,
    #[schema(required = true)]
    organization: Option<OrganizationBudgetAllocationResponse>,
    #[schema(required = true)]
    team: Option<OrganizationBudgetAllocationResponse>,
    #[schema(required = true)]
    member: Option<OrganizationBudgetAllocationResponse>,
}

async fn get_organization_budgets(
    State(state): State<OrganizationBudgetHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
) -> Result<Response, ManagementError> {
    let context = authorize(&state, &authentication, organization_id).await?;
    let policies = state
        .budget_service
        .list_policies(context.clone())
        .await
        .map_err(map_budget_error)?;
    let windows = state
        .budget_service
        .list_windows(context)
        .await
        .map_err(map_budget_error)?;
    let policies = policies
        .iter()
        .map(OrganizationBudgetPolicyResponse::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let windows = windows
        .iter()
        .map(OrganizationBudgetWindowResponse::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_status_json(
        StatusCode::OK,
        OrganizationBudgetOverviewResponse { policies, windows },
    ))
}

async fn create_organization_budget_policy(
    State(state): State<OrganizationBudgetHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    request: Result<Json<OrganizationBudgetPolicyCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let context = authorize(&state, &authentication, organization_id).await?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = request
        .into_command()
        .map_err(|_| ManagementError::InvalidRequest)?;
    let policy = state
        .budget_service
        .create_policy(context, command)
        .await
        .map_err(map_budget_error)?;
    Ok(no_store_status_json(
        StatusCode::CREATED,
        OrganizationBudgetPolicyResponse::from_record(&policy)?,
    ))
}

async fn get_organization_budget_reservation(
    State(state): State<OrganizationBudgetHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, reservation_id)): Path<(i64, String)>,
) -> Result<Response, ManagementError> {
    let context = authorize(&state, &authentication, organization_id).await?;
    let reservation_id = BillingReservationId::from_persistence_key(&reservation_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let reservation = state
        .budget_service
        .reservation(context, reservation_id)
        .await
        .map_err(map_budget_error)?;
    Ok(no_store_status_json(
        StatusCode::OK,
        OrganizationBudgetReservationResponse::from_record(&reservation),
    ))
}

impl OrganizationBudgetPolicyCreateRequest {
    fn into_command(
        self,
    ) -> Result<OrganizationBudgetPolicyCreateCommand, OrganizationBudgetServiceError> {
        let scope = match self.scope {
            OrganizationBudgetScopeRequest::Organization => OrganizationBudgetScope::Organization,
            OrganizationBudgetScopeRequest::Team { team_id } => OrganizationBudgetScope::Team(
                OrganizationTeamId::new(team_id)
                    .map_err(|_| OrganizationBudgetServiceError::InvalidRequest)?,
            ),
            OrganizationBudgetScopeRequest::Member { membership_id } => {
                OrganizationBudgetScope::Member(
                    OrganizationMembershipId::new(membership_id)
                        .map_err(|_| OrganizationBudgetServiceError::InvalidRequest)?,
                )
            }
        };
        let period = match self.period {
            OrganizationBudgetPeriodRequest::UtcDay => OrganizationBudgetPeriod::UtcDay,
            OrganizationBudgetPeriodRequest::UtcWeek => OrganizationBudgetPeriod::UtcWeek,
            OrganizationBudgetPeriodRequest::UtcMonth => OrganizationBudgetPeriod::UtcMonth,
            OrganizationBudgetPeriodRequest::Fixed { seconds } => {
                OrganizationBudgetPeriod::fixed(seconds)
                    .map_err(|_| OrganizationBudgetServiceError::InvalidRequest)?
            }
        };
        let limit = OrganizationBudgetLimit::new(
            self.quota_limit
                .map(Quota::new)
                .transpose()
                .map_err(|_| OrganizationBudgetServiceError::InvalidRequest)?,
        )
        .map_err(|_| OrganizationBudgetServiceError::InvalidRequest)?;
        OrganizationBudgetPolicyCreateCommand::new(scope, period, limit, self.effective_from)
    }
}

impl OrganizationBudgetPolicyResponse {
    fn from_record(record: &OrganizationBudgetPolicyRecord) -> Result<Self, ManagementError> {
        Ok(Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            scope: OrganizationBudgetScopeResponse::from_scope(record.scope()),
            period: OrganizationBudgetPeriodResponse::from_period(record.period()),
            quota_limit: record.limit().quota().map(Quota::units),
            effective_from: record.effective_from().unix_timestamp(),
            effective_until: record.effective_until().map(|value| value.unix_timestamp()),
            version: record.version(),
        })
    }
}

impl OrganizationBudgetWindowResponse {
    fn from_record(record: &OrganizationBudgetWindowRecord) -> Result<Self, ManagementError> {
        let remaining_quota = record
            .limit_snapshot()
            .units()
            .checked_sub(record.used_quota().units())
            .and_then(|value| value.checked_sub(record.frozen_quota().units()))
            .ok_or(ManagementError::Internal)?;
        Ok(Self {
            id: record.id().get(),
            policy_id: record.policy_id().get(),
            organization_id: record.organization_id().get(),
            scope: OrganizationBudgetScopeResponse::from_scope(record.scope()),
            policy_version: record.policy_version(),
            limit_snapshot: record.limit_snapshot().units(),
            used_quota: record.used_quota().units(),
            frozen_quota: record.frozen_quota().units(),
            remaining_quota,
            window_starts_at: record.window_starts_at().unix_timestamp(),
            window_ends_at: record.window_ends_at().unix_timestamp(),
            version: record.version(),
        })
    }
}

impl OrganizationBudgetScopeResponse {
    fn from_scope(scope: OrganizationBudgetScope) -> Self {
        match scope {
            OrganizationBudgetScope::Organization => Self::Organization,
            OrganizationBudgetScope::Team(team_id) => Self::Team {
                team_id: team_id.get(),
            },
            OrganizationBudgetScope::Member(membership_id) => Self::Member {
                membership_id: membership_id.get(),
            },
        }
    }
}

impl OrganizationBudgetPeriodResponse {
    fn from_period(period: OrganizationBudgetPeriod) -> Self {
        match period {
            OrganizationBudgetPeriod::UtcDay => Self::UtcDay,
            OrganizationBudgetPeriod::UtcWeek => Self::UtcWeek,
            OrganizationBudgetPeriod::UtcMonth => Self::UtcMonth,
            OrganizationBudgetPeriod::Fixed { seconds } => Self::Fixed { seconds },
        }
    }
}

impl OrganizationBudgetReservationResponse {
    fn from_record(record: &OrganizationBillingReservationRecord) -> Self {
        let principal = record.principal();
        Self {
            reservation_id: record.id().persistence_key(),
            organization_id: principal.organization_id().get(),
            membership_id: principal.membership_id().get(),
            team_id: principal.team_id().map(|value| value.get()),
            user_id: principal.user_id().get(),
            status: match record.status() {
                OrganizationBillingReservationStatus::Reserved => {
                    OrganizationBudgetReservationStatusResponse::Reserved
                }
                OrganizationBillingReservationStatus::SettlementPending => {
                    OrganizationBudgetReservationStatusResponse::SettlementPending
                }
                OrganizationBillingReservationStatus::Settled => {
                    OrganizationBudgetReservationStatusResponse::Settled
                }
                OrganizationBillingReservationStatus::Released => {
                    OrganizationBudgetReservationStatusResponse::Released
                }
            },
            reserved_quota: record.reserved_quota().units(),
            actual_quota: record.actual_quota().map(Quota::units),
            organization: record
                .organization_allocation()
                .map(OrganizationBudgetAllocationResponse::from_record),
            team: record
                .team_allocation()
                .map(OrganizationBudgetAllocationResponse::from_record),
            member: record
                .member_allocation()
                .map(OrganizationBudgetAllocationResponse::from_record),
        }
    }
}

impl OrganizationBudgetAllocationResponse {
    fn from_record(record: OrganizationBudgetAllocationRecord) -> Self {
        Self {
            policy_id: record.policy_id().get(),
            window_id: record.window_id().get(),
            policy_version: record.policy_version(),
            window_version: record.window_version(),
            reserved_quota: record.reserved_quota().units(),
        }
    }
}

async fn authorize(
    state: &OrganizationBudgetHttpState,
    authentication: &SessionAuthentication,
    organization_id: i64,
) -> Result<OrganizationContext, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    state
        .authorization_service
        .context(
            authentication.principal(),
            organization_id,
            OrganizationPermission::BudgetsManage,
        )
        .await
        .map_err(map_authorization_error)
}

fn map_authorization_error(error: OrganizationAuthorizationError) -> ManagementError {
    match error {
        OrganizationAuthorizationError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
        OrganizationAuthorizationError::Internal => ManagementError::Internal,
    }
}

fn map_budget_error(error: OrganizationBudgetServiceError) -> ManagementError {
    match error {
        OrganizationBudgetServiceError::InvalidRequest => ManagementError::InvalidRequest,
        OrganizationBudgetServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationBudgetServiceError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationBudgetServiceError::Conflict => ManagementError::BudgetConflict,
        OrganizationBudgetServiceError::OutcomeUnknown => ManagementError::BudgetOutcomeUnknown,
        OrganizationBudgetServiceError::Internal => ManagementError::Internal,
    }
}

fn no_store_status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
