use std::sync::Arc;

use af_admin::{
    OrganizationActivityService, OrganizationApprovalDecisionCommand,
    OrganizationApprovalDecisionRecord, OrganizationApprovalRequestCreateCommand,
    OrganizationApprovalRequestView, OrganizationApprovalService, OrganizationApprovalServiceError,
    OrganizationApprovalTemplateCreateCommand, OrganizationApprovalTemplateRecord,
    OrganizationAuditOutcome, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationContext, SessionAuthentication,
};
use af_domain::{
    OrganizationApprovalDecisionKind, OrganizationApprovalRequestId,
    OrganizationApprovalResourceType, OrganizationId,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    middleware::ensure_request_id,
    organization_activity::{audit_organization_action_result, authorize_organization_action},
};

const DEFAULT_PAGE_SIZE: u64 = 50;
const MAX_PAGE_SIZE: u64 = 100;

#[derive(Clone)]
struct OrganizationApprovalHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    approval_service: Arc<dyn OrganizationApprovalService>,
    activity_service: Arc<dyn OrganizationActivityService>,
}

/// 构建企业审批模板、申请与决定管理接口。
pub fn build_organization_approval_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    approval_service: Arc<dyn OrganizationApprovalService>,
    activity_service: Arc<dyn OrganizationActivityService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/approvals/templates",
            get(list_templates).post(create_template),
        )
        .route(
            "/api/organizations/{organization_id}/approvals/requests",
            get(list_requests).post(create_request),
        )
        .route(
            "/api/organizations/{organization_id}/approvals/requests/{request_id}",
            get(get_request),
        )
        .route(
            "/api/organizations/{organization_id}/approvals/requests/{request_id}/decision",
            post(decide_request),
        )
        .layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(OrganizationApprovalHttpState {
            authorization_service,
            approval_service,
            activity_service,
        })
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct CursorQuery {
    after: Option<i64>,
    limit: u64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct TemplateCreateRequest {
    template_key: String,
    resource_type: ResourceTypeRequest,
    definition_digest: String,
    expected_version: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum ResourceTypeRequest {
    Member,
    Budget,
    ServiceAccount,
    ContractPrice,
    Funds,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct RequestCreateRequest {
    request_key: String,
    resource_type: ResourceTypeRequest,
    resource_summary: String,
    template_id: i64,
    template_version: i64,
    expires_at: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct DecisionRequest {
    decision_key: String,
    kind: DecisionKindRequest,
    reason: Option<String>,
    decided_at: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum DecisionKindRequest {
    Approved,
    Rejected,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct TemplateResponse {
    id: i64,
    organization_id: i64,
    template_key: String,
    resource_type: &'static str,
    version: i64,
    definition_digest: String,
    created_by_user_id: i64,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct TemplateListResponse {
    templates: Vec<TemplateResponse>,
    next_after: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct DecisionResponse {
    id: i64,
    decision_key: String,
    kind: &'static str,
    decided_by_user_id: Option<i64>,
    reason: Option<String>,
    decided_at: i64,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct RequestResponse {
    id: i64,
    request_key: String,
    organization_id: i64,
    resource_type: &'static str,
    resource_summary: String,
    template_id: i64,
    template_version: i64,
    requested_by_user_id: i64,
    expires_at: i64,
    created_at: i64,
    status: &'static str,
    decision: Option<DecisionResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct RequestListResponse {
    requests: Vec<RequestResponse>,
    next_after: Option<i64>,
}

async fn list_templates(
    State(state): State<OrganizationApprovalHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let query = parse_cursor_query(raw_query.as_deref())?;
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let templates = state
        .approval_service
        .list_templates(context, query.after, query.limit)
        .await
        .map_err(map_approval_error)?;
    let next_after = next_after(
        templates.last().map(|template| template.id().get()),
        query.limit,
        templates.len(),
    );
    let templates = templates
        .iter()
        .map(TemplateResponse::from_record)
        .collect::<Vec<_>>();
    Ok(no_store_json(TemplateListResponse {
        templates,
        next_after,
    }))
}

async fn create_template(
    State(state): State<OrganizationApprovalHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<TemplateCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        af_domain::OrganizationPermission::ApprovalsManage,
        &request_id,
        &headers,
        "organization.approval.template.create",
        "approval_template",
        "new".to_owned(),
    )
    .await?;
    let Json(request) = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsManage,
        "organization.approval.template.create",
        "approval_template",
        "new".to_owned(),
        request.map_err(|_| ManagementError::InvalidRequest),
    )
    .await?;
    let command = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsManage,
        "organization.approval.template.create",
        "approval_template",
        "new".to_owned(),
        TemplateCreateCommand::try_from(request),
    )
    .await?;
    let template = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsManage,
        "organization.approval.template.create",
        "approval_template",
        "new".to_owned(),
        state
            .approval_service
            .create_template(context.clone(), command)
            .await
            .map_err(map_approval_error),
    )
    .await?;
    record_success(
        &state,
        context,
        &request_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsManage,
        "organization.approval.template.create",
        "approval_template",
        template.id().get().to_string(),
    )
    .await?;
    Ok(no_store_status_json(
        StatusCode::CREATED,
        TemplateResponse::from_record(&template),
    ))
}

async fn list_requests(
    State(state): State<OrganizationApprovalHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let query = parse_cursor_query(raw_query.as_deref())?;
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let requests = state
        .approval_service
        .list_requests(context, query.after, query.limit)
        .await
        .map_err(map_approval_error)?;
    let next_after = next_after(
        requests.last().map(|value| value.request().id().get()),
        query.limit,
        requests.len(),
    );
    let requests = requests.iter().map(RequestResponse::from_view).collect();
    Ok(no_store_json(RequestListResponse {
        requests,
        next_after,
    }))
}

async fn get_request(
    State(state): State<OrganizationApprovalHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, request_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let request_id = OrganizationApprovalRequestId::new(request_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let request = state
        .approval_service
        .get_request(context, request_id)
        .await
        .map_err(map_approval_error)?;
    Ok(no_store_json(RequestResponse::from_view(&request)))
}

async fn create_request(
    State(state): State<OrganizationApprovalHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(trace_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<RequestCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        af_domain::OrganizationPermission::ApprovalsCreate,
        &trace_id,
        &headers,
        "organization.approval.request.create",
        "approval_request",
        "new".to_owned(),
    )
    .await?;
    let Json(request) = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsCreate,
        "organization.approval.request.create",
        "approval_request",
        "new".to_owned(),
        request.map_err(|_| ManagementError::InvalidRequest),
    )
    .await?;
    let command = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsCreate,
        "organization.approval.request.create",
        "approval_request",
        "new".to_owned(),
        RequestCreateCommand::try_from(request),
    )
    .await?;
    let result = state
        .approval_service
        .create_request(context.clone(), command)
        .await
        .map_err(map_approval_error);
    let request = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsCreate,
        "organization.approval.request.create",
        "approval_request",
        "new".to_owned(),
        result,
    )
    .await?;
    record_success(
        &state,
        context,
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsCreate,
        "organization.approval.request.create",
        "approval_request",
        request.request().id().get().to_string(),
    )
    .await?;
    Ok(no_store_status_json(
        StatusCode::CREATED,
        RequestResponse::from_view(&request),
    ))
}

async fn decide_request(
    State(state): State<OrganizationApprovalHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(trace_id): Extension<RequestId>,
    Path((organization_id, request_id_path)): Path<(i64, i64)>,
    headers: HeaderMap,
    request: Result<Json<DecisionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = parse_organization_id(organization_id)?;
    let request_id = OrganizationApprovalRequestId::new(request_id_path)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        af_domain::OrganizationPermission::ApprovalsDecide,
        &trace_id,
        &headers,
        "organization.approval.request.decide",
        "approval_request",
        request_id_path.to_string(),
    )
    .await?;
    let Json(request) = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsDecide,
        "organization.approval.request.decide",
        "approval_request",
        request_id_path.to_string(),
        request.map_err(|_| ManagementError::InvalidRequest),
    )
    .await?;
    let command = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsDecide,
        "organization.approval.request.decide",
        "approval_request",
        request_id_path.to_string(),
        DecisionCommand::try_from(request),
    )
    .await?;
    let result = state
        .approval_service
        .decide(context.clone(), request_id, command)
        .await
        .map_err(map_approval_error);
    let request = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsDecide,
        "organization.approval.request.decide",
        "approval_request",
        request_id_path.to_string(),
        result,
    )
    .await?;
    record_success(
        &state,
        context,
        &trace_id,
        &headers,
        af_domain::OrganizationPermission::ApprovalsDecide,
        "organization.approval.request.decide",
        "approval_request",
        request_id_path.to_string(),
    )
    .await?;
    Ok(no_store_json(RequestResponse::from_view(&request)))
}

