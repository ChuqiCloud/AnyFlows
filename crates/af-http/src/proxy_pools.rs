use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
};
use af_admin::{
    AdminCredentialProxyService, ProxyCleanupMode, ProxyCleanupRequest, ProxyCleanupTarget,
    ProxyPoolError, ProxyPoolService, ProxyPoolView, ProxyPoolWrite, SessionAuthenticator,
};
use af_domain::ProxyId;
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminProxyPoolWriteRequest {
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    subscribed: bool,
    #[schema(write_only = true, max_length = 2048)]
    source_url: Option<String>,
    #[schema(minimum = 60, maximum = 86400)]
    interval_secs: i32,
    #[schema(example = "socks5")]
    default_scheme: String,
    trust_proxy_dns: bool,
    #[schema(max_length = 2048)]
    probe_url: Option<String>,
    #[schema(write_only = true, max_length = 1048576)]
    content: Option<String>,
    revision: Option<i64>,
}
impl From<AdminProxyPoolWriteRequest> for ProxyPoolWrite {
    fn from(v: AdminProxyPoolWriteRequest) -> Self {
        Self {
            name: v.name,
            subscribed: v.subscribed,
            source_url: v.source_url,
            interval_secs: v.interval_secs,
            default_scheme: v.default_scheme,
            trust_proxy_dns: v.trust_proxy_dns,
            probe_url: v.probe_url,
            content: v.content,
            revision: v.revision,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AdminProxyPoolNode {
    id: i64,
    scheme: String,
    host: String,
    port: u16,
    password_configured: bool,
    healthy: bool,
    latency_ms: Option<i64>,
    checked_at: Option<i64>,
    remark: String,
    enabled: bool,
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AdminProxyPool {
    id: i64,
    name: String,
    subscribed: bool,
    source_host: Option<String>,
    interval_secs: i32,
    default_scheme: String,
    trust_proxy_dns: bool,
    probe_url: String,
    revision: i64,
    last_refresh_at: Option<i64>,
    last_refresh_attempt_at: Option<i64>,
    last_check_at: Option<i64>,
    status: String,
    busy: bool,
    nodes: Vec<AdminProxyPoolNode>,
    channel_ids: Vec<i64>,
}
impl From<ProxyPoolView> for AdminProxyPool {
    fn from(v: ProxyPoolView) -> Self {
        Self {
            id: v.id,
            name: v.name,
            subscribed: v.subscribed,
            source_host: v.source_host,
            interval_secs: v.interval_secs,
            default_scheme: v.default_scheme,
            trust_proxy_dns: v.trust_proxy_dns,
            probe_url: v.probe_url,
            revision: v.revision,
            last_refresh_at: v.last_refresh_at,
            last_refresh_attempt_at: v.last_refresh_attempt_at,
            last_check_at: v.last_check_at,
            status: v.status,
            busy: v.busy,
            nodes: v
                .nodes
                .into_iter()
                .map(|n| AdminProxyPoolNode {
                    id: n.id,
                    scheme: n.scheme,
                    host: n.host,
                    port: n.port,
                    password_configured: n.password_configured,
                    healthy: n.healthy,
                    latency_ms: n.latency_ms,
                    checked_at: n.checked_at,
                    remark: n.remark,
                    enabled: n.enabled,
                })
                .collect(),
            channel_ids: v.channel_ids,
        }
    }
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AdminProxyPoolList {
    pools: Vec<AdminProxyPool>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminProxyCleanupRequest {
    target: String,
    mode: String,
    pool_id: Option<i64>,
    revision: Option<i64>,
    #[schema(max_items = 256)]
    ids: Vec<i64>,
    after_id: Option<i64>,
    preview: bool,
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AdminProxyCleanupResult {
    eligible_ids: Vec<i64>,
    deleted_ids: Vec<i64>,
    protected_ids: Vec<i64>,
    skipped_ids: Vec<i64>,
    more_available: bool,
    next_after_id: Option<i64>,
}
#[derive(Serialize, ToSchema)]
pub(crate) struct AdminProxyPoolImportResult {
    pool: AdminProxyPool,
    skipped: usize,
    duplicates: usize,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminProxyPoolBindingRequest {
    /// 0 表示全局，其余值为渠道 ID。
    #[schema(minimum = 0)]
    channel_id: i64,
    #[schema(required = true)]
    pool_id: Option<i64>,
}

pub(crate) fn build_proxy_pool_router(
    service: Arc<dyn AdminCredentialProxyService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let Some(pools) = service.proxy_pools() else {
        return Router::new();
    };
    Router::new()
        .route(
            "/api/admin/proxy-pools",
            get(list_admin_proxy_pools).post(create_admin_proxy_pool),
        )
        .route(
            "/api/admin/proxy-pools/{id}",
            axum::routing::put(update_admin_proxy_pool).delete(delete_admin_proxy_pool),
        )
        .route(
            "/api/admin/proxy-pools/{id}/refresh",
            post(refresh_admin_proxy_pool),
        )
        .route(
            "/api/admin/proxy-pools/{id}/check",
            post(check_admin_proxy_pool),
        )
        .route(
            "/api/admin/proxy-pool-bindings",
            axum::routing::put(bind_admin_proxy_pool),
        )
        .route(
            "/api/admin/proxy-pool-cleanup",
            post(cleanup_admin_proxy_items),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(pools)
}
async fn list_admin_proxy_pools(
    State(service): State<Arc<ProxyPoolService>>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    Ok(json(AdminProxyPoolList {
        pools: service
            .list(auth.principal())
            .await
            .map_err(error)?
            .into_iter()
            .map(Into::into)
            .collect(),
    }))
}
async fn create_admin_proxy_pool(
    State(service): State<Arc<ProxyPoolService>>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminProxyPoolWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let result = service
        .save(auth.principal(), None, request.into())
        .await
        .map_err(error)?;
    let mut response = json(AdminProxyPoolImportResult {
        pool: result.pool.into(),
        skipped: result.skipped,
        duplicates: result.duplicates,
    });
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}
async fn update_admin_proxy_pool(
    State(service): State<Arc<ProxyPoolService>>,
    Path(raw): Path<String>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminProxyPoolWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let result = service
        .save(auth.principal(), Some(id(&raw)?), request.into())
        .await
        .map_err(error)?;
    Ok(json(AdminProxyPoolImportResult {
        pool: result.pool.into(),
        skipped: result.skipped,
        duplicates: result.duplicates,
    }))
}
async fn refresh_admin_proxy_pool(
    State(service): State<Arc<ProxyPoolService>>,
    Path(raw): Path<String>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    Ok(json(AdminProxyPool::from(
        service
            .action(auth.principal(), id(&raw)?, true)
            .await
            .map_err(error)?,
    )))
}
async fn check_admin_proxy_pool(
    State(service): State<Arc<ProxyPoolService>>,
    Path(raw): Path<String>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    Ok(json(AdminProxyPool::from(
        service
            .action(auth.principal(), id(&raw)?, false)
            .await
            .map_err(error)?,
    )))
}
async fn delete_admin_proxy_pool(
    State(service): State<Arc<ProxyPoolService>>,
    Path(raw): Path<String>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    service
        .delete(auth.principal(), id(&raw)?)
        .await
        .map_err(error)?;
    Ok(empty())
}
async fn bind_admin_proxy_pool(
    State(service): State<Arc<ProxyPoolService>>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminProxyPoolBindingRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let pool = request
        .pool_id
        .map(|raw| ProxyId::new(raw).map_err(|_| ManagementError::InvalidRequest))
        .transpose()?;
    service
        .bind(auth.principal(), request.channel_id, pool)
        .await
        .map_err(error)?;
    Ok(empty())
}
async fn cleanup_admin_proxy_items(
    State(service): State<Arc<ProxyPoolService>>,
    Extension(auth): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminProxyCleanupRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let target = match request.target.as_str() {
        "pools" => ProxyCleanupTarget::Pools,
        "nodes" => ProxyCleanupTarget::Nodes,
        _ => return Err(ManagementError::InvalidRequest),
    };
    let mode = match request.mode.as_str() {
        "selected" => ProxyCleanupMode::Selected,
        "failed" => ProxyCleanupMode::Failed,
        "unused" => ProxyCleanupMode::Unused,
        _ => return Err(ManagementError::InvalidRequest),
    };
    let parse = |raw| ProxyId::new(raw).map_err(|_| ManagementError::InvalidRequest);
    let result = service
        .cleanup(
            auth.principal(),
            ProxyCleanupRequest {
                target,
                mode,
                pool_id: request.pool_id.map(parse).transpose()?,
                revision: request.revision,
                ids: request
                    .ids
                    .into_iter()
                    .map(parse)
                    .collect::<Result<_, _>>()?,
                after_id: request.after_id.map(parse).transpose()?,
                preview: request.preview,
            },
        )
        .await
        .map_err(error)?;
    Ok(json(AdminProxyCleanupResult {
        eligible_ids: result.eligible_ids,
        deleted_ids: result.deleted_ids,
        protected_ids: result.protected_ids,
        skipped_ids: result.skipped_ids,
        more_available: result.more_available,
        next_after_id: result.next_after_id,
    }))
}
fn id(raw: &str) -> Result<ProxyId, ManagementError> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    raw.parse()
        .ok()
        .and_then(|v| ProxyId::new(v).ok())
        .ok_or(ManagementError::InvalidRequest)
}
fn error(value: ProxyPoolError) -> ManagementError {
    match value {
        ProxyPoolError::InvalidInput => ManagementError::InvalidRequest,
        ProxyPoolError::Forbidden => ManagementError::Forbidden,
        ProxyPoolError::NotFound => ManagementError::CredentialProxyNotFound,
        ProxyPoolError::Conflict => ManagementError::CredentialProxyConflict,
        ProxyPoolError::Unavailable | ProxyPoolError::Internal => ManagementError::Internal,
    }
}
fn json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn empty() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
