use std::sync::Arc;

use af_admin::{
    OrganizationVerificationDecisionCommand, OrganizationVerificationDecisionKind,
    OrganizationVerificationListQuery, OrganizationVerificationService,
    OrganizationVerificationServiceError, OrganizationVerificationSubmitCommand,
};
use af_domain::{OrganizationId, OrganizationVerificationCaseId, OrganizationVerificationStatus};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, RawQuery, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    middleware::ensure_request_id,
};

#[derive(Clone)]
struct OrganizationVerificationHttpState {
    service: Option<Arc<dyn OrganizationVerificationService>>,
}

/// 构建企业认证申请人与平台审核接口。
pub fn build_organization_verification_router(
    service: Option<Arc<dyn OrganizationVerificationService>>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/verification",
            get(get_self).post(submit),
        )
        .route("/api/admin/organization-verifications", get(list_admin))
        .route(
            "/api/admin/organization-verifications/{case_id}",
            get(get_admin),
        )
        .route(
            "/api/admin/organization-verifications/{case_id}/decision",
            post(decide),
        )
        .layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(OrganizationVerificationHttpState { service })
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct SubmitRequest {
    #[schema(min_length = 1, max_length = 512)]
    summary: String,
    #[schema(max_items = 20)]
    materials: Vec<MaterialRequest>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct MaterialRequest {
    #[schema(min_length = 1, max_length = 64)]
    kind: String,
    #[schema(min_length = 1, max_length = 256)]
    object_reference: String,
    #[schema(min_length = 1, max_length = 255)]
    file_name: String,
    #[schema(min_length = 1, max_length = 128)]
    content_type: String,
    #[schema(minimum = 0, maximum = 50000000)]
    size_bytes: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct DecisionRequest {
    #[schema(minimum = 1)]
    expected_version: i64,
    decision: DecisionKind,
    #[schema(max_length = 512)]
    reason: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum DecisionKind {
    RequestSupplement,
    Approve,
    Reject,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct VerificationResponse {
    case: CaseResponse,
    materials: Vec<MaterialResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct CaseResponse {
    id: i64,
    organization_id: i64,
    applicant_user_id: i64,
    status: StatusResponse,
    version: i64,
    summary: String,
    reviewer_user_id: Option<i64>,
    review_reason: Option<String>,
    created_at: u64,
    updated_at: u64,
    decided_at: Option<u64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct MaterialResponse {
    id: i64,
    case_id: i64,
    kind: String,
    file_name: String,
    content_type: String,
    size_bytes: i64,
    created_at: u64,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum StatusResponse {
    Submitted,
    InReview,
    NeedsSupplement,
    Approved,
    Rejected,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct ListResponse {
    cases: Vec<VerificationResponse>,
    next_cursor: Option<i64>,
}

async fn submit(
    State(state): State<OrganizationVerificationHttpState>,
    Path(organization_id): Path<i64>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<SubmitRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = OrganizationId::new(organization_id)
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let Json(request) =
        request.map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let command = OrganizationVerificationSubmitCommand {
        summary: request.summary,
        materials: request
            .materials
            .into_iter()
            .map(|material| af_admin::OrganizationVerificationMaterialWrite {
                kind: material.kind,
                object_reference: material.object_reference,
                file_name: material.file_name,
                content_type: material.content_type,
                size_bytes: material.size_bytes,
            })
            .collect(),
    };
    let bundle = service(&state)?
        .submit(authentication.principal(), organization_id, command)
        .await
        .map_err(map_error)?;
    Ok(no_store_status_json(StatusCode::OK, to_response(&bundle)))
}

async fn get_self(
    State(state): State<OrganizationVerificationHttpState>,
    Path(organization_id): Path<i64>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let organization_id = OrganizationId::new(organization_id)
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let bundle = service(&state)?
        .get_self(authentication.principal(), organization_id)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(to_response(&bundle)))
}

async fn list_admin(
    State(state): State<OrganizationVerificationHttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    let (status, before, limit) = parse_query(raw_query.as_deref())?;
    let page = service(&state)?
        .list_admin(
            authentication.principal(),
            OrganizationVerificationListQuery {
                status,
                before,
                limit,
            },
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ListResponse {
        cases: page.bundles().iter().map(to_response).collect(),
        next_cursor: page.next_cursor(),
    }))
}

async fn get_admin(
    State(state): State<OrganizationVerificationHttpState>,
    Path(case_id): Path<i64>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    let case_id = OrganizationVerificationCaseId::new(case_id)
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let bundle = service(&state)?
        .get_admin(
            authentication.principal(),
            case_id,
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(to_response(&bundle)))
}

async fn decide(
    State(state): State<OrganizationVerificationHttpState>,
    Path(case_id): Path<i64>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    request: Result<Json<DecisionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let case_id = OrganizationVerificationCaseId::new(case_id)
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let Json(request) =
        request.map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let decision = match request.decision {
        DecisionKind::RequestSupplement => OrganizationVerificationDecisionKind::RequestSupplement,
        DecisionKind::Approve => OrganizationVerificationDecisionKind::Approve,
        DecisionKind::Reject => OrganizationVerificationDecisionKind::Reject,
    };
    let bundle = service(&state)?
        .decide(
            authentication.principal(),
            case_id,
            OrganizationVerificationDecisionCommand {
                expected_version: request.expected_version,
                decision,
                reason: request.reason,
            },
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(to_response(&bundle)))
}

fn service(
    state: &OrganizationVerificationHttpState,
) -> Result<&Arc<dyn OrganizationVerificationService>, ManagementError> {
    state
        .service
        .as_ref()
        .ok_or(ManagementError::OrganizationVerificationUnavailable)
}

fn to_response(bundle: &af_admin::OrganizationVerificationBundle) -> VerificationResponse {
    let case = bundle.case();
    VerificationResponse {
        case: CaseResponse {
            id: case.id().get(),
            organization_id: case.organization_id().get(),
            applicant_user_id: case.applicant_user_id().get(),
            status: status_response(case.status()),
            version: case.version(),
            summary: case.summary().to_owned(),
            reviewer_user_id: case.reviewer_user_id().map(|value| value.get()),
            review_reason: case.review_reason().map(str::to_owned),
            created_at: case.created_at(),
            updated_at: case.updated_at(),
            decided_at: case.decided_at(),
        },
        materials: bundle
            .materials()
            .iter()
            .map(|material| MaterialResponse {
                id: material.id().get(),
                case_id: material.case_id().get(),
                kind: material.kind().to_owned(),
                file_name: material.file_name().to_owned(),
                content_type: material.content_type().to_owned(),
                size_bytes: material.size_bytes(),
                created_at: material.created_at(),
            })
            .collect(),
    }
}

fn status_response(status: OrganizationVerificationStatus) -> StatusResponse {
    match status {
        OrganizationVerificationStatus::Submitted => StatusResponse::Submitted,
        OrganizationVerificationStatus::InReview => StatusResponse::InReview,
        OrganizationVerificationStatus::NeedsSupplement => StatusResponse::NeedsSupplement,
        OrganizationVerificationStatus::Approved => StatusResponse::Approved,
        OrganizationVerificationStatus::Rejected => StatusResponse::Rejected,
    }
}

fn parse_query(
    raw: Option<&str>,
) -> Result<(Option<OrganizationVerificationStatus>, Option<i64>, usize), ManagementError> {
    let mut status = None;
    let mut before = None;
    let mut limit = 50_usize;
    if let Some(raw) = raw {
        for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
            match key.as_ref() {
                "status" => {
                    status = Some(match value.as_ref() {
                        "submitted" => OrganizationVerificationStatus::Submitted,
                        "in_review" => OrganizationVerificationStatus::InReview,
                        "needs_supplement" => OrganizationVerificationStatus::NeedsSupplement,
                        "approved" => OrganizationVerificationStatus::Approved,
                        "rejected" => OrganizationVerificationStatus::Rejected,
                        _ => return Err(ManagementError::OrganizationVerificationInvalidRequest),
                    })
                }
                "before" => {
                    before = Some(
                        value
                            .parse()
                            .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?,
                    )
                }
                "limit" => {
                    limit = value
                        .parse()
                        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?
                }
                _ => return Err(ManagementError::OrganizationVerificationInvalidRequest),
            }
        }
    }
    Ok((status, before, limit))
}

fn map_error(error: OrganizationVerificationServiceError) -> ManagementError {
    match error {
        OrganizationVerificationServiceError::InvalidRequest => {
            ManagementError::OrganizationVerificationInvalidRequest
        }
        OrganizationVerificationServiceError::NotFound => {
            ManagementError::OrganizationVerificationNotFound
        }
        OrganizationVerificationServiceError::Forbidden => {
            ManagementError::OrganizationVerificationForbidden
        }
        OrganizationVerificationServiceError::Conflict => {
            ManagementError::OrganizationVerificationConflict
        }
        OrganizationVerificationServiceError::Unavailable => {
            ManagementError::OrganizationVerificationUnavailable
        }
        OrganizationVerificationServiceError::Internal => ManagementError::Internal,
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
