//! Public, installation-wide status with low-sample suppression.

use std::{sync::Arc, time::Instant};

use af_admin::{PublicServiceStatusReader, PublicStatusKind, PublicStatusReport};
use axum::{
    Router,
    extract::State,
    http::{StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::ReadinessHandle;

#[derive(Clone)]
struct PublicStatusState {
    reader: Arc<dyn PublicServiceStatusReader>,
    readiness: ReadinessHandle,
    readiness_cache: Arc<tokio::sync::Mutex<Option<(Instant, bool)>>>,
}

#[derive(Serialize, ToSchema)]
pub struct PublicServiceStatusPoint {
    period_start: i64,
    successful_request_count: i64,
    failed_request_count: i64,
    unknown_request_count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct PublicServiceStatusResponse {
    sampled_at: i64,
    period_start: i64,
    period_end: i64,
    status: String,
    api_ready: bool,
    sample_count: Option<i64>,
    successful_request_count: Option<i64>,
    failed_request_count: Option<i64>,
    unknown_request_count: Option<i64>,
    availability_percent: Option<f64>,
    average_success_duration_ms: Option<i64>,
    hourly: Vec<PublicServiceStatusPoint>,
}

pub fn build_public_service_status_router(
    reader: Arc<dyn PublicServiceStatusReader>,
    readiness: ReadinessHandle,
) -> Router {
    Router::new()
        .route("/api/public/service-status", get(read))
        .with_state(PublicStatusState {
            reader,
            readiness,
            readiness_cache: Arc::new(tokio::sync::Mutex::new(None)),
        })
}

async fn read(State(state): State<PublicStatusState>) -> Response {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default();
    let ready = {
        let mut cached = state.readiness_cache.lock().await;
        if let Some((sampled, ready)) = cached.as_ref() {
            if sampled.elapsed() < std::time::Duration::from_secs(10) {
                *ready
            } else {
                let ready = state.readiness.check().await;
                *cached = Some((Instant::now(), ready));
                ready
            }
        } else {
            let ready = state.readiness.check().await;
            *cached = Some((Instant::now(), ready));
            ready
        }
    };
    let Ok(report) = state.reader.read().await else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [(
                CACHE_CONTROL,
                "public, max-age=10, stale-while-revalidate=20",
            )],
            axum::Json(PublicServiceStatusResponse {
                sampled_at: now,
                period_start: now.saturating_sub(86_400),
                period_end: now,
                status: "unknown".to_owned(),
                api_ready: ready,
                sample_count: None,
                successful_request_count: None,
                failed_request_count: None,
                unknown_request_count: None,
                availability_percent: None,
                average_success_duration_ms: None,
                hourly: Vec::new(),
            }),
        )
            .into_response();
    };
    let status = if !ready {
        "unavailable"
    } else {
        status_name(report.status)
    };
    let response = response(report, status, ready);
    (
        StatusCode::OK,
        [(
            CACHE_CONTROL,
            "public, max-age=30, stale-while-revalidate=30",
        )],
        axum::Json(response),
    )
        .into_response()
}

fn response(
    report: PublicStatusReport,
    status: &str,
    api_ready: bool,
) -> PublicServiceStatusResponse {
    PublicServiceStatusResponse {
        sampled_at: report.sampled_at,
        period_start: report.period_start,
        period_end: report.period_end,
        status: status.to_owned(),
        api_ready,
        sample_count: report.sample_count,
        successful_request_count: report.successful_request_count,
        failed_request_count: report.failed_request_count,
        unknown_request_count: report.unknown_request_count,
        availability_percent: report.availability_percent,
        average_success_duration_ms: report.average_success_duration_ms,
        hourly: report
            .hourly
            .into_iter()
            .map(|point| PublicServiceStatusPoint {
                period_start: point.period_start,
                successful_request_count: point.successful_request_count,
                failed_request_count: point.failed_request_count,
                unknown_request_count: point.unknown_request_count,
            })
            .collect(),
    }
}

fn status_name(status: PublicStatusKind) -> &'static str {
    match status {
        PublicStatusKind::InsufficientData => "insufficient_data",
        PublicStatusKind::Operational => "operational",
        PublicStatusKind::Degraded => "degraded",
        PublicStatusKind::Monitoring => "monitoring",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_admin::PublicStatusReport;

    fn report() -> PublicStatusReport {
        PublicStatusReport {
            sampled_at: 90_000,
            period_start: 3_600,
            period_end: 90_000,
            status: PublicStatusKind::Operational,
            sample_count: Some(100),
            successful_request_count: Some(99),
            failed_request_count: Some(1),
            unknown_request_count: Some(0),
            availability_percent: Some(99.0),
            average_success_duration_ms: Some(42),
            hourly: Vec::new(),
        }
    }

    #[test]
    fn public_response_contains_only_aggregates() {
        let response = response(report(), "operational", true);
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["api_ready"], true);
        assert_eq!(value["sample_count"], 100);
        assert!(value.get("channel_id").is_none());
        assert!(value.get("model").is_none());
        assert!(value.get("user_id").is_none());
        assert_eq!(
            status_name(PublicStatusKind::InsufficientData),
            "insufficient_data"
        );
    }
}
