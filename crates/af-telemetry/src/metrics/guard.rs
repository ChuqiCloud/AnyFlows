use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use ::metrics::{Counter, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit};
use metrics_exporter_prometheus::PrometheusRecorder;

use crate::MetricLabelError;

use super::{
    ANALYTICS_EXPORT_BACKLOG, ANALYTICS_EXPORT_BACKLOG_DESCRIPTION, BILLING_FLUSH_LAG,
    BILLING_FLUSH_LAG_DESCRIPTION, CHANNEL_HEALTH, CHANNEL_HEALTH_DESCRIPTION,
    CLIENT_SIMULATION_ATTEMPTS_DESCRIPTION, CLIENT_SIMULATION_ATTEMPTS_TOTAL,
    CLIENT_SIMULATION_BODY_PATCHES_DESCRIPTION, CLIENT_SIMULATION_BODY_PATCHES_TOTAL,
    CONCURRENCY_SLOTS, CONCURRENCY_SLOTS_DESCRIPTION, FIRST_TOKEN_DESCRIPTION, FIRST_TOKEN_SECONDS,
    ORGANIZATION_APPROVAL_TIMEOUT_DESCRIPTION, ORGANIZATION_APPROVAL_TIMEOUT_LABELS,
    ORGANIZATION_APPROVAL_TIMEOUT_TOTAL, OTHER_LABEL, PAYMENT_CONFIRMATION_LABELS,
    PAYMENT_WEBHOOK_CONFIRMATIONS_DESCRIPTION, PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL,
    QUOTA_CONSUMED_DESCRIPTION, QUOTA_CONSUMED_TOTAL, REQUEST_DURATION_DESCRIPTION,
    REQUEST_DURATION_SECONDS, REQUEST_RATE_LIMIT_ADMISSIONS_DESCRIPTION,
    REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL, REQUEST_RATE_LIMIT_FAILURES_DESCRIPTION,
    REQUEST_RATE_LIMIT_FAILURES_TOTAL, REQUEST_RATE_LIMIT_REJECTIONS_DESCRIPTION,
    REQUEST_RATE_LIMIT_REJECTIONS_TOTAL, REQUEST_RATE_LIMIT_RETRY_AFTER_DESCRIPTION,
    REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS, REQUEST_RATE_LIMIT_RULES_DESCRIPTION,
    REQUEST_RATE_LIMIT_RULES_PER_CHECK, REQUESTS_DESCRIPTION, REQUESTS_TOTAL,
    UPSTREAM_ERRORS_DESCRIPTION, UPSTREAM_ERRORS_TOTAL,
};

const REQUEST_LABELS: &[&str] = &["protocol", "model", "status"];
const FIRST_TOKEN_LABELS: &[&str] = &["protocol", "model"];
const CHANNEL_LABELS: &[&str] = &["channel"];
const UPSTREAM_ERROR_LABELS: &[&str] = &["channel", "type"];
const GROUP_LABELS: &[&str] = &["group"];
const CONCURRENCY_LABELS: &[&str] = &["level"];
const CLIENT_SIMULATION_LABELS: &[&str] = &["profile", "result"];
const CLIENT_SIMULATION_BODY_LABELS: &[&str] = &["profile", "result"];
const RATE_LIMIT_SUBJECT_LABELS: &[&str] = &["subject"];
const RATE_LIMIT_FAILURE_LABELS: &[&str] = &["reason"];
const NO_LABELS: &[&str] = &[];
const METRICS_CALLSITE: &str = super::METRICS_CALLSITE;
const ENUM_LABEL_MAX_BYTES: usize = 32;
const REGISTRY_LABEL_MAX_BYTES: usize = 64;

pub(crate) const MAX_MODEL_LABELS: usize = 256;
pub(crate) const MAX_CHANNEL_LABELS: usize = 1_024;
pub(crate) const MAX_GROUP_LABELS: usize = 256;
/// 单个指标 family 在一次进程生命周期内最多接纳的不同序列数。
pub(crate) const MAX_SERIES_PER_FAMILY: usize = 4_096;

