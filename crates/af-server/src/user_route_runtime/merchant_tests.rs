use super::*;
use crate::{
    test_database::SqliteTestDatabase,
    user_route_execution::{MerchantRequestFeatures, UserRouteAttemptError, UserRouteExecution},
};
use af_billing::{CachedRequestPricingSnapshotSource, GroupPricingCache, ModelPriceCache};
use af_db::{
    EncryptedCredentialEnvelope, MerchantPoolRepository, MerchantRouteMember, MerchantRouteWrite,
};
use af_domain::{
    ChannelId, MerchantApplication, MerchantModelWrite, MerchantOwnedService, MerchantPoolWrite,
    MerchantReview, MerchantServiceWrite, TokenId, UserId,
};
use af_protocol::{TokenCount, UsageDetails, UsageSemantics, UsageSource};
use sea_orm::{ConnectionTrait, Statement};
use std::sync::Mutex;

struct Fixture {
    database: SqliteTestDatabase,
    pools: MerchantPoolRepository,
    routes: UserRouteRepository,
    runtime: UserRouteRuntime,
    pool_id: i64,
    merchant_a: i64,
    merchant_b: i64,
    service_a: MerchantOwnedService,
    service_b: MerchantOwnedService,
    principal: GatewayPrincipal,
    pricing: CachedRequestPricingSnapshotSource,
}

