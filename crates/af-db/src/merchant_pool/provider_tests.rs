use super::*;

async fn add_catalog_model(
    fixture: &Fixture,
    name: &str,
    provider: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    if name != "gpt-test" {
        fixture
            .pool
            .connection()
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO model_prices (model,billing_mode,input_price,output_price) VALUES (?,1,2,4)",
                [name.into()],
            ))
            .await?;
    }
    if let Some(provider) = provider {
        fixture.pool.connection().execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO models (model,display_name,provider,tags,supports_text_input,supports_text_output,visibility,lifecycle) VALUES (?,?,?,'[]',1,1,2,2)",
            [name.into(),name.into(),provider.into()],
        )).await?;
    }
    Ok(())
}

async fn publish_catalog_models(
    fixture: &Fixture,
    name: &str,
    models: &[&str],
) -> Result<MerchantOwnedService, Box<dyn Error>> {
    let mut write = service_write(fixture.shared_pool);
    write.name = name.to_owned();
    let template = write.models[0].clone();
    write.models = models
        .iter()
        .map(|model| MerchantModelWrite {
            model: (*model).to_owned(),
            ..template.clone()
        })
        .collect();
    let service = fixture
        .repository
        .write_service(UserId::new(1)?, None, write, envelope)
        .await?;
    let service = fixture
        .repository
        .submit_service(UserId::new(1)?, service.id)
        .await?;
    Ok(fixture
        .repository
        .review_service(
            UserId::new(3)?,
            service.id,
            review(
                "approve",
                service.draft.as_ref().map(|draft| draft.revision),
            ),
        )
        .await?)
}

