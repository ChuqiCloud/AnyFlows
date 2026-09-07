use std::sync::Arc;

use af_admin::{
    OrganizationVerificationDecisionCommand, OrganizationVerificationDecisionKind,
    OrganizationVerificationListQuery, OrganizationVerificationService,
    OrganizationVerificationServiceError, OrganizationVerificationSubmitCommand,
};
use af_domain::{
    OrganizationId, OrganizationVerificationCaseId, OrganizationVerificationMaterialId,
    OrganizationVerificationStatus,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, RawQuery, State, rejection::JsonRejection},
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
        .route(
            "/api/admin/organization-verifications/{case_id}/materials/{material_id}",
            get(download_material),
        )
        .layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .layer(DefaultBodyLimit::max(MAX_SUBMIT_BODY_BYTES))
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
    #[schema(min_length = 1, max_length = 64)]
    file_field: String,
}

const MAX_MATERIAL_BYTES: usize = 50_000_000;
const MAX_TOTAL_MATERIAL_BYTES: usize = 100_000_000;
const MAX_SUBMIT_BODY_BYTES: usize = MAX_TOTAL_MATERIAL_BYTES + 2 * 1024 * 1024;

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
    content_available: bool,
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
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> Result<Response, ManagementError> {
    let organization_id = OrganizationId::new(organization_id)
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let mut multipart =
        multipart.map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let mut summary = None;
    let mut metadata = None;
    let mut files = std::collections::HashMap::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?
    {
        let name = field
            .name()
            .ok_or(ManagementError::OrganizationVerificationInvalidRequest)?
            .to_owned();
        if name == "summary" {
            if summary.is_some() {
                return Err(ManagementError::OrganizationVerificationInvalidRequest);
            }
            summary = Some(
                field
                    .text()
                    .await
                    .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?,
            );
        } else if name == "materials" {
            if metadata.is_some() {
                return Err(ManagementError::OrganizationVerificationInvalidRequest);
            }
            metadata = Some(
                field
                    .text()
                    .await
                    .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?,
            );
        } else if let Some(file_field) = name.strip_prefix("file:") {
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned();
            let file_name = normalize_upload_name(field.file_name().unwrap_or("material"))
                .ok_or(ManagementError::OrganizationVerificationInvalidRequest)?;
            let bytes = field
                .bytes()
                .await
                .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
            if bytes.len() > MAX_MATERIAL_BYTES {
                return Err(ManagementError::OrganizationVerificationInvalidRequest);
            }
            if files
                .insert(
                    file_field.to_owned(),
                    (file_name, content_type, bytes.to_vec()),
                )
                .is_some()
            {
                return Err(ManagementError::OrganizationVerificationInvalidRequest);
            }
        } else {
            return Err(ManagementError::OrganizationVerificationInvalidRequest);
        }
    }
    let request = SubmitRequest {
        summary: summary.ok_or(ManagementError::OrganizationVerificationInvalidRequest)?,
        materials: serde_json::from_str(
            &metadata.ok_or(ManagementError::OrganizationVerificationInvalidRequest)?,
        )
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?,
    };
    if request.materials.is_empty() || request.materials.len() > 20 {
        return Err(ManagementError::OrganizationVerificationInvalidRequest);
    }
    let mut total_size = 0_usize;
    let mut writes = Vec::with_capacity(request.materials.len());
    for material in request.materials {
        let (file_name, content_type, content_bytes) = files
            .remove(&material.file_field)
            .ok_or(ManagementError::OrganizationVerificationInvalidRequest)?;
        if material.kind.trim() != material.kind
            || material.kind.is_empty()
            || material.kind.len() > 64
            || material.kind.chars().any(char::is_control)
            || content_type.is_empty()
            || content_type.len() > 128
            || content_type.chars().any(char::is_control)
            || !matches!(
                content_type.as_str(),
                "application/pdf" | "image/png" | "image/jpeg" | "image/webp"
            )
        {
            return Err(ManagementError::OrganizationVerificationInvalidRequest);
        }
        total_size = total_size
            .checked_add(content_bytes.len())
            .ok_or(ManagementError::OrganizationVerificationInvalidRequest)?;
        writes.push(af_admin::OrganizationVerificationMaterialWrite {
            kind: material.kind,
            object_reference: format!("verification/material/{}", uuid::Uuid::new_v4()),
            file_name,
            content_type,
            size_bytes: i64::try_from(content_bytes.len())
                .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?,
            content_bytes,
        });
    }
    if total_size > MAX_TOTAL_MATERIAL_BYTES || !files.is_empty() {
        return Err(ManagementError::OrganizationVerificationInvalidRequest);
    }
    let command = OrganizationVerificationSubmitCommand {
        summary: request.summary,
        materials: writes,
    };
    let bundle = service(&state)?
        .submit(authentication.principal(), organization_id, command)
        .await
        .map_err(map_error)?;
    Ok(no_store_status_json(StatusCode::OK, to_response(&bundle)))
}

async fn download_material(
    State(state): State<OrganizationVerificationHttpState>,
    Path((case_id, material_id)): Path<(i64, i64)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
) -> Result<Response, ManagementError> {
    let case_id = OrganizationVerificationCaseId::new(case_id)
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let material_id = OrganizationVerificationMaterialId::new(material_id)
        .map_err(|_| ManagementError::OrganizationVerificationInvalidRequest)?;
    let download = service(&state)?
        .download_admin(
            authentication.principal(),
            case_id,
            material_id,
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_error)?;
    let disposition = format!(
        "attachment; filename=\"{}\"",
        safe_download_name(&download.file_name)
    );
    Ok((
        StatusCode::OK,
        [
            (http::header::CONTENT_TYPE, download.content_type),
            (http::header::CONTENT_DISPOSITION, disposition),
            (http::header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        download.content,
    )
        .into_response())
}

fn safe_download_name(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '"' | '\\' | '/' | '\r' | '\n' => '_',
            _ => character,
        })
        .collect()
}

fn normalize_upload_name(value: &str) -> Option<String> {
    let name = value.rsplit(['/', '\\']).next()?.trim();
    (!name.is_empty() && name.len() <= 255 && !name.chars().any(char::is_control))
        .then(|| name.to_owned())
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
                content_available: material.content_available(),
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
