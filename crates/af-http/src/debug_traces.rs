use std::sync::Arc;

use af_admin::{
    AdminDebugTrace, AdminDebugTraceAttempt, AdminDebugTraceError, AdminDebugTraceListQuery,
    AdminDebugTraceService, AdminDebugTraceSettings, AdminDebugTraceSettingsCommand,
};
use axum::{
    Json, Router,
    extract::{Extension, Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct DebugTraceHttpState {
    service: Arc<dyn AdminDebugTraceService>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDebugTraceSettings)]
pub(crate) struct AdminDebugTraceSettingsResponse {
    enabled: bool,
    #[schema(minimum = 0, maximum = 1_000_000)]
    sample_per_million: i64,
    #[schema(minimum = 1, maximum = 720)]
    retention_hours: i32,
    #[schema(minimum = 1)]
    version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDebugTraceSettingsRequest)]
pub(crate) struct AdminDebugTraceSettingsRequest {
    enabled: bool,
    #[schema(minimum = 0, maximum = 1_000_000)]
    sample_per_million: i64,
    #[schema(minimum = 1, maximum = 720)]
    retention_hours: i32,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDebugTrace)]
pub(crate) struct AdminDebugTraceResponse {
    id: i64,
    request_id: String,
    user_id: i64,
    token_id: i64,
    group_id: i64,
    requested_model: String,
    downstream_protocol: String,
    upstream_protocol: String,
    operation: String,
    outcome: String,
    selected_channel_id: Option<i64>,
    selected_credential_id: Option<i64>,
    routing_elapsed_ms: i64,
    attempt_count: i32,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDebugTraceAttempt)]
pub(crate) struct AdminDebugTraceAttemptResponse {
    candidate_index: i16,
    channel_id: i64,
    credential_id: i64,
    outcome: String,
    failure_kind: Option<String>,
    upstream_status: Option<i16>,
    retry_decision: bool,
    elapsed_ms: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDebugTraceListResponse)]
pub(crate) struct AdminDebugTraceListResponse {
    #[schema(max_items = 100)]
    traces: Vec<AdminDebugTraceResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminDebugTraceDetailResponse)]
pub(crate) struct AdminDebugTraceDetailResponse {
    trace: AdminDebugTraceResponse,
    attempts: Vec<AdminDebugTraceAttemptResponse>,
}

/// 构建仅管理员可访问的调试追踪设置与时间线路由。
pub(crate) fn build_debug_trace_router(
    service: Arc<dyn AdminDebugTraceService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/debug-trace-settings",
            get(get_settings).put(update_settings),
        )
        .route("/api/admin/debug-traces", get(list_traces))
        .route("/api/admin/debug-traces/{id}", get(get_trace))
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(DebugTraceHttpState { service })
}

async fn get_settings(
    State(state): State<DebugTraceHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let settings = state
        .service
        .settings(authentication.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminDebugTraceSettingsResponse::from(
        settings,
    )))
}

async fn update_settings(
    State(state): State<DebugTraceHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminDebugTraceSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = AdminDebugTraceSettingsCommand::new(
        request.enabled,
        request.sample_per_million,
        request.retention_hours,
    )
    .map_err(map_error)?;
    let settings = state
        .service
        .update(authentication.principal(), command)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminDebugTraceSettingsResponse::from(
        settings,
    )))
}

async fn list_traces(
    State(state): State<DebugTraceHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let page = state
        .service
        .list(authentication.principal(), query)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminDebugTraceListResponse::from_page(page)))
}

async fn get_trace(
    State(state): State<DebugTraceHttpState>,
    Path(id): Path<i64>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let detail = state
        .service
        .detail(authentication.principal(), id)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminDebugTraceDetailResponse::from_detail(
        detail,
    )))
}

impl From<AdminDebugTraceSettings> for AdminDebugTraceSettingsResponse {
    fn from(settings: AdminDebugTraceSettings) -> Self {
        Self {
            enabled: settings.enabled(),
            sample_per_million: settings.sample_per_million(),
            retention_hours: settings.retention_hours(),
            version: settings.version(),
        }
    }
}

