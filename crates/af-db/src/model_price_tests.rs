use std::error::Error;

use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ConnectionTrait, DbBackend, EntityTrait,
    IntoActiveModel, Set, Statement,
};

use crate::{
    DatabaseOptions, MigrationOptions, ModelPriceBillingMode, ModelPriceRepository,
    ModelPriceRepositoryError,
    entity::{SensitiveDecimal, SensitiveString, model_prices},
};

#[test]
fn price_decimal_matches_production_column_precision() {
    assert!(crate::model_price::is_valid_model_price_decimal(
        Decimal::from(9_999_999_999_i64)
    ));
    assert!(crate::model_price::is_valid_model_price_decimal(
        Decimal::new(1, 28)
    ));
    assert!(!crate::model_price::is_valid_model_price_decimal(
        Decimal::from(10_000_000_000_i64)
    ));
    assert!(!crate::model_price::is_valid_model_price_decimal(
        Decimal::NEGATIVE_ONE
    ));
}

#[tokio::test]
async fn repository_loads_a_stable_validated_catalog() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    model_price("z-model", 1, [Decimal::new(1, 28); 5], 7)
        .insert(pool.connection())
        .await?;
    model_price("a-free", 2, [Decimal::ZERO; 5], 3)
        .insert(pool.connection())
        .await?;

    let records = ModelPriceRepository::new(pool.clone()).load_all().await?;

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].model(), "a-free");
    assert_eq!(records[0].billing_mode(), ModelPriceBillingMode::Free);
    assert_eq!(records[0].prices(), [Decimal::ZERO; 5]);
    assert_eq!(records[0].version(), 3);
    assert_eq!(records[1].model(), "z-model");
    assert_eq!(records[1].billing_mode(), ModelPriceBillingMode::PerToken);
    assert_eq!(records[1].prices(), [Decimal::new(1, 28); 5]);
    assert_eq!(records[1].version(), 7);
    assert_eq!(records[1].billing_expression(), None);

    let rendered = format!("{:?}", records[1]);
    assert!(!rendered.contains("z-model"));
    assert!(!rendered.contains("0.0000000000000000000000000001"));
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn database_and_repository_reject_invalid_pricing_states() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;

    assert!(
        model_price("negative", 1, [Decimal::NEGATIVE_ONE; 5], 1)
            .insert(pool.connection())
            .await
            .is_err()
    );
    assert!(
        model_price("invalid-free", 2, [Decimal::ONE; 5], 1)
            .insert(pool.connection())
            .await
            .is_err()
    );
    assert!(
        model_price("invalid-mode", 3, [Decimal::ZERO; 5], 1)
            .insert(pool.connection())
            .await
            .is_err()
    );
    assert!(
        model_price_with_expression(
            "valid-expression",
            3,
            [Decimal::ZERO; 5],
            1,
            Some("tier(\"base\", p)")
        )
        .insert(pool.connection())
        .await
        .is_ok()
    );
    assert!(
        model_price_with_expression(
            "expression-with-price",
            3,
            [Decimal::ONE; 5],
            1,
            Some("tier(\"base\", p)")
        )
        .insert(pool.connection())
        .await
        .is_err()
    );
    assert!(
        model_price_with_expression("expression-without-source", 3, [Decimal::ZERO; 5], 1, None)
            .insert(pool.connection())
            .await
            .is_err()
    );
    assert!(
        model_price_with_expression(
            "per-token-with-source",
            1,
            [Decimal::ONE; 5],
            1,
            Some("tier(\"base\", p)")
        )
        .insert(pool.connection())
        .await
        .is_err()
    );
    assert!(
        model_price(" invalid-name ", 1, [Decimal::ZERO; 5], 1)
            .insert(pool.connection())
            .await
            .is_err()
    );

    // 裸 SQL 可绕过 ActiveModel 校验，读取边界仍必须让整个目录失败关闭。
    pool.connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO model_prices (model, billing_mode) VALUES (?, ?)",
            [" corrupt-model ".into(), 1_i16.into()],
        ))
        .await?;
    assert_eq!(
        ModelPriceRepository::new(pool.clone()).load_all().await,
        Err(ModelPriceRepositoryError::Invariant)
    );

    model_prices::Entity::delete_many()
        .exec(pool.connection())
        .await?;
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn active_model_enforces_pricing_invariants_without_database_checks()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;

    for invalid in [
        model_price("negative", 1, [Decimal::NEGATIVE_ONE; 5], 1),
        model_price("invalid-free", 2, [Decimal::ONE; 5], 1),
        model_price("invalid-mode", 3, [Decimal::ZERO; 5], 1),
        model_price("invalid-version", 1, [Decimal::ZERO; 5], 0),
        model_price("too-large", 1, [Decimal::from(10_000_000_000_i64); 5], 1),
        model_price(" invalid-name ", 1, [Decimal::ZERO; 5], 1),
    ] {
        assert!(invalid.before_save(pool.connection(), true).await.is_err());
    }

    let saved = model_price("updated-model", 1, [Decimal::ONE; 5], 1)
        .insert(pool.connection())
        .await?;
    let mut update = saved.into_active_model();
    update.billing_mode = Set(2);
    assert!(update.before_save(pool.connection(), false).await.is_err());

    pool.close().await?;
    Ok(())
}

fn model_price(
    model: &str,
    billing_mode: i16,
    prices: [Decimal; 5],
    version: i64,
) -> model_prices::ActiveModel {
    model_price_with_expression(model, billing_mode, prices, version, None)
}

fn model_price_with_expression(
    model: &str,
    billing_mode: i16,
    prices: [Decimal; 5],
    version: i64,
    billing_expression: Option<&str>,
) -> model_prices::ActiveModel {
    let [
        input_price,
        output_price,
        cache_read_price,
        cache_creation_5m_price,
        cache_creation_1h_price,
    ] = prices;
    model_prices::ActiveModel {
        model: Set(SensitiveString::from(model)),
        billing_mode: Set(billing_mode),
        input_price: Set(SensitiveDecimal::from(input_price)),
        output_price: Set(SensitiveDecimal::from(output_price)),
        cache_read_price: Set(SensitiveDecimal::from(cache_read_price)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(cache_creation_5m_price)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(cache_creation_1h_price)),
        billing_expression: Set(billing_expression.map(SensitiveString::from)),
        version: Set(version),
        ..Default::default()
    }
}