async fn fixture(label: &str, only_first: bool, ceiling: Option<i64>) -> Fixture {
    let database = SqliteTestDatabase::new(label).await;
    for statement in [
        "INSERT INTO groups (id,name,display_name,flags) VALUES (1,'pool-group','Shared pool','{}')".to_owned(),
        "INSERT INTO users (id,username,status,role,default_group_id,aff_code,settings) VALUES (1,'merchant-a',1,0,1,'ma','{}'),(2,'merchant-b',1,0,1,'mb','{}'),(3,'merchant-admin',1,1,1,'admin','{}'),(4,'merchant-consumer',1,0,1,'consumer','{}')".to_owned(),
        "INSERT INTO model_prices (model,billing_mode,input_price,output_price) VALUES ('gpt-test',1,2,4)".to_owned(),
        format!("INSERT INTO tokens (id,user_id,key_hash,key_prefix,name,status,remain_quota) VALUES (1,4,'{}','sk-pool','pool-key',1,1000000000)","ab".repeat(32)),
    ] { database.seed().execute_unprepared(&statement).await.unwrap(); }
    let pools = MerchantPoolRepository::new(database.pool().clone(), Duration::from_secs(5));
    let admin = UserId::new(3).unwrap();
    let pool = pools
        .write_pool(
            admin,
            None,
            MerchantPoolWrite {
                group_id: 1,
                name: "Shared pool".to_owned(),
                description: String::new(),
                enabled: true,
                commission_bps: 500,
                commission_inherit_global: false,
                min_ratio_micros: 100_000,
                max_ratio_micros: 5_000_000,
                settlement_delay_secs: 3600,
            },
        )
        .await
        .unwrap();
    let mut merchants = Vec::new();
    let mut supplies = Vec::new();
    for (owner, name, ratio) in [(1, "Merchant A", 500_000), (2, "Merchant B", 1_000_000)] {
        let owner = UserId::new(owner).unwrap();
        let profile = pools
            .apply(
                owner,
                MerchantApplication {
                    name: name.to_owned(),
                    description: String::new(),
                },
            )
            .await
            .unwrap();
        pools
            .review_merchant(admin, profile.id, review("approve", None))
            .await
            .unwrap();
        let supply = pools
            .write_service(owner, None, service_write(pool.id, ratio), |_, _, _, _| {
                EncryptedCredentialEnvelope::new("merchant-tests", [1; 24], vec![2; 32])
                    .map_err(|_| af_domain::MerchantPoolError::Unavailable)
            })
            .await
            .unwrap();
        pools.submit_service(owner, supply.id).await.unwrap();
        let supply = pools
            .review_service(admin, supply.id, review("approve", Some(1)))
            .await
            .unwrap();
        merchants.push(profile.id);
        supplies.push(supply);
    }
    let routes = UserRouteRepository::new(database.pool().clone(), Duration::from_secs(5)).unwrap();
    let mut members = vec![MerchantRouteMember {
        merchant_id: merchants[0],
        priority: 0,
        weight: 1,
        last_selected_at_millis: None,
    }];
    if !only_first {
        members.push(MerchantRouteMember {
            merchant_id: merchants[1],
            priority: 1,
            weight: 1,
            last_selected_at_millis: None,
        });
    }
    let route = routes
        .write_merchant_route(
            UserId::new(4).unwrap(),
            None,
            MerchantRouteWrite {
                excluded_merchant_ids: Vec::new(),
                name: "Pool route".to_owned(),
                pool_id: pool.id,
                strategy: UserRouteStrategy::Ordered,
                enabled: true,
                max_merchant_attempts: 2,
                max_price_ratio_micros: ceiling,
                members,
                allow_pool_fallback: false,
                accept_new_merchants: false,
            },
        )
        .await
        .unwrap();
    database
        .seed()
        .execute(Statement::from_sql_and_values(
            database.seed().get_database_backend(),
            "UPDATE tokens SET user_route_id=? WHERE id=1",
            [route.id.into()],
        ))
        .await
        .unwrap();
    let runtime = UserRouteRuntime::new(routes.clone())
        .with_merchant_pools(pools.clone())
        .with_group_model_policy(
            GroupModelPolicyRepository::new(database.pool().clone(), Duration::from_secs(5))
                .unwrap(),
        );
    let pricing = CachedRequestPricingSnapshotSource::new(
        ModelPriceCache::load_from_database(database.pool().clone())
            .await
            .unwrap(),
        GroupPricingCache::load_from_database(database.pool().clone())
            .await
            .unwrap(),
    );
    Fixture {
        database,
        pools,
        routes,
        runtime,
        pool_id: pool.id,
        merchant_a: merchants[0],
        merchant_b: merchants[1],
        service_a: supplies.remove(0),
        service_b: supplies.remove(0),
        principal: GatewayPrincipal::new(
            TokenId::new(1).unwrap(),
            UserId::new(4).unwrap(),
            GroupId::new(1).unwrap(),
        )
        .with_user_route_id(Some(route.id)),
        pricing,
    }
}
fn service_write(pool: i64, ratio: i64) -> MerchantServiceWrite {
    MerchantServiceWrite {
        pool_id: pool,
        name: "Merchant service".to_owned(),
        description: "Approved service".to_owned(),
        base_url: "https://api.example.com/v1".to_owned(),
        protocol: "openai_chat".to_owned(),
        api_key: Some("fixture-upstream".to_owned()),
        default_ratio_micros: ratio,
        models: vec![MerchantModelWrite {
            model: "gpt-test".to_owned(),
            upstream_model: "upstream-model".to_owned(),
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
fn review(decision: &str, version: Option<i64>) -> MerchantReview {
    MerchantReview {
        decision: decision.to_owned(),
        reason: "Fixture administrator decision".to_owned(),
        expected_revision: version,
    }
}
fn features() -> MerchantRequestFeatures {
    MerchantRequestFeatures {
        streaming: false,
        tools: false,
        vision: false,
        unsupported_media: false,
    }
}
fn usage() -> Usage {
    let zero = TokenCount::new(0).unwrap();
    Usage::new(
        TokenCount::new(100).unwrap(),
        TokenCount::new(10).unwrap(),
        UsageDetails::new(zero, zero, zero, zero, zero, zero),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}
async fn candidates(fixture: &Fixture) -> Vec<UserRouteCandidate> {
    let plan = fixture
        .runtime
        .load(fixture.principal, "gpt-test")
        .await
        .unwrap()
        .unwrap();
    fixture
        .runtime
        .candidates_for_operation(
            &plan,
            fixture.principal,
            &usage(),
            &fixture.pricing,
            Protocol::OpenAiChat,
            Some(Operation::Chat),
            Some(features()),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn explicit_merchant_never_expands_to_another_merchant_in_same_pool() {
    let fixture = fixture("merchant-fixed-scope", true, None).await;
    let selected = candidates(&fixture).await;
    assert_eq!(selected.len(), 1);
    let offer = selected[0].merchant.as_ref().unwrap();
    assert_eq!(offer.merchant_id, fixture.merchant_a);
    assert_ne!(offer.merchant_id, fixture.merchant_b);
    let principal = UserRouteRuntime::merchant_principal(fixture.principal, offer).unwrap();
    assert_eq!(
        principal.merchant_channel_id().unwrap().get(),
        offer.channel_id
    );
    assert_eq!(principal.user_id(), fixture.principal.user_id());
    assert_eq!(principal.token_id(), fixture.principal.token_id());
    assert_eq!(
        principal.merchant_billing_snapshot().unwrap().merchant_id(),
        fixture.merchant_a
    );
    fixture.database.close().await;
}

#[tokio::test]
async fn failover_assigns_final_channel_pricing_and_income_to_actual_successful_merchant() {
    let fixture = fixture("merchant-failover-attribution", false, None).await;
    let upper = usage();
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let a = fixture.merchant_a;
    let b = fixture.merchant_b;
    let execution = UserRouteExecution {
        principal: fixture.principal,
        request_id: "merchant-failover",
        model: "gpt-test",
        protocol: Protocol::OpenAiChat,
        operation: Operation::Chat,
        upper_bound: &upper,
        pricing: &fixture.pricing,
        defer_success_feedback: false,
        merchant_features: Some(features()),
    };
    let result = execution
        .run(Some(&fixture.runtime), (), |principal, (), snapshot, _| {
            let attempts = attempts.clone();
            async move {
                let terms = principal.merchant_billing_snapshot().unwrap();
                attempts
                    .lock()
                    .unwrap()
                    .push((terms.merchant_id(), terms.sales_ratio_micros()));
                assert!(snapshot.is_some());
                if terms.merchant_id() == a {
                    return Err(UserRouteAttemptError::before_delivery(
                        UpstreamError::overloaded().into(),
                    ));
                }
                Ok(crate::RoutedExecution::new(
                    terms,
                    principal.merchant_channel_id().unwrap(),
                ))
            }
        })
        .await
        .unwrap();
    let expected_channel = fixture
        .pools
        .published_candidates(fixture.pool_id, Some(b), "gpt-test")
        .await
        .unwrap()[0]
        .channel_id;
    assert_eq!(
        result.channel_id(),
        ChannelId::new(expected_channel).unwrap()
    );
    assert_eq!(result.into_value().merchant_id(), b);
    assert_eq!(
        *attempts.lock().unwrap(),
        vec![(a, 500_000), (b, 1_000_000)]
    );
    let stats = fixture
        .routes
        .merchant_statistics(fixture.pool_id, Some("gpt-test"), 900)
        .await
        .unwrap();
    assert_eq!(
        stats
            .iter()
            .find(|row| row.merchant_id == a)
            .unwrap()
            .failure_count,
        1
    );
    assert_eq!(
        stats
            .iter()
            .find(|row| row.merchant_id == b)
            .unwrap()
            .success_count,
        1
    );
    fixture.database.close().await;
}

#[tokio::test]
async fn merchant_price_ceiling_eliminates_expensive_fallback() {
    let fixture = fixture("merchant-price-ceiling", false, Some(750_000)).await;
    let selected = candidates(&fixture).await;
    assert_eq!(selected.len(), 1);
    assert_eq!(
        selected[0].merchant.as_ref().unwrap().merchant_id,
        fixture.merchant_a
    );
    fixture.database.close().await;
}

#[tokio::test]
async fn draft_changes_preserve_live_candidate_and_publication_invalidates_old_snapshot() {
    let fixture = fixture("merchant-revision-isolation", true, None).await;
    let mut selected = candidates(&fixture).await;
    let offer = selected.remove(0).merchant.unwrap();
    let plan = fixture
        .runtime
        .load(fixture.principal, "gpt-test")
        .await
        .unwrap()
        .unwrap();
    let draft = fixture
        .pools
        .write_service(
            UserId::new(1).unwrap(),
            Some(fixture.service_a.id),
            service_write(fixture.pool_id, 700_000),
            |_, _, _, _| {
                EncryptedCredentialEnvelope::new("merchant-tests", [3; 24], vec![4; 32])
                    .map_err(|_| af_domain::MerchantPoolError::Unavailable)
            },
        )
        .await
        .unwrap();
    fixture
        .pools
        .submit_service(UserId::new(1).unwrap(), draft.id)
        .await
        .unwrap();
    assert!(
        fixture
            .runtime
            .validate_merchant_candidate(fixture.principal, &plan, &offer)
            .await
            .unwrap()
    );
    fixture
        .pools
        .review_service(
            UserId::new(3).unwrap(),
            draft.id,
            review("approve", Some(2)),
        )
        .await
        .unwrap();
    assert!(
        !fixture
            .runtime
            .validate_merchant_candidate(fixture.principal, &plan, &offer)
            .await
            .unwrap()
    );
    let changed = candidates(&fixture).await;
    assert_eq!(
        changed[0].merchant.as_ref().unwrap().sales_ratio_micros,
        700_000
    );
    fixture.database.close().await;
}

#[tokio::test]
async fn revoked_or_cross_owner_route_binding_never_falls_back_to_other_supply() {
    let fixture = fixture("merchant-route-owner", true, None).await;
    let wrong = GatewayPrincipal::new(
        fixture.principal.token_id(),
        UserId::new(2).unwrap(),
        fixture.principal.group_id(),
    )
    .with_user_route_id(fixture.principal.user_route_id());
    assert!(fixture.runtime.load(wrong, "gpt-test").await.is_err());
    let route_id = fixture.principal.user_route_id().unwrap();
    assert!(
        fixture
            .routes
            .merchant_route(UserId::new(2).unwrap(), route_id)
            .await
            .is_err()
    );
    fixture
        .pools
        .pause_service(UserId::new(1).unwrap(), fixture.service_a.id, true)
        .await
        .unwrap();
    let plan = fixture
        .runtime
        .load(fixture.principal, "gpt-test")
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        fixture
            .runtime
            .candidates_for_operation(
                &plan,
                fixture.principal,
                &usage(),
                &fixture.pricing,
                Protocol::OpenAiChat,
                Some(Operation::Chat),
                Some(features())
            )
            .await,
        Err(AfError::ModelUnavailable)
    ));
    assert!(
        fixture
            .pools
            .published_candidates(fixture.pool_id, Some(fixture.merchant_b), "gpt-test")
            .await
            .unwrap()
            .iter()
            .any(|row| row.service_id == fixture.service_b.id)
    );
    fixture.database.close().await;
}

#[tokio::test]
async fn merchant_media_candidates_require_the_declared_protocol_and_keep_channel_scope() {
    let fixture = fixture("merchant-media-scope", true, None).await;
    for protocol in [
        Protocol::OpenAiImages,
        Protocol::OpenAiAudio,
        Protocol::OpenAiSpeech,
    ] {
        let mut write = service_write(fixture.pool_id, 700_000);
        write.protocol = protocol.as_str().to_owned();
        write.models[0].supports_streaming = false;
        write.models[0].supports_tools = false;
        write.models[0].context_window = Some(1);
        write.models[0].max_output_tokens = Some(1);
        let service = fixture
            .pools
            .write_service(UserId::new(1).unwrap(), None, write, |_, _, _, _| {
                EncryptedCredentialEnvelope::new("merchant-tests", [1; 24], vec![2; 32])
                    .map_err(|_| af_domain::MerchantPoolError::Unavailable)
            })
            .await
            .unwrap();
        fixture
            .pools
            .submit_service(UserId::new(1).unwrap(), service.id)
            .await
            .unwrap();
        fixture
            .pools
            .review_service(
                UserId::new(3).unwrap(),
                service.id,
                review("approve", Some(1)),
            )
            .await
            .unwrap();
        let plan = fixture
            .runtime
            .load(fixture.principal, "gpt-test")
            .await
            .unwrap()
            .unwrap();
        let operation = if protocol == Protocol::OpenAiImages {
            Operation::Image
        } else {
            Operation::Audio
        };
        let selected = fixture
            .runtime
            .candidates_for_operation(
                &plan,
                fixture.principal,
                &usage(),
                &fixture.pricing,
                protocol,
                Some(operation),
                None,
            )
            .await
            .unwrap();
        assert_eq!(selected.len(), 1);
        let offer = selected[0].merchant.as_ref().unwrap();
        assert_eq!(offer.service_id, service.id);
        assert_eq!(offer.merchant_id, fixture.merchant_a);
        let scoped = UserRouteRuntime::merchant_principal(fixture.principal, offer).unwrap();
        assert_eq!(
            scoped.merchant_channel_id().unwrap().get(),
            offer.channel_id
        );
        assert_eq!(
            scoped
                .merchant_billing_snapshot()
                .unwrap()
                .sales_ratio_micros(),
            700_000
        );
    }
    fixture.database.close().await;
}

#[tokio::test]
async fn merchant_video_candidates_use_platform_card_and_exclude_text_supply() {
    let fixture = fixture("merchant-video-scope", true, None).await;
    let mut write = service_write(fixture.pool_id, 500_000);
    write.protocol = "xai_video".to_owned();
    write.models[0].model = "grok-imagine-video-1.5".to_owned();
    write.models[0].upstream_model = write.models[0].model.clone();
    write.models[0].supports_streaming = false;
    write.models[0].supports_tools = false;
    let service = fixture
        .pools
        .write_service(UserId::new(1).unwrap(), None, write, |_, _, _, _| {
            EncryptedCredentialEnvelope::new("merchant-tests", [1; 24], vec![2; 32])
                .map_err(|_| af_domain::MerchantPoolError::Unavailable)
        })
        .await
        .unwrap();
    fixture
        .pools
        .submit_service(UserId::new(1).unwrap(), service.id)
        .await
        .unwrap();
    fixture
        .pools
        .review_service(
            UserId::new(3).unwrap(),
            service.id,
            review("approve", Some(1)),
        )
        .await
        .unwrap();
    let plan = fixture
        .runtime
        .load(fixture.principal, "grok-imagine-video-1.5")
        .await
        .unwrap()
        .unwrap();
    let offers = fixture
        .runtime
        .merchant_video_candidates(
            &plan,
            fixture.principal,
            Some(af_protocol::VideoResolution::P720),
        )
        .await
        .unwrap();
    assert_eq!(offers.len(), 1);
    assert_eq!(offers[0].offer.service_id, service.id);
    assert_eq!(offers[0].pricing.ratios().group().micros(), 500_000);
    assert_eq!(
        offers[0]
            .pricing
            .rate_microusd("grok-imagine-video-1.5")
            .unwrap(),
        140_000
    );
    fixture.database.close().await;
}
