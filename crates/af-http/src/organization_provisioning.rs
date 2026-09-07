use std::sync::Arc;

use af_admin::{
    OrganizationProvisioningCreateCommand, OrganizationProvisioningDecisionCommand,
    OrganizationProvisioningEntitlementCommand, OrganizationProvisioningListQuery,
    OrganizationProvisioningMaterial, OrganizationProvisioningMaterialWrite,
    OrganizationProvisioningService, OrganizationProvisioningServiceError,
    OrganizationProvisioningStatus, SessionAuthentication, SessionAuthenticator,
};
use af_domain::{
    OrganizationProvisioningDecisionKind, OrganizationProvisioningRecord,
    OrganizationProvisioningRequestKey,
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

const DEFAULT_PAGE_SIZE: u64 = 50;
const MAX_MATERIAL_BYTES: usize = 50_000_000;
const MAX_TOTAL_MATERIAL_BYTES: usize = 100_000_000;
const MAX_SUBMIT_BODY_BYTES: usize = MAX_TOTAL_MATERIAL_BYTES + 2 * 1024 * 1024;

#[derive(Clone)]
struct OrganizationProvisioningHttpState {
    service: Option<Arc<dyn OrganizationProvisioningService>>,
}

/// 构建企业开通申请的用户与平台管理员接口。
pub fn build_organization_provisioning_router(
    service: Option<Arc<dyn OrganizationProvisioningService>>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/account/organization-provisioning",
            post(create_request),
        )
        .route(
            "/api/account/organization-provisioning/active",
            get(get_active_request),
        )
        .route(
            "/api/account/organization-provisioning/{request_key}",
            get(get_self_request),
        )
        .route(
            "/api/admin/organization-provisioning",
            get(list_admin_requests),
        )
        .route(
            "/api/admin/organization-provisioning/{request_key}",
            get(get_admin_request),
        )
        .route(
            "/api/admin/organization-provisioning/{request_key}/decision",
            post(decide_request),
        )
        .route(
            "/api/admin/organization-provisioning/{request_key}/materials",
            get(list_materials),
        )
        .route(
            "/api/admin/organization-provisioning/{request_key}/materials/{material_id}",
            get(download_material),
        )
        .route_layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .layer(DefaultBodyLimit::max(MAX_SUBMIT_BODY_BYTES))
        .with_state(OrganizationProvisioningHttpState { service })
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct CreateRequest {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    idempotency_key: String,
    #[schema(min_length = 1, max_length = 128)]
    organization_name: String,
    #[schema(
        min_length = 1,
        max_length = 64,
        pattern = "^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$"
    )]
    organization_slug: String,
    #[schema(min_length = 1, max_length = 2000)]
    business_reason: String,
    #[serde(default)]
    materials: Vec<MaterialRequest>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct MaterialRequest {
    kind: String,
    file_field: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct DecisionRequest {
    #[schema(minimum = 1)]
    expected_version: u64,
    decision: DecisionKindRequest,
    #[schema(max_length = 2000)]
    reason: Option<String>,
    entitlement: Option<EntitlementRequest>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum DecisionKindRequest {
    Approve,
    Reject,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct EntitlementRequest {
    #[schema(minimum = 1)]
    pub(super) seat_limit: i64,
    #[schema(minimum = 1)]
    team_limit: i64,
    #[schema(minimum = 1)]
    organization_key_limit: i64,
    team_management_enabled: bool,
    sso_enabled: bool,
    custom_roles_enabled: bool,
    #[schema(minimum = 1)]
    audit_retention_days: i64,
    valid_from: i64,
    valid_until: i64,
    pub(super) grace_until: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct ProvisioningResponse {
    pub(super) id: i64,
    pub(super) request_key: String,
    pub(super) idempotency_key: String,
    pub(super) applicant_user_id: i64,
    pub(super) organization_name: String,
    pub(super) organization_slug: String,
    pub(super) business_reason: String,
    pub(super) status: StatusResponse,
    pub(super) version: u64,
    pub(super) reviewer_user_id: Option<i64>,
    pub(super) decision_reason: Option<String>,
    pub(super) organization_id: Option<i64>,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
    pub(super) decided_at: Option<u64>,
    pub(super) replayed: bool,
}

#[derive(Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(super) enum StatusResponse {
    Pending,
    Approved,
    Rejected,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct AdminListResponse {
    requests: Vec<ProvisioningResponse>,
    next_before: Option<i64>,
}

async fn create_request(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    multipart: Result<Multipart, axum::extract::multipart::MultipartRejection>,
) -> Result<Response, ManagementError> {
    let service = service(&state)?;
    let mut multipart =
        multipart.map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?;
    let mut metadata = None;
    let mut files = std::collections::HashMap::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?
    {
        let name = field
            .name()
            .ok_or(ManagementError::OrganizationProvisioningInvalidRequest)?
            .to_owned();
        if name == "metadata" {
            if metadata.is_some() {
                return Err(ManagementError::OrganizationProvisioningInvalidRequest);
            }
            metadata = Some(
                field
                    .text()
                    .await
                    .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?,
            );
        } else if let Some(file_field) = name.strip_prefix("file:") {
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_owned();
            let file_name = normalize_upload_name(field.file_name().unwrap_or("material"))
                .ok_or(ManagementError::OrganizationProvisioningInvalidRequest)?;
            let bytes = field
                .bytes()
                .await
                .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?;
            if bytes.len() > MAX_MATERIAL_BYTES
                || files
                    .insert(
                        file_field.to_owned(),
                        (file_name, content_type, bytes.to_vec()),
                    )
                    .is_some()
            {
                return Err(ManagementError::OrganizationProvisioningInvalidRequest);
            }
        } else {
            return Err(ManagementError::OrganizationProvisioningInvalidRequest);
        }
    }
    let request: CreateRequest = serde_json::from_str(
        &metadata.ok_or(ManagementError::OrganizationProvisioningInvalidRequest)?,
    )
    .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?;
    let materials = request_materials(&request, &mut files)?;
    let command = OrganizationProvisioningCreateCommand::new(
        request.idempotency_key,
        request.organization_name,
        request.organization_slug,
        request.business_reason,
    )
    .with_materials(materials);
    let result = service
        .create(authentication.principal().user_id(), command)
        .await
        .map_err(map_error)?;
    let status = if result.replayed() {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok(no_store_status_json(
        status,
        ProvisioningResponse::from_result(result)?,
    ))
}

fn request_materials(
    request: &CreateRequest,
    files: &mut std::collections::HashMap<String, (String, String, Vec<u8>)>,
) -> Result<Vec<OrganizationProvisioningMaterialWrite>, ManagementError> {
    let mut total_size = 0_usize;
    let mut writes = Vec::with_capacity(request.materials.len());
    // 材料元数据随申请字段传入；旧 JSON 结构不再接受，避免正文与元数据分离。
    if request.materials.is_empty() || request.materials.len() > 20 {
        return Err(ManagementError::OrganizationProvisioningInvalidRequest);
    }
    for material in &request.materials {
        let file_field = material.file_field.trim();
        if file_field != material.file_field
            || file_field.is_empty()
            || file_field.len() > 64
            || file_field.chars().any(char::is_control)
            || material.kind.trim() != material.kind
            || material.kind.is_empty()
            || material.kind.len() > 64
            || material.kind.chars().any(char::is_control)
        {
            return Err(ManagementError::OrganizationProvisioningInvalidRequest);
        }
        let (file_name, content_type, content_bytes) = files
            .remove(file_field)
            .ok_or(ManagementError::OrganizationProvisioningInvalidRequest)?;
        if !matches!(
            content_type.as_str(),
            "application/pdf" | "image/png" | "image/jpeg" | "image/webp"
        ) {
            return Err(ManagementError::OrganizationProvisioningInvalidRequest);
        }
        total_size = total_size
            .checked_add(content_bytes.len())
            .ok_or(ManagementError::OrganizationProvisioningInvalidRequest)?;
        writes.push(OrganizationProvisioningMaterialWrite {
            kind: material.kind.clone(),
            object_reference: format!("provisioning/material/{}", uuid::Uuid::new_v4()),
            file_name,
            content_type,
            size_bytes: i64::try_from(content_bytes.len())
                .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?,
            content_bytes,
        });
    }
    if total_size > MAX_TOTAL_MATERIAL_BYTES || !files.is_empty() {
        return Err(ManagementError::OrganizationProvisioningInvalidRequest);
    }
    Ok(writes)
}

fn normalize_upload_name(value: &str) -> Option<String> {
    let name = value.rsplit(['/', '\\']).next()?.trim();
    (!name.is_empty() && name.len() <= 255 && !name.chars().any(char::is_control))
        .then(|| name.to_owned())
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

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct MaterialResponse {
    id: i64,
    request_key: String,
    kind: String,
    file_name: String,
    content_type: String,
    size_bytes: i64,
    content_available: bool,
    created_at: u64,
}

async fn list_materials(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(request_key): Path<String>,
) -> Result<Response, ManagementError> {
    let request_key = parse_request_key(&request_key)?;
    let materials = service(&state)?
        .list_materials_admin(
            request_key,
            authentication.principal(),
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(
        materials
            .iter()
            .map(to_material_response)
            .collect::<Vec<_>>(),
    ))
}

async fn download_material(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((request_key, material_id)): Path<(String, i64)>,
) -> Result<Response, ManagementError> {
    let request_key = parse_request_key(&request_key)?;
    let (file_name, content_type, content) = service(&state)?
        .download_material_admin(
            request_key,
            material_id,
            authentication.principal(),
            request_id.as_str().to_owned(),
        )
        .await
        .map_err(map_error)?;
    Ok((
        StatusCode::OK,
        [
            (http::header::CONTENT_TYPE, content_type),
            (
                http::header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"{}\"",
                    safe_download_name(&file_name)
                ),
            ),
            (http::header::CACHE_CONTROL, "no-store".to_owned()),
        ],
        content,
    )
        .into_response())
}

fn to_material_response(material: &OrganizationProvisioningMaterial) -> MaterialResponse {
    MaterialResponse {
        id: material.id(),
        request_key: material.request_key().persistence_key(),
        kind: material.kind().to_owned(),
        file_name: material.file_name().to_owned(),
        content_type: material.content_type().to_owned(),
        size_bytes: material.size_bytes(),
        content_available: material.content_available(),
        created_at: material.created_at(),
    }
}

async fn get_active_request(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = service(&state)?;
    let record = service
        .get_active_self(authentication.principal().user_id())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ProvisioningResponse::from_record(
        &record, false,
    )?))
}

async fn get_self_request(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(request_key): Path<String>,
) -> Result<Response, ManagementError> {
    let service = service(&state)?;
    let request_key = parse_request_key(&request_key)?;
    let record = service
        .get_self(authentication.principal().user_id(), request_key)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ProvisioningResponse::from_record(
        &record, false,
    )?))
}

async fn list_admin_requests(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    let service = service(&state)?;
    let query = parse_admin_query(raw_query.as_deref())?;
    let page = service
        .list_admin(query, authentication.principal())
        .await
        .map_err(map_error)?;
    let requests = page
        .records()
        .iter()
        .map(|record| ProvisioningResponse::from_record(record, false))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_json(AdminListResponse {
        requests,
        next_before: page.next_cursor(),
    }))
}