#[derive(Clone, Copy, Eq, PartialEq)]
enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

struct MetricContract {
    name: &'static str,
    kind: MetricKind,
    labels: &'static [&'static str],
    unit: Option<Unit>,
    description: &'static str,
}

const METRIC_CONTRACTS: &[MetricContract] = &[
    MetricContract {
        name: REQUESTS_TOTAL,
        kind: MetricKind::Counter,
        labels: REQUEST_LABELS,
        unit: Some(Unit::Count),
        description: REQUESTS_DESCRIPTION,
    },
    MetricContract {
        name: REQUEST_DURATION_SECONDS,
        kind: MetricKind::Histogram,
        labels: REQUEST_LABELS,
        unit: Some(Unit::Seconds),
        description: REQUEST_DURATION_DESCRIPTION,
    },
    MetricContract {
        name: FIRST_TOKEN_SECONDS,
        kind: MetricKind::Histogram,
        labels: FIRST_TOKEN_LABELS,
        unit: Some(Unit::Seconds),
        description: FIRST_TOKEN_DESCRIPTION,
    },
    MetricContract {
        name: CHANNEL_HEALTH,
        kind: MetricKind::Gauge,
        labels: CHANNEL_LABELS,
        unit: None,
        description: CHANNEL_HEALTH_DESCRIPTION,
    },
    MetricContract {
        name: UPSTREAM_ERRORS_TOTAL,
        kind: MetricKind::Counter,
        labels: UPSTREAM_ERROR_LABELS,
        unit: Some(Unit::Count),
        description: UPSTREAM_ERRORS_DESCRIPTION,
    },
    MetricContract {
        name: QUOTA_CONSUMED_TOTAL,
        kind: MetricKind::Counter,
        labels: GROUP_LABELS,
        unit: Some(Unit::Count),
        description: QUOTA_CONSUMED_DESCRIPTION,
    },
    MetricContract {
        name: CONCURRENCY_SLOTS,
        kind: MetricKind::Gauge,
        labels: CONCURRENCY_LABELS,
        unit: Some(Unit::Count),
        description: CONCURRENCY_SLOTS_DESCRIPTION,
    },
    MetricContract {
        name: BILLING_FLUSH_LAG,
        kind: MetricKind::Gauge,
        labels: NO_LABELS,
        unit: Some(Unit::Seconds),
        description: BILLING_FLUSH_LAG_DESCRIPTION,
    },
    MetricContract {
        name: CLIENT_SIMULATION_ATTEMPTS_TOTAL,
        kind: MetricKind::Counter,
        labels: CLIENT_SIMULATION_LABELS,
        unit: Some(Unit::Count),
        description: CLIENT_SIMULATION_ATTEMPTS_DESCRIPTION,
    },
    MetricContract {
        name: CLIENT_SIMULATION_BODY_PATCHES_TOTAL,
        kind: MetricKind::Counter,
        labels: CLIENT_SIMULATION_BODY_LABELS,
        unit: Some(Unit::Count),
        description: CLIENT_SIMULATION_BODY_PATCHES_DESCRIPTION,
    },
    MetricContract {
        name: ANALYTICS_EXPORT_BACKLOG,
        kind: MetricKind::Gauge,
        labels: NO_LABELS,
        unit: Some(Unit::Count),
        description: ANALYTICS_EXPORT_BACKLOG_DESCRIPTION,
    },
    MetricContract {
        name: REQUEST_RATE_LIMIT_ADMISSIONS_TOTAL,
        kind: MetricKind::Counter,
        labels: NO_LABELS,
        unit: Some(Unit::Count),
        description: REQUEST_RATE_LIMIT_ADMISSIONS_DESCRIPTION,
    },
    MetricContract {
        name: REQUEST_RATE_LIMIT_REJECTIONS_TOTAL,
        kind: MetricKind::Counter,
        labels: RATE_LIMIT_SUBJECT_LABELS,
        unit: Some(Unit::Count),
        description: REQUEST_RATE_LIMIT_REJECTIONS_DESCRIPTION,
    },
    MetricContract {
        name: REQUEST_RATE_LIMIT_FAILURES_TOTAL,
        kind: MetricKind::Counter,
        labels: RATE_LIMIT_FAILURE_LABELS,
        unit: Some(Unit::Count),
        description: REQUEST_RATE_LIMIT_FAILURES_DESCRIPTION,
    },
    MetricContract {
        name: REQUEST_RATE_LIMIT_RULES_PER_CHECK,
        kind: MetricKind::Histogram,
        labels: NO_LABELS,
        unit: Some(Unit::Count),
        description: REQUEST_RATE_LIMIT_RULES_DESCRIPTION,
    },
    MetricContract {
        name: REQUEST_RATE_LIMIT_RETRY_AFTER_SECONDS,
        kind: MetricKind::Histogram,
        labels: RATE_LIMIT_SUBJECT_LABELS,
        unit: Some(Unit::Seconds),
        description: REQUEST_RATE_LIMIT_RETRY_AFTER_DESCRIPTION,
    },
    MetricContract {
        name: PAYMENT_WEBHOOK_CONFIRMATIONS_TOTAL,
        kind: MetricKind::Counter,
        labels: PAYMENT_CONFIRMATION_LABELS,
        unit: Some(Unit::Count),
        description: PAYMENT_WEBHOOK_CONFIRMATIONS_DESCRIPTION,
    },
    MetricContract {
        name: ORGANIZATION_APPROVAL_TIMEOUT_TOTAL,
        kind: MetricKind::Counter,
        labels: ORGANIZATION_APPROVAL_TIMEOUT_LABELS,
        unit: Some(Unit::Count),
        description: ORGANIZATION_APPROVAL_TIMEOUT_DESCRIPTION,
    },
];

