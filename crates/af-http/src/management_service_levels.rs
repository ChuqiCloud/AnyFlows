use af_admin::{PlatformPolicy, SessionAuthentication};
use af_analytics::{
    AdminDashboardAccess, AdminDashboardReadError, ServiceLevelDimension, ServiceLevelQuery,
};
use af_domain::PlatformPermission;
use axum::{
    extract::{Extension, Query, State},
    response::Response,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServiceLevelParameters {
    dimension: Option<String>,
    search: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
    sort: Option<String>,
}

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
    parameters: Result<Query<ServiceLevelParameters>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, ManagementError> {
    if !PlatformPolicy::allows(
        authentication.principal(),
        PlatformPermission::DashboardReadAll,
    ) {
        return Err(ManagementError::Forbidden);
    }
    let Query(parameters) = parameters.map_err(|_| ManagementError::InvalidRequest)?;
    let dimension = match parameters.dimension.as_deref().unwrap_or("model") {
        "model" => ServiceLevelDimension::Model,
        "channel" => ServiceLevelDimension::Channel,
        _ => return Err(ManagementError::InvalidRequest),
    };
    let failures_first = match parameters.sort.as_deref().unwrap_or("requests") {
        "requests" => false,
        "failures" => true,
        _ => return Err(ManagementError::InvalidRequest),
    };
    let query = ServiceLevelQuery {
        dimension,
        search: parameters.search.unwrap_or_default().trim().to_owned(),
        page: parameters.page.unwrap_or(1),
        page_size: parameters.page_size.unwrap_or(10),
        failures_first,
    };
    if !query.is_valid() {
        return Err(ManagementError::InvalidRequest);
    }
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
