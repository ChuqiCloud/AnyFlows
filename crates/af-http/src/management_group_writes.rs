use af_admin::{
    AdminGroupCreateCommand, AdminGroupPeakCommand, AdminGroupUpdateCommand, AdminGroupWriteError,
};
use af_domain::GroupId;
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Deserialize;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_groups::{AdminGroupResponse, parse_group_id},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
/// 管理端创建或完整更新分组时使用的写入正文。
pub(crate) struct AdminGroupWriteRequest {
    name: String,
    display_name: String,
    ratio_micros: i64,
    peak_ratio_micros: Option<i64>,
    peak_start: Option<String>,
    peak_end: Option<String>,
    is_exclusive: bool,
    daily_limit: Option<i64>,
    weekly_limit: Option<i64>,
    monthly_limit: Option<i64>,
    rpm_limit: Option<i32>,
    fallback_group_id: Option<i64>,
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
        .map_err(map_write_error)
    }

    fn into_update_command(self) -> Result<AdminGroupUpdateCommand, ManagementError> {
        let peak = self.peak()?;
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