struct RegisteredValues {
    values: HashSet<String>,
    limit: usize,
}

impl RegisteredValues {
    fn new(limit: usize) -> Self {
        Self {
            values: HashSet::from([OTHER_LABEL.to_owned()]),
            limit,
        }
    }

    fn register(&mut self, value: &str) -> Result<String, MetricLabelError> {
        if !canonical_registry_label(value) {
            return Err(MetricLabelError::Invalid);
        }
        if self.values.contains(value) {
            return Ok(value.to_owned());
        }
        if self.values.len() >= self.limit {
            return Err(MetricLabelError::CapacityExceeded);
        }
        self.values.insert(value.to_owned());
        Ok(value.to_owned())
    }

    fn contains(&self, value: &str) -> bool {
        self.values.contains(value)
    }
}

pub(super) struct LabelRegistries {
    models: Mutex<RegisteredValues>,
    channels: Mutex<RegisteredValues>,
    groups: Mutex<RegisteredValues>,
}

impl Default for LabelRegistries {
    fn default() -> Self {
        Self {
            models: Mutex::new(RegisteredValues::new(MAX_MODEL_LABELS)),
            channels: Mutex::new(RegisteredValues::new(MAX_CHANNEL_LABELS)),
            groups: Mutex::new(RegisteredValues::new(MAX_GROUP_LABELS)),
        }
    }
}

impl LabelRegistries {
    pub(super) fn register_model(&self, value: &str) -> Result<String, MetricLabelError> {
        register_value(&self.models, value)
    }

    pub(super) fn register_channel(&self, value: &str) -> Result<String, MetricLabelError> {
        register_value(&self.channels, value)
    }

    pub(super) fn register_group(&self, value: &str) -> Result<String, MetricLabelError> {
        register_value(&self.groups, value)
    }

    fn accepts(&self, key: &str, value: &str) -> bool {
        match key {
            "model" => contains_value(&self.models, value),
            "channel" => contains_value(&self.channels, value),
            "group" => contains_value(&self.groups, value),
            _ => false,
        }
    }
}