async fn get_admin_request(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(request_key): Path<String>,
) -> Result<Response, ManagementError> {
    let service = service(&state)?;
    let request_key = parse_request_key(&request_key)?;
    let record = service
        .get_admin(request_key, authentication.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(ProvisioningResponse::from_record(
        &record, false,
    )?))
}

async fn decide_request(
    State(state): State<OrganizationProvisioningHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(request_key): Path<String>,
    request: Result<Json<DecisionRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let service = service(&state)?;
    let request_key = parse_request_key(&request_key)?;
    let Json(request) =
        request.map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?;
    let entitlement = request.entitlement.map(EntitlementRequest::into_command);
    let command = OrganizationProvisioningDecisionCommand::new(
        request.expected_version,
        request.decision.into_domain(),
        request.reason,
        entitlement,
    );
    let result = service
        .decide(request_key, authentication.principal(), command)
        .await
        .map_err(map_error)?;
    let status = if result.replayed() {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok(no_store_status_json(
        status,
        ProvisioningResponse::from_result(result)?,
    ))
}

fn service(
    state: &OrganizationProvisioningHttpState,
) -> Result<&Arc<dyn OrganizationProvisioningService>, ManagementError> {
    state
        .service
        .as_ref()
        .ok_or(ManagementError::OrganizationProvisioningUnavailable)
}

fn parse_request_key(value: &str) -> Result<OrganizationProvisioningRequestKey, ManagementError> {
    OrganizationProvisioningRequestKey::from_persistence_key(value)
        .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)
}

