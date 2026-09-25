use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
    middleware::ensure_request_id,
};
use af_admin::{
    AccountVerificationError, AccountVerificationRecord, AccountVerificationService,
    AccountVerificationSubmit, SessionAuthentication, SessionAuthenticator,
};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, RawQuery, State},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{StatusCode, header};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

pub fn build_account_verification_router(
    service: Arc<AccountVerificationService>,
    auth: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route("/api/account/verifications", get(list_self).post(submit))
        .route("/api/account/verifications/eligibility", get(eligibility))
        .route("/api/account/verifications/{case_id}", get(get_self))
        .route(
            "/api/account/verifications/{case_id}/provider-sync",
            post(sync_provider),
        )
        .route(
            "/api/account/verifications/{case_id}/materials/{material_id}",
            get(download_self),
        )
        .route("/api/admin/account-verifications", get(list_admin))
        .route("/api/admin/account-verifications/{case_id}", get(get_admin))
        .route(
            "/api/admin/account-verifications/{case_id}/materials/{material_id}",
            get(download_admin),
        )
        .route(
            "/api/admin/account-verifications/{case_id}/decision",
            post(decide),
        )
        .route_layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(auth),
            authenticate_management_session,
        ))
        .layer(middleware::from_fn(ensure_request_id))
        .layer(DefaultBodyLimit::max(26_000_000))
        .with_state(service)
}

#[derive(Serialize, ToSchema)]
pub(crate) struct AccountVerificationResponse {
    pub(crate) id: i64,
    pub(crate) user_id: i64,
    pub(crate) kind: String,
    pub(crate) provider: String,
    pub(crate) provider_action_url: Option<String>,
    pub(crate) provider_status: Option<String>,
    pub(crate) document_country: String,
    pub(crate) document_type: String,
    pub(crate) document_number_masked: Option<String>,
    pub(crate) subject_name: String,
    pub(crate) summary: String,
    pub(crate) status: i16,
    pub(crate) version: i64,
    pub(crate) review_reason: Option<String>,
    pub(crate) reviewer_user_id: Option<i64>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}
