use std::{fmt, time::Duration};

use af_analytics::{
    AnalyticsExportControl, AnalyticsExportControlError, AnalyticsExportQueueSnapshot,
    AnalyticsExportReplayFuture, AnalyticsExportStatusFuture,
};
use af_config::ClickHouseExportSettings;
use af_db::{
    AnalyticsExportCompletionOutcome, AnalyticsExportFact, AnalyticsExportFactKind,
    AnalyticsExportRepository, AnalyticsExportRepositoryError,
};
use af_httpclient::{
    Body, HeaderMap, HeaderName, HeaderValue, HttpClientProvider, Method, StatusCode,
};
use af_telemetry::set_analytics_export_backlog;
use thiserror::Error;
use tokio::time::sleep;
use url::Url;

const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// 将主库 outbox 仓储暴露为不含敏感材料的管理运维端口。
#[derive(Clone)]
pub(crate) struct ClickHouseExportControl {
    repository: AnalyticsExportRepository,
}

impl ClickHouseExportControl {
    pub(crate) fn new(repository: AnalyticsExportRepository) -> Self {
        Self { repository }
    }
}

impl AnalyticsExportControl for ClickHouseExportControl {
    fn status(&self) -> AnalyticsExportStatusFuture<'_> {
        let repository = self.repository.clone();
        Box::pin(async move {
            let counts = repository.queue_counts().await.map_err(map_control_error)?;
            Ok(AnalyticsExportQueueSnapshot::new(
                counts.pending_count(),
                counts.leased_count(),
                counts.published_count(),
            ))
        })
    }

    fn replay(&self, limit: u16) -> AnalyticsExportReplayFuture<'_> {
        let repository = self.repository.clone();
        Box::pin(async move {
            repository
                .replay_now(u64::from(limit))
                .await
                .map_err(map_control_error)
        })
    }
}

fn map_control_error(error: AnalyticsExportRepositoryError) -> AnalyticsExportControlError {
    match error {
        AnalyticsExportRepositoryError::Query | AnalyticsExportRepositoryError::Timeout => {
            AnalyticsExportControlError::Unavailable
        }
        AnalyticsExportRepositoryError::Invariant => AnalyticsExportControlError::Invariant,
    }
}

/// ClickHouse 事实投递运行时的装配错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ClickHouseExportRuntimeError {
    /// 端点无法构造安全请求目标。
    #[error("ClickHouse 事实投递端点无效")]
    InvalidEndpoint,
    /// ClickHouse 事实投递认证头无法构造。
    #[error("ClickHouse 事实投递认证配置无效")]
    InvalidHeaders,
}

/// 受监督的 ClickHouse 异步事实投递 worker。
#[derive(Clone)]
pub struct ClickHouseExportRuntime {
    repository: AnalyticsExportRepository,
    clients: HttpClientProvider,
    endpoint: Url,
    usage_query: String,
    outcome_query: String,
    username: HeaderValue,
    password: HeaderValue,
    batch_size: usize,
    interval: Duration,
    timeout: Duration,
    max_request_bytes: usize,
    backfill_batch_size: usize,
}

