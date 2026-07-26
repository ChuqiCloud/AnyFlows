use std::{error::Error, time::Duration};

use af_domain::{GroupId, TokenId, UserId};
use sea_orm::{
    ActiveModelTrait, EntityTrait, IntoActiveModel, JsonValue, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use super::{
    AdminTokenLookupOutcome, AdminTokenRepository, AdminTokenRepositoryConfigError,
    AdminTokenRepositoryError, DatabaseOptions, MigrationOptions,
};
use crate::entity::{TokenHash, TokenIpAllowlist, TokenModelAllowlist, groups, tokens, users};

#[tokio::test]
async fn list_uses_stable_cursor_and_excludes_soft_deleted_tokens() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = fixture.repository.list(None, 2).await?;
    let (tokens, next_cursor) = first.into_parts();
    assert_eq!(
        tokens.iter().map(|token| token.name()).collect::<Vec<_>>(),
        ["primary", "backup"]
    );
    let next_cursor = next_cursor.expect("仍有第三个有效令牌时必须返回游标");
    assert_eq!(next_cursor, tokens[1].token_id());

    let second = fixture.repository.list(Some(next_cursor), 2).await?;
    let (tokens, next_cursor) = second.into_parts();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name(), "fallback");
    assert_eq!(next_cursor, None);
    assert_eq!(format!("{:?}", tokens[0]), "AdminTokenRecord(<redacted>)");

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn detail_returns_only_safe_fields_and_preserves_allowlists() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let AdminTokenLookupOutcome::Found(token) = fixture.repository.get(fixture.primary_id).await?
    else {
        panic!("有效令牌详情必须存在");
    };
    assert_eq!(token.token_id(), fixture.primary_id);
    assert_eq!(token.user_id(), fixture.user_id);
    assert_eq!(token.key_prefix(), "sk-af-public000001");
    assert_eq!(token.name(), "primary");
    assert_eq!(token.status(), 1);
    assert_eq!(token.group_id(), Some(fixture.vip_group_id));
    assert_eq!(token.remain_quota(), 1_000);
    assert!(!token.unlimited_quota());
    assert_eq!(token.used_quota(), 25);
    assert_eq!(token.model_limits().unwrap(), ["gpt-5.5", "gpt-5.5"]);
    assert_eq!(
        token.allow_ips().unwrap(),
        ["192.0.2.0/24", "198.51.100.10/32"]
    );
    assert!(token.cross_group_retry());
    assert_eq!(token.rate_limit_5h(), Some(100));
    assert_eq!(token.rate_limit_1d(), Some(200));
    assert_eq!(token.rate_limit_7d(), Some(300));
    assert_eq!(token.usage_5h(), 10);
    assert_eq!(token.usage_1d(), 20);
    assert_eq!(token.usage_7d(), 30);
    assert_eq!(token.max_requests(), Some(1_000));
    assert_eq!(token.used_requests(), 4);
    assert!(matches!(
        fixture.repository.get(fixture.deleted_id).await?,
        AdminTokenLookupOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_pagination_broken_association_and_closed_pool_fail_closed()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for limit in [0, 101] {
        assert_eq!(
            fixture.repository.list(None, limit).await.unwrap_err(),
            AdminTokenRepositoryError::Invariant
        );
    }

    let mut user = users::Entity::find_by_id(fixture.user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在")
        .into_active_model();
    user.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    user.update(fixture.pool.connection()).await?;
    match fixture.repository.get(fixture.primary_id).await {
        Err(error) => assert_eq!(error, AdminTokenRepositoryError::Invariant),
        Ok(_) => panic!("坏用户关联必须失败关闭"),
    }

    fixture.pool.clone().close().await?;
    assert_eq!(
        fixture.repository.list(None, 1).await.unwrap_err(),
        AdminTokenRepositoryError::Query
    );
    Ok(())
}

#[test]
fn zero_lookup_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = runtime.block_on(crate::connect(&DatabaseOptions::new("sqlite::memory:")?))?;
    assert!(matches!(
        AdminTokenRepository::new(pool.clone(), Duration::ZERO),
        Err(AdminTokenRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminTokenRepository,
    user_id: UserId,
    vip_group_id: GroupId,
    primary_id: TokenId,
    deleted_id: TokenId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let default_group = insert_group(&pool, "default", "Default").await?;
    let vip_group = insert_group(&pool, "vip", "VIP").await?;
    let user = insert_user(&pool, default_group.id).await?;
    let primary = insert_token(&pool, user.id, Some(vip_group.id), "primary", 1, true).await?;
    insert_token(&pool, user.id, Some(vip_group.id), "backup", 2, false).await?;
    let deleted = insert_token(&pool, user.id, Some(vip_group.id), "deleted", 1, false).await?;
    let mut deleted_model = tokens::ActiveModel::from(deleted.clone());
    deleted_model.deleted_at = Set(Some(TimeDateTimeWithTimeZone::now_utc()));
    deleted_model.update(pool.connection()).await?;
    insert_token(&pool, user.id, None, "fallback", 1, false).await?;
    Ok(Fixture {
        repository: AdminTokenRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        user_id: UserId::new(user.id)?,
        vip_group_id: GroupId::new(vip_group.id)?,
        primary_id: TokenId::new(primary.id)?,
        deleted_id: TokenId::new(deleted.id)?,
    })
}

async fn insert_group(
    pool: &super::DatabasePool,
    name: &str,
    display_name: &str,
) -> Result<groups::Model, sea_orm::DbErr> {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(display_name.to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

async fn insert_user(
    pool: &super::DatabasePool,
    default_group_id: i64,
) -> Result<users::Model, sea_orm::DbErr> {
    users::ActiveModel {
        username: Set("token-owner".to_owned()),
        email: Set(Some("token-owner@example.com".to_owned())),
        role: Set(1),
        status: Set(1),
        default_group_id: Set(default_group_id),
        quota: Set(10_000),
        aff_code: Set("owner-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

async fn insert_token(
    pool: &super::DatabasePool,
    user_id: i64,
    group_id: Option<i64>,
    name: &str,
    status: i16,
    with_limits: bool,
) -> Result<tokens::Model, sea_orm::DbErr> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    tokens::ActiveModel {
        user_id: Set(user_id),
        key_hash: Set(TokenHash::parse(&format!("{:064x}", token_hash_seed(name)))
            .expect("测试令牌哈希必须有效")),
        key_prefix: Set("sk-af-public000001".to_owned()),
        name: Set(name.to_owned()),
        status: Set(status),
        group_id: Set(group_id),
        remain_quota: Set(1_000),
        unlimited_quota: Set(false),
        used_quota: Set(25),
        expired_at: Set(Some(now)),
        model_limits: Set(with_limits.then(|| {
            TokenModelAllowlist::validate(serde_json::json!(["gpt-5.5", "gpt-5.5"]))
                .expect("测试模型白名单必须有效")
        })),
        allow_ips: Set(with_limits.then(|| {
            TokenIpAllowlist::validate(serde_json::json!(["192.0.2.0/24", "198.51.100.10/32"]))
                .expect("测试 IP 白名单必须有效")
        })),
        cross_group_retry: Set(with_limits),
        rate_limit_5h: Set(with_limits.then_some(100)),
        rate_limit_1d: Set(with_limits.then_some(200)),
        rate_limit_7d: Set(with_limits.then_some(300)),
        usage_5h: Set(10),
        usage_1d: Set(20),
        usage_7d: Set(30),
        window_5h_start: Set(now),
        window_1d_start: Set(now),
        window_7d_start: Set(now),
        max_requests: Set(with_limits.then_some(1_000)),
        used_requests: Set(4),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

fn token_hash_seed(name: &str) -> i64 {
    match name {
        "primary" => 1,
        "backup" => 2,
        "deleted" => 3,
        "fallback" => 4,
        _ => 9,
    }
}
