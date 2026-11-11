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
