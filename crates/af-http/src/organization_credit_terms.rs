use std::sync::Arc;

use af_admin::{
    OrganizationActivityService, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationContext, OrganizationCreditInvoiceRecord,
    OrganizationCreditRepaymentAllocationRecord, OrganizationCreditRepaymentRecord,
    OrganizationCreditTermsCreateCommand, OrganizationCreditTermsRecord,
    OrganizationCreditTermsService, OrganizationCreditTermsServiceError,
    OrganizationCreditTermsStatusCommand, SessionAuthentication, SessionAuthenticator,
};
use af_domain::{
    OrganizationCreditCurrency, OrganizationCreditInvoiceId, OrganizationCreditInvoiceStatus,
    OrganizationCreditLimit, OrganizationCreditRepaymentAllocationId,
    OrganizationCreditRepaymentId, OrganizationCreditRepaymentSourceKind, OrganizationCreditTermId,
    OrganizationCreditTermPeriod, OrganizationCreditTermStatus, OrganizationId,
    OrganizationPermission,
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
use serde_json::json;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    middleware::ensure_request_id,
    organization_activity::{
        audit_organization_action_result, authorize_organization_action, record_organization_action,
    },
};

#[derive(Clone)]
struct OrganizationCreditTermsHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    service: Arc<dyn OrganizationCreditTermsService>,
    activity_service: Arc<dyn OrganizationActivityService>,
}

pub fn build_organization_credit_terms_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    service: Arc<dyn OrganizationCreditTermsService>,
    activity_service: Arc<dyn OrganizationActivityService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/credit-terms",
            get(list_credit_terms).post(create_credit_term),
        )
        .route(
            "/api/organizations/{organization_id}/credit-terms/{term_id}",
            get(get_credit_term),
        )
        .route(
            "/api/organizations/{organization_id}/credit-terms/{term_id}/status",
            post(update_credit_term_status),
        )
        .route(
            "/api/organizations/{organization_id}/credit-invoices",
            get(list_credit_invoices),
        )
        .route(
            "/api/organizations/{organization_id}/credit-invoices/{invoice_id}",
            get(get_credit_invoice),
        )
        .route(
            "/api/organizations/{organization_id}/credit-repayments",
            get(list_credit_repayments),
        )
        .route(
            "/api/organizations/{organization_id}/credit-repayments/{repayment_id}",
            get(get_credit_repayment),
        )
        .route(
            "/api/organizations/{organization_id}/credit-allocations",
            get(list_credit_allocations),
        )
        .route(
            "/api/organizations/{organization_id}/credit-allocations/{allocation_id}",
            get(get_credit_allocation),
        )
        .route_layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(OrganizationCreditTermsHttpState {
            authorization_service,
            service,
            activity_service,
        })
}

