use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use af_db::{
    PUBLIC_STATUS_MINIMUM_SAMPLES, PublicServiceStatusError, PublicServiceStatusRecord,
    PublicServiceStatusRepository, PublicStatusPoint,
};

#[derive(Clone, Debug)]
pub struct PublicStatusReport {
    pub sampled_at: i64,
    pub period_start: i64,
    pub period_end: i64,
    pub status: PublicStatusKind,
    pub sample_count: Option<i64>,
    pub successful_request_count: Option<i64>,
    pub failed_request_count: Option<i64>,
    pub unknown_request_count: Option<i64>,
    pub availability_percent: Option<f64>,
    pub average_success_duration_ms: Option<i64>,
    pub hourly: Vec<PublicStatusPoint>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicStatusKind {
    InsufficientData,
    Operational,
    Degraded,
    Monitoring,
}

pub type PublicStatusFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PublicStatusReport, PublicServiceStatusError>> + Send + 'a>>;

pub trait PublicServiceStatusReader: Send + Sync {
    fn read(&self) -> PublicStatusFuture<'_>;
}

#[derive(Clone)]
pub struct DatabasePublicServiceStatusReader {
    repository: PublicServiceStatusRepository,
    cache: Arc<tokio::sync::Mutex<Option<(Instant, PublicStatusReport)>>>,
}

impl DatabasePublicServiceStatusReader {
    #[must_use]
    pub fn new(repository: PublicServiceStatusRepository) -> Self {
        Self {
            repository,
            cache: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
}

impl PublicServiceStatusReader for DatabasePublicServiceStatusReader {
    fn read(&self) -> PublicStatusFuture<'_> {
        Box::pin(async move {
            let mut cache = self.cache.lock().await;
            if let Some((sampled, report)) = cache.as_ref()
                && sampled.elapsed() < std::time::Duration::from_secs(30)
            {
                return Ok(report.clone());
            }
            let sampled_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| PublicServiceStatusError::Invariant)?
                .as_secs();
            let now = i64::try_from(sampled_at).map_err(|_| PublicServiceStatusError::Invariant)?;
            let record = self.repository.report(now).await?;
            let report = public_report(record);
            *cache = Some((Instant::now(), report.clone()));
            Ok(report)
        })
    }
}

fn public_report(record: PublicServiceStatusRecord) -> PublicStatusReport {
    let measured = record
        .successful_request_count
        .saturating_add(record.failed_request_count);
    let enough_data = record.request_count >= PUBLIC_STATUS_MINIMUM_SAMPLES
        && measured >= PUBLIC_STATUS_MINIMUM_SAMPLES;
    let availability_percent =
        enough_data.then(|| (record.successful_request_count as f64 * 100.0) / measured as f64);
    let unknown_ratio_high =
        enough_data && record.unknown_request_count.saturating_mul(20) > record.request_count;
    let status = if !enough_data {
        PublicStatusKind::InsufficientData
    } else if unknown_ratio_high {
        PublicStatusKind::Monitoring
    } else if availability_percent.is_some_and(|value| value < 95.0) {
        PublicStatusKind::Degraded
    } else {
        PublicStatusKind::Operational
    };
    PublicStatusReport {
        sampled_at: record.sampled_at,
        period_start: record.period_start,
        period_end: record.period_end,
        status,
        sample_count: enough_data.then_some(record.request_count),
        successful_request_count: enough_data.then_some(record.successful_request_count),
        failed_request_count: enough_data.then_some(record.failed_request_count),
        unknown_request_count: enough_data.then_some(record.unknown_request_count),
        availability_percent,
        average_success_duration_ms: enough_data
            .then_some(record.average_success_duration_ms)
            .flatten(),
        hourly: if enough_data {
            record.hourly
        } else {
            Vec::new()
        },
    }
}

impl std::fmt::Debug for DatabasePublicServiceStatusReader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabasePublicServiceStatusReader(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(total: i64, success: i64, failure: i64, unknown: i64) -> PublicServiceStatusRecord {
        PublicServiceStatusRecord {
            sampled_at: 90_000,
            period_start: 3_600,
            period_end: 90_000,
            request_count: total,
            successful_request_count: success,
            failed_request_count: failure,
            unknown_request_count: unknown,
            average_success_duration_ms: Some(42),
            hourly: vec![PublicStatusPoint {
                period_start: 3_600,
                successful_request_count: success,
                failed_request_count: failure,
                unknown_request_count: unknown,
            }],
        }
    }

    #[test]
    fn public_status_hides_low_sample_aggregates() {
        let report = public_report(record(49, 49, 0, 0));
        assert_eq!(report.status, PublicStatusKind::InsufficientData);
        assert_eq!(report.sample_count, None);
        assert_eq!(report.hourly.len(), 0);
    }

    #[test]
    fn public_status_distinguishes_degradation_and_unknown_outcomes() {
        assert_eq!(
            public_report(record(100, 99, 1, 0)).status,
            PublicStatusKind::Operational
        );
        assert_eq!(
            public_report(record(100, 95, 5, 0)).status,
            PublicStatusKind::Operational
        );
        assert_eq!(
            public_report(record(100, 90, 10, 0)).status,
            PublicStatusKind::Degraded
        );
        assert_eq!(
            public_report(record(100, 80, 0, 20)).status,
            PublicStatusKind::Monitoring
        );
    }
}
