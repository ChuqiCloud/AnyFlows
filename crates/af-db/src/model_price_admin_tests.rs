use std::error::Error;

use crate::{
    AdminModelCreateRecord, AdminModelLifecycleRecord, AdminModelModalitiesRecord,
    AdminModelRepository, AdminModelVisibilityRecord, AdminModelWriteRecord, DatabaseOptions,
    MigrationOptions, ModelPriceBillingMode, ModelPriceRepository, ModelPriceWriteError,
    ModelPriceWriteRecord,
};
use rust_decimal::Decimal;

#[tokio::test]
async fn batch_insert_and_update_enforce_optimistic_versions() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let models = AdminModelRepository::new(pool.clone(), std::time::Duration::from_secs(1))?;
    models
        .create(AdminModelCreateRecord::new(
            "gpt-test".to_owned(),
            model_fields(),
        ))
        .await?;
    let prices = ModelPriceRepository::new(pool.clone());

    let created = prices.apply_batch(vec![write(None, Decimal::ONE)]).await?;
    assert_eq!(created[0].version(), 1);
    assert_eq!(created[0].prices()[0], Decimal::ONE);

    assert_eq!(
        prices
            .apply_batch(vec![write(None, Decimal::new(2, 0))])
            .await,
        Err(ModelPriceWriteError::Conflict)
    );
    let updated = prices
        .apply_batch(vec![write(Some(1), Decimal::new(2, 0))])
        .await?;
    assert_eq!(updated[0].version(), 2);
    assert_eq!(updated[0].prices()[4], Decimal::new(2, 0));

    let (page, next) = prices.list_page(None, 100).await?.into_parts();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].model(), "gpt-test");
    assert!(next.is_none());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_updates_context_window_with_price_in_the_same_transaction()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let models = AdminModelRepository::new(pool.clone(), std::time::Duration::from_secs(1))?;
    let created = models
        .create(AdminModelCreateRecord::new(
            "gpt-test".to_owned(),
            model_fields(),
        ))
        .await?;
    let prices = ModelPriceRepository::new(pool.clone());

    let write = ModelPriceWriteRecord::new_with_metadata(
        "gpt-test".to_owned(),
        None,
        Some(400_000),
        ModelPriceBillingMode::PerToken,
        [Decimal::ONE; 5],
        None,
    )?;
    prices.apply_batch(vec![write]).await?;

    let updated = models.get(created.model_id()).await?;
    let crate::AdminModelLookupOutcome::Found(updated) = updated else {
        panic!("模型应保持存在");
    };
    assert_eq!(updated.context_window(), Some(400_000));
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn price_version_conflict_rolls_back_context_window_update() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let models = AdminModelRepository::new(pool.clone(), std::time::Duration::from_secs(1))?;
    let created = models
        .create(AdminModelCreateRecord::new(
            "gpt-test".to_owned(),
            model_fields(),
        ))
        .await?;
    let prices = ModelPriceRepository::new(pool.clone());
    prices.apply_batch(vec![write(None, Decimal::ONE)]).await?;

    let conflicting = ModelPriceWriteRecord::new_with_metadata(
        "gpt-test".to_owned(),
        None,
        Some(400_000),
        ModelPriceBillingMode::PerToken,
        [Decimal::new(2, 0); 5],
        None,
    )?;
    assert_eq!(
        prices.apply_batch(vec![conflicting]).await,
        Err(ModelPriceWriteError::Conflict)
    );

    let updated = models.get(created.model_id()).await?;
    let crate::AdminModelLookupOutcome::Found(updated) = updated else {
        panic!("模型应保持存在");
    };
    assert_eq!(updated.context_window(), Some(128_000));
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn expression_price_round_trips_and_is_cleared_when_switching_mode()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let models = AdminModelRepository::new(pool.clone(), std::time::Duration::from_secs(1))?;
    models
        .create(AdminModelCreateRecord::new(
            "gpt-test".to_owned(),
            model_fields(),
        ))
        .await?;
    let prices = ModelPriceRepository::new(pool.clone());

    let expression = ModelPriceWriteRecord::new_with_expression(
        "gpt-test".to_owned(),
        None,
        ModelPriceBillingMode::Expression,
        [Decimal::ZERO; 5],
        Some("tier(\"base\", p)".to_owned()),
    )?;
    let created = prices.apply_batch(vec![expression]).await?;
    assert_eq!(created[0].billing_expression(), Some("tier(\"base\", p)"));

    let fixed = ModelPriceWriteRecord::new(
        "gpt-test".to_owned(),
        Some(created[0].version()),
        ModelPriceBillingMode::PerToken,
        [Decimal::ONE; 5],
    )?;
    let updated = prices.apply_batch(vec![fixed]).await?;
    assert_eq!(updated[0].billing_mode(), ModelPriceBillingMode::PerToken);
    assert_eq!(updated[0].billing_expression(), None);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_is_atomic_when_any_model_or_version_is_invalid() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let models = AdminModelRepository::new(pool.clone(), std::time::Duration::from_secs(1))?;
    models
        .create(AdminModelCreateRecord::new(
            "gpt-test".to_owned(),
            model_fields(),
        ))
        .await?;
    let prices = ModelPriceRepository::new(pool.clone());

    let error = prices
        .apply_batch(vec![
            write(None, Decimal::ONE),
            ModelPriceWriteRecord::new(
                "missing-model".to_owned(),
                None,
                ModelPriceBillingMode::PerToken,
                [Decimal::ONE; 5],
            )?,
        ])
        .await;
    assert_eq!(error, Err(ModelPriceWriteError::ModelNotFound));
    assert!(prices.load_all().await?.is_empty());

    pool.close().await?;
    Ok(())
}

fn write(expected_version: Option<u64>, price: Decimal) -> ModelPriceWriteRecord {
    ModelPriceWriteRecord::new(
        "gpt-test".to_owned(),
        expected_version,
        ModelPriceBillingMode::PerToken,
        [price; 5],
    )
    .unwrap()
}

fn model_fields() -> AdminModelWriteRecord {
    AdminModelWriteRecord::new(
        "GPT Test".to_owned(),
        "openai".to_owned(),
        None,
        None,
        Vec::new(),
        Some(128_000),
        AdminModelModalitiesRecord::new(true, false, false, false),
        AdminModelModalitiesRecord::new(true, false, false, false),
        false,
        true,
        AdminModelVisibilityRecord::Public,
        AdminModelLifecycleRecord::Active,
    )
}