#[derive(Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum CreditTermPeriodRequest {
    NetDays { days: u16 },
    EndOfMonth,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditTermCreateRequest {
    currency: String,
    #[schema(minimum = 0)]
    credit_limit: i64,
    period: CreditTermPeriodRequest,
    #[schema(minimum = 0)]
    effective_from: i64,
    #[schema(minimum = 0, required = true)]
    effective_until: Option<i64>,
    #[schema(minimum = 0, required = true)]
    expected_version: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditTermStatusRequest {
    status: String,
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditTermResponse {
    id: i64,
    organization_id: i64,
    currency: String,
    credit_limit: i64,
    period: CreditTermPeriodResponse,
    status: CreditTermStatusResponse,
    effective_from: i64,
    #[schema(required = true)]
    effective_until: Option<i64>,
    version: i64,
    created_by_user_id: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditTermListResponse {
    #[schema(max_items = 100)]
    terms: Vec<CreditTermResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditInvoiceListResponse {
    #[schema(max_items = 100)]
    invoices: Vec<CreditInvoiceResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditInvoiceResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    organization_id: i64,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    currency: String,
    #[schema(minimum = 0)]
    amount_minor: i64,
    #[schema(minimum = 1)]
    term_version: i64,
    period: CreditTermPeriodResponse,
    #[schema(minimum = 0)]
    issued_at: i64,
    #[schema(minimum = 0)]
    due_at: i64,
    status: CreditInvoiceStatusResponse,
    #[schema(minimum = 0)]
    status_changed_at: i64,
    #[schema(minimum = 0, required = true)]
    settled_at: Option<i64>,
    #[schema(minimum = 0, required = true)]
    voided_at: Option<i64>,
    #[schema(minimum = 0)]
    created_at: i64,
    #[schema(minimum = 0)]
    updated_at: i64,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CreditInvoiceStatusResponse {
    Due,
    Overdue,
    Settled,
    Voided,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditRepaymentListResponse {
    #[schema(max_items = 100)]
    repayments: Vec<CreditRepaymentResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditRepaymentResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    organization_id: i64,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    currency: String,
    #[schema(minimum = 1)]
    amount_minor: i64,
    source_kind: CreditRepaymentSourceKindResponse,
    #[schema(minimum = 0)]
    received_at: i64,
    #[schema(minimum = 0)]
    created_at: i64,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CreditRepaymentSourceKindResponse {
    BankTransfer,
    OtherConfirmed,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditAllocationListResponse {
    #[schema(max_items = 100)]
    allocations: Vec<CreditAllocationResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreditAllocationResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    organization_id: i64,
    #[schema(minimum = 1)]
    repayment_id: i64,
    #[schema(minimum = 1)]
    invoice_id: i64,
    #[schema(min_length = 3, max_length = 3, pattern = "^[A-Z]{3}$")]
    currency: String,
    #[schema(minimum = 1)]
    amount_minor: i64,
    #[schema(minimum = 0)]
    allocated_at: i64,
    invoice_settled: bool,
    #[schema(minimum = 0)]
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum CreditTermPeriodResponse {
    NetDays { days: u16 },
    EndOfMonth,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CreditTermStatusResponse {
    Active,
    Paused,
    Closed,
}

async fn list_credit_terms(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let query = parse_credit_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_page(context, query.before, query.limit)
        .await
        .map_err(map_error)?;
    let (records, next_cursor) = page.into_parts();
    let terms = records
        .iter()
        .map(CreditTermResponse::from_record)
        .collect();
    Ok(no_store_json(
        StatusCode::OK,
        CreditTermListResponse { terms, next_cursor },
    ))
}

async fn list_credit_invoices(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let query = parse_credit_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_invoices(context, query.before, query.limit)
        .await
        .map_err(map_error)?;
    let (records, next_cursor) = page.into_parts();
    let invoices = records
        .iter()
        .map(CreditInvoiceResponse::from_record)
        .collect();
    Ok(no_store_json(
        StatusCode::OK,
        CreditInvoiceListResponse {
            invoices,
            next_cursor,
        },
    ))
}

async fn get_credit_invoice(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, invoice_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let invoice_id = OrganizationCreditInvoiceId::new(invoice_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let record = state
        .service
        .get_invoice(context, invoice_id)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        CreditInvoiceResponse::from_record(&record),
    ))
}

async fn list_credit_repayments(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let query = parse_credit_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_repayments(context, query.before, query.limit)
        .await
        .map_err(map_error)?;
    let (records, next_cursor) = page.into_parts();
    let repayments = records
        .iter()
        .map(CreditRepaymentResponse::from_record)
        .collect();
    Ok(no_store_json(
        StatusCode::OK,
        CreditRepaymentListResponse {
            repayments,
            next_cursor,
        },
    ))
}

async fn get_credit_repayment(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, repayment_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let repayment_id = OrganizationCreditRepaymentId::new(repayment_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let record = state
        .service
        .get_repayment(context, repayment_id)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        CreditRepaymentResponse::from_record(&record),
    ))
}

async fn list_credit_allocations(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let query = parse_credit_query(raw_query.as_deref())?;
    let page = state
        .service
        .list_allocations(context, query.before, query.limit)
        .await
        .map_err(map_error)?;
    let (records, next_cursor) = page.into_parts();
    let allocations = records
        .iter()
        .map(CreditAllocationResponse::from_record)
        .collect();
    Ok(no_store_json(
        StatusCode::OK,
        CreditAllocationListResponse {
            allocations,
            next_cursor,
        },
    ))
}

async fn get_credit_allocation(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, allocation_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let allocation_id = OrganizationCreditRepaymentAllocationId::new(allocation_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let record = state
        .service
        .get_allocation(context, allocation_id)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        CreditAllocationResponse::from_record(&record),
    ))
}

async fn get_credit_term(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, term_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let context = authorize_read(&state, &authentication, organization_id).await?;
    let term_id =
        OrganizationCreditTermId::new(term_id).map_err(|_| ManagementError::InvalidRequest)?;
    let record = state
        .service
        .get(context, term_id)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(
        StatusCode::OK,
        CreditTermResponse::from_record(&record),
    ))
}

async fn create_credit_term(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<CreditTermCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::CreditManage,
        &request_id,
        &headers,
        "organization.credit.create",
        "credit_term",
        "new".to_owned(),
    )
    .await?;
    let Json(request) = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::CreditManage,
        "organization.credit.create",
        "credit_term",
        "new".to_owned(),
        request.map_err(|_| ManagementError::InvalidRequest),
    )
    .await?;
    let command = request
        .into_command()
        .map_err(|_| ManagementError::InvalidRequest)?;
    let record = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::CreditManage,
        "organization.credit.create",
        "credit_term",
        "new".to_owned(),
        state
            .service
            .create(context.clone(), command)
            .await
            .map_err(map_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::CreditManage.code(),
        "organization.credit.create",
        "credit_term",
        record.id().get().to_string(),
        af_admin::OrganizationAuditOutcome::Succeeded,
        Some(json!({"version": record.version()})),
    )
    .await?;
    Ok(no_store_json(
        StatusCode::CREATED,
        CreditTermResponse::from_record(&record),
    ))
}

async fn update_credit_term_status(
    State(state): State<OrganizationCreditTermsHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, term_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    request: Result<Json<CreditTermStatusRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    let term_id =
        OrganizationCreditTermId::new(term_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::CreditManage,
        &request_id,
        &headers,
        "organization.credit.status",
        "credit_term",
        term_id.get().to_string(),
    )
    .await?;
    let Json(request) = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::CreditManage,
        "organization.credit.status",
        "credit_term",
        term_id.get().to_string(),
        request.map_err(|_| ManagementError::InvalidRequest),
    )
    .await?;
    let command = request
        .into_command(term_id)
        .map_err(|_| ManagementError::InvalidRequest)?;
    let record = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::CreditManage,
        "organization.credit.status",
        "credit_term",
        term_id.get().to_string(),
        state
            .service
            .update_status(context.clone(), command)
            .await
            .map_err(map_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::CreditManage.code(),
        "organization.credit.status",
        "credit_term",
        term_id.get().to_string(),
        af_admin::OrganizationAuditOutcome::Succeeded,
        Some(json!({"status": status_code(record.status()), "version": record.version()})),
    )
    .await?;
    Ok(no_store_json(
        StatusCode::OK,
        CreditTermResponse::from_record(&record),
    ))
}

impl CreditTermCreateRequest {
    fn into_command(self) -> Result<OrganizationCreditTermsCreateCommand, ()> {
        let currency = OrganizationCreditCurrency::new(self.currency).map_err(|_| ())?;
        let limit = OrganizationCreditLimit::new(self.credit_limit).map_err(|_| ())?;
        let period = match self.period {
            CreditTermPeriodRequest::NetDays { days } => {
                OrganizationCreditTermPeriod::net_days(days).map_err(|_| ())?
            }
            CreditTermPeriodRequest::EndOfMonth => OrganizationCreditTermPeriod::EndOfMonth,
        };
        OrganizationCreditTermsCreateCommand::new(
            currency,
            limit,
            period,
            self.effective_from,
            self.effective_until,
            self.expected_version,
        )
        .map_err(|_| ())
    }
}

impl CreditTermStatusRequest {
    fn into_command(
        self,
        term_id: OrganizationCreditTermId,
    ) -> Result<OrganizationCreditTermsStatusCommand, ()> {
        let status = match self.status.as_str() {
            "active" => OrganizationCreditTermStatus::Active,
            "paused" => OrganizationCreditTermStatus::Paused,
            "closed" => OrganizationCreditTermStatus::Closed,
            _ => return Err(()),
        };
        OrganizationCreditTermsStatusCommand::new(term_id, status, self.expected_version)
            .map_err(|_| ())
    }
}

impl CreditTermResponse {
    fn from_record(record: &OrganizationCreditTermsRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            currency: record.currency().as_str().to_owned(),
            credit_limit: record.credit_limit().units(),
            period: match record.period() {
                OrganizationCreditTermPeriod::NetDays(days) => {
                    CreditTermPeriodResponse::NetDays { days }
                }
                OrganizationCreditTermPeriod::EndOfMonth => CreditTermPeriodResponse::EndOfMonth,
            },
            status: match record.status() {
                OrganizationCreditTermStatus::Active => CreditTermStatusResponse::Active,
                OrganizationCreditTermStatus::Paused => CreditTermStatusResponse::Paused,
                OrganizationCreditTermStatus::Closed => CreditTermStatusResponse::Closed,
            },
            effective_from: record.effective_from(),
            effective_until: record.effective_until(),
            version: record.version(),
            created_by_user_id: record.created_by_user_id().get(),
            created_at: record.created_at().unix_timestamp(),
            updated_at: record.updated_at().unix_timestamp(),
        }
    }
}

impl CreditInvoiceResponse {
    fn from_record(record: &OrganizationCreditInvoiceRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            currency: record.currency().as_str().to_owned(),
            amount_minor: record.amount().units(),
            term_version: record.term_version(),
            period: match record.period() {
                OrganizationCreditTermPeriod::NetDays(days) => {
                    CreditTermPeriodResponse::NetDays { days }
                }
                OrganizationCreditTermPeriod::EndOfMonth => CreditTermPeriodResponse::EndOfMonth,
            },
            issued_at: record.issued_at(),
            due_at: record.due_at(),
            status: match record.status() {
                OrganizationCreditInvoiceStatus::Due => CreditInvoiceStatusResponse::Due,
                OrganizationCreditInvoiceStatus::Overdue => CreditInvoiceStatusResponse::Overdue,
                OrganizationCreditInvoiceStatus::Settled => CreditInvoiceStatusResponse::Settled,
                OrganizationCreditInvoiceStatus::Voided => CreditInvoiceStatusResponse::Voided,
            },
            status_changed_at: record.status_changed_at(),
            settled_at: record.settled_at(),
            voided_at: record.voided_at(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }
}

impl CreditRepaymentResponse {
    fn from_record(record: &OrganizationCreditRepaymentRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            currency: record.currency().as_str().to_owned(),
            amount_minor: record.amount().units(),
            source_kind: match record.source_kind() {
                OrganizationCreditRepaymentSourceKind::BankTransfer => {
                    CreditRepaymentSourceKindResponse::BankTransfer
                }
                OrganizationCreditRepaymentSourceKind::OtherConfirmed => {
                    CreditRepaymentSourceKindResponse::OtherConfirmed
                }
            },
            received_at: record.received_at(),
            created_at: record.created_at().unix_timestamp(),
        }
    }
}

impl CreditAllocationResponse {
    fn from_record(record: &OrganizationCreditRepaymentAllocationRecord) -> Self {
        Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            repayment_id: record.repayment_id().get(),
            invoice_id: record.invoice_id().get(),
            currency: record.currency().as_str().to_owned(),
            amount_minor: record.amount().units(),
            allocated_at: record.allocated_at(),
            invoice_settled: record.invoice_settled(),
            created_at: record.created_at().unix_timestamp(),
        }
    }
}

fn status_code(status: OrganizationCreditTermStatus) -> &'static str {
    match status {
        OrganizationCreditTermStatus::Active => "active",
        OrganizationCreditTermStatus::Paused => "paused",
        OrganizationCreditTermStatus::Closed => "closed",
    }
}

