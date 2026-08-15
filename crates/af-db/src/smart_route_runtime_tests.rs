use std::{error::Error, time::Duration};

use af_domain::{RouteChannelId, RouteMode, RouteStrategy};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

use super::{
    DatabaseOptions, MigrationOptions, SmartRouteAttemptFeedback, SmartRouteRuntimeRepository,
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, channels, credentials,
        route_channels, routes,
    },
    validate_smart_route_model_mapping,
};

#[tokio::test]
async fn matching_rules_use_fixed_specificity_and_deterministic_model_mapping()
-> Result<(), Box<dyn Error>> {
    assert!(!validate_smart_route_model_mapping(&serde_json::json!({
        " smart-chat": "gpt-5.5"
    })));
    assert!(!validate_smart_route_model_mapping(&serde_json::json!({
        "smart-chat": "gpt-5.5 "
    })));
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = SmartRouteRuntimeRepository::new(pool.clone(), Duration::from_secs(1))?;

    insert_route(
        &pool,
        "正则规则",
        "re:^gpt-5\\.[0-9]+$",
        RouteMode::Pattern,
        serde_json::json!({"re:^gpt-5\\.[0-9]+$": "gpt-regex"}),
    )
    .await?;
    insert_route(
        &pool,
        "精确规则",
        "gpt-5.5",
        RouteMode::Pattern,
        serde_json::json!({"gpt-5.5": "gpt-exact"}),
    )
    .await?;
    insert_route(
        &pool,
        "后置精确规则",
        "gpt-5.5",
        RouteMode::Pattern,
        serde_json::json!({"gpt-5.5": "gpt-exact-later"}),
    )
    .await?;
    insert_route(
        &pool,
        "显式分组",
        "gpt-5.5",
        RouteMode::ExplicitGroup,
        serde_json::json!({"gpt-5.5": "gpt-group"}),
    )
    .await?;

    let rules = repository.matching_rules("gpt-5.5").await?;

    assert_eq!(
        rules
            .iter()
            .map(|rule| (rule.mode(), rule.routed_model()))
            .collect::<Vec<_>>(),
        [
            (RouteMode::ExplicitGroup, "gpt-group"),
            (RouteMode::Pattern, "gpt-exact"),
            (RouteMode::Pattern, "gpt-exact-later"),
            (RouteMode::Pattern, "gpt-regex"),
        ]
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn attempt_feedback_updates_statistics_and_rolls_back_invalid_batches()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = SmartRouteRuntimeRepository::new(pool.clone(), Duration::from_secs(1))?;
    let route = insert_route(
        &pool,
        "统计规则",
        "gpt-5.5",
        RouteMode::Pattern,
        serde_json::json!({}),
    )
    .await?;
    let channel = insert_channel(&pool).await?;
    let credential = insert_credential(&pool, channel.id).await?;
    let route_channel = route_channels::ActiveModel {
        route_id: Set(route.id),
        channel_id: Set(channel.id),
        credential_id: Set(credential.id),
        priority: Set(10),
        weight: Set(20),
        enabled: Set(true),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let route_channel_id = RouteChannelId::new(route_channel.id)?;

    repository
        .record_attempts(&[SmartRouteAttemptFeedback::new(route_channel_id, true, 17)?])
        .await?;
    repository
        .record_attempts(&[SmartRouteAttemptFeedback::new(route_channel_id, false, 24)?])
        .await?;

    let updated = route_channels::Entity::find_by_id(route_channel.id)
        .one(pool.connection())
        .await?
        .expect("路由候选必须存在");
    assert_eq!(updated.success_count, 1);
    assert_eq!(updated.fail_count, 1);
    assert_eq!(updated.total_latency, 41);
    assert!(updated.last_selected_at.is_some());
    assert!(updated.last_failure_at.is_some());

    let missing_id = RouteChannelId::new(route_channel.id + 100)?;
    assert!(
        repository
            .record_attempts(&[
                SmartRouteAttemptFeedback::new(route_channel_id, true, 10)?,
                SmartRouteAttemptFeedback::new(missing_id, false, 10)?,
            ])
            .await
            .is_err()
    );
    let rolled_back = route_channels::Entity::find_by_id(route_channel.id)
        .one(pool.connection())
        .await?
        .expect("路由候选必须存在");
    assert_eq!(rolled_back.success_count, 1);
    assert_eq!(rolled_back.fail_count, 1);
    assert_eq!(rolled_back.total_latency, 41);

    pool.close().await?;
    Ok(())
}

async fn insert_route(
    pool: &super::DatabasePool,
    name: &str,
    model_pattern: &str,
    mode: RouteMode,
    model_mapping: serde_json::Value,
) -> Result<routes::Model, sea_orm::DbErr> {
    routes::ActiveModel {
        name: Set(name.to_owned()),
        model_pattern: Set(model_pattern.to_owned()),
        route_mode: Set(mode.code()),
        strategy: Set(RouteStrategy::Weighted.code()),
        model_mapping: Set(model_mapping),
        enabled: Set(true),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

async fn insert_channel(pool: &super::DatabasePool) -> Result<channels::Model, sea_orm::DbErr> {
    channels::ActiveModel {
        name: Set("智能路由测试渠道".to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        base_url: Set(Some(
            ChannelBaseUrl::parse("https://api.example.com/v1").expect("测试地址必须有效"),
        )),
        model_mapping: Set(serde_json::json!({})),
        param_override: Set(serde_json::json!({})),
        header_override: Set(
            HeaderOverrides::validate(serde_json::json!({})).expect("空请求头必须有效")
        ),
        settings: Set(SensitiveJson::from(serde_json::json!({}))),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

async fn insert_credential(
    pool: &super::DatabasePool,
    channel_id: i64,
) -> Result<credentials::Model, sea_orm::DbErr> {
    credentials::ActiveModel {
        channel_id: Set(channel_id),
        kind: Set("api_key".to_owned()),
        secret: Set(EncryptedJson::from_envelope(serde_json::json!({
            "version": 1,
            "algorithm": "xchacha20poly1305",
            "key_id": "smart-route-test",
            "nonce": URL_SAFE_NO_PAD.encode([0x42; 24]),
            "ciphertext": URL_SAFE_NO_PAD.encode(b"smart-route-test-secret-with-tag")
        }))
        .expect("测试密文必须有效")),
        quota_dimension: Set("global".to_owned()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}
