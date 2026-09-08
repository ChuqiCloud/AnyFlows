use std::{error::Error, time::Duration};

use af_domain::UserId;
use sea_orm::{EntityTrait, PaginatorTrait};
use serde_json::json;

use super::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MAX_ACTIVE_PLAYGROUND_SHARES_PER_USER, MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES, MigrationOptions,
    PlaygroundShareRepository, PlaygroundShareRepositoryError, PlaygroundShareRevokeOutcome,
    PlaygroundShareWrite,
};
use crate::entity::playground_shares;

#[tokio::test]
async fn share_round_trip_revoke_and_cleanup_hide_sensitive_content() -> Result<(), Box<dyn Error>>
{
    let (pool, owner_user_id) = setup_database().await?;
    let repository = PlaygroundShareRepository::new(pool.clone(), Duration::from_secs(5))?;
    let snapshot = json!({
        "version": 1,
        "sessions": [{
            "model": "private-model",
            "messages": [
                {"role": "user", "content": "private-user-message"},
                {"role": "assistant", "content": "private-assistant-message"}
            ]
        }]
    });
    let token_hash = hash_for(1);
    let created = repository
        .create(write(owner_user_id, &token_hash, snapshot.clone(), 1))
        .await?;
    assert!(created.expires_at() > created.created_at());

    let active = repository
        .find_active(&token_hash)
        .await?
        .expect("新建分享必须可以公开读取");
    let debug = format!("{active:?}");
    assert!(!debug.contains("private-model"));
    assert!(!debug.contains("private-user-message"));
    let (stored, _, expires_at) = active.into_parts();
    assert_eq!(stored, snapshot);
    assert_eq!(expires_at, created.expires_at());

    assert_eq!(
        repository
            .revoke(UserId::new(owner_user_id.get() + 1)?, &token_hash)
            .await?,
        PlaygroundShareRevokeOutcome::NotFound
    );
    assert_eq!(
        repository.revoke(owner_user_id, &token_hash).await?,
        PlaygroundShareRevokeOutcome::Revoked
    );
    assert!(repository.find_active(&token_hash).await?.is_none());
    assert_eq!(
        repository.revoke(owner_user_id, &token_hash).await?,
        PlaygroundShareRevokeOutcome::NotFound
    );

    // 下一次创建会清理同一所有者已经撤销的记录，避免长期占用容量。
    repository
        .create(write(
            owner_user_id,
            &hash_for(2),
            json!({"version": 1, "sessions": []}),
            1,
        ))
        .await?;
    assert_eq!(
        playground_shares::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn share_capacity_and_unique_digest_fail_closed() -> Result<(), Box<dyn Error>> {
    let (pool, owner_user_id) = setup_database().await?;
    let repository = PlaygroundShareRepository::new(pool.clone(), Duration::from_secs(5))?;
    let snapshot = json!({"version": 1, "sessions": []});
    for index in 0..MAX_ACTIVE_PLAYGROUND_SHARES_PER_USER {
        repository
            .create(write(
                owner_user_id,
                &hash_for(index + 1),
                snapshot.clone(),
                30,
            ))
            .await?;
    }
    assert_eq!(
        repository
            .create(write(owner_user_id, &hash_for(999), snapshot.clone(), 30))
            .await,
        Err(PlaygroundShareRepositoryError::LimitReached)
    );

    assert_eq!(
        repository
            .create(write(owner_user_id, "not-a-digest", snapshot.clone(), 1))
            .await,
        Err(PlaygroundShareRepositoryError::InvalidInput)
    );
    assert!(repository.find_active("not-a-digest").await?.is_none());

    repository.revoke(owner_user_id, &hash_for(1)).await?;
    assert_eq!(
        repository
            .create(write(owner_user_id, &hash_for(2), snapshot, 1))
            .await,
        Err(PlaygroundShareRepositoryError::TokenConflict)
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn share_snapshot_and_expiration_bounds_are_enforced() -> Result<(), Box<dyn Error>> {
    let (pool, owner_user_id) = setup_database().await?;
    let repository = PlaygroundShareRepository::new(pool.clone(), Duration::from_secs(5))?;
    let oversized = json!({"content": "x".repeat(MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES)});
    assert_eq!(
        repository
            .create(write(owner_user_id, &hash_for(1), oversized, 1))
            .await,
        Err(PlaygroundShareRepositoryError::InvalidInput)
    );
    assert_eq!(
        repository
            .create(PlaygroundShareWrite::new(
                owner_user_id,
                hash_for(2),
                json!([]),
                unix_now() + 60,
            ))
            .await,
        Err(PlaygroundShareRepositoryError::InvalidInput)
    );
    assert_eq!(
        repository
            .create(PlaygroundShareWrite::new(
                owner_user_id,
                hash_for(3),
                json!({}),
                unix_now() - 1,
            ))
            .await,
        Err(PlaygroundShareRepositoryError::InvalidInput)
    );
    pool.close().await?;
    Ok(())
}

async fn setup_database() -> Result<(crate::DatabasePool, UserId), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let setup = InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let InitialSetupOutcome::Initialized { user_id } = setup
        .initialize(InitialSetupRecord::new(
            "share-owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await?
    else {
        panic!("空测试数据库必须完成首次安装")
    };
    Ok((pool, user_id))
}

fn write(
    owner_user_id: UserId,
    token_hash: &str,
    snapshot: serde_json::Value,
    ttl_days: i64,
) -> PlaygroundShareWrite {
    PlaygroundShareWrite::new(
        owner_user_id,
        token_hash.to_owned(),
        snapshot,
        unix_now() + ttl_days * 24 * 60 * 60,
    )
}

fn hash_for(seed: usize) -> String {
    format!("{seed:064x}")
}

fn unix_now() -> i64 {
    sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc().unix_timestamp()
}
