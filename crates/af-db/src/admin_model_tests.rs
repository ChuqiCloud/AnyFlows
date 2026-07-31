use std::{error::Error, time::Duration};

use af_domain::ModelId;
use rust_decimal::Decimal;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

use super::{
    AdminModelCreateRecord, AdminModelDeleteOutcome, AdminModelLifecycleRecord,
    AdminModelLookupOutcome, AdminModelModalitiesRecord, AdminModelMutationOutcome,
    AdminModelRepository, AdminModelRepositoryConfigError, AdminModelRepositoryError,
    AdminModelVisibilityRecord, AdminModelWriteRecord, DatabaseOptions, MigrationOptions,
};
use crate::entity::{SensitiveDecimal, SensitiveString, model_prices};

#[tokio::test]
async fn model_metadata_crud_preserves_canonical_identity_and_soft_delete()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    insert_free_price(&fixture.pool, "gpt-5.5").await?;
    let created = fixture
        .repository
        .create(create_record("gpt-5.5", "GPT-5.5"))
        .await?;
    assert_eq!(created.model(), "gpt-5.5");
    assert_eq!(created.display_name(), "GPT-5.5");
    assert_eq!(created.provider(), "openai");
    assert_eq!(created.description(), Some("通用推理模型"));
    assert_eq!(created.icon_url(), Some("/assets/models/openai.svg"));
    assert_eq!(created.tags(), ["推理", "工具"]);
    assert_eq!(created.context_window(), Some(128_000));
    assert!(created.input_modalities().image());
    assert!(created.output_modalities().text());
    assert!(created.supports_reasoning());
    assert!(created.supports_tool_calls());
    assert_eq!(created.visibility(), AdminModelVisibilityRecord::Public);
    assert_eq!(created.lifecycle(), AdminModelLifecycleRecord::Active);
    assert!(created.updated_at() >= created.created_at());
    assert_eq!(format!("{created:?}"), "AdminModelRecord(<redacted>)");

    let model_id = created.model_id();
    let updated = fixture
        .repository
        .update(
            model_id,
            write_record(
                "GPT-5.5 稳定版",
                AdminModelVisibilityRecord::Authenticated,
                AdminModelLifecycleRecord::Deprecated,
            ),
        )
        .await?;
    let AdminModelMutationOutcome::Mutated(updated) = updated else {
        panic!("有效模型元数据必须完成更新");
    };
    assert_eq!(updated.model(), "gpt-5.5");
    assert_eq!(updated.display_name(), "GPT-5.5 稳定版");
    assert_eq!(
        updated.visibility(),
        AdminModelVisibilityRecord::Authenticated
    );
    assert_eq!(updated.lifecycle(), AdminModelLifecycleRecord::Deprecated);

    assert_eq!(
        fixture.repository.delete(model_id).await?,
        AdminModelDeleteOutcome::Deleted
    );
    assert!(matches!(
        fixture.repository.get(model_id).await?,
        AdminModelLookupOutcome::NotFound
    ));
    assert!(
        model_prices::Entity::find_by_id(SensitiveString::from("gpt-5.5"))
            .one(fixture.pool.connection())
            .await?
            .is_some(),
        "删除商品元数据不得级联删除同名价格记录"
    );
    assert_eq!(
        fixture.repository.delete(model_id).await?,
        AdminModelDeleteOutcome::NotFound
    );

    // 软删除释放活动标识，新记录可以重新使用同一 Canonical 模型名。
    let recreated = fixture
        .repository
        .create(create_record("gpt-5.5", "GPT-5.5 重建"))
        .await?;
    assert_ne!(recreated.model_id(), model_id);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn list_uses_stable_cursor_and_conflicts_fail_closed() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for (model, display_name) in [
        ("model-a", "Model A"),
        ("model-b", "Model B"),
        ("model-c", "Model C"),
    ] {
        fixture
            .repository
            .create(create_record(model, display_name))
            .await?;
    }
    assert_eq!(
        fixture
            .repository
            .create(create_record("model-a", "Duplicate"))
            .await
            .unwrap_err(),
        AdminModelRepositoryError::Conflict
    );

    let first = fixture.repository.list(None, 2).await?;
    let (models, cursor) = first.into_parts();
    assert_eq!(
        models.iter().map(|item| item.model()).collect::<Vec<_>>(),
        ["model-a", "model-b"]
    );
    let cursor = cursor.expect("第三条记录存在时必须返回稳定游标");
    let second = fixture.repository.list(Some(cursor), 2).await?;
    let (models, cursor) = second.into_parts();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model(), "model-c");
    assert_eq!(cursor, None);

    for limit in [0, 101] {
        assert_eq!(
            fixture.repository.list(None, limit).await.unwrap_err(),
            AdminModelRepositoryError::Invariant
        );
    }
    assert!(matches!(
        fixture.repository.get(ModelId::new(99_999)?).await?,
        AdminModelLookupOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn repository_rejects_empty_modalities_and_invalid_context() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let invalid_modalities = AdminModelWriteRecord::new(
        "Invalid".to_owned(),
        "provider".to_owned(),
        None,
        None,
        Vec::new(),
        Some(1),
        AdminModelModalitiesRecord::new(false, false, false, false),
        AdminModelModalitiesRecord::new(true, false, false, false),
        false,
        false,
        AdminModelVisibilityRecord::Hidden,
        AdminModelLifecycleRecord::Draft,
    );
    assert_eq!(
        fixture
            .repository
            .create(AdminModelCreateRecord::new(
                "invalid-modalities".to_owned(),
                invalid_modalities,
            ))
            .await
            .unwrap_err(),
        AdminModelRepositoryError::Invariant
    );
    let invalid_context = AdminModelWriteRecord::new(
        "Invalid".to_owned(),
        "provider".to_owned(),
        None,
        None,
        Vec::new(),
        Some(0),
        AdminModelModalitiesRecord::new(true, false, false, false),
        AdminModelModalitiesRecord::new(true, false, false, false),
        false,
        false,
        AdminModelVisibilityRecord::Hidden,
        AdminModelLifecycleRecord::Draft,
    );
    assert_eq!(
        fixture
            .repository
            .create(AdminModelCreateRecord::new(
                "invalid-context".to_owned(),
                invalid_context,
            ))
            .await
            .unwrap_err(),
        AdminModelRepositoryError::Invariant
    );

    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn zero_lookup_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = runtime.block_on(crate::connect(&DatabaseOptions::new("sqlite::memory:")?))?;
    assert!(matches!(
        AdminModelRepository::new(pool.clone(), Duration::ZERO),
        Err(AdminModelRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminModelRepository,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    Ok(Fixture {
        repository: AdminModelRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
    })
}

fn create_record(model: &str, display_name: &str) -> AdminModelCreateRecord {
    AdminModelCreateRecord::new(
        model.to_owned(),
        write_record(
            display_name,
            AdminModelVisibilityRecord::Public,
            AdminModelLifecycleRecord::Active,
        ),
    )
}

fn write_record(
    display_name: &str,
    visibility: AdminModelVisibilityRecord,
    lifecycle: AdminModelLifecycleRecord,
) -> AdminModelWriteRecord {
    AdminModelWriteRecord::new(
        display_name.to_owned(),
        "openai".to_owned(),
        Some("通用推理模型".to_owned()),
        Some("/assets/models/openai.svg".to_owned()),
        vec!["推理".to_owned(), "工具".to_owned()],
        Some(128_000),
        AdminModelModalitiesRecord::new(true, true, false, false),
        AdminModelModalitiesRecord::new(true, false, false, false),
        true,
        true,
        visibility,
        lifecycle,
    )
}

async fn insert_free_price(pool: &super::DatabasePool, model: &str) -> Result<(), sea_orm::DbErr> {
    model_prices::ActiveModel {
        model: Set(SensitiveString::from(model)),
        billing_mode: Set(2),
        input_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        output_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_read_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_5m_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        cache_creation_1h_price: Set(SensitiveDecimal::from(Decimal::ZERO)),
        version: Set(1),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(())
}
