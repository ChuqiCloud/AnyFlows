use af_admin::{AdminUsageLog, AdminUsageLogListQuery, AdminUsageLogPage, AdminUsageLogReadError};
use axum::{
    Json,
    extract::{Extension, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUsageLog)]
pub(crate) struct AdminUsageLogResponse {
    #[schema(minimum = 1)]
    id: i64,
    /// 非全零的 32 位小写十六进制计费事件标识。
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    event_id: String,
    #[schema(minimum = 1)]
    user_id: i64,
    #[schema(min_length = 1, max_length = 64)]
    username: String,
    #[schema(minimum = 1)]
    token_id: i64,
    #[schema(minimum = 1)]
    group_id: i64,
    #[schema(minimum = 1, required = true)]
    organization_id: Option<i64>,
    #[schema(minimum = 1, required = true)]
    organization_team_id: Option<i64>,
    #[schema(value_type = crate::openapi::schema::AdminUsageLogBillingModeSchema)]
    billing_mode: af_admin::AdminUsageLogBillingMode,
    #[schema(minimum = 0)]
    input_tokens: i64,
    #[schema(minimum = 0)]
    output_tokens: i64,
    #[schema(minimum = 0)]
    cache_read: i64,
    #[schema(minimum = 0)]
    cache_creation_5m: i64,
    #[schema(minimum = 0)]
    cache_creation_1h: i64,
    #[schema(minimum = 0)]
    reasoning_tokens: i64,
    #[schema(minimum = 0)]
    audio_input_tokens: i64,
    #[schema(minimum = 0)]
    audio_output_tokens: i64,
    /// 音频文件时长纳秒；非 Audio 请求或无真实事实时为 null。
    #[schema(minimum = 0, maximum = 86400000000000_i64, required = true)]
    audio_duration_nanoseconds: Option<i64>,
    /// 视频成功终态返回的真实时长秒数；非视频或缺少可信事实时为 null。
    #[schema(minimum = 1, maximum = 86400_i64, required = true)]
    video_duration_seconds: Option<i64>,
    /// 请求明确携带的视频分辨率；交由上游选择默认值时为 null。
    #[schema(
        value_type = Option<crate::openapi::schema::AdminUsageLogVideoResolutionSchema>,
        required = true
    )]
    video_resolution: Option<af_admin::AdminUsageLogVideoResolution>,
    #[schema(min_length = 1, max_length = 128, required = true)]
    request_id: Option<String>,
    #[schema(min_length = 1, max_length = 255, required = true)]
    model: Option<String>,
    #[schema(value_type = Option<crate::openapi::schema::UsageLogProtocolSchema>, required = true)]
    protocol: Option<af_admin::UsageLogProtocol>,
    #[schema(value_type = Option<crate::openapi::schema::UsageLogOperationSchema>, required = true)]
    operation: Option<af_admin::UsageLogOperation>,
    #[schema(required = true)]
    is_stream: Option<bool>,
    #[schema(value_type = Option<crate::openapi::schema::UsageLogReasoningEffortSchema>, required = true)]
    reasoning_effort: Option<af_admin::UsageLogReasoningEffort>,
    #[schema(minimum = 0, required = true)]
    reasoning_budget_tokens: Option<i64>,
    #[schema(minimum = 0, required = true)]
    first_token_ms: Option<i64>,
    #[schema(minimum = 0, required = true)]
    duration_ms: Option<i64>,
    #[schema(value_type = crate::openapi::schema::AdminUsageLogSourceSchema)]
    usage_source: af_admin::AdminUsageLogSource,
    #[schema(value_type = crate::openapi::schema::AdminUsageLogSemanticsSchema)]
    usage_semantics: af_admin::AdminUsageLogSemantics,
    #[schema(minimum = 0)]
    quota: i64,
    /// Unix 秒时间戳。
    #[schema(minimum = 0)]
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminUsageLogListResponse)]
pub(crate) struct AdminUsageLogListResponse {
    #[schema(max_items = 100)]
    logs: Vec<AdminUsageLogResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserUsageLog)]