struct CreditListQuery {
    before: Option<i64>,
    limit: usize,
}

fn parse_credit_query(raw_query: Option<&str>) -> Result<CreditListQuery, ManagementError> {
    let Some(raw_query) = raw_query.filter(|value| !value.is_empty()) else {
        return Ok(CreditListQuery {
            before: None,
            limit: 50,
        });
    };
    crate::management_usage_logs::validate_percent_encoding(raw_query)?;
    let mut before = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "before" if before.is_none() => {
                before = Some(crate::management_usage_logs::parse_positive_i64(&value)?);
            }
            "limit" if limit.is_none() => {
                limit = Some(crate::management_usage_logs::parse_limit(&value)?);
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok(CreditListQuery {
        before,
        limit: limit.unwrap_or(50),
    })
}

async fn authorize_read(
    state: &OrganizationCreditTermsHttpState,
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
            OrganizationPermission::CreditRead,
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

fn map_error(error: OrganizationCreditTermsServiceError) -> ManagementError {
    match error {
        OrganizationCreditTermsServiceError::InvalidRequest => {
            ManagementError::OrganizationCreditInvalidRequest
        }
        OrganizationCreditTermsServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationCreditTermsServiceError::NotFound => {
            ManagementError::OrganizationCreditNotFound
        }
        OrganizationCreditTermsServiceError::IntervalConflict => {
            ManagementError::OrganizationCreditIntervalConflict
        }
        OrganizationCreditTermsServiceError::Conflict => {
            ManagementError::OrganizationCreditConflict
        }
        OrganizationCreditTermsServiceError::InvalidTransition => {
            ManagementError::OrganizationCreditTransitionInvalid
        }
        OrganizationCreditTermsServiceError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
