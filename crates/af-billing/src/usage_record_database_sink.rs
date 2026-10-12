use std::{fmt, future::Future, sync::Arc};

use af_db::{
    UsageLogBillingMode, UsageLogRepository, UsageLogRepositoryError, UsageLogSemantics,
    UsageLogSource, UsageLogUsage, UsageLogVideoResolution, UsageLogWrite,
};
use af_protocol::{ReasoningEffort, UsageSemantics, UsageSource, VideoResolution};
use thiserror::Error;

use crate::{
    BillingMode, BillingUsageRecord, UsageRecordSink, UsageRecordSinkError, UsageRecordSinkFuture,
};

/// 将计费生命周期的最小用量事实写入 `af-db::UsageLogRepository`。
#[derive(Clone)]
pub struct DatabaseUsageRecordSink {
    repository: UsageLogRepository,
    projection: Option<Arc<dyn UsageRecordProjection>>,
}

/// Optional distribution-owned projection of a usage fact.
pub trait UsageRecordProjection: Send + Sync + 'static {
    fn persist<'a>(&'a self, record: BillingUsageRecord) -> UsageRecordProjectionFuture<'a>;
}

pub type UsageRecordProjectionFuture<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<(), UsageRecordProjectionError>> + Send + 'a>>;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum UsageRecordProjectionError {
    #[error("用量投影不可用")]
    Unavailable,
    #[error("用量投影数据无效")]
    Invalid,
    #[error("用量投影结果未知")]
    OutcomeUnknown,
    #[error("用量投影发生冲突")]
    Conflict,
}

impl DatabaseUsageRecordSink {
    /// 包装已完成数据库迁移与连接配置的用量日志仓储。
    #[must_use]
    pub fn new(repository: UsageLogRepository) -> Self {
        Self {
            repository,
            projection: None,
        }
    }

    /// Register distribution storage; service facts bypass the personal usage table.
    #[must_use]
    pub fn with_projection(mut self, projection: Arc<dyn UsageRecordProjection>) -> Self {
        self.projection = Some(projection);
        self
    }
}

impl UsageRecordSink for DatabaseUsageRecordSink {
    fn persist<'a>(&'a self, record: BillingUsageRecord) -> UsageRecordSinkFuture<'a> {
        Box::pin(async move {
            if record.service_account_identity().is_some() {
                // Service usage must never create a personal usage row or disappear
                // when the distribution forgot to register its private storage.
                return self
                    .projection
                    .as_ref()
                    .ok_or(UsageRecordSinkError::Unavailable)?
                    .persist(record)
                    .await
                    .map_err(map_projection_error);
            }
            let write = to_usage_log_write(record).map_err(|_| UsageRecordSinkError::Invariant)?;
            self.repository
                .record(&write)
                .await
                .map_err(map_repository_error)?;
            if let Some(projection) = self.projection.as_ref() {
                projection
                    .persist(record)
                    .await
                    .map_err(map_projection_error)?;
            }
            Ok(())
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
        record.principal().ok_or(UsageRecordSinkError::Invariant)?,
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
    let write = write
        .with_video_dimensions(video_duration_seconds, video_resolution)
        .map_err(|_| UsageRecordSinkError::Invariant)?;
    let observation = record.observation();
    let (Some(context), Some(timing)) = (observation.context(), observation.timing()) else {
        return Ok(write);
    };
    write
        .with_call_observation(
            context.request_id(),
            context.model(),
            context.protocol(),
            context.operation(),
            context.stream(),
            context
                .reasoning_effort()
                .map(reasoning_effort_database_value),
            context.reasoning_budget_tokens().map(|value| value.get()),
            timing.first_token_ms(),
            timing.duration_ms(),
        )
        .map_err(|_| UsageRecordSinkError::Invariant)
}

const fn map_video_resolution(resolution: VideoResolution) -> UsageLogVideoResolution {
    match resolution {
        VideoResolution::P480 => UsageLogVideoResolution::P480,
        VideoResolution::P720 => UsageLogVideoResolution::P720,
        VideoResolution::P1080 => UsageLogVideoResolution::P1080,
    }
}

const fn reasoning_effort_database_value(effort: ReasoningEffort) -> i16 {
    match effort {
        ReasoningEffort::None => 1,
        ReasoningEffort::Minimal => 2,
        ReasoningEffort::Low => 3,
        ReasoningEffort::Medium => 4,
        ReasoningEffort::High => 5,
        ReasoningEffort::ExtraHigh => 6,
        ReasoningEffort::Max => 7,
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

fn map_projection_error(error: UsageRecordProjectionError) -> UsageRecordSinkError {
    match error {
        UsageRecordProjectionError::Unavailable => UsageRecordSinkError::Unavailable,
        UsageRecordProjectionError::Invalid => UsageRecordSinkError::Invariant,
        UsageRecordProjectionError::OutcomeUnknown => UsageRecordSinkError::OutcomeUnknown,
        UsageRecordProjectionError::Conflict => UsageRecordSinkError::Conflict,
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

    #[tokio::test]
    async fn service_usage_requires_projection_and_never_uses_personal_storage() {
        use af_domain::{
            GatewayIdentity, OrganizationId, OrganizationServiceAccountId,
            OrganizationServiceAccountKey, OrganizationServiceAccountLocator,
            OrganizationServiceAccountRuntimeIdentity,
        };
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Projection(AtomicUsize);
        impl UsageRecordProjection for Projection {
            fn persist<'a>(
                &'a self,
                record: BillingUsageRecord,
            ) -> UsageRecordProjectionFuture<'a> {
                Box::pin(async move {
                    assert!(record.principal().is_none());
                    self.0.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }
        }
        let pool = af_db::connect_and_migrate(
            &af_db::DatabaseOptions::new("sqlite::memory:").unwrap(),
            af_db::MigrationOptions::default(),
        )
        .await
        .unwrap();
        let identity = OrganizationServiceAccountRuntimeIdentity::new(
            OrganizationServiceAccountLocator::new(
                OrganizationId::new(44).unwrap(),
                OrganizationServiceAccountId::new(55).unwrap(),
            ),
            OrganizationServiceAccountKey::new([1; 16]).unwrap(),
            OrganizationServiceAccountKey::new([2; 16]).unwrap(),
        );
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
        let record = BillingUsageRecord::new_with_identity(
            BillingReservationId::new([63; 16]).unwrap(),
            GatewayIdentity::ServiceAccount(identity),
            usage,
            crate::BillingUsageDimensions::empty(),
            BillingMode::Free,
            Quota::ZERO,
        );
        assert!(matches!(
            to_usage_log_write(record),
            Err(UsageRecordSinkError::Invariant)
        ));
        let sink = DatabaseUsageRecordSink::new(UsageLogRepository::new(pool.clone()));
        assert_eq!(
            sink.persist(record).await,
            Err(UsageRecordSinkError::Unavailable)
        );
        let projection = Arc::new(Projection(AtomicUsize::new(0)));
        sink.with_projection(projection.clone())
            .persist(record)
            .await
            .unwrap();
        assert_eq!(projection.0.load(Ordering::SeqCst), 1);
        use sea_orm::ConnectionTrait as _;
        let row = pool
            .extension_connection()
            .query_one(sea_orm::Statement::from_string(
                sea_orm::DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM usage_logs".to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get::<i64>("", "count").unwrap(), 0);
    }
}
