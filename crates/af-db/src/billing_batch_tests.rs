use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use af_domain::{BillingReservationId, ChannelId, QuotaDelta, TokenId, UserId};
use sea_orm::entity::prelude::Json;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tokio::{sync::Barrier, task::JoinSet};

use crate::{
    BillingBatchRepository, BillingBatchRepositoryError, BillingBatchWrite,
    BillingBatchWriteOutcome, ChannelBillingWrite, DatabaseOptions, DatabasePool, MigrationOptions,
    PoolOptions, TokenBillingWrite, UserBillingWrite,
    entity::{
        BillingBatchWriterKey, HeaderOverrides, SensitiveJson, TokenHash,
        billing_batch_checkpoints, channels, groups, tokens, users,
    },
};

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-billing-batch-{}-{serial}.db",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let mut sqlite_path = path.to_string_lossy().replace('\\', "/");
        if !sqlite_path.starts_with('/') {
            sqlite_path.insert(0, '/');
        }
        Self {
            path,
            url: format!("sqlite://{sqlite_path}?mode=rwc"),
        }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

struct Fixture {
    pool: DatabasePool,
    repository: BillingBatchRepository,
    user_id: UserId,
    token_id: TokenId,
    channel_id: ChannelId,
}

impl Fixture {
    async fn close(self) -> Result<(), Box<dyn Error>> {
        let Self {
            pool, repository, ..
        } = self;
        drop(repository);
        pool.close().await?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    user_quota: i64,
    user_used_quota: i64,
    user_request_count: i64,
    token_remain_quota: i64,
    token_used_quota: i64,
    channel_used_quota: i64,
}

#[tokio::test]
async fn applies_all_dimensions_and_replays_without_double_counting() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(DatabaseOptions::new("sqlite::memory:")?).await?;
    let batch = complete_batch(&fixture, 1, 1, 2, 15, 2);

    assert_eq!(
        fixture.repository.apply(&batch).await?,
        BillingBatchWriteOutcome::Applied
    );
    assert_eq!(snapshot(&fixture).await?, expected_after(15, 2));
    assert_eq!(
        fixture.repository.apply(&batch).await?,
        BillingBatchWriteOutcome::Existing
    );
    assert_eq!(snapshot(&fixture).await?, expected_after(15, 2));

    let checkpoint = checkpoint(&fixture.pool, &batch)
        .await?
        .expect("批次提交后必须保存 writer checkpoint");
    assert_eq!(checkpoint.last_start_sequence, 1);
    assert_eq!(checkpoint.last_end_sequence, 2);
    assert_eq!(checkpoint.last_event_count, 2);

    fixture.close().await
}

#[tokio::test]
async fn sequence_gap_and_same_range_with_different_payload_are_rejected()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(DatabaseOptions::new("sqlite::memory:")?).await?;
    let first = complete_batch(&fixture, 2, 1, 1, 10, 1);
    fixture.repository.apply(&first).await?;

    let conflicting = complete_batch(&fixture, 2, 1, 1, 11, 1);
    assert_eq!(
        fixture.repository.apply(&conflicting).await,
        Err(BillingBatchRepositoryError::SequenceConflict)
    );
    let gap = complete_batch(&fixture, 2, 3, 3, 7, 1);
    assert_eq!(
        fixture.repository.apply(&gap).await,
        Err(BillingBatchRepositoryError::SequenceConflict)
    );
    assert_eq!(snapshot(&fixture).await?, expected_after(10, 1));

    fixture.close().await
}

#[tokio::test]
async fn result_unknown_replays_as_existing_after_commit() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(DatabaseOptions::new("sqlite::memory:")?).await?;
    let batch = complete_batch(&fixture, 3, 1, 1, 9, 1);
    fixture.repository.inject_outcome_unknown_after_commit();

    assert_eq!(
        fixture.repository.apply(&batch).await,
        Err(BillingBatchRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        fixture.repository.apply(&batch).await?,
        BillingBatchWriteOutcome::Existing
    );
    assert_eq!(snapshot(&fixture).await?, expected_after(9, 1));

    fixture.close().await
}

#[tokio::test]
async fn failed_subject_guard_rolls_back_the_entire_batch_and_checkpoint()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(DatabaseOptions::new("sqlite::memory:")?).await?;
    let batch = BillingBatchWrite::new(
        [4; 16],
        1,
        1,
        vec![reservation_id(4)],
        vec![UserBillingWrite::new(
            fixture.user_id,
            QuotaDelta::new(-10).unwrap(),
            QuotaDelta::new(10).unwrap(),
            1,
        )?],
        vec![TokenBillingWrite::new(
            fixture.token_id,
            QuotaDelta::new(-101).unwrap(),
            QuotaDelta::new(10).unwrap(),
        )?],
        vec![ChannelBillingWrite::new(
            fixture.channel_id,
            QuotaDelta::new(10).unwrap(),
        )?],
    )?;

    assert_eq!(
        fixture.repository.apply(&batch).await,
        Err(BillingBatchRepositoryError::Invariant)
    );
    assert_eq!(snapshot(&fixture).await?, initial_snapshot());
    assert!(checkpoint(&fixture.pool, &batch).await?.is_none());

    fixture.close().await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_replay_of_the_same_next_batch_applies_once() -> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let options = DatabaseOptions::new(database.url.clone())?.with_pool_options(PoolOptions {
        max_connections: 8,
        min_connections: 1,
        acquire_timeout: Duration::from_secs(15),
        ..PoolOptions::default()
    });
    let fixture = fixture(options).await?;
    let first = complete_batch(&fixture, 5, 1, 1, 3, 1);
    fixture.repository.apply(&first).await?;
    let next = Arc::new(complete_batch(&fixture, 5, 2, 2, 7, 1));
    let barrier = Arc::new(Barrier::new(9));
    let mut tasks = JoinSet::new();
    for _ in 0..8 {
        let repository = fixture.repository.clone();
        let batch = Arc::clone(&next);
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository.apply(&batch).await
        });
    }
    barrier.wait().await;

    let mut applied = 0;
    let mut existing = 0;
    while let Some(result) = tasks.join_next().await {
        match result?? {
            BillingBatchWriteOutcome::Applied => applied += 1,
            BillingBatchWriteOutcome::Existing => existing += 1,
        }
    }
    assert_eq!((applied, existing), (1, 7));
    assert_eq!(snapshot(&fixture).await?, expected_after(10, 2));

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn zero_operation_timeout_is_rejected_without_exposing_pool_details()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    assert_eq!(
        BillingBatchRepository::with_operation_timeout(pool.clone(), Duration::ZERO).unwrap_err(),
        BillingBatchRepositoryError::InvalidConfiguration
    );
    pool.close().await?;
    Ok(())
}

