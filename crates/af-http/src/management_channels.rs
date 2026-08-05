use af_admin::{AdminChannel, AdminChannelListQuery, AdminChannelPage, AdminChannelReadError};
use af_domain::{ChannelAutoBanRules, ChannelId, CredentialId};
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

/// 管理端可配置的精确 5xx 状态码与关键词规则。
#[derive(Clone, Default, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminChannelAutoBanRules)]
pub(crate) struct AdminChannelAutoBanRulesDto {
    #[schema(min_items = 0, max_items = 64)]
    pub(crate) status_codes: Vec<u16>,
    #[schema(min_items = 0, max_items = 64)]
    pub(crate) keywords: Vec<String>,
}

impl AdminChannelAutoBanRulesDto {
    pub(crate) fn into_domain(self) -> Result<ChannelAutoBanRules, ManagementError> {
        ChannelAutoBanRules::new(self.status_codes, self.keywords)
            .map_err(|_| ManagementError::InvalidRequest)
    }
}

impl From<&ChannelAutoBanRules> for AdminChannelAutoBanRulesDto {
    fn from(rules: &ChannelAutoBanRules) -> Self {
        Self {
            status_codes: rules
                .server_statuses()
                .iter()
                .map(|status| status.get())
                .collect(),
            keywords: rules.keywords().to_vec(),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminChannel)]
pub(crate) struct AdminChannelResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[serde(rename = "type")]
    #[schema(value_type = crate::openapi::schema::AdminChannelTypeSchema)]
    channel_type: af_domain::ChannelType,
    #[schema(min_length = 1, max_length = 128)]
    name: String,
    #[schema(value_type = crate::openapi::schema::AdminChannelProtocolSchema)]
    protocol: af_domain::Protocol,
    /// 不含 userinfo、查询串或片段的 HTTP(S) 基础地址。
    #[schema(max_length = 2048, format = "uri", required = true)]
    base_url: Option<String>,
    /// 渠道级读取与完整请求超时；null 表示使用对应场景的服务默认值。
    #[schema(minimum = 1, maximum = 900, required = true)]
    timeout_secs: Option<u64>,
    #[schema(value_type = crate::openapi::schema::AdminRoutingStatusSchema)]
    status: af_admin::AdminRoutingStatus,
    #[schema(minimum = 0)]
    weight: i32,
    priority: i32,
    auto_ban: bool,
    auto_ban_rules: AdminChannelAutoBanRulesDto,
    /// 是否显式允许该原生 Responses 渠道使用 WebSocket。
    responses_websocket_enabled: bool,
    #[schema(schema_with = crate::openapi::schema::channel_models_schema)]
    models: Vec<String>,
    #[schema(schema_with = crate::openapi::schema::channel_group_ids_schema)]
    group_ids: Vec<i64>,
    #[schema(schema_with = crate::openapi::schema::model_mapping_schema)]
    model_mapping: serde_json::Value,
    /// 不包含认证信息的闭合 Canonical 采样参数覆盖。
    #[schema(schema_with = crate::openapi::schema::channel_parameter_overrides_schema)]
    param_override: serde_json::Value,
    #[schema(required = true)]
    balance: Option<i64>,
    #[schema(minimum = 0)]
    used_quota: i64,
    #[schema(min_length = 1, max_length = 64, required = true)]
    tag: Option<String>,
    /// Unix 秒时间戳。
    #[schema(minimum = 0)]
    created_at: i64,
    /// Unix 秒时间戳。
    #[schema(minimum = 0)]
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminChannelListResponse)]
pub(crate) struct AdminChannelListResponse {
    #[schema(max_items = 100)]
    channels: Vec<AdminChannelResponse>,
    #[schema(minimum = 1, required = true)]
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
            timeout_secs: channel.timeout().map(af_domain::ChannelTimeout::seconds),
            status: channel.status(),
            weight: channel.weight(),
            priority: channel.priority(),
            auto_ban: channel.auto_ban(),
            auto_ban_rules: AdminChannelAutoBanRulesDto::from(channel.auto_ban_rules()),
            responses_websocket_enabled: channel.responses_websocket_enabled(),
            models: channel.models().to_vec(),
            group_ids: channel
                .group_ids()
                .iter()
                .copied()
                .map(af_domain::GroupId::get)
                .collect(),
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

pub(crate) fn status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = status;
    response
}

pub(crate) fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