#[tokio::test]
async fn provider_facets_count_canonical_models_and_filter_before_pagination()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for (model, provider) in [
        ("gpt-test", Some("DeepSeek")),
        ("deepseek-second", Some("DeepSeek")),
        ("anthropic-test", Some("Anthropic")),
        ("missing-metadata", None),
        ("deleted-metadata", Some("OpenAI")),
        ("blank-metadata", Some("  ")),
        ("draft-only", Some("Google")),
    ] {
        add_catalog_model(&fixture, model, provider).await?;
    }
    fixture
        .pool
        .connection()
        .execute_unprepared(
            "UPDATE models SET deleted_at=CURRENT_TIMESTAMP WHERE model='deleted-metadata'",
        )
        .await?;
    publish_catalog_models(
        &fixture,
        "A Other providers",
        &[
            "anthropic-test",
            "missing-metadata",
            "deleted-metadata",
            "blank-metadata",
        ],
    )
    .await?;
    let service =
        publish_catalog_models(&fixture, "B DeepSeek", &["gpt-test", "deepseek-second"]).await?;
    publish_catalog_models(&fixture, "C DeepSeek", &["gpt-test", "deepseek-second"]).await?;
    let mut draft = service_write(fixture.shared_pool);
    draft.models[0].model = "draft-only".to_owned();
    fixture
        .repository
        .write_service(UserId::new(1)?, None, draft, envelope)
        .await?;

    let page = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery {
                provider: Some(r#"["DeepSeek"]"#.to_owned()),
                page_size: Some(1),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(page.total, 2);
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, service.id);
    assert_eq!(page.items[0].protocol, "openai_chat");
    assert!(
        page.items[0]
            .models
            .iter()
            .all(|model| model.provider.as_deref() == Some("DeepSeek"))
    );
    let detail = fixture
        .repository
        .catalog_service(UserId::new(2)?, fixture.shared_pool, service.id)
        .await?;
    assert_eq!(detail.models[0].provider.as_deref(), Some("DeepSeek"));
    let facets = fixture
        .repository
        .catalog_providers(UserId::new(2)?, fixture.shared_pool)
        .await?;
    let counts = facets
        .providers
        .into_iter()
        .map(|provider| (provider.name, provider.model_count))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(counts.len(), 3);
    assert_eq!(counts.get(&Some("DeepSeek".to_owned())), Some(&2));
    assert_eq!(counts.get(&Some("Anthropic".to_owned())), Some(&1));
    assert_eq!(counts.get(&None), Some(&3));

    let unclassified = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery {
                provider: Some("[null]".to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(unclassified.total, 1);
    assert_eq!(unclassified.items[0].models.len(), 3);
    assert!(
        unclassified.items[0]
            .models
            .iter()
            .all(|model| model.provider.is_none())
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn provider_names_with_commas_and_sentinel_text_remain_exact() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    add_catalog_model(&fixture, "gpt-test", Some("__unclassified__")).await?;
    add_catalog_model(&fixture, "comma-provider", Some("A,B")).await?;
    add_catalog_model(&fixture, "no-provider", None).await?;
    publish_catalog_models(
        &fixture,
        "Exact provider names",
        &["gpt-test", "comma-provider", "no-provider"],
    )
    .await?;
    for (encoded, expected) in [
        (r#"["__unclassified__"]"#, "gpt-test"),
        (r#"["A,B"]"#, "comma-provider"),
        ("[null]", "no-provider"),
    ] {
        let page = fixture
            .repository
            .catalog(
                UserId::new(2)?,
                fixture.shared_pool,
                MerchantListQuery {
                    provider: Some(encoded.to_owned()),
                    ..Default::default()
                },
            )
            .await?;
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].models.len(), 1);
        assert_eq!(page.items[0].models[0].model, expected);
    }
    let page = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery {
                provider: Some(r#"["A,B",null]"#.to_owned()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(page.items[0].models.len(), 2);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn provider_filter_cannot_hide_an_unavailable_sibling_offer() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    add_catalog_model(&fixture, "gpt-test", Some("DeepSeek")).await?;
    add_catalog_model(&fixture, "unpriced-sibling", Some("Anthropic")).await?;
    publish_catalog_models(
        &fixture,
        "Partially unpriced service",
        &["gpt-test", "unpriced-sibling"],
    )
    .await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("DELETE FROM model_prices WHERE model='unpriced-sibling'")
        .await?;
    let query = MerchantListQuery {
        model: Some("gpt-test".to_owned()),
        provider: Some(r#"["DeepSeek"]"#.to_owned()),
        capability: Some("tools".to_owned()),
        max_ratio_micros: Some(500_000),
        status: Some("unavailable".to_owned()),
        ..Default::default()
    };
    let page = fixture
        .repository
        .catalog(UserId::new(2)?, fixture.shared_pool, query.clone())
        .await?;
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].status, "unavailable");
    assert_eq!(page.items[0].models.len(), 1);
    assert!(page.items[0].models[0].prices.is_some());
    let page = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery {
                status: Some("published".to_owned()),
                ..query
            },
        )
        .await?;
    assert_eq!(page.total, 0);
    fixture.pool.close().await?;
    Ok(())
}

#[test]
fn provider_filter_rejects_invalid_names_and_bounds_unique_values() -> Result<(), Box<dyn Error>> {
    let mut invalid = vec![
        "DeepSeek".to_owned(),
        "null".to_owned(),
        "[]".to_owned(),
        "[1]".to_owned(),
        r#"[""]"#.to_owned(),
        r#"[" "]"#.to_owned(),
        r#"["DeepSeek\n"]"#.to_owned(),
        serde_json::to_string(&vec!["x".repeat(65)])?,
        serde_json::to_string(&(0..33).map(|n| format!("provider-{n}")).collect::<Vec<_>>())?,
        " ".repeat(16_385),
    ];
    invalid.push(serde_json::to_string(&vec!["厂".repeat(22)])?);
    for provider in invalid {
        assert!(matches!(
            provider_filter(&MerchantListQuery {
                provider: Some(provider),
                ..Default::default()
            }),
            Err(MerchantPoolError::InvalidInput)
        ));
    }
    let names = provider_filter(&MerchantListQuery {
        provider: Some(serde_json::to_string(&vec![Some("DeepSeek"); 33])?),
        ..Default::default()
    })?
    .expect("provider filter");
    assert_eq!(names.len(), 1);
    assert!(names.contains(&Some("DeepSeek".to_owned())));
    Ok(())
}
