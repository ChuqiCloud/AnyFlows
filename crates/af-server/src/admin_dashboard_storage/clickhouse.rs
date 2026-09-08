use std::collections::BTreeSet;
use std::{fmt, future::Future, pin::Pin, str::FromStr as _, sync::Arc, time::Duration};

use af_analytics::{
    AdminDashboardAnalyticsSnapshot, AdminDashboardAnalyticsStorage,
    AdminDashboardAnalyticsStorageFuture, AdminDashboardChannelFlow, AdminDashboardFailure,
    AdminDashboardFailureKind, AdminDashboardFlowPath, AdminDashboardHourlyPoint,
    AdminDashboardOutcomeSnapshot, AdminDashboardPerformance, AdminDashboardStorageError,
    AdminDashboardUsageSnapshot,
};
use af_config::{
    DEFAULT_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES, DEFAULT_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS,
    MAX_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES, MAX_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS, SecretString,
};
use af_domain::{ChannelId, Protocol};
use af_httpclient::{
    Body, HeaderMap, HeaderName, HeaderValue, HttpClientProvider, Method, StatusCode,
};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use af_db::{
    MAX_DASHBOARD_CHANNEL_FLOWS, MAX_DASHBOARD_FLOW_PATHS, SLOW_FIRST_TOKEN_THRESHOLD_MS,
    SLOW_REQUEST_THRESHOLD_MS,
};

/// ClickHouse 看板查询默认硬截止时间。
pub const DEFAULT_CLICKHOUSE_DASHBOARD_TIMEOUT: Duration =
    Duration::from_secs(DEFAULT_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS);
/// ClickHouse 看板查询默认最大响应体。
pub const DEFAULT_CLICKHOUSE_DASHBOARD_RESPONSE_BYTES: usize =
    DEFAULT_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES;
/// 防止配置把管理请求变成无界长查询。
pub const MAX_CLICKHOUSE_DASHBOARD_TIMEOUT: Duration =
    Duration::from_secs(MAX_CLICKHOUSE_ANALYTICS_TIMEOUT_SECS);
/// 单行快照的绝对响应体上限。
pub const MAX_CLICKHOUSE_DASHBOARD_RESPONSE_BYTES: usize = MAX_CLICKHOUSE_ANALYTICS_RESPONSE_BYTES;

const MAX_CLICKHOUSE_QUERY_BYTES: usize = 64 * 1024;
const MAX_CLICKHOUSE_USERNAME_BYTES: usize = 256;
const MAX_CLICKHOUSE_PASSWORD_BYTES: usize = 1024;
const FAILURE_KIND_COUNT: usize = 15;

/// ClickHouse 只读看板适配器的启动配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ClickHouseAdminDashboardConfigError {
    /// HTTP(S) 端点缺失、携带认证信息或包含不受控查询参数。
    #[error("ClickHouse 看板端点无效")]
    InvalidEndpoint,
    /// 查询缺少固定窗口参数、包含多语句分隔符或超过容量。
    #[error("ClickHouse 看板查询无效")]
    InvalidQuery,
    /// 用户名无法安全写入 ClickHouse 认证头。
    #[error("ClickHouse 看板用户名无效")]
    InvalidUsername,
    /// 密码无法安全写入 ClickHouse 认证头。
    #[error("ClickHouse 看板密码无效")]
    InvalidPassword,
    /// 查询硬截止必须位于受控范围内。
    #[error("ClickHouse 看板查询超时无效")]
    InvalidTimeout,
    /// 响应体上限必须位于受控范围内。
    #[error("ClickHouse 看板响应体上限无效")]
    InvalidResponseLimit,
}

/// 显式、默认不接线的 ClickHouse 看板查询配置。
pub struct ClickHouseAdminDashboardConfig {
    endpoint: Url,
    query: String,
    username: SecretString,
    password: SecretString,
    timeout: Duration,
    max_response_bytes: usize,
}

