use af_admin::{AdminCredential, AdminCredentialListQuery, AdminCredentialPage};
use af_domain::CredentialId;
use axum::{
    extract::{Extension, Path, RawQuery, State},
    response::Response,
};
use serde::Serialize;

use crate::{
    chat_completions::HttpState,
    management_channels::{
        map_channel_read_error, no_store_json, parse_channel_id, parse_credential_id,
        parse_raw_pagination,
    },
    management_error::ManagementError,
};

#[derive(Serialize)]
struct AdminCredentialResponse {
    id: i64,
    channel_id: i64,
    kind: af_domain::CredentialKind,
    status: af_admin::AdminRoutingStatus,
    multi_key_mode: Option<af_admin::AdminCredentialMultiKeyMode>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    rate_limited_at: Option<i64>,
    rate_limit_reset_at: Option<i64>,
    overload_until: Option<i64>,
    temp_unschedulable_until: Option<i64>,
    session_window_start: Option<i64>,
    session_window_end: Option<i64>,
    parent_id: Option<i64>,
    quota_dimension: af_admin::AdminCredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
    last_used_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize)]
struct AdminCredentialListResponse {
    credentials: Vec<AdminCredentialResponse>,
    next_cursor: Option<i64>,
}

/// 返回指定渠道下的非敏感凭据列表。
pub(crate) async fn list_admin_credentials(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let (after, limit) = parse_raw_pagination(raw_query.as_deref(), parse_credential_id)?;
    let query = AdminCredentialListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_CHANNEL_PAGE_SIZE),
    )
    .map_err(map_channel_read_error)?;
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = reader
        .list_credentials(authentication.principal(), channel_id, query)
        .await
        .map_err(map_channel_read_error)?;
    Ok(no_store_json(AdminCredentialListResponse::from_page(page)))
}

/// 返回指定渠道下单个凭据的非敏感元数据。
pub(crate) async fn get_admin_credential(
    State(state): State<HttpState>,
    Path((channel_id, credential_id)): Path<(String, String)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let credential_id = parse_credential_id(&credential_id)?;
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let credential = reader
        .get_credential(authentication.principal(), channel_id, credential_id)
        .await
        .map_err(map_channel_read_error)?;
    Ok(no_store_json(AdminCredentialResponse::from_credential(
        &credential,
    )))
}

impl AdminCredentialListResponse {
    fn from_page(page: AdminCredentialPage) -> Self {
        Self {
            credentials: page
                .credentials()
                .iter()
                .map(AdminCredentialResponse::from_credential)
                .collect(),
            next_cursor: page.next_cursor().map(CredentialId::get),
        }
    }
}

impl AdminCredentialResponse {
    fn from_credential(credential: &AdminCredential) -> Self {
        Self {
            id: credential.credential_id().get(),
            channel_id: credential.channel_id().get(),
            kind: credential.kind(),
            status: credential.status(),
            multi_key_mode: credential.multi_key_mode(),
            priority: credential.priority(),
            weight: credential.weight(),
            concurrency: credential.concurrency(),
            load_factor_micros: credential.load_factor_micros(),
            rate_multiplier_micros: credential.rate_multiplier_micros(),
            schedulable: credential.schedulable(),
            rate_limited_at: credential.rate_limited_at(),
            rate_limit_reset_at: credential.rate_limit_reset_at(),
            overload_until: credential.overload_until(),
            temp_unschedulable_until: credential.temp_unschedulable_until(),
            session_window_start: credential.session_window_start(),
            session_window_end: credential.session_window_end(),
            parent_id: credential.parent_id().map(CredentialId::get),
            quota_dimension: credential.quota_dimension(),
            proxy_id: credential.proxy_id(),
            oauth_provider: credential.oauth_provider().map(str::to_owned),
            oauth_account_key: credential.oauth_account_key().map(str::to_owned),
            oauth_project_id: credential.oauth_project_id().map(str::to_owned),
            last_used_at: credential.last_used_at(),
            created_at: credential.created_at(),
            updated_at: credential.updated_at(),
        }
    }
}
