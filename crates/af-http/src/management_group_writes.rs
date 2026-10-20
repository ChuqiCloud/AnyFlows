use af_admin::{
    AdminGroupCreateCommand, AdminGroupPeakCommand, AdminGroupUpdateCommand, AdminGroupWriteError,
};
use af_domain::{GroupId, GroupPeakSchedule, GroupPeakWindow};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_groups::{AdminGroupResponse, parse_group_id},
};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminGroupWriteRequest)]
/// 管理端创建或完整更新分组时使用的写入正文。
pub(crate) struct AdminGroupWriteRequest {
    #[schema(min_length = 1, max_length = 64)]
    name: String,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    /// 百万分比定点倍率，1000000 表示 1.0。
    #[schema(minimum = 0)]
    ratio_micros: i64,
    /// 与 peak_start、peak_end 同时为空或同时有值。
    #[schema(minimum = 0)]
    peak_ratio_micros: Option<i64>,
    #[schema(pattern = "^([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]$")]
    peak_start: Option<String>,
    #[schema(pattern = "^([01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9]$")]
    peak_end: Option<String>,
    /// 周计划优先于旧每日窗口；省略或 null 保留现有计划，空数组关闭分时倍率。
    #[schema(max_items = 32)]
    peak_windows: Option<Vec<AdminGroupPeakWindow>>,
    is_exclusive: bool,
    #[schema(minimum = 0)]
    daily_limit: Option<i64>,
    #[schema(minimum = 0)]
    weekly_limit: Option<i64>,
    #[schema(minimum = 0)]
    monthly_limit: Option<i64>,
    #[schema(minimum = 0)]
    rpm_limit: Option<i32>,
    #[schema(minimum = 1)]
    fallback_group_id: Option<i64>,
    /// 受限 JSON 对象，服务端编码后最大 16 KiB。
    #[schema(schema_with = crate::openapi::schema::free_form_object_schema)]
    flags: serde_json::Value,
}

/// 创建一个管理端分组，并返回完整管理快照。
pub(crate) async fn create_admin_group(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminGroupWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let group = state
        .admin_group_writer
        .create(authentication.principal(), request.into_create_command()?)
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminGroupResponse::from_group(&group),
    ))
}