impl ClickHouseAdminDashboardConfig {
    /// 构造单语句只读查询；SQL 必须使用两个强类型窗口参数且不得自行添加输出格式。
    pub fn new(
        endpoint: impl AsRef<str>,
        query: impl Into<String>,
        username: SecretString,
        password: SecretString,
        timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self, ClickHouseAdminDashboardConfigError> {
        let endpoint = Url::parse(endpoint.as_ref())
            .map_err(|_| ClickHouseAdminDashboardConfigError::InvalidEndpoint)?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || !endpoint.has_host()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || endpoint.port() == Some(0)
        {
            return Err(ClickHouseAdminDashboardConfigError::InvalidEndpoint);
        }

        let query = query.into();
        let trimmed = query.trim();
        if trimmed.is_empty()
            || trimmed.len() > MAX_CLICKHOUSE_QUERY_BYTES
            || trimmed.contains(';')
            || trimmed
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
            || !trimmed.contains("{period_start:Int64}")
            || !trimmed.contains("{period_end:Int64}")
        {
            return Err(ClickHouseAdminDashboardConfigError::InvalidQuery);
        }
        let mut query = trimmed.to_owned();
        query.push_str("\nFORMAT JSONEachRow");

        validate_header_secret(
            &username,
            MAX_CLICKHOUSE_USERNAME_BYTES,
            false,
            ClickHouseAdminDashboardConfigError::InvalidUsername,
        )?;
        validate_header_secret(
            &password,
            MAX_CLICKHOUSE_PASSWORD_BYTES,
            true,
            ClickHouseAdminDashboardConfigError::InvalidPassword,
        )?;
        if timeout.is_zero() || timeout > MAX_CLICKHOUSE_DASHBOARD_TIMEOUT {
            return Err(ClickHouseAdminDashboardConfigError::InvalidTimeout);
        }
        if max_response_bytes == 0 || max_response_bytes > MAX_CLICKHOUSE_DASHBOARD_RESPONSE_BYTES {
            return Err(ClickHouseAdminDashboardConfigError::InvalidResponseLimit);
        }

        Ok(Self {
            endpoint,
            query,
            username,
            password,
            timeout,
            max_response_bytes,
        })
    }

    fn request_target(
        &self,
        period_start: i64,
        period_end: i64,
    ) -> Result<String, AdminDashboardStorageError> {
        if period_start < 0 || period_start >= period_end {
            return Err(AdminDashboardStorageError::Invariant);
        }
        let mut target = self.endpoint.clone();
        let execution_seconds =
            self.timeout
                .as_secs()
                .saturating_add(if self.timeout.subsec_nanos() != 0 {
                    1
                } else {
                    0
                });
        {
            let mut query = target.query_pairs_mut();
            query
                .append_pair("readonly", "2")
                .append_pair("wait_end_of_query", "1")
                .append_pair("max_result_rows", "1")
                .append_pair("result_overflow_mode", "throw")
                .append_pair("output_format_json_named_tuples_as_objects", "1")
                .append_pair("output_format_json_quote_64bit_integers", "0")
                .append_pair("max_result_bytes", &self.max_response_bytes.to_string())
                .append_pair("max_execution_time", &execution_seconds.to_string())
                .append_pair("param_period_start", &period_start.to_string())
                .append_pair("param_period_end", &period_end.to_string());
        }
        Ok(target.to_string())
    }

    fn request_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::with_capacity(3);
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        headers.insert(
            HeaderName::from_static("x-clickhouse-user"),
            HeaderValue::from_str(self.username.expose())
                .expect("已校验的 ClickHouse 用户名必须能写入认证头"),
        );
        headers.insert(
            HeaderName::from_static("x-clickhouse-key"),
            HeaderValue::from_str(self.password.expose())
                .expect("已校验的 ClickHouse 密码必须能写入认证头"),
        );
        headers
    }
}