pub(super) fn parse_admin_query(
    raw_query: Option<&str>,
) -> Result<OrganizationProvisioningListQuery, ManagementError> {
    let mut before = None;
    let mut limit = DEFAULT_PAGE_SIZE;
    let mut status = None;
    let Some(raw_query) = raw_query else {
        return OrganizationProvisioningListQuery::new(before, limit, status).map_err(map_error);
    };
    let mut seen_before = false;
    let mut seen_limit = false;
    let mut seen_status = false;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        match key.as_ref() {
            "before" if !seen_before => {
                seen_before = true;
                before = Some(
                    value
                        .parse()
                        .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?,
                );
            }
            "limit" if !seen_limit => {
                seen_limit = true;
                limit = value
                    .parse()
                    .map_err(|_| ManagementError::OrganizationProvisioningInvalidRequest)?;
            }
            "status" if !seen_status => {
                seen_status = true;
                status = Some(match value.as_ref() {
                    "pending" => OrganizationProvisioningStatus::Pending,
                    "approved" => OrganizationProvisioningStatus::Approved,
                    "rejected" => OrganizationProvisioningStatus::Rejected,
                    _ => return Err(ManagementError::OrganizationProvisioningInvalidRequest),
                });
            }
            _ => return Err(ManagementError::OrganizationProvisioningInvalidRequest),
        }
    }
    OrganizationProvisioningListQuery::new(before, limit, status).map_err(map_error)
}