impl ClickHouseExportRuntime {
    /// 按独立写入账号构造运行时；不会复用只读分析配置。
    pub fn new(
        settings: &ClickHouseExportSettings,
        clients: HttpClientProvider,
        repository: AnalyticsExportRepository,
    ) -> Result<Self, ClickHouseExportRuntimeError> {
        let endpoint = Url::parse(settings.endpoint().expose())
            .map_err(|_| ClickHouseExportRuntimeError::InvalidEndpoint)?;
        if !matches!(endpoint.scheme(), "http" | "https")
            || !endpoint.has_host()
            || endpoint.username() != ""
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(ClickHouseExportRuntimeError::InvalidEndpoint);
        }
        let username = HeaderValue::from_str(settings.username().expose())
            .map_err(|_| ClickHouseExportRuntimeError::InvalidHeaders)?;
        let password = HeaderValue::from_str(settings.password().expose())
            .map_err(|_| ClickHouseExportRuntimeError::InvalidHeaders)?;
        Ok(Self {
            repository,
            clients,
            endpoint,
            usage_query: settings.usage_insert_query().expose().trim().to_owned(),
            outcome_query: settings.outcome_insert_query().expose().trim().to_owned(),
            username,
            password,
            batch_size: settings.batch_size(),
            interval: Duration::from_secs(settings.interval_secs()),
            timeout: Duration::from_secs(settings.timeout_secs()),
            max_request_bytes: settings.max_request_bytes(),
            backfill_batch_size: settings.backfill_batch_size(),
        })
    }

    /// 持续执行有界回填、领取、写入和 CAS 闭合；关闭时不再领取新事件。
    pub async fn run_until(&self, shutdown: impl std::future::Future<Output = ()>) {
        let mut shutdown = std::pin::pin!(shutdown);
        let _ = self
            .repository
            .backfill_missing(self.backfill_batch_size as u64)
            .await;
        loop {
            let processed = tokio::select! {
                () = &mut shutdown => return,
                processed = self.process_batch() => processed,
            };
            if let Ok(backlog) = self.repository.backlog_count().await {
                set_analytics_export_backlog(backlog);
            }
            if processed == 0 {
                // 空闲时继续用小批次补录历史事实，直到没有缺失指针，避免只回填启动瞬间的一页数据。
                let backfilled = self
                    .repository
                    .backfill_missing(self.backfill_batch_size as u64)
                    .await
                    .unwrap_or_default();
                if backfilled > 0 {
                    continue;
                }
                tokio::select! {
                    () = &mut shutdown => return,
                    () = sleep(self.interval) => {}
                }
            } else {
                tokio::select! {
                    () = &mut shutdown => return,
                    () = tokio::task::yield_now() => {}
                }
            }
        }
    }

    async fn process_batch(&self) -> usize {
        let mut processed = 0;
        for _ in 0..self.batch_size {
            let lease = match self.repository.claim_next_due().await {
                Ok(Some(lease)) => lease,
                Ok(None) | Err(_) => break,
            };
            processed += 1;
            let success = match self.repository.load_fact(&lease).await {
                Ok(fact) => self.write_fact(&fact).await,
                Err(_) => false,
            };
            let result = if success {
                self.repository.mark_published_now(&lease).await
            } else {
                self.repository.record_failure_now(&lease).await
            };
            if matches!(result, Ok(AnalyticsExportCompletionOutcome::Stale)) {
                tracing::debug!(target: "af_server::analytics_export", "分析事实投递租约已由其他实例推进");
            }
        }
        processed
    }

    async fn write_fact(&self, fact: &AnalyticsExportFact) -> bool {
        let mut body = match serde_json::to_vec(fact.payload()) {
            Ok(body) => body,
            Err(_) => return false,
        };
        body.push(b'\n');
        if body.len() > self.max_request_bytes {
            return false;
        }
        let query = match fact.kind() {
            AnalyticsExportFactKind::UsageLog => &self.usage_query,
            AnalyticsExportFactKind::RequestOutcome => &self.outcome_query,
        };
        let mut target = self.endpoint.clone();
        target
            .query_pairs_mut()
            .append_pair("query", &format!("{query}\nFORMAT JSONEachRow"))
            .append_pair("async_insert", "1")
            .append_pair("wait_for_async_insert", "1")
            .append_pair("insert_deduplication_token", fact.deduplication_token());
        let mut headers = HeaderMap::with_capacity(3);
        headers.insert(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("application/x-ndjson"),
        );
        headers.insert(
            HeaderName::from_static("x-clickhouse-user"),
            self.username.clone(),
        );
        headers.insert(
            HeaderName::from_static("x-clickhouse-key"),
            self.password.clone(),
        );
        let client = match self.clients.get(Some(self.timeout)) {
            Ok(client) => client,
            Err(_) => return false,
        };
        let response = match client
            .execute(
                Method::POST,
                target.as_str(),
                headers,
                Some(Body::from(body)),
            )
            .await
        {
            Ok(response) => response,
            Err(_) => return false,
        };
        if response.status() != StatusCode::OK
            || response
                .content_length()
                .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return false;
        }
        let mut body_len = 0_usize;
        let mut stream = response.into_bytes_stream();
        while let Some(chunk) = stream.next_chunk().await {
            let Ok(chunk) = chunk else {
                return false;
            };
            let Some(next_len) = body_len.checked_add(chunk.len()) else {
                return false;
            };
            if next_len > MAX_RESPONSE_BYTES {
                return false;
            }
            body_len = next_len;
        }
        true
    }
}

impl fmt::Debug for ClickHouseExportRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ClickHouseExportRuntime(<redacted>)")
    }
}