impl fmt::Debug for ClickHouseAdminDashboardConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ClickHouseAdminDashboardConfig")
            .field("endpoint", &"<redacted>")
            .field("query", &"<redacted>")
            .field("username", &"<redacted>")
            .field("password", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

fn validate_header_secret(
    value: &SecretString,
    max_bytes: usize,
    allow_empty: bool,
    error: ClickHouseAdminDashboardConfigError,
) -> Result<(), ClickHouseAdminDashboardConfigError> {
    let value = value.expose();
    if (!allow_empty && value.is_empty())
        || value.len() > max_bytes
        || value.trim() != value
        || !value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
        || HeaderValue::from_str(value).is_err()
    {
        return Err(error);
    }
    Ok(())
}

type ClickHouseQueryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, ClickHouseQueryError>> + Send + 'a>>;

trait ClickHouseQueryTransport: Send + Sync {
    fn query(&self, period_start: i64, period_end: i64) -> ClickHouseQueryFuture<'_>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ClickHouseQueryError {
    Unavailable,
    ResponseTooLarge,
}

struct ManagedClickHouseQueryTransport {
    config: ClickHouseAdminDashboardConfig,
    clients: HttpClientProvider,
}

impl ManagedClickHouseQueryTransport {
    fn new(config: ClickHouseAdminDashboardConfig, clients: HttpClientProvider) -> Self {
        Self { config, clients }
    }
}

impl ClickHouseQueryTransport for ManagedClickHouseQueryTransport {
    fn query(&self, period_start: i64, period_end: i64) -> ClickHouseQueryFuture<'_> {
        Box::pin(async move {
            let target = self
                .config
                .request_target(period_start, period_end)
                .map_err(|_| ClickHouseQueryError::Unavailable)?;
            let client = self
                .clients
                .get(Some(self.config.timeout))
                .map_err(|_| ClickHouseQueryError::Unavailable)?;
            let response = client
                .execute(
                    Method::POST,
                    &target,
                    self.config.request_headers(),
                    Some(Body::from(self.config.query.clone())),
                )
                .await
                .map_err(|_| ClickHouseQueryError::Unavailable)?;
            if response.status() != StatusCode::OK {
                return Err(ClickHouseQueryError::Unavailable);
            }
            if response
                .content_length()
                .is_some_and(|length| length > self.config.max_response_bytes as u64)
            {
                return Err(ClickHouseQueryError::ResponseTooLarge);
            }

            let mut body = Vec::new();
            let mut stream = response.into_bytes_stream();
            while let Some(chunk) = stream.next_chunk().await {
                let chunk = chunk.map_err(|_| ClickHouseQueryError::Unavailable)?;
                let next_len = body
                    .len()
                    .checked_add(chunk.len())
                    .ok_or(ClickHouseQueryError::ResponseTooLarge)?;
                if next_len > self.config.max_response_bytes {
                    return Err(ClickHouseQueryError::ResponseTooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(body)
        })
    }
}

impl fmt::Debug for ManagedClickHouseQueryTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ManagedClickHouseQueryTransport(<redacted>)")
    }
}

/// 通过 ClickHouse HTTP 接口读取历史分析事实，不读取或写入当前渠道状态。
#[derive(Clone)]
pub struct ClickHouseAdminDashboardStorage {
    transport: Arc<dyn ClickHouseQueryTransport>,
}

impl ClickHouseAdminDashboardStorage {
    /// 使用共享受控 HTTP Client 构造显式适配器；调用方仍需组合主库渠道状态源。
    #[must_use]
    pub fn new(config: ClickHouseAdminDashboardConfig, clients: HttpClientProvider) -> Self {
        Self {
            transport: Arc::new(ManagedClickHouseQueryTransport::new(config, clients)),
        }
    }

    #[cfg(test)]
    fn with_transport(transport: Arc<dyn ClickHouseQueryTransport>) -> Self {
        Self { transport }
    }
}

impl AdminDashboardAnalyticsStorage for ClickHouseAdminDashboardStorage {
    fn analytics_snapshot(
        &self,
        period_start: i64,
        period_end: i64,
    ) -> AdminDashboardAnalyticsStorageFuture<'_> {
        Box::pin(async move {
            if period_start < 0 || period_start >= period_end {
                return Err(AdminDashboardStorageError::Invariant);
            }
            let body = self
                .transport
                .query(period_start, period_end)
                .await
                .map_err(|error| match error {
                    ClickHouseQueryError::Unavailable => AdminDashboardStorageError::Unavailable,
                    ClickHouseQueryError::ResponseTooLarge => AdminDashboardStorageError::Invariant,
                })?;
            decode_snapshot(&body)
        })
    }
}