async fn fixture(options: DatabaseOptions) -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let group = groups::ActiveModel {
        name: Set("batch-group".to_owned()),
        display_name: Set("批量落盘测试分组".to_owned()),
        flags: Set(empty_json()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("batch-user".to_owned()),
        default_group_id: Set(group.id),
        quota: Set(100),
        used_quota: Set(10),
        frozen_quota: Set(0),
        request_count: Set(3),
        aff_code: Set("batch-invite".to_owned()),
        settings: Set(empty_json()),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&"a".repeat(64))?),
        key_prefix: Set("sk-af-batch".to_owned()),
        name: Set("batch-token".to_owned()),
        group_id: Set(Some(group.id)),
        remain_quota: Set(100),
        unlimited_quota: Set(false),
        used_quota: Set(4),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let channel = channels::ActiveModel {
        name: Set("batch-channel".to_owned()),
        r#type: Set("openai".to_owned()),
        protocol: Set("openai_chat".to_owned()),
        model_mapping: Set(empty_json()),
        param_override: Set(empty_json()),
        header_override: Set(HeaderOverrides::validate(empty_json())?),
        used_quota: Set(7),
        settings: Set(SensitiveJson::from(empty_json())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(Fixture {
        repository: BillingBatchRepository::new(pool.clone()),
        pool,
        user_id: UserId::new(user.id)?,
        token_id: TokenId::new(token.id)?,
        channel_id: ChannelId::new(channel.id)?,
    })
}

fn complete_batch(
    fixture: &Fixture,
    writer_marker: u8,
    start_sequence: u64,
    end_sequence: u64,
    used: i64,
    requests: i64,
) -> BillingBatchWrite {
    let event_ids = (start_sequence..=end_sequence)
        .map(|sequence| reservation_id(u8::try_from(sequence).unwrap_or(99)))
        .collect();
    BillingBatchWrite::new(
        [writer_marker; 16],
        start_sequence,
        end_sequence,
        event_ids,
        vec![
            UserBillingWrite::new(
                fixture.user_id,
                QuotaDelta::new(-used).unwrap(),
                QuotaDelta::new(used).unwrap(),
                requests,
            )
            .unwrap(),
        ],
        vec![
            TokenBillingWrite::new(
                fixture.token_id,
                QuotaDelta::new(-used).unwrap(),
                QuotaDelta::new(used).unwrap(),
            )
            .unwrap(),
        ],
        vec![ChannelBillingWrite::new(fixture.channel_id, QuotaDelta::new(used).unwrap()).unwrap()],
    )
    .unwrap()
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).unwrap()
}

async fn snapshot(fixture: &Fixture) -> Result<Snapshot, Box<dyn Error>> {
    let user = users::Entity::find_by_id(fixture.user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在");
    let token = tokens::Entity::find_by_id(fixture.token_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试令牌必须存在");
    let channel = channels::Entity::find_by_id(fixture.channel_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试渠道必须存在");
    Ok(Snapshot {
        user_quota: user.quota,
        user_used_quota: user.used_quota,
        user_request_count: user.request_count,
        token_remain_quota: token.remain_quota,
        token_used_quota: token.used_quota,
        channel_used_quota: channel.used_quota,
    })
}

async fn checkpoint(
    pool: &DatabasePool,
    batch: &BillingBatchWrite,
) -> Result<Option<billing_batch_checkpoints::Model>, Box<dyn Error>> {
    let key = BillingBatchWriterKey::parse(&batch.writer_key())?;
    Ok(billing_batch_checkpoints::Entity::find_by_id(key)
        .one(pool.connection())
        .await?)
}

const fn initial_snapshot() -> Snapshot {
    Snapshot {
        user_quota: 100,
        user_used_quota: 10,
        user_request_count: 3,
        token_remain_quota: 100,
        token_used_quota: 4,
        channel_used_quota: 7,
    }
}

fn expected_after(used: i64, requests: i64) -> Snapshot {
    Snapshot {
        user_quota: 100 - used,
        user_used_quota: 10 + used,
        user_request_count: 3 + requests,
        token_remain_quota: 100 - used,
        token_used_quota: 4 + used,
        channel_used_quota: 7 + used,
    }
}

fn empty_json() -> Json {
    Json::Object(Default::default())
}
