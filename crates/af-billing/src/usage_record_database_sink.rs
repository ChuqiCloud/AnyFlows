use std::fmt;

use af_db::{
    UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError, UsageLogSemantics,
    UsageLogSource, UsageLogUsage, UsageLogVideoResolution, UsageLogWrite,
};
use af_protocol::{UsageSemantics, UsageSource, VideoResolution};

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
        BillingMode::PerCall => UsageLogBillingMode::PerCall,
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
    let write = UsageLogWrite::new(
        record.event_id(),
        record.principal(),
        billing_mode,
        usage,
        source,
        semantics,
        record.quota(),
    );
    let audio_duration_nanoseconds = record
        .dimensions()
        .audio_duration()
        .map(|duration| i64::try_from(duration.as_nanoseconds()))
        .transpose()
        .map_err(|_| UsageRecordSinkError::Invariant)?;
    let write = write
        .with_audio_duration_nanoseconds(audio_duration_nanoseconds)
        .map_err(|_| UsageRecordSinkError::Invariant)?;
    let dimensions = record.dimensions();
    let video_duration_seconds = dimensions
        .video_duration()
        .map(|duration| i64::from(duration.seconds()));
    let video_resolution = dimensions.video_resolution().map(map_video_resolution);
    write
        .with_video_dimensions(video_duration_seconds, video_resolution)
        .map_err(|_| UsageRecordSinkError::Invariant)
}

const fn map_video_resolution(resolution: VideoResolution) -> UsageLogVideoResolution {
    match resolution {
        VideoResolution::P480 => UsageLogVideoResolution::P480,
        VideoResolution::P720 => UsageLogVideoResolution::P720,
        VideoResolution::P1080 => UsageLogVideoResolution::P1080,
    }
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

#[cfg(test)]
mod tests {
    use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};
    use af_protocol::{
        TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource, VideoDuration,
        VideoResolution,
    };

    use super::*;

    #[test]
    fn video_dimensions_are_preserved_for_database_audit() {
        let usage = Usage::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            UsageDetails::new(
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
                TokenCount::ZERO,
            ),
            UsageSource::Upstream,
            UsageSemantics::Inclusive,
        )
        .unwrap();
        let dimensions = crate::BillingUsageDimensions::with_video(
            VideoDuration::new(8).unwrap(),
            Some(VideoResolution::P720),
        );
        let record = BillingUsageRecord::new_with_dimensions(
            BillingReservationId::new([1; 16]).unwrap(),
            GatewayPrincipal::new(
                TokenId::new(1).unwrap(),
                UserId::new(2).unwrap(),
                GroupId::new(3).unwrap(),
            ),
            usage,
            dimensions,
            BillingMode::Free,
            Quota::ZERO,
        );

        let write = to_usage_log_write(record).unwrap();
        assert_eq!(write.video_duration_seconds(), Some(8));
        assert_eq!(
            write.video_resolution(),
            Some(UsageLogVideoResolution::P720)
        );
    }
}