async fn authorize_read(
    state: &OrganizationApprovalHttpState,
    authentication: &SessionAuthentication,
    organization_id: OrganizationId,
) -> Result<OrganizationContext, ManagementError> {
    state
        .authorization_service
        .context(
            authentication.principal(),
            organization_id,
            af_domain::OrganizationPermission::OverviewRead,
        )
        .await
        .map_err(map_authorization_error)
}

async fn record_success(
    state: &OrganizationApprovalHttpState,
    context: OrganizationContext,
    request_id: &RequestId,
    headers: &HeaderMap,
    permission: af_domain::OrganizationPermission,
    operation: &'static str,
    target_type: &'static str,
    target_identifier: String,
) -> Result<(), ManagementError> {
    crate::organization_activity::record_organization_action(
        &state.activity_service,
        context,
        request_id,
        headers,
        permission.code(),
        operation,
        target_type,
        target_identifier,
        OrganizationAuditOutcome::Succeeded,
        None,
    )
    .await
}

fn parse_cursor_query(raw_query: Option<&str>) -> Result<CursorQuery, ManagementError> {
    let mut query = CursorQuery {
        after: None,
        limit: DEFAULT_PAGE_SIZE,
    };
    let Some(raw_query) = raw_query else {
        return Ok(query);
    };
    let mut seen_after = false;
    let mut seen_limit = false;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        match key.as_ref() {
            "after" if !seen_after => {
                seen_after = true;
                query.after = Some(value.parse().map_err(|_| ManagementError::InvalidRequest)?);
            }
            "limit" if !seen_limit => {
                seen_limit = true;
                let limit = value.parse().map_err(|_| ManagementError::InvalidRequest)?;
                if !(1..=MAX_PAGE_SIZE).contains(&limit) {
                    return Err(ManagementError::InvalidRequest);
                }
                query.limit = limit;
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    if query.after.is_some_and(|value| value <= 0) {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(query)
}

fn next_after(last: Option<i64>, limit: u64, length: usize) -> Option<i64> {
    (length == limit as usize).then_some(last).flatten()
}

impl TryFrom<TemplateCreateRequest> for TemplateCreateCommand {
    type Error = ManagementError;
    fn try_from(value: TemplateCreateRequest) -> Result<Self, Self::Error> {
        OrganizationApprovalTemplateCreateCommand::new(
            value.template_key,
            value.resource_type.into_domain(),
            value.definition_digest,
            value.expected_version,
        )
        .map_err(|_| ManagementError::InvalidRequest)
    }
}

type TemplateCreateCommand = OrganizationApprovalTemplateCreateCommand;
type RequestCreateCommand = OrganizationApprovalRequestCreateCommand;
type DecisionCommand = OrganizationApprovalDecisionCommand;

impl TryFrom<RequestCreateRequest> for RequestCreateCommand {
    type Error = ManagementError;
    fn try_from(value: RequestCreateRequest) -> Result<Self, Self::Error> {
        RequestCreateCommand::new(
            value.request_key,
            value.resource_type.into_domain(),
            value.resource_summary,
            value.template_id,
            value.template_version,
            value.expires_at,
        )
        .map_err(|_| ManagementError::InvalidRequest)
    }
}

impl TryFrom<DecisionRequest> for DecisionCommand {
    type Error = ManagementError;
    fn try_from(value: DecisionRequest) -> Result<Self, Self::Error> {
        DecisionCommand::new(
            value.decision_key,
            value.kind.into_domain(),
            value.reason,
            value.decided_at,
        )
        .map_err(|_| ManagementError::InvalidRequest)
    }
}

impl ResourceTypeRequest {
    fn into_domain(self) -> OrganizationApprovalResourceType {
        match self {
            Self::Member => OrganizationApprovalResourceType::Member,
            Self::Budget => OrganizationApprovalResourceType::Budget,
            Self::ServiceAccount => OrganizationApprovalResourceType::ServiceAccount,
            Self::ContractPrice => OrganizationApprovalResourceType::ContractPrice,
            Self::Funds => OrganizationApprovalResourceType::Funds,
        }
    }
}

impl DecisionKindRequest {
    fn into_domain(self) -> OrganizationApprovalDecisionKind {
        match self {
            Self::Approved => OrganizationApprovalDecisionKind::Approved,
            Self::Rejected => OrganizationApprovalDecisionKind::Rejected,
        }
    }
}

impl TemplateResponse {
    fn from_record(record: &OrganizationApprovalTemplateRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            template_key: record.template_key().as_str().to_owned(),
            resource_type: resource_type_code(record.resource_type()),
            version: record.version(),
            definition_digest: record.definition_digest().persistence_key(),
            created_by_user_id: record.created_by_user_id().get(),
            created_at: record.created_at().unix_timestamp(),
        }
    }
}

impl RequestResponse {
    fn from_view(view: &OrganizationApprovalRequestView) -> Self {
        let request = view.request();
        Self {
            id: request.id().get(),
            request_key: request.request_key().persistence_key(),
            organization_id: request.organization_id().get(),
            resource_type: resource_type_code(request.resource_type()),
            resource_summary: request.resource_summary().to_owned(),
            template_id: request.template_id().get(),
            template_version: request.template_version(),
            requested_by_user_id: request.requested_by_user_id().get(),
            expires_at: request.expires_at(),
            created_at: request.created_at().unix_timestamp(),
            status: view.status().code(),
            decision: view.decision().map(DecisionResponse::from_record),
        }
    }
}

impl DecisionResponse {
    fn from_record(record: &OrganizationApprovalDecisionRecord) -> Self {
        Self {
            id: record.id().get(),
            decision_key: record.decision_key().persistence_key(),
            kind: decision_kind_code(record.kind()),
            decided_by_user_id: record.decided_by_user_id().map(|value| value.get()),
            reason: record.reason().map(str::to_owned),
            decided_at: record.decided_at(),
            created_at: record.created_at().unix_timestamp(),
        }
    }
}

fn resource_type_code(value: OrganizationApprovalResourceType) -> &'static str {
    match value {
        OrganizationApprovalResourceType::Member => "member",
        OrganizationApprovalResourceType::Budget => "budget",
        OrganizationApprovalResourceType::ServiceAccount => "service_account",
        OrganizationApprovalResourceType::ContractPrice => "contract_price",
        OrganizationApprovalResourceType::Funds => "funds",
    }
}

fn decision_kind_code(value: OrganizationApprovalDecisionKind) -> &'static str {
    match value {
        OrganizationApprovalDecisionKind::Approved => "approved",
        OrganizationApprovalDecisionKind::Rejected => "rejected",
        OrganizationApprovalDecisionKind::TimedOut => "timed_out",
    }
}

fn parse_organization_id(value: i64) -> Result<OrganizationId, ManagementError> {
    OrganizationId::new(value).map_err(|_| ManagementError::InvalidRequest)
}

fn map_authorization_error(error: OrganizationAuthorizationError) -> ManagementError {
    match error {
        OrganizationAuthorizationError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
        OrganizationAuthorizationError::Internal => ManagementError::Internal,
    }
}

fn map_approval_error(error: OrganizationApprovalServiceError) -> ManagementError {
    match error {
        OrganizationApprovalServiceError::InvalidRequest => ManagementError::InvalidRequest,
        OrganizationApprovalServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationApprovalServiceError::NotFound => ManagementError::OrganizationApprovalNotFound,
        OrganizationApprovalServiceError::Conflict => ManagementError::OrganizationApprovalConflict,
        OrganizationApprovalServiceError::Expired => ManagementError::OrganizationApprovalExpired,
        OrganizationApprovalServiceError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    no_store_status_json(StatusCode::OK, value)
}

fn no_store_status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