pub(crate) struct UserUsageLogResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(minimum = 1)]
    token_id: i64,
    #[schema(min_length = 1, max_length = 128, required = true)]
    request_id: Option<String>,
    #[schema(min_length = 1, max_length = 255, required = true)]
    model: Option<String>,
    #[schema(value_type = Option<crate::openapi::schema::UsageLogProtocolSchema>, required = true)]
    protocol: Option<af_admin::UsageLogProtocol>,
    #[schema(value_type = Option<crate::openapi::schema::UsageLogOperationSchema>, required = true)]
    operation: Option<af_admin::UsageLogOperation>,
    #[schema(required = true)]
    is_stream: Option<bool>,
    #[schema(value_type = Option<crate::openapi::schema::UsageLogReasoningEffortSchema>, required = true)]
    reasoning_effort: Option<af_admin::UsageLogReasoningEffort>,
    #[schema(minimum = 0, required = true)]
    reasoning_budget_tokens: Option<i64>,
    #[schema(minimum = 0, required = true)]
    first_token_ms: Option<i64>,
    #[schema(minimum = 0, required = true)]
    duration_ms: Option<i64>,
    #[schema(value_type = crate::openapi::schema::AdminUsageLogBillingModeSchema)]
    billing_mode: af_admin::AdminUsageLogBillingMode,
    #[schema(minimum = 0)]
    input_tokens: i64,
    #[schema(minimum = 0)]
    output_tokens: i64,
    #[schema(minimum = 0)]
    cache_read: i64,
    #[schema(minimum = 0)]
    cache_creation_5m: i64,
    #[schema(minimum = 0)]
    cache_creation_1h: i64,
    #[schema(minimum = 0)]
    reasoning_tokens: i64,
    #[schema(minimum = 0)]
    quota: i64,
    created_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserUsageLogListResponse)]
pub(crate) struct UserUsageLogListResponse {
    #[schema(max_items = 100)]
    logs: Vec<UserUsageLogResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 返回管理员可见的最新已确认用量日志。
pub(crate) async fn list_admin_usage_logs(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let reader = state
        .admin_usage_log_reader
        .as_deref()
        .ok_or(ManagementError::Internal)?;
    let page = reader
        .list(authentication.principal(), query)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminUsageLogListResponse::from_page(page)))
}

/// 返回当前会话用户自己的最新已结算调用日志。
pub(crate) async fn list_user_usage_logs(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let reader = state
        .admin_usage_log_reader
        .as_deref()
        .ok_or(ManagementError::Internal)?;
    let page = reader
        .list_own(authentication.principal(), query)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(UserUsageLogListResponse::from_page(page)))
}

