use std::{error::Error, time::Duration};

use sea_orm::{ActiveModelTrait, Set, entity::prelude::TimeDateTimeWithTimeZone};

use super::{
    DatabaseOptions, MigrationOptions, PublicServiceStatusError, PublicServiceStatusRepository,
};
use crate::entity::request_outcome_logs;

#[tokio::test]
async fn public_status_aggregates_terminal_requests_and_excludes_window_end()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let sampled_at = 87_400;
    let event_at = TimeDateTimeWithTimeZone::from_unix_timestamp(2_000)?;
    for id in 1..=51_i64 {
        request_outcome_logs::ActiveModel {
            request_id: Set(format!("public-status-{id}")),
            protocol: Set("openai_chat".to_owned()),
            operation: Set("chat".to_owned()),
            model: Set("private-model-name-is-not-in-the-public-report".to_owned()),
            outcome: Set(if id == 51 { 2 } else { 1 }),
            error_kind: Set((id == 51).then(|| "upstream_network".to_owned())),
            channel_id: Set(None),
            duration_ms: Set(10),
            created_at: Set(if id == 51 {
                TimeDateTimeWithTimeZone::from_unix_timestamp(sampled_at)?
            } else {
                event_at
            }),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
    }

    let repository = PublicServiceStatusRepository::new(pool.clone(), Duration::from_secs(2))?;
    let report = repository.report(sampled_at).await?;
    assert_eq!(report.sampled_at, sampled_at);
    assert_eq!(report.period_start, 1_000);
    assert_eq!(report.request_count, 50);
    assert_eq!(report.successful_request_count, 50);
    assert_eq!(report.failed_request_count, 0);
    assert_eq!(report.unknown_request_count, 0);
    assert_eq!(report.average_success_duration_ms, Some(10));
    assert_eq!(report.hourly.len(), 24);
    assert_eq!(
        report
            .hourly
            .iter()
            .map(|point| point.successful_request_count)
            .sum::<i64>(),
        50
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn public_status_rejects_invalid_configuration_and_time_window() -> Result<(), Box<dyn Error>>
{
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    assert!(matches!(
        PublicServiceStatusRepository::new(pool.clone(), Duration::ZERO),
        Err(PublicServiceStatusError::InvalidConfiguration)
    ));
    let repository = PublicServiceStatusRepository::new(pool.clone(), Duration::from_secs(1))?;
    assert!(matches!(
        repository.report(10).await,
        Err(PublicServiceStatusError::Invariant)
    ));
    pool.close().await?;
    Ok(())
}