impl fmt::Debug for ClickHouseAdminDashboardStorage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClickHouseAdminDashboardStorage(<redacted>)")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireSnapshot {
    usage: WireUsage,
    outcomes: WireOutcomes,
    hourly: Vec<WireHourlyPoint>,
    performance: WirePerformance,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireUsage {
    request_count: i64,
    quota_consumed: i64,
    upstream_usage_count: i64,
    estimated_usage_count: i64,
    per_token_request_count: i64,
    per_call_request_count: i64,
    free_request_count: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireOutcomes {
    request_count: i64,
    successful_request_count: i64,
    failed_request_count: i64,
    other_success_count: i64,
    failures: Vec<WireFailure>,
    channel_flows: Vec<WireChannelFlow>,
    /// 旧版 ClickHouse 快照没有流向字段时保持可读，前端显示为空流向。
    #[serde(default)]
    flow_request_count: i64,
    #[serde(default)]
    flow_quota_consumed: i64,
    #[serde(default)]
    flow_paths: Vec<WireFlowPath>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFailure {
    kind: String,
    request_count: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireChannelFlow {
    protocol: String,
    channel_id: i64,
    channel_name: String,
    request_count: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFlowPath {
    user_id: i64,
    group_id: i64,
    group_name: String,
    channel_id: i64,
    channel_name: String,
    model: String,
    request_count: i64,
    quota_consumed: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireHourlyPoint {
    period_start: i64,
    period_end: i64,
    request_count: i64,
    quota_consumed: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePerformance {
    first_token_sample_count: i64,
    average_first_token_ms: Option<i64>,
    slow_first_token_count: i64,
    slow_first_token_threshold_ms: i64,
    duration_sample_count: i64,
    average_duration_ms: Option<i64>,
    slow_request_count: i64,
    slow_request_threshold_ms: i64,
}

fn decode_snapshot(
    body: &[u8],
) -> Result<AdminDashboardAnalyticsSnapshot, AdminDashboardStorageError> {
    let mut rows = serde_json::Deserializer::from_slice(body).into_iter::<WireSnapshot>();
    let row = rows
        .next()
        .ok_or(AdminDashboardStorageError::Invariant)?
        .map_err(|_| AdminDashboardStorageError::Invariant)?;
    if rows.next().is_some()
        || row.hourly.len() != 24
        || row.outcomes.failures.len() > FAILURE_KIND_COUNT
        || row.outcomes.channel_flows.len() > MAX_DASHBOARD_CHANNEL_FLOWS
        || row.outcomes.flow_paths.len() > MAX_DASHBOARD_FLOW_PATHS
        || row.performance.slow_first_token_threshold_ms != SLOW_FIRST_TOKEN_THRESHOLD_MS
        || row.performance.slow_request_threshold_ms != SLOW_REQUEST_THRESHOLD_MS
    {
        return Err(AdminDashboardStorageError::Invariant);
    }
    let mut failure_kinds = BTreeSet::new();
    if row
        .outcomes
        .failures
        .iter()
        .any(|failure| failure.request_count <= 0 || !failure_kinds.insert(failure.kind.as_str()))
    {
        return Err(AdminDashboardStorageError::Invariant);
    }
    let mut flow_keys = BTreeSet::new();
    if row.outcomes.channel_flows.iter().any(|flow| {
        flow.request_count <= 0 || !flow_keys.insert((flow.protocol.as_str(), flow.channel_id))
    }) {
        return Err(AdminDashboardStorageError::Invariant);
    }
    let mut path_keys = BTreeSet::new();
    if row.outcomes.flow_request_count < 0
        || row.outcomes.flow_quota_consumed < 0
        || row.outcomes.flow_paths.iter().any(|path| {
            path.user_id <= 0
                || path.group_id <= 0
                || path.channel_id <= 0
                || path.request_count <= 0
                || path.quota_consumed < 0
                || !path_keys.insert((
                    path.user_id,
                    path.group_id,
                    path.channel_id,
                    path.model.as_str(),
                ))
        })
    {
        return Err(AdminDashboardStorageError::Invariant);
    }

    let failures = row
        .outcomes
        .failures
        .into_iter()
        .map(|failure| {
            Ok(AdminDashboardFailure::new(
                decode_failure_kind(&failure.kind)?,
                failure.request_count,
            ))
        })
        .collect::<Result<Vec<_>, AdminDashboardStorageError>>()?;
    let channel_flows = row
        .outcomes
        .channel_flows
        .into_iter()
        .map(decode_channel_flow)
        .collect::<Result<Vec<_>, _>>()?;
    let flow_paths = row
        .outcomes
        .flow_paths
        .into_iter()
        .map(decode_flow_path)
        .collect::<Result<Vec<_>, _>>()?;
    let hourly = row
        .hourly
        .into_iter()
        .map(|point| {
            AdminDashboardHourlyPoint::new(
                point.period_start,
                point.period_end,
                point.request_count,
                point.quota_consumed,
            )
        })
        .collect();

    Ok(AdminDashboardAnalyticsSnapshot::new(
        AdminDashboardUsageSnapshot::new(
            row.usage.request_count,
            row.usage.quota_consumed,
            row.usage.upstream_usage_count,
            row.usage.estimated_usage_count,
            row.usage.per_token_request_count,
            row.usage.per_call_request_count,
            row.usage.free_request_count,
        ),
        AdminDashboardOutcomeSnapshot::new(
            row.outcomes.request_count,
            row.outcomes.successful_request_count,
            row.outcomes.failed_request_count,
            row.outcomes.other_success_count,
            failures,
            channel_flows,
            row.outcomes.flow_request_count,
            row.outcomes.flow_quota_consumed,
            flow_paths,
        ),
        hourly,
        AdminDashboardPerformance::new(
            row.performance.first_token_sample_count,
            row.performance.average_first_token_ms,
            row.performance.slow_first_token_count,
            row.performance.slow_first_token_threshold_ms,
            row.performance.duration_sample_count,
            row.performance.average_duration_ms,
            row.performance.slow_request_count,
            row.performance.slow_request_threshold_ms,
        ),
    ))
}

fn decode_failure_kind(
    kind: &str,
) -> Result<AdminDashboardFailureKind, AdminDashboardStorageError> {
    match kind {
        "invalid_request" => Ok(AdminDashboardFailureKind::InvalidRequest),
        "model_not_allowed" => Ok(AdminDashboardFailureKind::ModelNotAllowed),
        "insufficient_quota" => Ok(AdminDashboardFailureKind::InsufficientQuota),
        "quota_limited" => Ok(AdminDashboardFailureKind::QuotaLimited),
        "concurrency_limited" => Ok(AdminDashboardFailureKind::ConcurrencyLimited),
        "outcome_unknown" => Ok(AdminDashboardFailureKind::OutcomeUnknown),
        "upstream_rate_limited" => Ok(AdminDashboardFailureKind::UpstreamRateLimited),
        "upstream_overloaded" => Ok(AdminDashboardFailureKind::UpstreamOverloaded),
        "upstream_authentication" => Ok(AdminDashboardFailureKind::UpstreamAuthentication),
        "upstream_quota" => Ok(AdminDashboardFailureKind::UpstreamQuota),
        "upstream_model" => Ok(AdminDashboardFailureKind::UpstreamModel),
        "upstream_protocol" => Ok(AdminDashboardFailureKind::UpstreamProtocol),
        "upstream_server" => Ok(AdminDashboardFailureKind::UpstreamServer),
        "upstream_network" => Ok(AdminDashboardFailureKind::UpstreamNetwork),
        "internal" => Ok(AdminDashboardFailureKind::Internal),
        _ => Err(AdminDashboardStorageError::Invariant),
    }
}

fn decode_channel_flow(
    flow: WireChannelFlow,
) -> Result<AdminDashboardChannelFlow, AdminDashboardStorageError> {
    let protocol =
        Protocol::from_str(&flow.protocol).map_err(|_| AdminDashboardStorageError::Invariant)?;
    let channel_id =
        ChannelId::new(flow.channel_id).map_err(|_| AdminDashboardStorageError::Invariant)?;
    if flow.channel_name.is_empty()
        || flow.channel_name.len() > 128
        || flow.channel_name.trim() != flow.channel_name
        || flow.channel_name.chars().any(char::is_control)
    {
        return Err(AdminDashboardStorageError::Invariant);
    }
    Ok(AdminDashboardChannelFlow::new(
        protocol,
        channel_id,
        flow.channel_name,
        flow.request_count,
    ))
}

fn decode_flow_path(
    path: WireFlowPath,
) -> Result<AdminDashboardFlowPath, AdminDashboardStorageError> {
    let channel_id =
        ChannelId::new(path.channel_id).map_err(|_| AdminDashboardStorageError::Invariant)?;
    for (value, max_bytes) in [
        (path.group_name.as_str(), 128_usize),
        (path.channel_name.as_str(), 128_usize),
        (path.model.as_str(), 255_usize),
    ] {
        if value.is_empty()
            || value.len() > max_bytes
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(AdminDashboardStorageError::Invariant);
        }
    }
    Ok(AdminDashboardFlowPath::new(
        path.user_id,
        path.group_id,
        path.group_name,
        channel_id,
        path.channel_name,
        path.model,
        path.request_count,
        path.quota_consumed,
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    struct StaticTransport {
        result: Result<Vec<u8>, ClickHouseQueryError>,
    }

    impl ClickHouseQueryTransport for StaticTransport {
        fn query(&self, _: i64, _: i64) -> ClickHouseQueryFuture<'_> {
            let result = self.result.clone();
            Box::pin(async move { result })
        }
    }

    fn config() -> ClickHouseAdminDashboardConfig {
        ClickHouseAdminDashboardConfig::new(
            "https://clickhouse.example/query",
            "SELECT usage, outcomes, hourly, performance FROM dashboard_snapshot WHERE period_start = {period_start:Int64} AND period_end = {period_end:Int64}",
            SecretString::new("dashboard_reader"),
            SecretString::new("dashboard_secret"),
            Duration::from_secs(4),
            128 * 1024,
        )
        .unwrap()
    }

    fn empty_wire(period_start: i64) -> serde_json::Value {
        let hourly = (0..24)
            .map(|index| {
                let start = period_start + index * 3_600;
                json!({
                    "period_start": start,
                    "period_end": start + 3_600,
                    "request_count": 0,
                    "quota_consumed": 0
                })
            })
            .collect::<Vec<_>>();
        json!({
            "usage": {
                "request_count": 0,
                "quota_consumed": 0,
                "upstream_usage_count": 0,
                "estimated_usage_count": 0,
                "per_token_request_count": 0,
                "per_call_request_count": 0,
                "free_request_count": 0
            },
            "outcomes": {
                "request_count": 0,
                "successful_request_count": 0,
                "failed_request_count": 0,
                "other_success_count": 0,
                "failures": [],
                "channel_flows": [],
                "flow_request_count": 0,
                "flow_quota_consumed": 0,
                "flow_paths": []
            },
            "hourly": hourly,
            "performance": {
                "first_token_sample_count": 0,
                "average_first_token_ms": null,
                "slow_first_token_count": 0,
                "slow_first_token_threshold_ms": SLOW_FIRST_TOKEN_THRESHOLD_MS,
                "duration_sample_count": 0,
                "average_duration_ms": null,
                "slow_request_count": 0,
                "slow_request_threshold_ms": SLOW_REQUEST_THRESHOLD_MS
            }
        })
    }

    #[test]
    fn config_owns_readonly_limits_and_redacts_sensitive_values() {
        let config = config();
        let target = Url::parse(&config.request_target(7, 86_407).unwrap()).unwrap();
        let pairs = target
            .query_pairs()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(pairs.get("readonly").map(|value| value.as_ref()), Some("2"));
        assert_eq!(
            pairs.get("param_period_start").map(|value| value.as_ref()),
            Some("7")
        );
        assert_eq!(
            pairs.get("param_period_end").map(|value| value.as_ref()),
            Some("86407")
        );
        assert!(config.query.ends_with("FORMAT JSONEachRow"));

        let debug = format!("{config:?}");
        for secret in [
            "clickhouse.example",
            "dashboard_snapshot",
            "dashboard_reader",
            "dashboard_secret",
        ] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn config_rejects_ambient_or_unbounded_query_inputs() {
        for endpoint in [
            "file:///tmp/clickhouse",
            "https://user:secret@clickhouse.example",
            "https://clickhouse.example/?readonly=0",
        ] {
            assert_eq!(
                ClickHouseAdminDashboardConfig::new(
                    endpoint,
                    "SELECT {period_start:Int64}, {period_end:Int64}",
                    SecretString::new("reader"),
                    SecretString::new("secret"),
                    Duration::from_secs(1),
                    1024,
                )
                .unwrap_err(),
                ClickHouseAdminDashboardConfigError::InvalidEndpoint
            );
        }
        for query in [
            "SELECT 1",
            "SELECT {period_start:Int64}; SELECT {period_end:Int64}",
        ] {
            assert_eq!(
                ClickHouseAdminDashboardConfig::new(
                    "https://clickhouse.example",
                    query,
                    SecretString::new("reader"),
                    SecretString::new("secret"),
                    Duration::from_secs(1),
                    1024,
                )
                .unwrap_err(),
                ClickHouseAdminDashboardConfigError::InvalidQuery
            );
        }
    }

    #[tokio::test]
    async fn adapter_decodes_one_closed_analytics_row() {
        let mut wire = empty_wire(7);
        wire["usage"] = json!({
            "request_count": 2,
            "quota_consumed": 9,
            "upstream_usage_count": 1,
            "estimated_usage_count": 1,
            "per_token_request_count": 1,
            "per_call_request_count": 0,
            "free_request_count": 1
        });
        wire["outcomes"] = json!({
            "request_count": 2,
            "successful_request_count": 1,
            "failed_request_count": 1,
            "other_success_count": 0,
            "failures": [{"kind": "invalid_request", "request_count": 1}],
            "channel_flows": [{
                "protocol": "openai_chat",
                "channel_id": 7,
                "channel_name": "primary",
                "request_count": 1
            }],
            "flow_request_count": 1,
            "flow_quota_consumed": 9,
            "flow_paths": [{
                "user_id": 11,
                "group_id": 3,
                "group_name": "Default",
                "channel_id": 7,
                "channel_name": "primary",
                "model": "gpt-5",
                "request_count": 1,
                "quota_consumed": 9
            }]
        });
        wire["hourly"][0]["request_count"] = json!(2);
        wire["hourly"][0]["quota_consumed"] = json!(9);
        wire["performance"] = json!({
            "first_token_sample_count": 1,
            "average_first_token_ms": 500,
            "slow_first_token_count": 0,
            "slow_first_token_threshold_ms": SLOW_FIRST_TOKEN_THRESHOLD_MS,
            "duration_sample_count": 2,
            "average_duration_ms": 11000,
            "slow_request_count": 1,
            "slow_request_threshold_ms": SLOW_REQUEST_THRESHOLD_MS
        });
        let body = format!("{wire}\n");
        let storage = ClickHouseAdminDashboardStorage::with_transport(Arc::new(StaticTransport {
            result: Ok(body.into_bytes()),
        }));
        let actual = storage.analytics_snapshot(7, 86_407).await.unwrap();
        let hourly = (0..24)
            .map(|index| {
                let start = 7 + index * 3_600;
                AdminDashboardHourlyPoint::new(
                    start,
                    start + 3_600,
                    if index == 0 { 2 } else { 0 },
                    if index == 0 { 9 } else { 0 },
                )
            })
            .collect();
        let expected = AdminDashboardAnalyticsSnapshot::new(
            AdminDashboardUsageSnapshot::new(2, 9, 1, 1, 1, 0, 1),
            AdminDashboardOutcomeSnapshot::new(
                2,
                1,
                1,
                0,
                vec![AdminDashboardFailure::new(
                    AdminDashboardFailureKind::InvalidRequest,
                    1,
                )],
                vec![AdminDashboardChannelFlow::new(
                    Protocol::OpenAiChat,
                    ChannelId::new(7).unwrap(),
                    "primary".to_owned(),
                    1,
                )],
                1,
                9,
                vec![AdminDashboardFlowPath::new(
                    11,
                    3,
                    "Default".to_owned(),
                    ChannelId::new(7).unwrap(),
                    "primary".to_owned(),
                    "gpt-5".to_owned(),
                    1,
                    9,
                )],
            ),
            hourly,
            AdminDashboardPerformance::new(
                1,
                Some(500),
                0,
                SLOW_FIRST_TOKEN_THRESHOLD_MS,
                2,
                Some(11_000),
                1,
                SLOW_REQUEST_THRESHOLD_MS,
            ),
        );
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn adapter_fails_closed_on_transport_size_or_wire_drift() {
        for error in [
            ClickHouseQueryError::Unavailable,
            ClickHouseQueryError::ResponseTooLarge,
        ] {
            let storage =
                ClickHouseAdminDashboardStorage::with_transport(Arc::new(StaticTransport {
                    result: Err(error),
                }));
            assert!(storage.analytics_snapshot(7, 86_407).await.is_err());
        }

        let valid = empty_wire(7);
        let duplicate = format!("{valid}\n{valid}\n");
        assert_eq!(
            decode_snapshot(duplicate.as_bytes()),
            Err(AdminDashboardStorageError::Invariant)
        );

        let mut invalid = empty_wire(7);
        invalid["performance"]["slow_request_threshold_ms"] = json!(9_999);
        assert_eq!(
            decode_snapshot(format!("{invalid}\n").as_bytes()),
            Err(AdminDashboardStorageError::Invariant)
        );
    }
}