impl DecisionKindRequest {
    fn into_domain(self) -> OrganizationProvisioningDecisionKind {
        match self {
            Self::Approve => OrganizationProvisioningDecisionKind::Approve,
            Self::Reject => OrganizationProvisioningDecisionKind::Reject,
        }
    }
}

impl EntitlementRequest {
    fn into_command(self) -> OrganizationProvisioningEntitlementCommand {
        OrganizationProvisioningEntitlementCommand::new(
            self.seat_limit,
            self.team_limit,
            self.organization_key_limit,
            self.team_management_enabled,
            self.sso_enabled,
            self.custom_roles_enabled,
            self.audit_retention_days,
            self.valid_from,
            self.valid_until,
            self.grace_until,
        )
    }
}

impl ProvisioningResponse {
    fn from_result(
        result: af_admin::OrganizationProvisioningResult,
    ) -> Result<Self, ManagementError> {
        Self::from_record(result.record(), result.replayed())
    }

    fn from_record(
        record: &OrganizationProvisioningRecord,
        replayed: bool,
    ) -> Result<Self, ManagementError> {
        Ok(Self {
            id: record.id().get(),
            request_key: record.request_key().persistence_key(),
            idempotency_key: record.idempotency_key().persistence_key(),
            applicant_user_id: record.applicant_user_id().get(),
            organization_name: record.organization_name().to_owned(),
            organization_slug: record.organization_slug().to_owned(),
            business_reason: record.business_reason().to_owned(),
            status: match record.status() {
                OrganizationProvisioningStatus::Pending => StatusResponse::Pending,
                OrganizationProvisioningStatus::Approved => StatusResponse::Approved,
                OrganizationProvisioningStatus::Rejected => StatusResponse::Rejected,
            },
            version: record.version(),
            reviewer_user_id: record.reviewer_user_id().map(|value| value.get()),
            decision_reason: record.decision_reason().map(str::to_owned),
            organization_id: record.organization_id().map(|value| value.get()),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            decided_at: record.decided_at(),
            replayed,
        })
    }
}

fn map_error(error: OrganizationProvisioningServiceError) -> ManagementError {
    match error {
        OrganizationProvisioningServiceError::InvalidRequest => {
            ManagementError::OrganizationProvisioningInvalidRequest
        }
        OrganizationProvisioningServiceError::NotFound => {
            ManagementError::OrganizationProvisioningNotFound
        }
        OrganizationProvisioningServiceError::Forbidden => {
            ManagementError::OrganizationProvisioningForbidden
        }
        OrganizationProvisioningServiceError::Conflict => {
            ManagementError::OrganizationProvisioningConflict
        }
        OrganizationProvisioningServiceError::Unavailable => {
            ManagementError::OrganizationProvisioningUnavailable
        }
        OrganizationProvisioningServiceError::Internal => ManagementError::Internal,
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
