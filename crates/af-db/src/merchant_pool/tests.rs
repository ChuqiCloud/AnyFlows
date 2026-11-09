use super::*;
use sea_orm::{DbBackend, Statement};
use std::error::Error;

struct Fixture {
    repository: MerchantPoolRepository,
    pool: DatabasePool,
    merchant: i64,
    shared_pool: i64,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &crate::DatabaseOptions::new("sqlite::memory:")?,
        crate::MigrationOptions::default(),
    )
    .await?;
    pool.connection().execute_unprepared("INSERT INTO groups (id,name,display_name,flags) VALUES (1,'merchant-test','Merchant Test','{}')").await?;
    for (id, name) in [
        (1, "merchant-one"),
        (2, "merchant-two"),
        (3, "merchant-admin"),
    ] {
        pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO users (id,username,status,role,default_group_id,aff_code,settings) VALUES (?,?,1,?,1,?,'{}')",
            [id.into(),name.into(),if id==3 {1_i16}else{0_i16}.into(),format!("merchant-aff-{id}").into()])).await?;
    }
    pool.connection().execute_unprepared("INSERT INTO model_prices (model,billing_mode,input_price,output_price) VALUES ('gpt-test',1,2,4)").await?;
    let repository = MerchantPoolRepository::new(pool.clone(), Duration::from_secs(5));
    let admin = UserId::new(3)?;
    let merchant = repository
        .apply(
            UserId::new(1)?,
            MerchantApplication {
                name: "Merchant One".to_owned(),
                description: "Service".to_owned(),
            },
        )
        .await?;
    repository
        .review_merchant(admin, merchant.id, review("approve", None))
        .await?;
    let shared_pool = repository.write_pool(admin, None, pool_write()).await?;
    Ok(Fixture {
        repository,
        pool,
        merchant: merchant.id,
        shared_pool: shared_pool.id,
    })
}

fn pool_write() -> MerchantPoolWrite {
    MerchantPoolWrite {
        group_id: 1,
        name: "Shared Pool".to_owned(),
        description: "Test shared supply".to_owned(),
        enabled: true,
        commission_bps: 900,
        commission_inherit_global: true,
        min_ratio_micros: 100_000,
        max_ratio_micros: 5_000_000,
        settlement_delay_secs: 86_400,
    }
}
fn service_write(pool: i64) -> MerchantServiceWrite {
    MerchantServiceWrite {
        pool_id: pool,
        name: "Reviewed Service".to_owned(),
        description: "Reviewed description".to_owned(),
        base_url: "https://api.example.com/v1".to_owned(),
        protocol: "openai_chat".to_owned(),
        api_key: Some("upstream-private-key".to_owned()),
        default_ratio_micros: 500_000,
        models: vec![MerchantModelWrite {
            model: "gpt-test".to_owned(),
            upstream_model: "private-upstream-model".to_owned(),
            ratio_micros: None,
            context_window: Some(8192),
            max_output_tokens: Some(2048),
            supports_streaming: true,
            supports_tools: true,
            supports_vision: false,
        }],
        concurrency: Some(5),
        timeout_secs: 60,
    }
}
fn review(decision: &str, expected: Option<i64>) -> MerchantReview {
    MerchantReview {
        decision: decision.to_owned(),
        reason: "Test administrator decision".to_owned(),
        expected_revision: expected,
    }
}
fn envelope(
    channel: ChannelId,
    credential: CredentialId,
    _previous: Option<MerchantCredentialMaterial>,
    key: Option<String>,
) -> Result<EncryptedCredentialEnvelope, MerchantPoolError> {
    assert!(channel.get() > 0 && credential.get() > 0);
    assert!(key.is_some());
    EncryptedCredentialEnvelope::new("merchant-test", [1; 24], vec![2; 32]).map_err(internal)
}

