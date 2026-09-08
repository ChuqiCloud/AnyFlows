use std::{
    error::Error,
    fs,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use af_domain::{BillingReservationId, GatewayPrincipal, GroupId, Quota, TokenId, UserId};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, JsonValue,
    PaginatorTrait, QueryFilter, entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use tokio::{sync::Barrier, task::JoinSet};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, PoolOptions, QuotaRepository,
    QuotaRepositoryError,
    entity::{
        BillingReservationKey, TokenHash, billing_reservations, billing_token_window_reservations,
        groups, tokens, users,
    },
    quota::{QuotaMutationOutcome, QuotaRepositoryOperation, QuotaReservationStatus},
};

const TEST_QUOTA: i64 = 1_000_000;
static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct WindowConfig {
    limits: [Option<i64>; 3],
    usages: [i64; 3],
    starts: [TimeDateTimeWithTimeZone; 3],
}

impl WindowConfig {
    fn new(
        limits: [Option<i64>; 3],
        usages: [i64; 3],
        starts: [TimeDateTimeWithTimeZone; 3],
    ) -> Self {
        Self {
            limits,
            usages,
            starts,
        }
    }
}

struct Fixture {
    pool: DatabasePool,
    repository: QuotaRepository,
    principal: GatewayPrincipal,
}

impl Fixture {
    async fn close(self) -> Result<(), crate::DatabaseError> {
        let pool = self.pool.clone();
        drop(self);
        pool.close().await
    }
}

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-token-window-{}-{serial}.db",
            std::process::id()
        ));
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
        for suffix in ["", "-shm", "-wal"] {
            let candidate = std::path::PathBuf::from(format!("{}{}", self.path.display(), suffix));
            let _ = fs::remove_file(candidate);
        }
    }
}

#[tokio::test]
async fn all_blocking_windows_return_longest_retry_or_permanent_shortage()
-> Result<(), Box<dyn Error>> {
    let now = second_time(0);
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(200), Some(300)],
        [90, 190, 290],
        [
            second_time(-3_600),
            second_time(-7_200),
            second_time(-10_800),
        ],
    ))
    .await?;
    let before_user = user(&fixture).await?;
    let before_token = token(&fixture).await?;

    let error = fixture
        .repository
        .precharge(reservation_id(1), fixture.principal, quota(11))
        .await
        .expect_err("任一窗口不足时不得创建部分预留");
    let QuotaRepositoryError::TokenWindowQuotaInsufficient {
        retry_after: Some(retry_after),
    } = error
    else {
        panic!("窗口不足应返回可恢复等待时间：{error:?}");
    };
    let expected = u32::try_from(
        7 * 24 * 60 * 60 - (now.unix_timestamp() - second_time(-10_800).unix_timestamp()),
    )?;
    assert!(retry_after.seconds() <= expected);
    assert!(retry_after.seconds() >= expected.saturating_sub(5));
    assert_eq!(reservation_count(&fixture).await?, 0);
    assert_eq!(allocation_count(&fixture).await?, 0);
    assert_eq!(user(&fixture).await?, before_user);
    assert_eq!(token(&fixture).await?, before_token);
    fixture.close().await?;

    let hard = new_fixture(WindowConfig::new(
        [Some(10), None, None],
        [0, 0, 0],
        [now; 3],
    ))
    .await?;
    assert_eq!(
        hard.repository
            .precharge(reservation_id(2), hard.principal, quota(11))
            .await,
        Err(QuotaRepositoryError::TokenWindowQuotaInsufficient { retry_after: None })
    );
    assert_eq!(reservation_count(&hard).await?, 0);
    hard.close().await?;
    Ok(())
}

