use super::*;
use af_domain::MerchantQuoteWrite;

#[tokio::test]
async fn unpriced_model_can_be_submitted_but_only_publishes_after_platform_pricing()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let owner = UserId::new(1)?;
    let mut write = service_write(fixture.shared_pool);
    write.models[0].model = "new-merchant-model".to_owned();
    let quote = MerchantQuoteWrite {
        pool_id: write.pool_id,
        default_ratio_micros: write.default_ratio_micros,
        models: write.models.clone(),
    };
    assert!(matches!(
        fixture.repository.quote(owner, quote).await,
        Err(MerchantPoolError::ModelPriceUnavailable)
    ));

    let draft = fixture
        .repository
        .write_service(owner, None, write, envelope)
        .await?;
    assert_eq!(draft.status, "draft");
    assert!(draft.published.is_none());
    assert!(draft.active_price.is_none());
    let revision = draft.draft.as_ref().expect("saved draft");
    assert_eq!(revision.models[0].model, "new-merchant-model");
    let revision_number = revision.revision;
    let row = revisions::Entity::find_by_id(revision.id)
        .one(fixture.pool.connection())
        .await?
        .expect("draft revision row");
    assert_eq!(
        channels::Entity::find_by_id(row.channel_id)
            .one(fixture.pool.connection())
            .await?
            .expect("draft channel")
            .status,
        Status::Disabled.code()
    );

    let pending = fixture.repository.submit_service(owner, draft.id).await?;
    assert_eq!(pending.status, "pending");
    assert_eq!(
        pending.draft.as_ref().expect("pending revision").status,
        "pending"
    );
    assert!(matches!(
        fixture
            .repository
            .review_service(
                UserId::new(3)?,
                draft.id,
                review("approve", Some(revision_number)),
            )
            .await,
        Err(MerchantPoolError::ModelPriceUnavailable)
    ));
    let still_pending = fixture
        .repository
        .get_service(Some(owner), draft.id)
        .await?;
    assert_eq!(still_pending.status, "pending");
    assert!(still_pending.published.is_none());
    assert!(still_pending.active_price.is_none());
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "new-merchant-model")
            .await?
            .is_empty()
    );
    assert!(
        model_prices::Entity::find_by_id(SensitiveString::from("new-merchant-model"))
            .one(fixture.pool.connection())
            .await?
            .is_none()
    );

    fixture
        .pool
        .connection()
        .execute_unprepared("INSERT INTO model_prices (model,billing_mode,input_price,output_price) VALUES ('new-merchant-model',1,2,4)")
        .await?;
    let published = fixture
        .repository
        .review_service(
            UserId::new(3)?,
            draft.id,
            review("approve", Some(revision_number)),
        )
        .await?;
    assert_eq!(published.status, "published");
    assert!(published.draft.is_none());
    assert!(published.active_price.is_some());
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "new-merchant-model")
            .await?
            .len(),
        1
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("DELETE FROM model_prices WHERE model='new-merchant-model'")
        .await?;
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "new-merchant-model")
            .await?
            .is_empty()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn unpriced_draft_update_preserves_the_previously_approved_supply()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let owner = UserId::new(1)?;
    let published = publish(&fixture).await?;
    let original_revision = published.published.as_ref().expect("published revision").id;
    let mut write = service_write(fixture.shared_pool);
    write.models[0].model = "future-merchant-model".to_owned();
    let draft = fixture
        .repository
        .write_service(owner, Some(published.id), write, envelope)
        .await?;
    assert_eq!(draft.status, "published");
    assert_eq!(
        draft.published.as_ref().expect("live revision").id,
        original_revision
    );
    let pending = fixture.repository.submit_service(owner, draft.id).await?;
    let revision = pending.draft.as_ref().expect("pending draft").revision;
    assert!(matches!(
        fixture
            .repository
            .review_service(UserId::new(3)?, draft.id, review("approve", Some(revision)))
            .await,
        Err(MerchantPoolError::ModelPriceUnavailable)
    ));
    let unchanged = fixture
        .repository
        .get_service(Some(owner), draft.id)
        .await?;
    assert_eq!(unchanged.status, "published");
    assert_eq!(
        unchanged.published.as_ref().expect("live revision").id,
        original_revision
    );
    assert_eq!(
        unchanged.draft.as_ref().expect("pending draft").status,
        "pending"
    );
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .len(),
        1
    );
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "future-merchant-model")
            .await?
            .is_empty()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn unsupported_price_cards_cannot_be_approved() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let owner = UserId::new(1)?;
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE model_prices SET billing_mode=3,input_price=0,output_price=0,billing_expression='input_tokens' WHERE model='gpt-test'")
        .await?;
    let write = service_write(fixture.shared_pool);
    let quote = MerchantQuoteWrite {
        pool_id: write.pool_id,
        default_ratio_micros: write.default_ratio_micros,
        models: write.models.clone(),
    };
    assert!(matches!(
        fixture.repository.quote(owner, quote).await,
        Err(MerchantPoolError::ModelPriceUnavailable)
    ));
    let draft = fixture
        .repository
        .write_service(owner, None, write, envelope)
        .await?;
    let pending = fixture.repository.submit_service(owner, draft.id).await?;
    let revision = pending.draft.as_ref().expect("pending draft").revision;
    assert!(matches!(
        fixture
            .repository
            .review_service(UserId::new(3)?, draft.id, review("approve", Some(revision)))
            .await,
        Err(MerchantPoolError::ModelPriceUnavailable)
    ));
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .is_empty()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn fixed_request_prices_are_available_in_merchant_quotes() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .pool
        .connection()
        .execute_unprepared(
            "UPDATE model_prices SET billing_mode=4,input_price=0,output_price=0,cache_read_price=0,cache_creation_5m_price=0,cache_creation_1h_price=0,request_price=0.025,billing_expression=NULL WHERE model='gpt-test'",
        )
        .await?;
    let write = service_write(fixture.shared_pool);
    let quote = fixture
        .repository
        .quote(
            UserId::new(1)?,
            MerchantQuoteWrite {
                pool_id: write.pool_id,
                default_ratio_micros: write.default_ratio_micros,
                models: write.models.clone(),
            },
        )
        .await?;
    let prices = &quote.models[0].prices;
    assert_eq!(prices.unit, "request");
    assert_eq!(prices.billing_mode, "per_call");
    assert_eq!(
        quote.models[0].base_prices.request_price.as_deref(),
        Some("0.025")
    );
    assert_eq!(prices.request_price.as_deref(), Some("0.0125"));
    let net_ratio = Decimal::from(10_000 - quote.effective_commission_bps)
        .checked_div(Decimal::from(10_000))
        .unwrap();
    let expected_net = (Decimal::new(125, 4) * net_ratio).normalize().to_string();
    assert_eq!(
        quote.models[0].net_prices.request_price.as_deref(),
        Some(expected_net.as_str())
    );

    let published = fixture
        .repository
        .write_service(UserId::new(1)?, None, write, envelope)
        .await?;
    let pending = fixture
        .repository
        .submit_service(UserId::new(1)?, published.id)
        .await?;
    let revision = pending.draft.as_ref().expect("pending draft").revision;
    let approved = fixture
        .repository
        .review_service(
            UserId::new(3)?,
            published.id,
            review("approve", Some(revision)),
        )
        .await?;
    assert!(approved.published.is_some());
    let catalog = fixture
        .repository
        .catalog(
            UserId::new(1)?,
            fixture.shared_pool,
            MerchantListQuery::default(),
        )
        .await?;
    assert_eq!(
        catalog.items[0].models[0]
            .prices
            .as_ref()
            .and_then(|prices| prices.request_price.as_deref()),
        Some("0.0125")
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_per_call_price_can_publish_without_a_platform_price_card()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let owner = UserId::new(1)?;
    let mut write = service_write(fixture.shared_pool);
    write.models[0].model = "merchant-only-paid-model".to_owned();
    write.models[0].pricing = Some(af_domain::MerchantModelPricing {
        mode: af_domain::MerchantModelPricingMode::PerCall,
        input: None,
        output: None,
        cache_read: None,
        cache_write_5m: None,
        cache_write_1h: None,
        request_price: Some("0.025".to_owned()),
        expression: None,
    });
    let quote = fixture
        .repository
        .quote(
            owner,
            MerchantQuoteWrite {
                pool_id: write.pool_id,
                default_ratio_micros: write.default_ratio_micros,
                models: write.models.clone(),
            },
        )
        .await?;
    assert_eq!(
        quote.models[0].base_prices.request_price.as_deref(),
        Some("0.025")
    );
    assert_eq!(
        quote.models[0].prices.request_price.as_deref(),
        Some("0.0125")
    );

    let draft = fixture
        .repository
        .write_service(owner, None, write, envelope)
        .await?;
    let pending = fixture.repository.submit_service(owner, draft.id).await?;
    let revision = pending.draft.as_ref().expect("pending revision").revision;
    let published = fixture
        .repository
        .review_service(UserId::new(3)?, draft.id, review("approve", Some(revision)))
        .await?;
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "merchant-only-paid-model")
            .await?
            .len(),
        1
    );
    assert_eq!(
        fixture
            .repository
            .catalog(owner, fixture.shared_pool, MerchantListQuery::default(),)
            .await?
            .items[0]
            .models[0]
            .prices
            .as_ref()
            .and_then(|prices| prices.request_price.as_deref()),
        Some("0.0125")
    );
    assert!(published.listed);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_expression_price_scales_for_sale_ratio_and_commission()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let mut write = service_write(fixture.shared_pool);
    write.models[0].model = "merchant-expression-model".to_owned();
    write.models[0].pricing = Some(af_domain::MerchantModelPricing {
        mode: af_domain::MerchantModelPricingMode::Expression,
        input: None,
        output: None,
        cache_read: None,
        cache_write_5m: None,
        cache_write_1h: None,
        request_price: None,
        expression: Some("v1:tier(\"base\", p * 2 + c * 10)".to_owned()),
    });
    let quote = fixture
        .repository
        .quote(
            UserId::new(1)?,
            MerchantQuoteWrite {
                pool_id: write.pool_id,
                default_ratio_micros: write.default_ratio_micros,
                models: write.models.clone(),
            },
        )
        .await?;
    let sale = quote.models[0]
        .prices
        .billing_expression
        .as_deref()
        .expect("sale expression");
    let net = quote.models[0]
        .net_prices
        .billing_expression
        .as_deref()
        .expect("net expression");
    let net_ratio = Decimal::new(i64::from(10_000 - quote.effective_commission_bps), 4).normalize();
    assert_eq!(sale, "v1:(tier(\"base\", p * 2 + c * 10)) * 0.5");
    assert_eq!(
        net,
        format!("v1:((tier(\"base\", p * 2 + c * 10)) * 0.5) * {net_ratio}")
    );
    assert_eq!(quote.models[0].prices.billing_mode, "expression");
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_free_price_can_quote_and_publish_without_a_platform_price_card()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let owner = UserId::new(1)?;
    let mut write = service_write(fixture.shared_pool);
    write.models[0].model = "merchant-only-free-model".to_owned();
    write.models[0].pricing = Some(af_domain::MerchantModelPricing {
        mode: af_domain::MerchantModelPricingMode::Free,
        input: None,
        output: None,
        cache_read: None,
        cache_write_5m: None,
        cache_write_1h: None,
        request_price: None,
        expression: None,
    });
    let quote = fixture
        .repository
        .quote(
            owner,
            MerchantQuoteWrite {
                pool_id: write.pool_id,
                default_ratio_micros: write.default_ratio_micros,
                models: write.models.clone(),
            },
        )
        .await?;
    assert_eq!(quote.models[0].prices.billing_mode, "free");
    assert_eq!(quote.models[0].prices.input, "0");
    let draft = fixture
        .repository
        .write_service(owner, None, write, envelope)
        .await?;
    let pending = fixture.repository.submit_service(owner, draft.id).await?;
    let revision = pending.draft.as_ref().expect("pending revision").revision;
    fixture
        .repository
        .review_service(UserId::new(3)?, draft.id, review("approve", Some(revision)))
        .await?;
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "merchant-only-free-model")
            .await?
            .len(),
        1
    );
    fixture.pool.close().await?;
    Ok(())
}