/// 完整更新一个未软删除分组的可写业务字段。
pub(crate) async fn update_admin_group(
    State(state): State<HttpState>,
    Path(group_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminGroupWriteRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let group_id = parse_group_id(&group_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let group = state
        .admin_group_writer
        .update(
            authentication.principal(),
            group_id,
            request.into_update_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminGroupResponse::from_group(&group)))
}

/// 安全软删除分组；仍被有效用户或有效令牌引用时返回冲突。
pub(crate) async fn delete_admin_group(
    State(state): State<HttpState>,
    Path(group_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let group_id = parse_group_id(&group_id)?;
    state
        .admin_group_writer
        .delete(authentication.principal(), group_id)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminGroupWriteRequest {
    fn into_create_command(self) -> Result<AdminGroupCreateCommand, ManagementError> {
        let peak = self.peak()?;
        let schedule = parse_peak_schedule(self.peak_windows)?;
        AdminGroupCreateCommand::new(
            self.name,
            self.display_name,
            self.ratio_micros,
            peak,
            self.is_exclusive,
            self.daily_limit,
            self.weekly_limit,
            self.monthly_limit,
            self.rpm_limit,
            parse_optional_group_id(self.fallback_group_id)?,
            self.flags,
        )
        .map(|command| command.with_peak_schedule(schedule))
        .map_err(map_write_error)
    }

    fn into_update_command(self) -> Result<AdminGroupUpdateCommand, ManagementError> {
        let peak = self.peak()?;
        let schedule = parse_peak_schedule(self.peak_windows)?;
        AdminGroupUpdateCommand::new(
            self.name,
            self.display_name,
            self.ratio_micros,
            peak,
            self.is_exclusive,
            self.daily_limit,
            self.weekly_limit,
            self.monthly_limit,
            self.rpm_limit,
            parse_optional_group_id(self.fallback_group_id)?,
            self.flags,
        )
        .map(|command| command.with_peak_schedule(schedule))
        .map_err(map_write_error)
    }

    fn peak(&self) -> Result<Option<AdminGroupPeakCommand>, ManagementError> {
        match (
            self.peak_ratio_micros,
            self.peak_start.as_deref(),
            self.peak_end.as_deref(),
        ) {
            (None, None, None) => Ok(None),
            (Some(ratio), Some(start), Some(end)) => AdminGroupPeakCommand::new(
                ratio,
                parse_second_of_day(start)?,
                parse_second_of_day(end)?,
            )
            .map(Some)
            .map_err(map_write_error),
            _ => Err(ManagementError::InvalidRequest),
        }
    }
}

/// 分组周计划规则；时间为所选 UTC 偏移下的日内秒数，跨午夜归属起始星期。
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdminGroupPeakWindow {
    /// ISO 星期，1 为周一、7 为周日，不允许重复。
    #[schema(min_items = 1, max_items = 7)]
    weekdays: Vec<u8>,
    #[schema(minimum = 0, maximum = 86399)]
    start_second: u32,
    /// 不包含终点，86400 表示当日结束；小于起点表示跨午夜。
    #[schema(minimum = 0, maximum = 86400)]
    end_second: u32,
    /// 额外倍率的百万分比值；2000000 为翻倍，500000 为五折。
    #[schema(minimum = 0)]
    ratio_micros: i64,
    /// 精确匹配公开计费模型名称；空数组表示分组内全部模型。
    #[serde(default)]
    #[schema(max_items = 64)]
    models: Vec<String>,
    /// 固定 UTC 偏移分钟数，例如北京时间为 480，UTC 为 0。
    #[serde(default)]
    #[schema(minimum = -720, maximum = 840)]
    utc_offset_minutes: i16,
}

impl AdminGroupPeakWindow {
    pub(crate) fn from_window(window: &GroupPeakWindow) -> Self {
        Self {
            weekdays: window.weekdays().to_vec(),
            start_second: window.start_second(),
            end_second: window.end_second(),
            ratio_micros: window.ratio_micros(),
            models: window.models().to_vec(),
            utc_offset_minutes: window.utc_offset_minutes(),
        }
    }

    fn into_window(self) -> Result<GroupPeakWindow, ManagementError> {
        GroupPeakWindow::new(
            self.weekdays,
            self.start_second,
            self.end_second,
            self.ratio_micros,
            self.models,
            self.utc_offset_minutes,
        )
        .map_err(|_| ManagementError::InvalidRequest)
    }
}

fn parse_peak_schedule(
    windows: Option<Vec<AdminGroupPeakWindow>>,
) -> Result<Option<GroupPeakSchedule>, ManagementError> {
    windows
        .map(|windows| {
            let windows = windows
                .into_iter()
                .map(AdminGroupPeakWindow::into_window)
                .collect::<Result<Vec<_>, _>>()?;
            GroupPeakSchedule::new(windows).map_err(|_| ManagementError::InvalidRequest)
        })
        .transpose()
}

fn parse_optional_group_id(value: Option<i64>) -> Result<Option<GroupId>, ManagementError> {
    value
        .map(|value| GroupId::new(value).map_err(|_| ManagementError::InvalidRequest))
        .transpose()
}

fn parse_second_of_day(value: &str) -> Result<u32, ManagementError> {
    let bytes = value.as_bytes();
    if bytes.len() != 8
        || bytes[2] != b':'
        || bytes[5] != b':'
        || [0, 1, 3, 4, 6, 7]
            .into_iter()
            .any(|index| !bytes[index].is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    let hour = u32::from(bytes[0] - b'0') * 10 + u32::from(bytes[1] - b'0');
    let minute = u32::from(bytes[3] - b'0') * 10 + u32::from(bytes[4] - b'0');
    let second = u32::from(bytes[6] - b'0') * 10 + u32::from(bytes[7] - b'0');
    if hour >= 24 || minute >= 60 || second >= 60 {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(hour * 3_600 + minute * 60 + second)
}

fn map_write_error(error: AdminGroupWriteError) -> ManagementError {
    match error {
        AdminGroupWriteError::InvalidInput => ManagementError::InvalidRequest,
        AdminGroupWriteError::Forbidden => ManagementError::Forbidden,
        AdminGroupWriteError::Conflict => ManagementError::GroupConflict,
        AdminGroupWriteError::NotFound => ManagementError::GroupNotFound,
        AdminGroupWriteError::InUse => ManagementError::GroupInUse,
        AdminGroupWriteError::RuntimeRefreshFailed => ManagementError::Internal,
        AdminGroupWriteError::Internal => ManagementError::Internal,
    }
}

fn no_store_json(value: impl serde::Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn status_json(status: StatusCode, value: impl serde::Serialize) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = status;
    response
}

fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekly_schedule_parser_distinguishes_omission_empty_and_valid_rules() {
        assert_eq!(parse_peak_schedule(None).unwrap(), None);
        assert!(
            parse_peak_schedule(Some(Vec::new()))
                .unwrap()
                .unwrap()
                .windows()
                .is_empty()
        );
        let rule = serde_json::json!({
            "weekdays": [1, 3], "start_second": 82800, "end_second": 7200,
            "ratio_micros": 500000, "models": ["deepseek-chat"], "utc_offset_minutes": 480
        });
        let rules = serde_json::from_value(serde_json::json!([rule.clone()])).unwrap();
        let schedule = parse_peak_schedule(Some(rules)).unwrap().unwrap();
        assert_eq!(schedule.windows()[0].models(), ["deepseek-chat"]);
        let response = AdminGroupPeakWindow::from_window(&schedule.windows()[0]);
        assert_eq!(serde_json::to_value(response).unwrap(), rule);
        let duplicates = serde_json::from_value(serde_json::json!([rule.clone(), rule])).unwrap();
        assert!(matches!(
            parse_peak_schedule(Some(duplicates)),
            Err(ManagementError::InvalidRequest)
        ));
    }

    #[test]
    fn peak_time_parser_requires_canonical_clock_text() {
        assert_eq!(parse_second_of_day("08:30:05").unwrap(), 30_605);
        for value in ["8:30:05", "24:00:00", "08:60:00", "08:30:60", "08-30-05"] {
            assert_eq!(
                parse_second_of_day(value),
                Err(ManagementError::InvalidRequest),
                "{value}"
            );
        }
    }
}