async fn publish(fixture: &Fixture) -> Result<MerchantOwnedService, Box<dyn Error>> {
    let service = fixture
        .repository
        .write_service(
            UserId::new(1)?,
            None,
            service_write(fixture.shared_pool),
            envelope,
        )
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
async fn approval_preserves_draft_isolation_and_consumer_secrets() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = fixture
        .repository
        .write_service(
            UserId::new(1)?,
            None,
            service_write(fixture.shared_pool),
            envelope,
        )
        .await?;
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .is_empty()
    );
    fixture
        .repository
        .submit_service(UserId::new(1)?, service.id)
        .await?;
    assert!(matches!(
        fixture
            .repository
            .review_service(UserId::new(3)?, service.id, review("approve", Some(99)))
            .await,
        Err(MerchantPoolError::Conflict)
    ));
    let published = fixture
        .repository
        .review_service(UserId::new(3)?, service.id, review("approve", Some(1)))
        .await?;
    assert_eq!(published.status, "published");
    let candidates = fixture
        .repository
        .published_candidates(fixture.shared_pool, None, "gpt-test")
        .await?;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].sales_ratio_micros, 500_000);
    let catalog = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery::default(),
        )
        .await?;
    assert_eq!(
        catalog.items[0].models[0]
            .prices
            .as_ref()
            .expect("priced model")
            .input,
        "1"
    );
    let encoded = serde_json::to_string(&catalog)?;
    for secret in [
        "api.example.com",
        "private-upstream-model",
        "upstream-private-key",
        "credential_id",
        "channel_id",
    ] {
        assert!(!encoded.contains(secret));
    }
    let mut draft = service_write(fixture.shared_pool);
    draft.name = "Unapproved title".to_owned();
    draft.default_ratio_micros = 1_000_000;
    fixture
        .repository
        .write_service(UserId::new(1)?, Some(service.id), draft, envelope)
        .await?;
    let catalog = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery::default(),
        )
        .await?;
    assert_eq!(catalog.items[0].name, "Reviewed Service");
    assert_eq!(catalog.items[0].models[0].sales_ratio_micros, 500_000);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn failed_credential_encryption_rolls_back_all_draft_resources() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let result = fixture
        .repository
        .write_service(
            UserId::new(1)?,
            None,
            service_write(fixture.shared_pool),
            |_, _, _, _| Err(MerchantPoolError::Unavailable),
        )
        .await;
    assert!(matches!(result, Err(MerchantPoolError::Unavailable)));
    assert_eq!(
        services::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        channels::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        credentials::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn owner_cannot_read_or_pause_another_merchants_service() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let other = fixture
        .repository
        .apply(
            UserId::new(2)?,
            MerchantApplication {
                name: "Merchant Two".to_owned(),
                description: String::new(),
            },
        )
        .await?;
    fixture
        .repository
        .review_merchant(UserId::new(3)?, other.id, review("approve", None))
        .await?;
    assert!(matches!(
        fixture
            .repository
            .get_service(Some(UserId::new(2)?), service.id)
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    assert!(matches!(
        fixture
            .repository
            .pause_service(UserId::new(2)?, service.id, true)
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    assert!(matches!(
        fixture
            .repository
            .write_service(
                UserId::new(2)?,
                Some(service.id),
                service_write(fixture.shared_pool),
                envelope
            )
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn platform_suspension_cannot_be_undone_by_merchant_resume() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    fixture
        .repository
        .pause_service(UserId::new(1)?, service.id, true)
        .await?;
    fixture
        .repository
        .review_service(UserId::new(3)?, service.id, review("suspend", Some(1)))
        .await?;
    assert!(matches!(
        fixture
            .repository
            .pause_service(UserId::new(1)?, service.id, false)
            .await,
        Err(MerchantPoolError::Conflict)
    ));
    let service = fixture
        .repository
        .review_service(UserId::new(3)?, service.id, review("approve", Some(1)))
        .await?;
    assert_eq!(service.status, "paused");
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .is_empty()
    );
    fixture
        .repository
        .pause_service(UserId::new(1)?, service.id, false)
        .await?;
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .len(),
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_commission_overrides_pool_and_global_and_is_resolved_for_owner()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    publish(&fixture).await?;
    let admin = UserId::new(3)?;
    fixture
        .repository
        .write_settings(
            admin,
            MerchantPoolSettings {
                commission_bps: 650,
            },
        )
        .await?;
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?[0]
            .commission_bps,
        650
    );
    fixture
        .repository
        .write_terms(
            admin,
            fixture.shared_pool,
            fixture.merchant,
            MerchantPoolTerms {
                commission_bps: Some(200),
            },
        )
        .await?;
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?[0]
            .commission_bps,
        200
    );
    assert_eq!(
        fixture
            .repository
            .available_supply_pools(UserId::new(1)?, MerchantListQuery::default())
            .await?
            .items[0]
            .effective_commission_bps,
        200
    );
    fixture
        .repository
        .write_terms(
            admin,
            fixture.shared_pool,
            fixture.merchant,
            MerchantPoolTerms {
                commission_bps: None,
            },
        )
        .await?;
    let mut config = pool_write();
    config.commission_inherit_global = false;
    fixture
        .repository
        .write_pool(admin, Some(fixture.shared_pool), config)
        .await?;
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?[0]
            .commission_bps,
        900
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn key_rotation_uses_existing_aead_binding_and_never_changes_offer()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let before = fixture
        .repository
        .published_candidates(fixture.shared_pool, None, "gpt-test")
        .await?
        .remove(0);
    fixture
        .repository
        .rotate_service_key(UserId::new(1)?, service.id, |channel, credential| {
            assert_eq!(channel.get(), before.channel_id);
            assert!(credential.get() > 0);
            EncryptedCredentialEnvelope::new("merchant-test", [3; 24], vec![4; 32])
                .map_err(internal)
        })
        .await?;
    let after = fixture
        .repository
        .published_candidates(fixture.shared_pool, None, "gpt-test")
        .await?
        .remove(0);
    assert_eq!(before.revision_id, after.revision_id);
    assert_eq!(before.sales_ratio_micros, after.sales_ratio_micros);
    assert_eq!(before.channel_id, after.channel_id);
    let row = credentials::Entity::find()
        .filter(credentials::Column::ChannelId.eq(before.channel_id))
        .one(fixture.pool.connection())
        .await?
        .expect("credential exists");
    assert_eq!(row.secret.envelope_parts()?.1, [3; 24]);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn catalog_applies_model_capability_and_ratio_filters_before_pagination()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    publish(&fixture).await?;
    let no_match = MerchantListQuery {
        model: Some("gpt-test".to_owned()),
        protocol: Some("openai_responses".to_owned()),
        ..Default::default()
    };
    assert_eq!(
        fixture
            .repository
            .catalog(UserId::new(2)?, fixture.shared_pool, no_match)
            .await?
            .total,
        0
    );
    let no_match = MerchantListQuery {
        capability: Some("vision".to_owned()),
        ..Default::default()
    };
    assert_eq!(
        fixture
            .repository
            .catalog(UserId::new(2)?, fixture.shared_pool, no_match)
            .await?
            .total,
        0
    );
    let no_match = MerchantListQuery {
        min_ratio_micros: Some(750_000),
        ..Default::default()
    };
    assert_eq!(
        fixture
            .repository
            .catalog(UserId::new(2)?, fixture.shared_pool, no_match)
            .await?
            .total,
        0
    );
    let matches = MerchantListQuery {
        model: Some("gpt-test".to_owned()),
        protocol: Some("openai_chat".to_owned()),
        capability: Some("tools".to_owned()),
        max_ratio_micros: Some(500_000),
        ..Default::default()
    };
    assert_eq!(
        fixture
            .repository
            .catalog(UserId::new(2)?, fixture.shared_pool, matches)
            .await?
            .total,
        1
    );
    let invalid = MerchantListQuery {
        sort: Some("price".to_owned()),
        ..Default::default()
    };
    assert!(matches!(
        fixture
            .repository
            .catalog(UserId::new(2)?, fixture.shared_pool, invalid)
            .await,
        Err(MerchantPoolError::InvalidInput)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn revoked_pool_access_hides_catalog_and_repricing_removes_runtime_supply()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    publish(&fixture).await?;
    fixture.pool.connection().execute_unprepared("INSERT INTO groups (id,name,display_name,flags) VALUES (2,'other-group','Other Group','{}')").await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE users SET default_group_id=2 WHERE id=2")
        .await?;
    assert!(
        !fixture
            .repository
            .check_pool_access(UserId::new(2)?, fixture.shared_pool)
            .await?
    );
    assert!(matches!(
        fixture
            .repository
            .catalog(
                UserId::new(2)?,
                fixture.shared_pool,
                MerchantListQuery::default()
            )
            .await,
        Err(MerchantPoolError::NotFound)
    ));
    let mut config = pool_write();
    config.min_ratio_micros = 750_000;
    fixture
        .repository
        .write_pool(UserId::new(3)?, Some(fixture.shared_pool), config)
        .await?;
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
async fn derived_catalog_state_filters_disabled_channels_after_projection()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let channel = fixture
        .repository
        .published_candidates(fixture.shared_pool, None, "gpt-test")
        .await?
        .remove(0)
        .channel_id;
    channels::Entity::update_many()
        .filter(channels::Column::Id.eq(channel))
        .col_expr(
            channels::Column::Status,
            Expr::value(Status::Disabled.code()),
        )
        .exec(fixture.pool.connection())
        .await?;
    let page = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery::default(),
        )
        .await?;
    assert_eq!(page.items[0].id, service.id);
    assert_eq!(page.items[0].status, "unavailable");
    assert_eq!(
        fixture
            .repository
            .catalog(
                UserId::new(2)?,
                fixture.shared_pool,
                MerchantListQuery {
                    status: Some("published".to_owned()),
                    ..Default::default()
                }
            )
            .await?
            .total,
        0
    );
    assert_eq!(
        fixture
            .repository
            .catalog(
                UserId::new(2)?,
                fixture.shared_pool,
                MerchantListQuery {
                    status: Some("unavailable".to_owned()),
                    ..Default::default()
                }
            )
            .await?
            .total,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn platform_resume_keeps_pending_revision_unpublished_and_merchant_pause_intact()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let service = publish(&fixture).await?;
    let mut write = service_write(fixture.shared_pool);
    write.default_ratio_micros = 800_000;
    write.name = "Pending changed name".to_owned();
    fixture
        .repository
        .write_service(UserId::new(1)?, Some(service.id), write, envelope)
        .await?;
    fixture
        .repository
        .submit_service(UserId::new(1)?, service.id)
        .await?;
    fixture
        .repository
        .pause_service(UserId::new(1)?, service.id, true)
        .await?;
    fixture
        .repository
        .review_service(UserId::new(3)?, service.id, review("suspend", None))
        .await?;
    let resumed = fixture
        .repository
        .resume_service(UserId::new(3)?, service.id)
        .await?;
    assert_eq!(resumed.status, "paused");
    assert_eq!(resumed.name, "Reviewed Service");
    assert_eq!(
        resumed
            .published
            .as_ref()
            .expect("published revision")
            .revision,
        1
    );
    assert_eq!(
        resumed.draft.as_ref().expect("pending revision").status,
        "pending"
    );
    assert_eq!(
        resumed.draft.as_ref().expect("pending revision").revision,
        2
    );
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .is_empty()
    );
    fixture
        .repository
        .pause_service(UserId::new(1)?, service.id, false)
        .await?;
    assert_eq!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?[0]
            .sales_ratio_micros,
        500_000
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn vision_supply_is_rejected_and_legacy_published_vision_is_not_routable()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let mut write = service_write(fixture.shared_pool);
    write.models[0].supports_vision = true;
    assert!(matches!(
        write.validate(),
        Err(MerchantPoolError::InvalidInput)
    ));
    assert!(matches!(
        fixture
            .repository
            .write_service(UserId::new(1)?, None, write, envelope)
            .await,
        Err(MerchantPoolError::InvalidInput)
    ));
    let service = publish(&fixture).await?;
    let revision = service.published.expect("published revision");
    let mut models = revision.models;
    models[0].supports_vision = true;
    revisions::Entity::update_many()
        .filter(revisions::Column::Id.eq(revision.id))
        .col_expr(
            revisions::Column::Models,
            Expr::value(serde_json::to_value(models)?),
        )
        .exec(fixture.pool.connection())
        .await?;
    assert!(
        fixture
            .repository
            .published_candidates(fixture.shared_pool, None, "gpt-test")
            .await?
            .is_empty()
    );
    let catalog = fixture
        .repository
        .catalog(
            UserId::new(2)?,
            fixture.shared_pool,
            MerchantListQuery::default(),
        )
        .await?;
    assert_eq!(catalog.items[0].status, "unsupported");
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn pool_groups_are_hidden_and_cannot_issue_direct_keys_but_route_keys_still_work()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    publish(&fixture).await?;
    let consumer = UserId::new(2)?;
    fixture
        .pool
        .connection()
        .execute_unprepared(
            "INSERT INTO groups (id,name,display_name,flags) VALUES (2,'ordinary','Ordinary','{}')",
        )
        .await?;
    let access =
        crate::GroupKeyAccessRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    access
        .update_policy(
            GroupId::new(2)?,
            crate::GroupKeySelectionMode::AllUsers,
            Vec::new(),
        )
        .await?;
    let directory = access.list_user_groups(consumer, None, 100).await?;
    assert_eq!(
        directory
            .groups
            .iter()
            .map(|group| group.group_id.get())
            .collect::<Vec<_>>(),
        vec![2]
    );
    let tokens = crate::UserTokenRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let key = |serial: u64| {
        crate::UserTokenCreateRecord::new(
            consumer,
            format!("{serial:064x}"),
            format!("sk-af-{serial:012}"),
            crate::UserTokenWriteRecord::new(
                "merchant key selection".to_owned(),
                1,
                1_000_000,
                false,
                None,
                None,
                None,
            ),
        )
    };
    assert!(matches!(
        tokens
            .create(key(1).with_group_id(Some(GroupId::new(1)?)))
            .await,
        Err(crate::UserTokenRepositoryError::GroupUnavailable)
    ));
    assert!(matches!(
        tokens.create(key(2)).await,
        Err(crate::UserTokenRepositoryError::GroupUnavailable)
    ));
    let routes = crate::UserRouteRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let route = routes
        .write_merchant_route(
            consumer,
            None,
            crate::MerchantRouteWrite {
                name: "Merchant-only route".to_owned(),
                pool_id: fixture.shared_pool,
                strategy: af_domain::UserRouteStrategy::Ordered,
                enabled: true,
                max_merchant_attempts: 1,
                max_price_ratio_micros: None,
                members: vec![crate::MerchantRouteMember {
                    merchant_id: fixture.merchant,
                    priority: 0,
                    weight: 1,
                    last_selected_at_millis: None,
                }],
                excluded_merchant_ids: Vec::new(),
                allow_pool_fallback: false,
                accept_new_merchants: false,
            },
        )
        .await?;
    let issued = tokens
        .create(key(3).with_user_route_id(Some(route.id)))
        .await?;
    assert_eq!(issued.user_route_id(), Some(route.id));
    assert_eq!(issued.group_id(), None);
    assert!(
        routes
            .lookup_for_token(consumer, issued.token_id())
            .await?
            .is_some()
    );
    assert_eq!(
        tokens
            .create(key(4).with_group_id(Some(GroupId::new(2)?)))
            .await?
            .effective_group_id(),
        GroupId::new(2)?
    );
    let mut config = pool_write();
    config.enabled = false;
    fixture
        .repository
        .write_pool(UserId::new(3)?, Some(fixture.shared_pool), config)
        .await?;
    assert_eq!(
        access
            .list_user_groups(consumer, None, 100)
            .await?
            .groups
            .iter()
            .map(|group| group.group_id.get())
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert!(matches!(
        tokens.create(key(5)).await,
        Err(crate::UserTokenRepositoryError::GroupUnavailable)
    ));
    fixture.pool.close().await?;
    Ok(())
}
