use std::{error::Error, time::Duration};

use crate::{
    DatabaseOptions, MigrationOptions, PluginEventDeliveryClaimOutcome,
    PluginEventDeliveryRepository, PluginEventDeliveryRepositoryConfigError,
};

#[tokio::test]
async fn event_repository_rejects_zero_timeout_and_empty_queue_is_empty()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    assert!(matches!(
        PluginEventDeliveryRepository::new(pool.clone(), Duration::ZERO),
        Err(PluginEventDeliveryRepositoryConfigError::ZeroOperationTimeout)
    ));
    let repository = PluginEventDeliveryRepository::new(pool.clone(), Duration::from_secs(5))?;
    assert_eq!(
        repository.claim_next(1_800_000_000).await?,
        PluginEventDeliveryClaimOutcome::Empty
    );
    pool.close().await?;
    Ok(())
}