impl AdminUsageLogListResponse {
    fn from_page(page: AdminUsageLogPage) -> Self {
        Self {
            logs: page
                .logs()
                .iter()
                .map(AdminUsageLogResponse::from_log)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl AdminUsageLogResponse {
    pub(crate) fn from_log(log: &AdminUsageLog) -> Self {
        let usage = log.usage();
        Self {
            id: log.id(),
            event_id: log.event_id().to_owned(),
            user_id: log.user_id().get(),
            username: log.username().to_owned(),
            token_id: log.token_id().get(),
            group_id: log.group_id().get(),
            organization_id: log.organization_id().map(|value| value.get()),
            organization_team_id: log.organization_team_id().map(|value| value.get()),
            billing_mode: log.billing_mode(),
            input_tokens: usage.input_tokens(),
            output_tokens: usage.output_tokens(),
            cache_read: usage.cache_read(),
            cache_creation_5m: usage.cache_creation_5m(),
            cache_creation_1h: usage.cache_creation_1h(),
            reasoning_tokens: usage.reasoning_tokens(),
            audio_input_tokens: usage.audio_input_tokens(),
            audio_output_tokens: usage.audio_output_tokens(),
            audio_duration_nanoseconds: log.audio_duration_nanoseconds(),
            video_duration_seconds: log.video_duration_seconds(),
            video_resolution: log.video_resolution(),
            request_id: log.request_id().map(str::to_owned),
            model: log.model().map(str::to_owned),
            protocol: log.protocol(),
            operation: log.operation(),
            is_stream: log.is_stream(),
            reasoning_effort: log.reasoning_effort(),
            reasoning_budget_tokens: log.reasoning_budget_tokens(),
            first_token_ms: log.first_token_ms(),
            duration_ms: log.duration_ms(),
            usage_source: log.source(),
            usage_semantics: log.semantics(),
            quota: log.quota(),
            created_at: log.created_at(),
        }
    }
}

impl UserUsageLogListResponse {
    fn from_page(page: AdminUsageLogPage) -> Self {
        Self {
            logs: page
                .logs()
                .iter()
                .map(UserUsageLogResponse::from_log)
                .collect(),
            next_cursor: page.next_cursor(),
        }
    }
}

impl UserUsageLogResponse {
    fn from_log(log: &AdminUsageLog) -> Self {
        let usage = log.usage();
        Self {
            id: log.id(),
            token_id: log.token_id().get(),
            request_id: log.request_id().map(str::to_owned),
            model: log.model().map(str::to_owned),
            protocol: log.protocol(),
            operation: log.operation(),
            is_stream: log.is_stream(),
            reasoning_effort: log.reasoning_effort(),
            reasoning_budget_tokens: log.reasoning_budget_tokens(),
            first_token_ms: log.first_token_ms(),
            duration_ms: log.duration_ms(),
            billing_mode: log.billing_mode(),
            input_tokens: usage.input_tokens(),
            output_tokens: usage.output_tokens(),
            cache_read: usage.cache_read(),
            cache_creation_5m: usage.cache_creation_5m(),
            cache_creation_1h: usage.cache_creation_1h(),
            reasoning_tokens: usage.reasoning_tokens(),
            quota: log.quota(),
            created_at: log.created_at(),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminUsageLogListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminUsageLogListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminUsageLogListQuery::default());
    }
    validate_percent_encoding(raw_query)?;
    let mut seen_before = false;
    let mut seen_limit = false;
    let mut before = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "before" => {
                if seen_before {
                    return Err(ManagementError::InvalidRequest);
                }
                seen_before = true;
                before = Some(parse_positive_i64(&value)?);
            }
            "limit" => {
                if seen_limit {
                    return Err(ManagementError::InvalidRequest);
                }
                seen_limit = true;
                limit = Some(parse_limit(&value)?);
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminUsageLogListQuery::new(
        before,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_USAGE_LOG_PAGE_SIZE),
    )
    .map_err(map_read_error)
}

pub(crate) fn parse_positive_i64(value: &str) -> Result<i64, ManagementError> {
    if value.is_empty()
        || value.starts_with('+')
        || value.starts_with('-')
        || value.chars().any(|character| !character.is_ascii_digit())
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

pub(crate) fn parse_limit(value: &str) -> Result<usize, ManagementError> {
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

pub(crate) fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
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

fn map_read_error(error: AdminUsageLogReadError) -> ManagementError {
    match error {
        AdminUsageLogReadError::InvalidPagination => ManagementError::InvalidRequest,
        AdminUsageLogReadError::Forbidden => ManagementError::Forbidden,
        AdminUsageLogReadError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_parser_rejects_unstable_inputs() {
        assert_eq!(parse_list_query(None).unwrap().limit(), 50);
        assert_eq!(
            parse_list_query(Some("before=9&limit=100"))
                .unwrap()
                .before(),
            Some(9)
        );
        for raw_query in [
            "before=0",
            "before=-1",
            "before=1&before=2",
            "limit=0",
            "limit=101",
            "limit=1&limit=2",
            "unknown=1",
            "before=%",
            "before=%ff",
        ] {
            assert_eq!(
                parse_list_query(Some(raw_query)),
                Err(ManagementError::InvalidRequest),
                "{raw_query}"
            );
        }
    }
}