fn register_value(
    registry: &Mutex<RegisteredValues>,
    value: &str,
) -> Result<String, MetricLabelError> {
    registry
        .lock()
        .map_err(|_| MetricLabelError::CapacityExceeded)?
        .register(value)
}

fn contains_value(registry: &Mutex<RegisteredValues>, value: &str) -> bool {
    registry
        .lock()
        .map(|registry| registry.contains(value))
        .unwrap_or(false)
}

fn canonical_registry_label(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= REGISTRY_LABEL_MAX_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/' | b':')
        })
}

#[derive(Default)]
struct AdmissionState {
    series: HashMap<&'static str, HashSet<SeriesIdentity>>,
}

#[derive(Eq, Hash, PartialEq)]
struct SeriesIdentity(Vec<(String, String)>);

/// 在 exporter 前执行固定契约校验，任何未知或异常指标都静默丢弃。
pub(crate) struct GuardedRecorder {
    exporter: PrometheusRecorder,
    admission: Mutex<AdmissionState>,
    labels: Arc<LabelRegistries>,
}

impl GuardedRecorder {
    pub(super) fn new(exporter: PrometheusRecorder, labels: Arc<LabelRegistries>) -> Self {
        Self {
            exporter,
            admission: Mutex::new(AdmissionState::default()),
            labels,
        }
    }

    fn describe(
        &self,
        kind: MetricKind,
        key: KeyName,
        unit: Option<Unit>,
        description: SharedString,
    ) {
        let Some(contract) = metric_contract(key.as_str()) else {
            return;
        };
        if contract.kind != kind
            || contract.unit != unit
            || description.as_ref() != contract.description
        {
            return;
        }

        match kind {
            MetricKind::Counter => self.exporter.describe_counter(key, unit, description),
            MetricKind::Gauge => self.exporter.describe_gauge(key, unit, description),
            MetricKind::Histogram => self.exporter.describe_histogram(key, unit, description),
        }
    }

    fn admit(&self, kind: MetricKind, key: &Key, metadata: &Metadata<'_>) -> Option<Key> {
        if metadata.target() != METRICS_CALLSITE || metadata.module_path() != Some(METRICS_CALLSITE)
        {
            return None;
        }

        let contract = metric_contract(key.name())?;
        if contract.kind != kind || !labels_match_contract(key, contract.labels, &self.labels) {
            return None;
        }

        let Ok(mut admission) = self.admission.lock() else {
            return None;
        };
        let identity = series_identity(key, contract)?;
        let canonical_labels = identity
            .0
            .iter()
            .map(|(key, value)| ::metrics::Label::new(key.clone(), value.clone()))
            .collect::<Vec<_>>();
        let canonical_key = Key::from_parts(contract.name, canonical_labels);
        let series = admission.series.entry(contract.name).or_default();
        if series.contains(&identity) {
            return Some(canonical_key);
        }
        if series.len() >= MAX_SERIES_PER_FAMILY {
            return None;
        }
        series.insert(identity);
        Some(canonical_key)
    }
}

impl Recorder for GuardedRecorder {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.describe(MetricKind::Counter, key, unit, description);
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.describe(MetricKind::Gauge, key, unit, description);
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.describe(MetricKind::Histogram, key, unit, description);
    }

    fn register_counter(&self, key: &Key, metadata: &Metadata<'_>) -> Counter {
        if let Some(key) = self.admit(MetricKind::Counter, key, metadata) {
            self.exporter.register_counter(&key, metadata)
        } else {
            Counter::noop()
        }
    }

    fn register_gauge(&self, key: &Key, metadata: &Metadata<'_>) -> Gauge {
        if let Some(key) = self.admit(MetricKind::Gauge, key, metadata) {
            self.exporter.register_gauge(&key, metadata)
        } else {
            Gauge::noop()
        }
    }

    fn register_histogram(&self, key: &Key, metadata: &Metadata<'_>) -> Histogram {
        if let Some(key) = self.admit(MetricKind::Histogram, key, metadata) {
            self.exporter.register_histogram(&key, metadata)
        } else {
            Histogram::noop()
        }
    }
}

