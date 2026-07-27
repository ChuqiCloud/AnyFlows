use af_admin::{AdminChannel, AdminChannelListQuery, AdminChannelPage, AdminChannelReadError};
use af_domain::{ChannelId, CredentialId};
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize)]
pub(crate) struct AdminChannelResponse {
    id: i64,
    #[serde(rename = "type")]
    channel_type: af_domain::ChannelType,
    name: String,
    protocol: af_domain::Protocol,
    base_url: Option<String>,
    status: af_admin::AdminRoutingStatus,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    model_mapping: serde_json::Value,
    param_override: serde_json::Value,
    balance: Option<i64>,
    used_quota: i64,
    tag: Option<String>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize)]
struct AdminChannelListResponse {
    channels: Vec<AdminChannelResponse>,
    next_cursor: Option<i64>,
}

/// 返回管理员可见的渠道列表，按稳定渠道 ID 游标分页。
pub(crate) async fn list_admin_channels(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_channel_list_query(raw_query.as_deref())?;
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = reader
        .list_channels(authentication.principal(), query)
        .await
        .map_err(map_channel_read_error)?;
    Ok(no_store_json(AdminChannelListResponse::from_page(page)))
}

/// 返回单个未软删除渠道的非敏感管理快照。
pub(crate) async fn get_admin_channel(
    State(state): State<HttpState>,
    Path(channel_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let channel = reader
        .get_channel(authentication.principal(), channel_id)
        .await
        .map_err(map_channel_read_error)?;
    Ok(no_store_json(AdminChannelResponse::from_channel(&channel)))
}

impl AdminChannelListResponse {
    fn from_page(page: AdminChannelPage) -> Self {
        Self {
            channels: page
                .channels()
                .iter()
                .map(AdminChannelResponse::from_channel)
                .collect(),
            next_cursor: page.next_cursor().map(ChannelId::get),
        }
    }
}

impl AdminChannelResponse {
    pub(crate) fn from_channel(channel: &AdminChannel) -> Self {
        Self {
            id: channel.channel_id().get(),
            channel_type: channel.channel_type(),
            name: channel.name().to_owned(),
            protocol: channel.protocol(),
            base_url: channel.base_url().map(str::to_owned),
            status: channel.status(),
            weight: channel.weight(),
            priority: channel.priority(),
            auto_ban: channel.auto_ban(),
            model_mapping: channel.model_mapping().clone(),
            param_override: channel.param_override().clone(),
            balance: channel.balance(),
            used_quota: channel.used_quota(),
            tag: channel.tag().map(str::to_owned),
            created_at: channel.created_at(),
            updated_at: channel.updated_at(),
        }
    }
}

fn parse_channel_list_query(
    raw_query: Option<&str>,
) -> Result<AdminChannelListQuery, ManagementError> {
    let (after, limit) = parse_raw_pagination(raw_query, parse_channel_id)?;
    AdminChannelListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_CHANNEL_PAGE_SIZE),
    )
    .map_err(map_channel_read_error)
}

pub(crate) fn parse_raw_pagination<T>(
    raw_query: Option<&str>,
    parse_id: impl Fn(&str) -> Result<T, ManagementError>,
) -> Result<(Option<T>, Option<usize>), ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok((None, None));
    };
    if raw_query.is_empty() {
        return Ok((None, None));
    }
    validate_percent_encoding(raw_query)?;
    let mut seen_after = false;
    let mut seen_limit = false;
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if !seen_after => {
                seen_after = true;
                after = Some(parse_id(&value)?);
            }
            "limit" if !seen_limit => {
                seen_limit = true;
                limit = Some(parse_limit(&value)?);
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok((after, limit))
}

pub(crate) fn parse_channel_id(value: &str) -> Result<ChannelId, ManagementError> {
    ChannelId::new(parse_positive_id(value)?).map_err(|_| ManagementError::InvalidRequest)
}

pub(crate) fn parse_credential_id(value: &str) -> Result<CredentialId, ManagementError> {
    CredentialId::new(parse_positive_id(value)?).map_err(|_| ManagementError::InvalidRequest)
}

fn parse_positive_id(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<i64>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn parse_limit(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
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

pub(crate) fn map_channel_read_error(error: AdminChannelReadError) -> ManagementError {
    match error {
        AdminChannelReadError::InvalidPagination => ManagementError::InvalidRequest,
        AdminChannelReadError::Forbidden => ManagementError::Forbidden,
        AdminChannelReadError::ChannelNotFound => ManagementError::ChannelNotFound,
        AdminChannelReadError::CredentialNotFound => ManagementError::CredentialNotFound,
        AdminChannelReadError::Internal => ManagementError::Internal,
    }
}

pub(crate) fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
