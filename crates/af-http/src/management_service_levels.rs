use af_admin::{PlatformPolicy, SessionAuthentication};
use af_analytics::{
    AdminDashboardAccess, AdminDashboardReadError, ServiceLevelDimension, ServiceLevelQuery,
};
use af_domain::PlatformPermission;
use axum::{
    extract::{Extension, RawQuery, State},
    response::Response,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Serialize, ToSchema)]
#[schema(as = ServiceLevelPoint)]
pub(crate) struct ServiceLevelPointResponse {
    period_start: i64,
    successful_request_count: i64,
    failed_request_count: i64,
    unknown_request_count: i64,
}

#[derive(Serialize, ToSchema)]
#[schema(as = ServiceLevelRow)]
pub(crate) struct ServiceLevelRowResponse {
    key: String,
    name: String,
    request_count: i64,
    successful_request_count: i64,
    failed_request_count: i64,
    unknown_request_count: i64,
    average_duration_ms: Option<i64>,
    hourly: Vec<ServiceLevelPointResponse>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = ServiceLevelReport)]
pub(crate) struct ServiceLevelReportResponse {
    period_start: i64,
    period_end: i64,
    total: i64,
    unattributed_request_count: i64,
    items: Vec<ServiceLevelRowResponse>,
}

pub(crate) async fn get_service_levels(
    State(state): State<HttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, ManagementError> {
    if !PlatformPolicy::allows(
        authentication.principal(),
        PlatformPermission::DashboardReadAll,
    ) {
        return Err(ManagementError::Forbidden);
    }
    let query = parse_query(raw_query.as_deref())?;
    let reader = state
        .admin_dashboard_reader
        .as_deref()
        .ok_or(ManagementError::Internal)?;
    let report = reader
        .service_levels(AdminDashboardAccess::Admin, query)
        .await
        .map_err(|error| match error {
            AdminDashboardReadError::Forbidden => ManagementError::Forbidden,
            AdminDashboardReadError::Internal => ManagementError::Internal,
        })?;
    Ok(crate::management_dashboard::no_store_json(
        ServiceLevelReportResponse {
            period_start: report.period_start,
            period_end: report.period_end,
            total: report.total,
            unattributed_request_count: report.unattributed_request_count,
            items: report
                .items
                .into_iter()
                .map(|row| ServiceLevelRowResponse {
                    key: row.key,
                    name: row.name,
                    request_count: row.request_count,
                    successful_request_count: row.successful_request_count,
                    failed_request_count: row.failed_request_count,
                    unknown_request_count: row.unknown_request_count,
                    average_duration_ms: row.average_duration_ms,
                    hourly: row
                        .hourly
                        .into_iter()
                        .map(|point| ServiceLevelPointResponse {
                            period_start: point.period_start,
                            successful_request_count: point.successful_request_count,
                            failed_request_count: point.failed_request_count,
                            unknown_request_count: point.unknown_request_count,
                        })
                        .collect(),
                })
                .collect(),
        },
    ))
}

fn parse_query(raw_query: Option<&str>) -> Result<ServiceLevelQuery, ManagementError> {
    let raw_query = raw_query.unwrap_or_default();
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
    let mut query = ServiceLevelQuery {
        dimension: ServiceLevelDimension::Model,
        search: String::new(),
        page: 1,
        page_size: 10,
        failures_first: false,
    };
    let mut seen = std::collections::HashSet::new();
    for (key, value) in url::form_urlencoded::parse(bytes) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') || !seen.insert(key.to_string()) {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "dimension" => {
                query.dimension = match value.as_ref() {
                    "model" => ServiceLevelDimension::Model,
                    "channel" => ServiceLevelDimension::Channel,
                    _ => return Err(ManagementError::InvalidRequest),
                };
            }
            "search" => query.search = value.trim().to_owned(),
            "page" => query.page = parse_number(&value)?,
            "page_size" => query.page_size = parse_number(&value)?,
            "sort" => {
                query.failures_first = match value.as_ref() {
                    "requests" => false,
                    "failures" => true,
                    _ => return Err(ManagementError::InvalidRequest),
                };
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    if !query.is_valid() {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(query)
}

fn parse_number(value: &str) -> Result<u32, ManagementError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ManagementError::InvalidRequest);
    }
    value.parse().map_err(|_| ManagementError::InvalidRequest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_service_level_filters_and_bounds() {
        let query = parse_query(Some(
            "dimension=channel&search=+%E6%B8%A0%E9%81%93+&page=2&page_size=5&sort=failures",
        ))
        .unwrap();
        assert_eq!(query.dimension, ServiceLevelDimension::Channel);
        assert_eq!(query.search, "渠道");
        assert_eq!(query.page, 2);
        assert_eq!(query.page_size, 5);
        assert!(query.failures_first);
        assert!(parse_query(None).is_ok());
        for invalid in [
            "dimension=user",
            "sort=name",
            "page=0",
            "page=10001",
            "page=-1",
            "page_size=21",
            "page_size=0",
            "page=1&page=2",
            "extra=true",
            "search=%",
            "search=%ZZ",
            "search=%FF",
            "page=%2B1",
        ] {
            assert!(parse_query(Some(invalid)).is_err(), "accepted {invalid}");
        }
        assert!(parse_query(Some(&format!("search={}", "a".repeat(129)))).is_err());
    }
}