#[cfg(test)]
impl GuardedRecorder {
    /// 仅供回归测试绕过宏调用点，直接验证可信调用点后的契约防线。
    pub(crate) fn record_counter_from_trusted_test_callsite(&self, key: Key, value: u64) {
        let metadata = Metadata::new(
            METRICS_CALLSITE,
            ::metrics::Level::INFO,
            Some(METRICS_CALLSITE),
        );
        self.register_counter(&key, &metadata).increment(value);
    }

    /// 仅供回归测试验证指标类型不匹配时会被静默拒绝。
    pub(crate) fn record_gauge_from_trusted_test_callsite(&self, key: Key, value: f64) {
        let metadata = Metadata::new(
            METRICS_CALLSITE,
            ::metrics::Level::INFO,
            Some(METRICS_CALLSITE),
        );
        self.register_gauge(&key, &metadata).set(value);
    }
}

fn metric_contract(name: &str) -> Option<&'static MetricContract> {
    METRIC_CONTRACTS
        .iter()
        .find(|contract| contract.name == name)
}

fn labels_match_contract(key: &Key, expected: &[&str], registries: &LabelRegistries) -> bool {
    if key.labels().len() != expected.len() {
        return false;
    }

    let mut seen = 0_u16;
    for label in key.labels() {
        let Some(position) = expected.iter().position(|key| *key == label.key()) else {
            return false;
        };
        let bit = 1_u16 << position;
        if seen & bit != 0 || !allowed_label_value(label.key(), label.value(), registries) {
            return false;
        }
        seen |= bit;
    }
    seen == (1_u16 << expected.len()) - 1
}

fn allowed_label_value(key: &str, value: &str, registries: &LabelRegistries) -> bool {
    if matches!(key, "model" | "channel" | "group") {
        return registries.accepts(key, value);
    }
    if value.is_empty() || value.len() > ENUM_LABEL_MAX_BYTES {
        return false;
    }
    match key {
        "protocol" => matches!(
            value,
            "openai" | "anthropic" | "gemini" | "bedrock" | "other"
        ),
        "status" => matches!(
            value,
            "success" | "client_error" | "upstream_error" | "internal_error" | "cancelled"
        ),
        "type" => matches!(
            value,
            "timeout"
                | "authentication"
                | "rate_limited"
                | "invalid_request"
                | "unavailable"
                | "protocol"
                | "other"
        ),
        "level" => matches!(value, "account" | "user" | "token"),
        "profile" => matches!(
            value,
            "anthropic_cli_headers_v1" | "anthropic_cli_system_date_v1"
        ),
        "result" => matches!(value, "not_applied" | "applied" | "failed" | "rejected"),
        "subject" => matches!(value, "user" | "group" | "token"),
        "reason" => matches!(
            value,
            "invalid_rule"
                | "store_missing"
                | "store_unavailable"
                | "store_protocol"
                | "store_other"
                | "invalid_retry_after"
        ),
        "outcome" => matches!(
            value,
            "applied"
                | "acknowledged"
                | "existing"
                | "rejected"
                | "ignored_non_terminal"
                | "not_found"
                | "verification_rejected"
                | "conflict"
                | "binding_conflict"
                | "outcome_unknown"
                | "unavailable"
                | "invariant"
                | "invalid_request"
                | "lease_held"
                | "scan_failed"
                | "skipped"
                | "failed"
                | "outcome_unknown_replay"
                | "truncated"
        ),
        _ => false,
    }
}

fn series_identity(key: &Key, contract: &MetricContract) -> Option<SeriesIdentity> {
    let mut labels = Vec::with_capacity(contract.labels.len());
    for expected_key in contract.labels {
        let label = key.labels().find(|label| label.key() == *expected_key)?;
        labels.push(((*expected_key).to_owned(), label.value().to_owned()));
    }
    Some(SeriesIdentity(labels))
}