impl From<AccountVerificationRecord> for AccountVerificationResponse {
    fn from(c: AccountVerificationRecord) -> Self {
        Self {
            id: c.id,
            user_id: c.user_id,
            kind: c.kind,
            provider: c.provider,
            provider_action_url: c.provider_action_url,
            provider_status: c.provider_status,
            document_country: c.document_country,
            document_type: c.document_type,
            document_number_masked: c.document_number_masked,
            subject_name: c.subject_name,
            summary: c.summary,
            status: c.status,
            version: c.version,
            review_reason: c.review_reason,
            reviewer_user_id: c.reviewer_user_id,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AccountVerificationMaterialResponse {
    id: i64,
    case_id: i64,
    kind: String,
    file_name: String,
    content_type: String,
    size_bytes: i64,
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AccountVerificationDetailResponse {
    case: AccountVerificationResponse,
    materials: Vec<AccountVerificationMaterialResponse>,
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AccountVerificationListResponse {
    cases: Vec<AccountVerificationResponse>,
    next_cursor: Option<i64>,
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AccountVerificationEligibilityResponse {
    enterprise_verified: bool,
    can_apply_for_organization: bool,
    providers: Vec<String>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AccountVerificationDecisionRequest {
    expected_version: i64,
    status: i16,
    reason: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    before: Option<i64>,
    status: Option<i16>,
    limit: Option<u64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SubmitMetadata {
    kind: String,
    #[serde(default = "default_provider")]
    provider: String,
    #[serde(default = "default_document_country")]
    document_country: String,
    #[serde(default = "default_document_type")]
    document_type: String,
    document_number: Option<String>,
    subject_name: String,
    summary: String,
    #[serde(default)]
    materials: Vec<MaterialMetadata>,
}

fn default_provider() -> String {
    "manual".to_owned()
}
fn default_document_country() -> String {
    "CN".to_owned()
}
fn default_document_type() -> String {
    "identity".to_owned()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MaterialMetadata {
    kind: String,
    file_field: String,
}

async fn list_cases(
    service: Arc<AccountVerificationService>,
    auth: SessionAuthentication,
    request: RequestId,
    query: ListQuery,
    admin: bool,
) -> Result<Response, ManagementError> {
    let limit = query.limit.unwrap_or(25);
    let rows = service
        .list(
            auth.principal(),
            admin,
            query.before,
            query.status,
            limit,
            request.as_str(),
        )
        .await
        .map_err(map_error)?;
    let next_cursor = (rows.len() as u64 == limit)
        .then(|| rows.last().map(|c| c.id))
        .flatten();
    Ok(no_store_json(AccountVerificationListResponse {
        cases: rows.into_iter().map(Into::into).collect(),
        next_cursor,
    }))
}
async fn list_self(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    RawQuery(raw): RawQuery,
) -> Result<Response, ManagementError> {
    list_cases(s, a, r, parse_list_query(raw.as_deref())?, false).await
}
async fn list_admin(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    RawQuery(raw): RawQuery,
) -> Result<Response, ManagementError> {
    list_cases(s, a, r, parse_list_query(raw.as_deref())?, true).await
}

async fn detail(
    s: Arc<AccountVerificationService>,
    a: SessionAuthentication,
    r: RequestId,
    id: i64,
    admin: bool,
) -> Result<Response, ManagementError> {
    let (case, materials) = s
        .get(a.principal(), admin, id, r.as_str())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AccountVerificationDetailResponse {
        case: case.into(),
        materials: materials
            .into_iter()
            .map(|m| AccountVerificationMaterialResponse {
                id: m.id,
                case_id: m.case_id,
                kind: m.kind,
                file_name: m.file_name,
                content_type: m.content_type,
                size_bytes: m.size_bytes,
            })
            .collect(),
    }))
}
async fn get_self(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    Path(id): Path<i64>,
) -> Result<Response, ManagementError> {
    detail(s, a, r, id, false).await
}

async fn sync_provider(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, ManagementError> {
    let case = s
        .sync_provider(a.principal(), id)
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AccountVerificationResponse::from(case)))
}
async fn get_admin(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    Path(id): Path<i64>,
) -> Result<Response, ManagementError> {
    detail(s, a, r, id, true).await
}

async fn download(
    s: Arc<AccountVerificationService>,
    a: SessionAuthentication,
    r: RequestId,
    ids: (i64, i64),
    admin: bool,
) -> Result<Response, ManagementError> {
    let (_name, kind, bytes) = s
        .download(a.principal(), admin, ids.0, ids.1, r.as_str())
        .await
        .map_err(map_error)?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, kind),
            (header::CONTENT_DISPOSITION, "attachment".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox; default-src 'none'".to_owned(),
            ),
        ],
        bytes,
    )
        .into_response())
}
async fn download_self(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    Path(ids): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    download(s, a, r, ids, false).await
}
async fn download_admin(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    Path(ids): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    download(s, a, r, ids, true).await
}

async fn eligibility(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let verified = s
        .enterprise_verified(a.principal())
        .await
        .map_err(map_error)?;
    let providers = s.available_providers().await;
    Ok(no_store_json(AccountVerificationEligibilityResponse {
        enterprise_verified: verified,
        can_apply_for_organization: verified,
        providers,
    }))
}
async fn decide(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    Extension(r): Extension<RequestId>,
    Path(id): Path<i64>,
    Json(body): Json<AccountVerificationDecisionRequest>,
) -> Result<Response, ManagementError> {
    let case = s
        .decide(
            a.principal(),
            id,
            body.expected_version,
            body.status,
            body.reason,
            r.as_str().to_owned(),
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AccountVerificationResponse::from(case)))
}
async fn submit(
    State(s): State<Arc<AccountVerificationService>>,
    Extension(a): Extension<SessionAuthentication>,
    mut multipart: Multipart,
) -> Result<Response, ManagementError> {
    let mut metadata = None;
    let mut files = std::collections::HashMap::new();
    let mut total = 0;
    while let Some(field) = multipart.next_field().await.map_err(|_| invalid())? {
        let name = field.name().ok_or_else(invalid)?.to_owned();
        if name == "metadata" {
            if metadata.is_some() {
                return Err(invalid());
            }
            let text = field.text().await.map_err(|_| invalid())?;
            if text.len() > 8_192 {
                return Err(invalid());
            }
            metadata = Some(serde_json::from_str::<SubmitMetadata>(&text).map_err(|_| invalid())?);
        } else if let Some(key) = name
            .strip_prefix("file:")
            .or_else(|| name.strip_prefix("file_"))
        {
            if key.len() > 16 || files.len() >= 5 {
                return Err(invalid());
            }
            let file_name = field
                .file_name()
                .ok_or_else(invalid)?
                .rsplit(['/', '\\'])
                .next()
                .ok_or_else(invalid)?
                .to_owned();
            let content_type = field.content_type().ok_or_else(invalid)?.to_owned();
            let bytes = field.bytes().await.map_err(|_| invalid())?;
            total += bytes.len();
            if total > 25_000_000
                || !af_admin::validate_verification_material(&content_type, &bytes)
            {
                return Err(invalid());
            }
            if files
                .insert(key.to_owned(), (file_name, content_type, bytes.to_vec()))
                .is_some()
            {
                return Err(invalid());
            }
        } else {
            return Err(invalid());
        }
    }
    let metadata = metadata.ok_or_else(invalid)?;
    if metadata.provider != "alipay" && !(1..=5).contains(&metadata.materials.len()) {
        return Err(invalid());
    }
    if metadata.provider == "alipay" && !metadata.materials.is_empty() {
        return Err(invalid());
    }
    let mut materials = Vec::new();
    for entry in metadata.materials {
        let (file_name, content_type, bytes) =
            files.remove(&entry.file_field).ok_or_else(invalid)?;
        materials.push(af_admin::OrganizationVerificationMaterialWrite {
            kind: entry.kind,
            object_reference: String::new(),
            file_name,
            content_type,
            size_bytes: bytes.len() as i64,
            content_bytes: bytes,
        });
    }
    if !files.is_empty() {
        return Err(invalid());
    }
    let case = s
        .submit(
            a.principal(),
            AccountVerificationSubmit {
                kind: metadata.kind,
                provider: metadata.provider,
                provider_reference: None,
                provider_action_url: None,
                provider_status: None,
                document_country: metadata.document_country,
                document_type: metadata.document_type,
                document_number: metadata.document_number,
                subject_name: metadata.subject_name,
                summary: metadata.summary,
                materials,
            },
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AccountVerificationResponse::from(case)))
}
fn invalid() -> ManagementError {
    ManagementError::OrganizationVerificationInvalidRequest
}
pub(crate) fn map_error(e: AccountVerificationError) -> ManagementError {
    match e {
        AccountVerificationError::Invalid => invalid(),
        AccountVerificationError::NotFound => ManagementError::OrganizationVerificationNotFound,
        AccountVerificationError::Forbidden => ManagementError::OrganizationVerificationForbidden,
        AccountVerificationError::SelfReview => ManagementError::AccountVerificationSelfReview,
        AccountVerificationError::Conflict => ManagementError::OrganizationVerificationConflict,
        AccountVerificationError::Unavailable => {
            ManagementError::OrganizationVerificationUnavailable
        }
    }
}

fn parse_list_query(raw: Option<&str>) -> Result<ListQuery, ManagementError> {
    let mut q = ListQuery::default();
    for (key, value) in url::form_urlencoded::parse(raw.unwrap_or_default().as_bytes()) {
        match key.as_ref() {
            "before" if q.before.is_none() => {
                q.before = Some(value.parse().map_err(|_| invalid())?)
            }
            "status" if q.status.is_none() => {
                q.status = Some(value.parse().map_err(|_| invalid())?)
            }
            "limit" if q.limit.is_none() => q.limit = Some(value.parse().map_err(|_| invalid())?),
            _ => return Err(invalid()),
        }
    }
    Ok(q)
}