#[tokio::test]
async fn unlimited_windows_accumulate_actual_usage_and_zero_limit_rejects()
-> Result<(), Box<dyn Error>> {
    let now = second_time(0);
    let fixture = new_fixture(WindowConfig::new([None, None, None], [5, 6, 7], [now; 3])).await?;
    let id = reservation_id(3);
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(10))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(window_usages(&fixture).await?, [5, 6, 7]);
    assert_eq!(
        fixture.repository.settle(id, quota(8)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(window_usages(&fixture).await?, [13, 14, 15]);
    fixture.close().await?;

    let zero = new_fixture(WindowConfig::new(
        [Some(0), None, None],
        [0, 0, 0],
        [now; 3],
    ))
    .await?;
    assert_eq!(
        zero.repository
            .precharge(reservation_id(4), zero.principal, quota(1))
            .await,
        Err(QuotaRepositoryError::TokenWindowQuotaInsufficient { retry_after: None })
    );
    zero.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_precharges_do_not_oversell_window_capacity() -> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let options = DatabaseOptions::new(database.url.clone())?.with_pool_options(PoolOptions {
        max_connections: 8,
        min_connections: 1,
        acquire_timeout: Duration::from_secs(15),
        ..PoolOptions::default()
    });
    let fixture = fixture_with_options(
        options,
        WindowConfig::new([Some(50), None, None], [0, 0, 0], [second_time(0); 3]),
    )
    .await?;
    let barrier = Arc::new(Barrier::new(11));
    let mut tasks = JoinSet::new();
    for marker in 10..20 {
        let repository = fixture.repository.clone();
        let principal = fixture.principal;
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository
                .precharge(reservation_id(marker), principal, quota(10))
                .await
        });
    }
    barrier.wait().await;

    let mut applied = 0;
    let mut limited = 0;
    while let Some(result) = tasks.join_next().await {
        match result? {
            Ok(QuotaMutationOutcome::Applied) => applied += 1,
            Err(QuotaRepositoryError::TokenWindowQuotaInsufficient {
                retry_after: Some(_),
            }) => limited += 1,
            outcome => panic!("并发窗口准入返回意外结果：{outcome:?}"),
        }
    }
    assert_eq!((applied, limited), (5, 5));
    assert_eq!(reservation_count(&fixture).await?, 5);
    assert_eq!(allocation_count(&fixture).await?, 5);
    assert_eq!(user(&fixture).await?.frozen_quota, 50);
    assert_eq!(window_usages(&fixture).await?, [0, 0, 0]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn refund_releases_commitment_without_reversing_settled_usage() -> Result<(), Box<dyn Error>>
{
    let fixture = new_fixture(WindowConfig::new(
        [Some(10), None, None],
        [0, 0, 0],
        [second_time(0); 3],
    ))
    .await?;
    let first = reservation_id(30);
    let second = reservation_id(31);
    fixture
        .repository
        .precharge(first, fixture.principal, quota(10))
        .await?;
    assert!(matches!(
        fixture
            .repository
            .precharge(second, fixture.principal, quota(1))
            .await,
        Err(QuotaRepositoryError::TokenWindowQuotaInsufficient {
            retry_after: Some(_)
        })
    ));
    assert_eq!(
        fixture.repository.refund(first).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(window_usages(&fixture).await?, [0, 0, 0]);
    assert_eq!(
        fixture
            .repository
            .precharge(second, fixture.principal, quota(1))
            .await?,
        QuotaMutationOutcome::Applied
    );
    fixture.repository.settle(second, quota(1)).await?;
    assert_eq!(window_usages(&fixture).await?, [1, 1, 1]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn settlement_after_partial_rollover_counts_only_matching_windows()
-> Result<(), Box<dyn Error>> {
    let old_start = second_time(-3_600);
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(100), Some(100)],
        [0, 0, 0],
        [old_start; 3],
    ))
    .await?;
    let id = reservation_id(40);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(20))
        .await?;

    let mut active = token(&fixture).await?.into_active_model();
    active.window_5h_start = Set(second_time(0));
    active.usage_5h = Set(0);
    active.update(fixture.pool.connection()).await?;

    fixture.repository.settle(id, quota(12)).await?;
    assert_eq!(window_usages(&fixture).await?, [0, 12, 12]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn outcome_unknown_replay_never_duplicates_window_mutation() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(100), Some(100)],
        [0, 0, 0],
        [second_time(0); 3],
    ))
    .await?;
    let id = reservation_id(50);
    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Precharge);
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(20))
            .await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );
    assert_eq!(allocation_count(&fixture).await?, 1);

    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Settle);
    assert_eq!(
        fixture.repository.settle(id, quota(12)).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        fixture.repository.settle(id, quota(12)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(window_usages(&fixture).await?, [12, 12, 12]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn damaged_active_snapshot_fails_closed() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture(WindowConfig::new(
        [Some(100), Some(100), Some(100)],
        [0, 0, 0],
        [second_time(0); 3],
    ))
    .await?;
    let first = reservation_id(60);
    fixture
        .repository
        .precharge(first, fixture.principal, quota(20))
        .await?;
    let key = BillingReservationKey::parse(&first.persistence_key())?;
    billing_token_window_reservations::Entity::update_many()
        .col_expr(
            billing_token_window_reservations::Column::ReservedQuota,
            Expr::value(19_i64),
        )
        .filter(billing_token_window_reservations::Column::IdempotencyKey.eq(key.clone()))
        .exec(fixture.pool.connection())
        .await?;

    assert_eq!(
        fixture
            .repository
            .precharge(reservation_id(61), fixture.principal, quota(1))
            .await,
        Err(QuotaRepositoryError::Invariant)
    );
    billing_token_window_reservations::Entity::update_many()
        .col_expr(
            billing_token_window_reservations::Column::ReservedQuota,
            Expr::value(20_i64),
        )
        .filter(billing_token_window_reservations::Column::IdempotencyKey.eq(key.clone()))
        .exec(fixture.pool.connection())
        .await?;
    billing_token_window_reservations::Entity::delete_by_id(key)
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture
            .repository
            .precharge(reservation_id(62), fixture.principal, quota(1))
            .await,
        Err(QuotaRepositoryError::Invariant)
    );
    assert_eq!(
        fixture.repository.settle(first, quota(10)).await,
        Err(QuotaRepositoryError::Invariant)
    );
    assert_eq!(reservation_count(&fixture).await?, 1);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn expired_windows_reset_lazily_before_new_precharge() -> Result<(), Box<dyn Error>> {
    let stale = second_time(-8 * 24 * 60 * 60);
    let fixture = new_fixture(WindowConfig::new(
        [Some(5), Some(5), Some(5)],
        [5, 5, 5],
        [stale; 3],
    ))
    .await?;
    let id = reservation_id(70);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(5))
        .await?;
    let current = token(&fixture).await?;
    assert_eq!(
        [current.usage_5h, current.usage_1d, current.usage_7d],
        [0, 0, 0]
    );
    assert!(current.window_5h_start > stale);
    assert!(current.window_1d_start > stale);
    assert!(current.window_7d_start > stale);
    fixture.repository.settle(id, quota(3)).await?;
    assert_eq!(window_usages(&fixture).await?, [3, 3, 3]);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_tasks_never_create_or_consume_token_window_snapshot() -> Result<(), Box<dyn Error>> {
    let fixture = new_fixture(WindowConfig::new(
        [Some(0), Some(0), Some(0)],
        [0, 0, 0],
        [second_time(0); 3],
    ))
    .await?;
    let id = reservation_id(80);
    assert_eq!(
        fixture
            .repository
            .reserve_batch_task(id, fixture.principal, quota(5))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(allocation_count(&fixture).await?, 0);
    fixture.repository.settle_batch_task(id, quota(3)).await?;
    assert_eq!(window_usages(&fixture).await?, [0, 0, 0]);
    fixture.close().await?;
    Ok(())
}

async fn new_fixture(config: WindowConfig) -> Result<Fixture, Box<dyn Error>> {
    fixture_with_options(DatabaseOptions::new("sqlite::memory:")?, config).await
}

async fn fixture_with_options(
    options: DatabaseOptions,
    config: WindowConfig,
) -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let group = groups::ActiveModel {
        name: Set("token-window-group".to_owned()),
        display_name: Set("令牌窗口测试分组".to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("token-window-user".to_owned()),
        status: Set(1),
        default_group_id: Set(group.id),
        quota: Set(TEST_QUOTA),
        used_quota: Set(0),
        frozen_quota: Set(0),
        request_count: Set(0),
        aff_code: Set("token-window-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&"a".repeat(64))?),
        key_prefix: Set("sk-af-window".to_owned()),
        name: Set("token-window-token".to_owned()),
        status: Set(1),
        group_id: Set(Some(group.id)),
        remain_quota: Set(TEST_QUOTA),
        unlimited_quota: Set(true),
        used_quota: Set(0),
        rate_limit_5h: Set(config.limits[0]),
        rate_limit_1d: Set(config.limits[1]),
        rate_limit_7d: Set(config.limits[2]),
        usage_5h: Set(config.usages[0]),
        usage_1d: Set(config.usages[1]),
        usage_7d: Set(config.usages[2]),
        window_5h_start: Set(config.starts[0]),
        window_1d_start: Set(config.starts[1]),
        window_7d_start: Set(config.starts[2]),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let principal = GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    );
    Ok(Fixture {
        repository: QuotaRepository::new(pool.clone()),
        pool,
        principal,
    })
}

async fn token(fixture: &Fixture) -> Result<tokens::Model, sea_orm::DbErr> {
    Ok(
        tokens::Entity::find_by_id(fixture.principal.token_id().get())
            .one(fixture.pool.connection())
            .await?
            .expect("测试令牌必须存在"),
    )
}

async fn user(fixture: &Fixture) -> Result<users::Model, sea_orm::DbErr> {
    Ok(users::Entity::find_by_id(fixture.principal.user_id().get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在"))
}

async fn window_usages(fixture: &Fixture) -> Result<[i64; 3], sea_orm::DbErr> {
    let token = token(fixture).await?;
    Ok([token.usage_5h, token.usage_1d, token.usage_7d])
}

async fn reservation_count(fixture: &Fixture) -> Result<u64, sea_orm::DbErr> {
    billing_reservations::Entity::find()
        .count(fixture.pool.connection())
        .await
}

async fn allocation_count(fixture: &Fixture) -> Result<u64, sea_orm::DbErr> {
    billing_token_window_reservations::Entity::find()
        .count(fixture.pool.connection())
        .await
}

fn second_time(offset_seconds: i64) -> TimeDateTimeWithTimeZone {
    let seconds = TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp()
        .checked_add(offset_seconds)
        .expect("测试时间不得溢出");
    TimeDateTimeWithTimeZone::from_unix_timestamp(seconds).expect("测试时间必须有效")
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).expect("测试预留标识必须有效")
}

fn quota(units: i64) -> Quota {
    Quota::new(units).expect("测试额度必须有效")
}