impl AdminDebugTraceListResponse {
    fn from_page(page: af_admin::AdminDebugTracePage) -> Self {
        Self {
            traces: page
                .traces()
                .iter()
                .map(AdminDebugTraceResponse::from_trace)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl AdminDebugTraceDetailResponse {
    fn from_detail(detail: af_admin::AdminDebugTraceDetail) -> Self {
        Self {
            trace: AdminDebugTraceResponse::from_trace(detail.trace()),
            attempts: detail
                .attempts()
                .iter()
                .map(AdminDebugTraceAttemptResponse::from_attempt)
                .collect(),
        }
    }
}

impl AdminDebugTraceResponse {
    fn from_trace(trace: &AdminDebugTrace) -> Self {
        Self {
            id: trace.id(),
            request_id: trace.request_id().to_owned(),
            user_id: trace.user_id().get(),
            token_id: trace.token_id().get(),
            group_id: trace.group_id().get(),
            requested_model: trace.requested_model().to_owned(),
            downstream_protocol: protocol_name(trace.downstream_protocol()),
            upstream_protocol: protocol_name(trace.upstream_protocol()),
            operation: operation_name(trace.operation()),
            outcome: outcome_name(trace.outcome()),
            selected_channel_id: trace.selected_channel_id().map(|id| id.get()),
            selected_credential_id: trace.selected_credential_id().map(|id| id.get()),
            routing_elapsed_ms: trace.routing_elapsed_ms(),
            attempt_count: trace.attempt_count(),
            created_at: trace.created_at(),
        }
    }
}

impl AdminDebugTraceAttemptResponse {
    fn from_attempt(attempt: &AdminDebugTraceAttempt) -> Self {
        Self {
            candidate_index: attempt.candidate_index(),
            channel_id: attempt.channel_id().get(),
            credential_id: attempt.credential_id().get(),
            outcome: attempt_outcome_name(attempt.outcome()),
            failure_kind: attempt.failure_kind().map(failure_kind_name),
            upstream_status: attempt.upstream_status(),
            retry_decision: attempt.retry_decision(),
            elapsed_ms: attempt.elapsed_ms(),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminDebugTraceListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminDebugTraceListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminDebugTraceListQuery::default());
    }
    validate_percent_encoding(raw_query)?;
    let mut before = None;
    let mut limit = None;
    let mut outcome = None;
    let mut model = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "before" if before.is_none() => before = Some(parse_positive_i64(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            "outcome" if outcome.is_none() => outcome = Some(parse_outcome(&value)?),
            "model" if model.is_none() => model = Some(value.into_owned()),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminDebugTraceListQuery::new(
        before,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_DEBUG_TRACE_PAGE_SIZE),
        outcome,
        model,
    )
    .map_err(map_error)
}

fn parse_outcome(value: &str) -> Result<af_admin::AdminDebugTraceOutcome, ManagementError> {
    match value {
        "succeeded" => Ok(af_admin::AdminDebugTraceOutcome::Succeeded),
        "failed" => Ok(af_admin::AdminDebugTraceOutcome::Failed),
        _ => Err(ManagementError::InvalidRequest),
    }
}

fn parse_positive_i64(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty()
        || value.starts_with(['+', '-'])
        || value.chars().any(|c| !c.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    let parsed = value
        .parse::<i64>()
        .map_err(|_| ManagementError::InvalidRequest)?;
    (parsed > 0)
        .then_some(parsed)
        .ok_or(ManagementError::InvalidRequest)
}

fn parse_limit(value: &str) -> Result<usize, ManagementError> {
    parse_positive_i64(value)
        .and_then(|value| usize::try_from(value).map_err(|_| ManagementError::InvalidRequest))
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn protocol_name(protocol: af_admin::AdminDebugTraceProtocol) -> String {
    match protocol {
        af_admin::AdminDebugTraceProtocol::OpenAiChat => "openai_chat",
        af_admin::AdminDebugTraceProtocol::OpenAiResponses => "openai_responses",
        af_admin::AdminDebugTraceProtocol::Anthropic => "anthropic",
        af_admin::AdminDebugTraceProtocol::Gemini => "gemini",
    }
    .to_owned()
}
fn operation_name(operation: af_admin::AdminDebugTraceOperation) -> String {
    match operation {
        af_admin::AdminDebugTraceOperation::Chat => "chat",
        af_admin::AdminDebugTraceOperation::Responses => "responses",
    }
    .to_owned()
}
fn outcome_name(outcome: af_admin::AdminDebugTraceOutcome) -> String {
    match outcome {
        af_admin::AdminDebugTraceOutcome::Succeeded => "succeeded",
        af_admin::AdminDebugTraceOutcome::Failed => "failed",
    }
    .to_owned()
}
fn attempt_outcome_name(outcome: af_admin::AdminDebugTraceAttemptOutcome) -> String {
    match outcome {
        af_admin::AdminDebugTraceAttemptOutcome::Succeeded => "succeeded",
        af_admin::AdminDebugTraceAttemptOutcome::Failed => "failed",
    }
    .to_owned()
}
fn failure_kind_name(kind: af_admin::AdminDebugTraceFailureKind) -> String {
    match kind {
        af_admin::AdminDebugTraceFailureKind::AuthExpired => "auth_expired",
        af_admin::AdminDebugTraceFailureKind::AuthRevoked => "auth_revoked",
        af_admin::AdminDebugTraceFailureKind::AccountDisabled => "account_disabled",
        af_admin::AdminDebugTraceFailureKind::RateLimited => "rate_limited",
        af_admin::AdminDebugTraceFailureKind::Overloaded => "overloaded",
        af_admin::AdminDebugTraceFailureKind::QuotaExhausted => "quota_exhausted",
        af_admin::AdminDebugTraceFailureKind::ModelUnsupported => "model_unsupported",
        af_admin::AdminDebugTraceFailureKind::ProtocolError => "protocol_error",
        af_admin::AdminDebugTraceFailureKind::ServerError => "server_error",
        af_admin::AdminDebugTraceFailureKind::BadRequest => "bad_request",
        af_admin::AdminDebugTraceFailureKind::Network => "network",
    }
    .to_owned()
}

fn map_error(error: AdminDebugTraceError) -> ManagementError {
    match error {
        AdminDebugTraceError::InvalidInput => ManagementError::InvalidRequest,
        AdminDebugTraceError::Forbidden => ManagementError::Forbidden,
        AdminDebugTraceError::NotFound => ManagementError::DebugTraceNotFound,
        AdminDebugTraceError::Internal => ManagementError::Internal,
    }
}
