use af_admin::{AdminGroup, AdminGroupListQuery, AdminGroupPage, AdminGroupReadError};
use af_domain::GroupId;
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminGroupWindow)]
pub(crate) struct AdminGroupWindowResponse {
    #[schema(minimum = 0)]
    usage: i64,
    #[schema(minimum = 0)]
    started_at: i64,
    #[schema(minimum = 1)]
    resets_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminGroup)]
pub(crate) struct AdminGroupResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 64)]
    name: String,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    /// 百万分比定点倍率，1000000 表示 1.0。
    #[schema(minimum = 0)]
    ratio_micros: i64,
    #[schema(minimum = 0, required = true)]
    peak_ratio_micros: Option<i64>,
    #[schema(
        pattern = "^([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]$",
        required = true
    )]
    peak_start: Option<String>,
    #[schema(
        pattern = "^([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]$",
        required = true
    )]
    peak_end: Option<String>,
    is_exclusive: bool,
    #[schema(minimum = 0, required = true)]
    daily_limit: Option<i64>,
    #[schema(minimum = 0, required = true)]
    weekly_limit: Option<i64>,
    #[schema(minimum = 0, required = true)]
    monthly_limit: Option<i64>,
    daily_window: AdminGroupWindowResponse,
    weekly_window: AdminGroupWindowResponse,
    monthly_window: AdminGroupWindowResponse,
    #[schema(minimum = 0, required = true)]
    rpm_limit: Option<i32>,
    #[schema(minimum = 1, required = true)]
    fallback_group_id: Option<i64>,
    /// 受限 JSON 对象，服务端编码后最大 16 KiB。
    #[schema(schema_with = crate::openapi::schema::free_form_object_schema)]
    flags: serde_json::Value,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminGroupListResponse)]
pub(crate) struct AdminGroupListResponse {
    #[schema(max_items = 100)]
    groups: Vec<AdminGroupResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 返回管理员可见的分组列表，按稳定分组 ID 游标分页。
pub(crate) async fn list_admin_groups(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_list_query(raw_query.as_deref())?;
    let page = state
        .admin_group_reader
        .list(authentication.principal(), query)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminGroupListResponse::from_page(page)))
}

/// 返回单个未软删除分组的完整管理快照。
pub(crate) async fn get_admin_group(
    State(state): State<HttpState>,
    Path(group_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let group_id = parse_group_id(&group_id)?;
    let group = state
        .admin_group_reader
        .get(authentication.principal(), group_id)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminGroupResponse::from_group(&group)))
}

impl AdminGroupListResponse {
    fn from_page(page: AdminGroupPage) -> Self {
        Self {
            groups: page
                .groups()
                .iter()
                .map(AdminGroupResponse::from_group)
                .collect(),
            next_cursor: page.next_cursor().map(GroupId::get),
        }
    }
}

impl AdminGroupResponse {
    pub(crate) fn from_group(group: &AdminGroup) -> Self {
        let (peak_ratio_micros, peak_start, peak_end) = match group.peak() {
            Some(peak) => (
                Some(peak.ratio_micros()),
                Some(format_second_of_day(peak.start_second())),
                Some(format_second_of_day(peak.end_second())),
            ),
            None => (None, None, None),
        };
        Self {
            id: group.group_id().get(),
            name: group.name().to_owned(),
            display_name: group.display_name().to_owned(),
            ratio_micros: group.ratio_micros(),
            peak_ratio_micros,
            peak_start,
            peak_end,
            is_exclusive: group.is_exclusive(),
            daily_limit: group.daily_limit(),
            weekly_limit: group.weekly_limit(),
            monthly_limit: group.monthly_limit(),
            daily_window: AdminGroupWindowResponse::from_window(group.daily_window()),
            weekly_window: AdminGroupWindowResponse::from_window(group.weekly_window()),
            monthly_window: AdminGroupWindowResponse::from_window(group.monthly_window()),
            rpm_limit: group.rpm_limit(),
            fallback_group_id: group.fallback_group_id().map(GroupId::get),
            flags: group.flags().clone(),
        }
    }
}

impl AdminGroupWindowResponse {
    fn from_window(window: af_admin::AdminGroupWindow) -> Self {
        Self {
            usage: window.usage(),
            started_at: window.started_at(),
            resets_at: window.resets_at(),
        }
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminGroupListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminGroupListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminGroupListQuery::default());
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
            "after" => {
                if seen_after {
                    return Err(ManagementError::InvalidRequest);
                }
                seen_after = true;
                after = Some(parse_group_id(&value)?);
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
    AdminGroupListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_GROUP_PAGE_SIZE),
    )
    .map_err(map_read_error)
}

pub(crate) fn parse_group_id(value: &str) -> Result<GroupId, ManagementError> {
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
    GroupId::new(parsed).map_err(|_| ManagementError::InvalidRequest)
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

fn format_second_of_day(second: u32) -> String {
    let hour = second / 3_600;
    let minute = (second % 3_600) / 60;
    let second = second % 60;
    format!("{hour:02}:{minute:02}:{second:02}")
}

fn map_read_error(error: AdminGroupReadError) -> ManagementError {
    match error {
        AdminGroupReadError::InvalidPagination => ManagementError::InvalidRequest,
        AdminGroupReadError::Forbidden => ManagementError::Forbidden,
        AdminGroupReadError::NotFound => ManagementError::GroupNotFound,
        AdminGroupReadError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
