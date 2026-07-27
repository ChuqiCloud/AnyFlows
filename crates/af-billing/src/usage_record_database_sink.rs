use std::fmt;

use af_db::{
    UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError, UsageLogSemantics,
    UsageLogSource, UsageLogUsage, UsageLogWrite,
};
use af_protocol::{UsageSemantics, UsageSource};

use crate::{
    BillingMode, BillingUsageRecord, UsageRecordSink, UsageRecordSinkError, UsageRecordSinkFuture,
};

/// 将计费生命周期的最小用量事实写入 `af-db::UsageLogRepository`。
#[derive(Clone)]
pub struct DatabaseUsageRecordSink {
    repository: UsageLogRepository,
}

impl DatabaseUsageRecordSink {
    /// 包装已完成数据库迁移与连接配置的用量日志仓储。
    #[must_use]
    pub fn new(repository: UsageLogRepository) -> Self {
        Self { repository }
    }
}

impl UsageRecordSink for DatabaseUsageRecordSink {
    fn persist<'a>(&'a self, record: BillingUsageRecord) -> UsageRecordSinkFuture<'a> {
        Box::pin(async move {
            let write = to_usage_log_write(record).map_err(|_| UsageRecordSinkError::Invariant)?;
            self.repository
                .record(&write)
                .await
                .map(|_| ())
                .map_err(map_repository_error)
        })
    }
}

impl fmt::Debug for DatabaseUsageRecordSink {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseUsageRecordSink")
            .finish_non_exhaustive()
    }
}

fn to_usage_log_write(record: BillingUsageRecord) -> Result<UsageLogWrite, UsageRecordSinkError> {
    let usage = record.usage();
    let details = usage.details();
    let usage = UsageLogUsage::new(
        usage.input_tokens().get(),
        usage.output_tokens().get(),
        details.cache_read().get(),
        details.cache_creation_5m().get(),
        details.cache_creation_1h().get(),
        details.reasoning().get(),
        details.audio_input().get(),
        details.audio_output().get(),
    )
    .map_err(|_| UsageRecordSinkError::Invariant)?;
    let billing_mode = match record.billing_mode() {
        BillingMode::PerToken => UsageLogBillingMode::PerToken,
        BillingMode::Free => UsageLogBillingMode::Free,
    };
    let source = match usage_source(record.usage()) {
        UsageSource::Upstream => UsageLogSource::Upstream,
        UsageSource::Estimated => UsageLogSource::Estimated,
    };
    let semantics = match usage_semantics(record.usage()) {
        UsageSemantics::Inclusive => UsageLogSemantics::Inclusive,
        UsageSemantics::CacheSeparated => UsageLogSemantics::CacheSeparated,
    };
    Ok(UsageLogWrite::new(
        record.event_id(),
        record.principal(),
        billing_mode,
        usage,
        source,
        semantics,
        record.quota(),
    ))
}

fn usage_source(record: af_protocol::Usage) -> UsageSource {
    record.source()
}

fn usage_semantics(record: af_protocol::Usage) -> UsageSemantics {
    record.semantics()
}

// af-db 的错误枚举允许后续扩展；未知分类统一失败关闭为持久化不变量错误。
#[allow(unreachable_patterns)]
fn map_repository_error(error: UsageLogRepositoryError) -> UsageRecordSinkError {
    match error {
        UsageLogRepositoryError::OutcomeUnknown => UsageRecordSinkError::OutcomeUnknown,
        UsageLogRepositoryError::Query | UsageLogRepositoryError::InvalidConfiguration => {
            UsageRecordSinkError::Unavailable
        }
        UsageLogRepositoryError::Conflict => UsageRecordSinkError::Conflict,
        UsageLogRepositoryError::Invariant => UsageRecordSinkError::Invariant,
        _ => UsageRecordSinkError::Invariant,
    }
}
