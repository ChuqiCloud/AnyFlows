use std::{
    error::Error,
    fs,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use af_domain::{
    BillingReservationId, GatewayPrincipal, GroupId, Quota, SubscriptionCycle, SubscriptionPlanId,
    SubscriptionWindow, TokenId, UserId, UserSubscriptionId,
};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, JsonValue, PaginatorTrait,
    QueryFilter, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, Query},
};
use tokio::{sync::Barrier, task::JoinSet};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, PoolOptions, QuotaRepository,
    QuotaRepositoryError, SubscriptionPlanWrite, SubscriptionRepository, UserSubscriptionBind,
    UserSubscriptionWindowAdvance, UserSubscriptionWindowAdvanceOutcome,
    entity::{
        BillingReservationKey, SensitiveString, TokenHash, billing_reservations,
        billing_subscription_reservations, groups, tokens, user_subscriptions, users,
    },
    quota::{QuotaMutationOutcome, QuotaRepositoryOperation, QuotaReservationStatus},
};

const RESERVATION_TTL: Duration = Duration::from_secs(15 * 60);
const VALID_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-quota-reservation-{}-{serial}.db",
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

#[derive(Clone, Copy)]
struct FixtureConfig {
    user_quota: i64,
    user_used_quota: i64,
    user_frozen_quota: i64,
    user_request_count: i64,
    token_remain_quota: i64,
    token_unlimited_quota: bool,
    token_used_quota: i64,
}

impl Default for FixtureConfig {
    fn default() -> Self {
        Self {
            user_quota: 100,
            user_used_quota: 17,
            user_frozen_quota: 9,
            user_request_count: 3,
            token_remain_quota: 60,
            token_unlimited_quota: false,
            token_used_quota: 11,
        }
    }
}

struct Fixture {
    pool: DatabasePool,
    repository: QuotaRepository,
    principal: GatewayPrincipal,
}

struct TestSubscription {
    id: i64,
    stable_id: UserSubscriptionId,
    window_ends_at: u64,
    repository: SubscriptionRepository,
}

impl Fixture {
    async fn close(self) -> Result<(), crate::DatabaseError> {
        let pool = self.pool.clone();
        drop(self);
        pool.close().await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct AccountSnapshot {
    user_quota: i64,
    user_used_quota: i64,
    user_frozen_quota: i64,
    user_request_count: i64,
    token_remain_quota: i64,
    token_unlimited_quota: bool,
    token_used_quota: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ReservationSnapshot {
    status: i16,
    reserved_quota: i64,
    token_reserved_quota: i64,
    actual_quota: Option<i64>,
    finalized: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SubscriptionBillingSnapshot {
    funding_source: i16,
    subscription_database_id: Option<i64>,
    reserved_quota: Option<i64>,
    subscription_actual_quota: Option<i64>,
}

#[tokio::test]
async fn subscription_settlement_preserves_wallet_and_records_actual_usage()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 200,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let subscription = add_subscription(&fixture, 0xa1, 100, SubscriptionCycle::Daily).await?;
    let id = reservation_id(0xa2);

    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(60))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        AccountSnapshot {
            token_remain_quota: config.token_remain_quota - 60,
            ..initial_snapshot(config)
        }
    );
    assert_eq!(subscription_quota_used(&fixture, subscription.id).await?, 0);
    assert_eq!(
        subscription_billing_snapshot(&fixture.pool, id).await?,
        Some(SubscriptionBillingSnapshot {
            funding_source: 2,
            subscription_database_id: Some(subscription.id),
            reserved_quota: Some(60),
            subscription_actual_quota: None,
        })
    );

    assert_eq!(
        fixture.repository.settle(id, quota(40)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        fixture.repository.settle(id, quota(40)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        AccountSnapshot {
            user_used_quota: config.user_used_quota + 40,
            user_request_count: config.user_request_count + 1,
            token_remain_quota: config.token_remain_quota - 40,
            token_used_quota: config.token_used_quota + 40,
            ..initial_snapshot(config)
        }
    );
    assert_eq!(
        subscription_quota_used(&fixture, subscription.id).await?,
        40
    );
    assert_eq!(
        subscription_billing_snapshot(&fixture.pool, id)
            .await?
            .expect("订阅结算快照必须存在")
            .subscription_actual_quota,
        Some(40)
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn subscription_refund_releases_hold_without_touching_wallet_or_usage()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 200,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let subscription = add_subscription(&fixture, 0xa3, 100, SubscriptionCycle::Daily).await?;
    let id = reservation_id(0xa4);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(60))
        .await?;

    assert_eq!(
        fixture.repository.refund(id).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        fixture.repository.refund(id).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        initial_snapshot(config)
    );
    assert_eq!(subscription_quota_used(&fixture, subscription.id).await?, 0);
    assert_eq!(
        subscription_billing_snapshot(&fixture.pool, id)
            .await?
            .expect("订阅退款快照必须保留审计事实")
            .subscription_actual_quota,
        None
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_task_reservation_uses_wallet_without_subscription() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 200,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let subscription = add_subscription(&fixture, 0xb1, 100, SubscriptionCycle::Daily).await?;
    let id = reservation_id(0xb2);

    assert_eq!(
        fixture
            .repository
            .reserve_batch_task(id, fixture.principal, quota(60))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_precharge(config, 60)
    );
    assert_eq!(subscription_quota_used(&fixture, subscription.id).await?, 0);
    assert_eq!(
        subscription_billing_snapshot(&fixture.pool, id).await?,
        Some(SubscriptionBillingSnapshot {
            funding_source: 1,
            subscription_database_id: None,
            reserved_quota: None,
            subscription_actual_quota: None,
        })
    );
    let key = BillingReservationKey::parse(&id.persistence_key())?;
    let reservation = billing_reservations::Entity::find_by_id(key)
        .one(fixture.pool.connection())
        .await?
        .expect("批量任务预留必须存在");
    assert_eq!(reservation.reservation_kind, 2);

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_task_actual_above_reservation_stays_reserved() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 200,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let id = reservation_id(0xb3);
    fixture
        .repository
        .reserve_batch_task(id, fixture.principal, quota(30))
        .await?;
    let precharged = account_snapshot(&fixture.pool, fixture.principal).await?;

    assert_eq!(
        fixture.repository.settle_batch_task(id, quota(31)).await,
        Err(QuotaRepositoryError::ActualExceedsReservation)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        precharged
    );
    assert_eq!(
        reservation_snapshot(&fixture.pool, id).await?,
        Some(ReservationSnapshot {
            status: status_code(QuotaReservationStatus::Reserved),
            reserved_quota: 30,
            token_reserved_quota: 30,
            actual_quota: None,
            finalized: false,
        })
    );

    assert_eq!(
        fixture.repository.settle_batch_task(id, quota(20)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        fixture.repository.settle_batch_task(id, quota(20)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_task_settlement_releases_unused_hold() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 200,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let id = reservation_id(0xb4);
    fixture
        .repository
        .reserve_batch_task(id, fixture.principal, quota(60))
        .await?;

    assert_eq!(
        fixture.repository.settle_batch_task(id, quota(40)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_settlement(config, 40)
    );
    assert_eq!(
        fixture.repository.settle_batch_task(id, quota(40)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        fixture.repository.release_batch_task(id).await,
        Err(QuotaRepositoryError::Conflict)
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn request_and_batch_task_interfaces_are_isolated() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let batch_id = reservation_id(0xb5);
    fixture
        .repository
        .reserve_batch_task(batch_id, fixture.principal, quota(20))
        .await?;
    let batch_precharged = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(
        fixture.repository.settle(batch_id, quota(10)).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        fixture.repository.refund(batch_id).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        batch_precharged
    );
    assert_eq!(
        fixture
            .repository
            .settle_batch_task(batch_id, quota(10))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        fixture.repository.settle(batch_id, quota(10)).await,
        Err(QuotaRepositoryError::Conflict)
    );

    let request_id = reservation_id(0xb6);
    fixture
        .repository
        .precharge(request_id, fixture.principal, quota(20))
        .await?;
    let request_precharged = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(
        fixture
            .repository
            .settle_batch_task(request_id, quota(10))
            .await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        fixture.repository.release_batch_task(request_id).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        request_precharged
    );
    assert_eq!(
        fixture.repository.settle(request_id, quota(10)).await?,
        QuotaMutationOutcome::Applied
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_task_outcome_unknown_replays_without_duplicate_mutation()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let settled_id = reservation_id(0xb7);

    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Precharge);
    assert_eq!(
        fixture
            .repository
            .reserve_batch_task(settled_id, fixture.principal, quota(20))
            .await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    let precharged = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(precharged, after_precharge(config, 20));
    assert_eq!(
        fixture
            .repository
            .reserve_batch_task(settled_id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );

    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Settle);
    assert_eq!(
        fixture
            .repository
            .settle_batch_task(settled_id, quota(15))
            .await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    let settled = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(settled, after_settlement(config, 15));
    assert_eq!(
        fixture
            .repository
            .settle_batch_task(settled_id, quota(15))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );

    let refunded_id = reservation_id(0xb8);
    fixture
        .repository
        .reserve_batch_task(refunded_id, fixture.principal, quota(20))
        .await?;
    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Refund);
    assert_eq!(
        fixture.repository.release_batch_task(refunded_id).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        fixture.repository.release_batch_task(refunded_id).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        settled
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn subscription_selection_skips_insufficient_candidate_and_falls_back_to_wallet()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 300,
        token_remain_quota: 300,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let daily = add_subscription(&fixture, 0xa5, 20, SubscriptionCycle::Daily).await?;
    let monthly = add_subscription(&fixture, 0xa6, 100, SubscriptionCycle::Monthly).await?;
    // 月末时两个日历窗口都可能在下一个月第一天结束；相等时由订阅主键顺序稳定决策。
    assert!(daily.window_ends_at <= monthly.window_ends_at);

    let subscription_id = reservation_id(0xa7);
    fixture
        .repository
        .precharge(subscription_id, fixture.principal, quota(30))
        .await?;
    assert_eq!(
        subscription_billing_snapshot(&fixture.pool, subscription_id)
            .await?
            .expect("额度足够的后续订阅必须被选中")
            .subscription_database_id,
        Some(monthly.id)
    );

    let wallet_id = reservation_id(0xa8);
    fixture
        .repository
        .precharge(wallet_id, fixture.principal, quota(80))
        .await?;
    assert_eq!(
        subscription_billing_snapshot(&fixture.pool, wallet_id).await?,
        Some(SubscriptionBillingSnapshot {
            funding_source: 1,
            subscription_database_id: None,
            reserved_quota: None,
            subscription_actual_quota: None,
        })
    );
    assert_eq!(subscription_quota_used(&fixture, daily.id).await?, 0);
    assert_eq!(subscription_quota_used(&fixture, monthly.id).await?, 0);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal)
            .await?
            .user_frozen_quota,
        config.user_frozen_quota + 80
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn pending_subscription_overflow_replays_frozen_split_after_wallet_topup()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 10,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let subscription = add_subscription(&fixture, 0xa9, 100, SubscriptionCycle::Daily).await?;
    let id = reservation_id(0xaa);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(60))
        .await?;

    assert_eq!(
        fixture.repository.settle(id, quota(120)).await,
        Err(QuotaRepositoryError::UserQuotaInsufficient)
    );
    assert_pending(&fixture.pool, id, 60, 60, 120).await?;
    assert_eq!(
        subscription_billing_snapshot(&fixture.pool, id)
            .await?
            .expect("待结算订阅分摊必须持久化")
            .subscription_actual_quota,
        Some(100)
    );
    assert_eq!(subscription_quota_used(&fixture, subscription.id).await?, 0);
    update_user(&fixture, |user| user.quota = Set(30)).await?;

    assert_eq!(
        fixture.repository.settle(id, quota(120)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        subscription_quota_used(&fixture, subscription.id).await?,
        100
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        AccountSnapshot {
            user_quota: 10,
            user_used_quota: config.user_used_quota + 120,
            user_frozen_quota: config.user_frozen_quota,
            user_request_count: config.user_request_count + 1,
            token_remain_quota: config.token_remain_quota - 120,
            token_unlimited_quota: false,
            token_used_quota: config.token_used_quota + 120,
        }
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn in_flight_subscription_blocks_window_advance_until_refund() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 200,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let subscription = add_subscription(&fixture, 0xab, 100, SubscriptionCycle::Daily).await?;
    let id = reservation_id(0xac);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(20))
        .await?;
    let advance = current_window_advance(&fixture, subscription.stable_id).await?;

    assert!(matches!(
        subscription
            .repository
            .advance_user_subscription_window(&advance)
            .await?,
        UserSubscriptionWindowAdvanceOutcome::InUse(_)
    ));
    fixture.repository.refund(id).await?;
    assert!(matches!(
        subscription
            .repository
            .advance_user_subscription_window(&advance)
            .await?,
        UserSubscriptionWindowAdvanceOutcome::Applied(_)
    ));

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn finite_token_precharge_records_wallet_and_token_snapshot() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let id = reservation_id(1);

    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(30))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_precharge(config, 30)
    );
    assert_eq!(
        reservation_snapshot(&fixture.pool, id).await?,
        Some(ReservationSnapshot {
            status: status_code(QuotaReservationStatus::Reserved),
            reserved_quota: 30,
            token_reserved_quota: 30,
            actual_quota: None,
            finalized: false,
        })
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn unlimited_token_precharge_preserves_remain_and_records_zero_snapshot()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        token_remain_quota: 7,
        token_unlimited_quota: true,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let id = reservation_id(2);

    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(30))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_precharge(config, 30)
    );
    assert_eq!(
        reservation_snapshot(&fixture.pool, id).await?,
        Some(ReservationSnapshot {
            status: status_code(QuotaReservationStatus::Reserved),
            reserved_quota: 30,
            token_reserved_quota: 0,
            actual_quota: None,
            finalized: false,
        })
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn sequential_replay_with_same_id_only_precharges_once() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let id = reservation_id(3);

    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Applied
    );
    let once = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        once
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 1);

    fixture.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_replay_with_same_id_only_precharges_once() -> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let config = FixtureConfig::default();
    let fixture = concurrent_fixture(&database, config).await?;
    let id = reservation_id(4);
    let results = concurrent_precharges(&fixture, vec![id; 8], quota(10)).await?;

    let mut applied = 0;
    let mut replayed = 0;
    for result in results {
        match result {
            Ok(QuotaMutationOutcome::Applied) => applied += 1,
            Ok(QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)) => replayed += 1,
            outcome => panic!("相同幂等键并发重放返回意外结果：{outcome:?}"),
        }
    }
    assert_eq!(applied, 1);
    assert_eq!(replayed, 7);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_precharge(config, 10)
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 1);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn same_id_with_different_parameters_conflicts_without_mutation() -> Result<(), Box<dyn Error>>
{
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let other = create_account(&fixture.pool, 2, config).await?;
    let id = reservation_id(5);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(20))
        .await?;
    let primary_before = account_snapshot(&fixture.pool, fixture.principal).await?;
    let other_before = account_snapshot(&fixture.pool, other).await?;

    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(21))
            .await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        fixture.repository.precharge(id, other, quota(20)).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        primary_before
    );
    assert_eq!(account_snapshot(&fixture.pool, other).await?, other_before);
    assert_eq!(reservation_count(&fixture.pool).await?, 1);

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn repeated_refund_and_settlement_have_no_extra_side_effects() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let refunded_id = reservation_id(6);
    fixture
        .repository
        .precharge(refunded_id, fixture.principal, quota(20))
        .await?;

    assert_eq!(
        fixture.repository.refund(refunded_id).await?,
        QuotaMutationOutcome::Applied
    );
    let refunded_snapshot = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(refunded_snapshot, initial_snapshot(config));
    assert_eq!(
        fixture.repository.refund(refunded_id).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(
        fixture
            .repository
            .precharge(refunded_id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        refunded_snapshot
    );
    assert_eq!(
        fixture.repository.settle(refunded_id, quota(20)).await,
        Err(QuotaRepositoryError::Conflict)
    );

    let settled_id = reservation_id(7);
    fixture
        .repository
        .precharge(settled_id, fixture.principal, quota(20))
        .await?;
    assert_eq!(
        fixture.repository.settle(settled_id, quota(15)).await?,
        QuotaMutationOutcome::Applied
    );
    let settled_snapshot = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(
        fixture.repository.settle(settled_id, quota(15)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        fixture.repository.settle(settled_id, quota(16)).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .repository
            .precharge(settled_id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        fixture.repository.refund(settled_id).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        settled_snapshot
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_identical_settlements_mutate_balances_once() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let database = TestDatabase::new();
    let fixture = concurrent_fixture(&database, config).await?;
    let id = reservation_id(58);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(30))
        .await?;

    let results = concurrent_settlements(&fixture, id, quota(20), 8).await?;

    assert_terminal_replay_results(results, QuotaReservationStatus::Settled, 8);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_settlement(config, 20)
    );
    assert_eq!(
        reservation_snapshot(&fixture.pool, id).await?,
        Some(ReservationSnapshot {
            status: status_code(QuotaReservationStatus::Settled),
            reserved_quota: 30,
            token_reserved_quota: 30,
            actual_quota: Some(20),
            finalized: true,
        })
    );

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_identical_refunds_restore_balances_once() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let database = TestDatabase::new();
    let fixture = concurrent_fixture(&database, config).await?;
    let id = reservation_id(59);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(30))
        .await?;

    let results = concurrent_refunds(&fixture, id, 8).await?;

    assert_terminal_replay_results(results, QuotaReservationStatus::Refunded, 8);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        initial_snapshot(config)
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 1);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn outcome_unknown_after_commit_replays_without_duplicate_mutation()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let settled_id = reservation_id(55);

    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Precharge);
    assert_eq!(
        fixture
            .repository
            .precharge(settled_id, fixture.principal, quota(20))
            .await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    let precharged = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(precharged, after_precharge(config, 20));
    assert_eq!(
        fixture
            .repository
            .precharge(settled_id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        precharged
    );

    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Settle);
    assert_eq!(
        fixture.repository.settle(settled_id, quota(15)).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    let settled = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(settled, after_settlement(config, 15));
    assert_eq!(
        fixture.repository.settle(settled_id, quota(15)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        settled
    );

    let refunded_id = reservation_id(56);
    fixture
        .repository
        .precharge(refunded_id, fixture.principal, quota(20))
        .await?;
    fixture
        .repository
        .inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Refund);
    assert_eq!(
        fixture.repository.refund(refunded_id).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        fixture.repository.refund(refunded_id).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        settled
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 2);

    fixture.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_operation_timeout_replays_precharge_once_after_lock_release()
-> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let config = FixtureConfig::default();
    let fixture = concurrent_fixture(&database, config).await?;
    let id = reservation_id(57);
    let timeout_repository = QuotaRepository::with_config(
        fixture.pool.clone(),
        Duration::from_millis(50),
        RESERVATION_TTL,
    )?;

    // 持有真实 SQLite 写锁，使预扣等待时间超过仓储操作截止时间。
    let blocker = fixture.pool.connection().begin().await?;
    let lock_statement = Query::update()
        .table(groups::Entity)
        .value(
            groups::Column::RatioMicros,
            Expr::col(groups::Column::RatioMicros),
        )
        .and_where(Expr::col(groups::Column::Id).eq(fixture.principal.group_id().get()))
        .to_owned();
    let lock_result = blocker
        .execute(blocker.get_database_backend().build(&lock_statement))
        .await?;
    assert_eq!(lock_result.rows_affected(), 1);

    assert_eq!(
        timeout_repository
            .precharge(id, fixture.principal, quota(20))
            .await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    blocker.rollback().await?;

    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_precharge(config, 20)
    );
    assert_eq!(
        reservation_snapshot(&fixture.pool, id).await?,
        Some(ReservationSnapshot {
            status: status_code(QuotaReservationStatus::Reserved),
            reserved_quota: 20,
            token_reserved_quota: 20,
            actual_quota: None,
            finalized: false,
        })
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 1);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn settlement_applies_actual_less_equal_and_greater_than_reserved()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    for (marker, actual) in [(8, 0), (9, 20), (10, 30), (11, 40)] {
        let fixture = fixture(config).await?;
        let id = reservation_id(marker);
        fixture
            .repository
            .precharge(id, fixture.principal, quota(30))
            .await?;

        assert_eq!(
            fixture.repository.settle(id, quota(actual)).await?,
            QuotaMutationOutcome::Applied
        );
        assert_eq!(
            account_snapshot(&fixture.pool, fixture.principal).await?,
            after_settlement(config, actual)
        );
        assert_eq!(
            reservation_snapshot(&fixture.pool, id).await?,
            Some(ReservationSnapshot {
                status: status_code(QuotaReservationStatus::Settled),
                reserved_quota: 30,
                token_reserved_quota: 30,
                actual_quota: Some(actual),
                finalized: true,
            })
        );
        fixture.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn user_supplement_failure_stays_pending_and_same_actual_can_retry()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_quota: 30,
        token_remain_quota: 100,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let id = reservation_id(11);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(20))
        .await?;
    let precharged = account_snapshot(&fixture.pool, fixture.principal).await?;

    assert_eq!(
        fixture.repository.settle(id, quota(31)).await,
        Err(QuotaRepositoryError::UserQuotaInsufficient)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        precharged
    );
    assert_pending(&fixture.pool, id, 20, 20, 31).await?;
    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(20))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::SettlementPending)
    );
    assert_eq!(
        fixture.repository.refund(id).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        fixture.repository.settle(id, quota(32)).await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        precharged
    );

    update_user(&fixture, |user| user.quota = Set(50)).await?;
    assert_eq!(
        fixture.repository.settle(id, quota(31)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        AccountSnapshot {
            user_quota: 39,
            user_used_quota: config.user_used_quota + 31,
            user_frozen_quota: config.user_frozen_quota,
            user_request_count: config.user_request_count + 1,
            token_remain_quota: 69,
            token_unlimited_quota: false,
            token_used_quota: config.token_used_quota + 31,
        }
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn token_mode_changes_keep_the_precharge_snapshot() -> Result<(), Box<dyn Error>> {
    let finite = FixtureConfig::default();

    let settled_finite = fixture(finite).await?;
    let settled_finite_id = reservation_id(50);
    settled_finite
        .repository
        .precharge(settled_finite_id, settled_finite.principal, quota(30))
        .await?;
    update_token(&settled_finite, |token| token.unlimited_quota = Set(true)).await?;
    settled_finite
        .repository
        .settle(settled_finite_id, quota(20))
        .await?;
    assert_eq!(
        account_snapshot(&settled_finite.pool, settled_finite.principal).await?,
        AccountSnapshot {
            token_unlimited_quota: true,
            ..after_settlement(finite, 20)
        }
    );
    settled_finite.close().await?;

    let refunded_finite = fixture(finite).await?;
    let refunded_finite_id = reservation_id(51);
    refunded_finite
        .repository
        .precharge(refunded_finite_id, refunded_finite.principal, quota(30))
        .await?;
    update_token(&refunded_finite, |token| token.unlimited_quota = Set(true)).await?;
    refunded_finite
        .repository
        .refund(refunded_finite_id)
        .await?;
    assert_eq!(
        account_snapshot(&refunded_finite.pool, refunded_finite.principal).await?,
        AccountSnapshot {
            token_unlimited_quota: true,
            ..initial_snapshot(finite)
        }
    );
    refunded_finite.close().await?;

    let unlimited = FixtureConfig {
        token_remain_quota: 7,
        token_unlimited_quota: true,
        ..FixtureConfig::default()
    };
    let settled_unlimited = fixture(unlimited).await?;
    let settled_unlimited_id = reservation_id(52);
    settled_unlimited
        .repository
        .precharge(settled_unlimited_id, settled_unlimited.principal, quota(30))
        .await?;
    update_token(&settled_unlimited, |token| {
        token.unlimited_quota = Set(false)
    })
    .await?;
    settled_unlimited
        .repository
        .settle(settled_unlimited_id, quota(20))
        .await?;
    assert_eq!(
        account_snapshot(&settled_unlimited.pool, settled_unlimited.principal).await?,
        AccountSnapshot {
            user_quota: unlimited.user_quota - 20,
            user_used_quota: unlimited.user_used_quota + 20,
            user_frozen_quota: unlimited.user_frozen_quota,
            user_request_count: unlimited.user_request_count + 1,
            token_remain_quota: unlimited.token_remain_quota,
            token_unlimited_quota: false,
            token_used_quota: unlimited.token_used_quota + 20,
        }
    );
    settled_unlimited.close().await?;

    let refunded_unlimited = fixture(unlimited).await?;
    let refunded_unlimited_id = reservation_id(53);
    refunded_unlimited
        .repository
        .precharge(
            refunded_unlimited_id,
            refunded_unlimited.principal,
            quota(30),
        )
        .await?;
    update_token(&refunded_unlimited, |token| {
        token.unlimited_quota = Set(false)
    })
    .await?;
    refunded_unlimited
        .repository
        .refund(refunded_unlimited_id)
        .await?;
    assert_eq!(
        account_snapshot(&refunded_unlimited.pool, refunded_unlimited.principal).await?,
        AccountSnapshot {
            token_unlimited_quota: false,
            ..initial_snapshot(unlimited)
        }
    );
    refunded_unlimited.close().await?;
    Ok(())
}

#[tokio::test]
async fn mismatched_token_owner_rolls_back_the_other_wallet() -> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let other = create_account(&fixture.pool, 2, config).await?;
    let mismatched = GatewayPrincipal::new(
        fixture.principal.token_id(),
        other.user_id(),
        fixture.principal.group_id(),
    );
    let before_primary = account_snapshot(&fixture.pool, fixture.principal).await?;
    let before_other = account_snapshot(&fixture.pool, other).await?;
    let id = reservation_id(54);

    assert_eq!(
        fixture
            .repository
            .precharge(id, mismatched, quota(10))
            .await,
        Err(QuotaRepositoryError::Invariant)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        before_primary
    );
    assert_eq!(account_snapshot(&fixture.pool, other).await?, before_other);
    assert_eq!(reservation_snapshot(&fixture.pool, id).await?, None);

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn token_supplement_failure_stays_pending_and_same_actual_can_retry()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        token_remain_quota: 25,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let id = reservation_id(12);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(20))
        .await?;
    let precharged = account_snapshot(&fixture.pool, fixture.principal).await?;

    assert_eq!(
        fixture.repository.settle(id, quota(26)).await,
        Err(QuotaRepositoryError::TokenQuotaInsufficient)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        precharged
    );
    assert_pending(&fixture.pool, id, 20, 20, 26).await?;
    assert_eq!(
        fixture.repository.settle(id, quota(25)).await,
        Err(QuotaRepositoryError::Conflict)
    );

    update_token(&fixture, |token| token.remain_quota = Set(10)).await?;
    assert_eq!(
        fixture.repository.settle(id, quota(26)).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        AccountSnapshot {
            user_quota: config.user_quota - 26,
            user_used_quota: config.user_used_quota + 26,
            user_frozen_quota: config.user_frozen_quota,
            user_request_count: config.user_request_count + 1,
            token_remain_quota: 4,
            token_unlimited_quota: false,
            token_used_quota: config.token_used_quota + 26,
        }
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn settlement_and_refund_race_reaches_only_one_terminal_state() -> Result<(), Box<dyn Error>>
{
    let database = TestDatabase::new();
    let config = FixtureConfig::default();
    let fixture = concurrent_fixture(&database, config).await?;
    let id = reservation_id(13);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(30))
        .await?;

    let barrier = Arc::new(Barrier::new(3));
    let settle_repository = fixture.repository.clone();
    let settle_barrier = Arc::clone(&barrier);
    let settle = tokio::spawn(async move {
        settle_barrier.wait().await;
        settle_repository.settle(id, quota(20)).await
    });
    let refund_repository = fixture.repository.clone();
    let refund_barrier = Arc::clone(&barrier);
    let refund = tokio::spawn(async move {
        refund_barrier.wait().await;
        refund_repository.refund(id).await
    });
    barrier.wait().await;
    let outcomes = [settle.await?, refund.await?];

    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Ok(QuotaMutationOutcome::Applied)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(QuotaRepositoryError::Conflict)))
            .count(),
        1
    );
    let reservation = reservation_snapshot(&fixture.pool, id)
        .await?
        .expect("竞态结束后预留必须存在");
    match reservation.status {
        status if status == status_code(QuotaReservationStatus::Settled) => {
            assert_eq!(
                account_snapshot(&fixture.pool, fixture.principal).await?,
                after_settlement(config, 20)
            );
        }
        status if status == status_code(QuotaReservationStatus::Refunded) => {
            assert_eq!(
                account_snapshot(&fixture.pool, fixture.principal).await?,
                initial_snapshot(config)
            );
        }
        status => panic!("竞态产生非法终态：{status}"),
    }

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_serializes_different_reservations_for_the_same_subject()
-> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let config = FixtureConfig {
        user_quota: 200,
        token_remain_quota: 200,
        ..FixtureConfig::default()
    };
    let fixture = concurrent_fixture(&database, config).await?;
    let settled_id = reservation_id(14);
    let refunded_id = reservation_id(15);
    fixture
        .repository
        .precharge(settled_id, fixture.principal, quota(50))
        .await?;
    fixture
        .repository
        .precharge(refunded_id, fixture.principal, quota(50))
        .await?;

    let barrier = Arc::new(Barrier::new(3));
    let settle_repository = fixture.repository.clone();
    let settle_barrier = Arc::clone(&barrier);
    let settle = tokio::spawn(async move {
        settle_barrier.wait().await;
        settle_repository.settle(settled_id, quota(70)).await
    });
    let refund_repository = fixture.repository.clone();
    let refund_barrier = Arc::clone(&barrier);
    let refund = tokio::spawn(async move {
        refund_barrier.wait().await;
        refund_repository.refund(refunded_id).await
    });
    barrier.wait().await;

    assert_eq!(settle.await??, QuotaMutationOutcome::Applied);
    assert_eq!(refund.await??, QuotaMutationOutcome::Applied);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        AccountSnapshot {
            user_quota: 130,
            user_used_quota: config.user_used_quota + 70,
            user_frozen_quota: config.user_frozen_quota,
            user_request_count: config.user_request_count + 1,
            token_remain_quota: 130,
            token_unlimited_quota: false,
            token_used_quota: config.token_used_quota + 70,
        }
    );

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_finite_token_competition_never_overdraws() -> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let config = FixtureConfig {
        user_quota: 100,
        token_remain_quota: 50,
        ..FixtureConfig::default()
    };
    let fixture = concurrent_fixture(&database, config).await?;
    let ids = (20..30).map(reservation_id).collect();
    let results = concurrent_precharges(&fixture, ids, quota(10)).await?;
    assert_competition_results(results, QuotaRepositoryError::TokenQuotaInsufficient);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_precharge(config, 50)
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 5);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_user_wallet_competition_never_overdraws() -> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let config = FixtureConfig {
        user_quota: 50,
        token_remain_quota: 7,
        token_unlimited_quota: true,
        ..FixtureConfig::default()
    };
    let fixture = concurrent_fixture(&database, config).await?;
    let ids = (30..40).map(reservation_id).collect();
    let results = concurrent_precharges(&fixture, ids, quota(10)).await?;
    assert_competition_results(results, QuotaRepositoryError::UserQuotaInsufficient);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        after_precharge(config, 50)
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 5);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn precharge_counter_overflow_rolls_back_reservation_and_balances()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig {
        user_frozen_quota: i64::MAX,
        ..FixtureConfig::default()
    };
    let fixture = fixture(config).await?;
    let id = reservation_id(40);
    let before = account_snapshot(&fixture.pool, fixture.principal).await?;

    assert_eq!(
        fixture
            .repository
            .precharge(id, fixture.principal, quota(1))
            .await,
        Err(QuotaRepositoryError::Invariant)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        before
    );
    assert_eq!(reservation_snapshot(&fixture.pool, id).await?, None);

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn damaged_settlement_counters_roll_back_state_and_all_balances() -> Result<(), Box<dyn Error>>
{
    for (marker, damage) in [
        (41, CounterDamage::UserUsedQuota),
        (42, CounterDamage::RequestCount),
        (43, CounterDamage::FrozenQuota),
        (44, CounterDamage::TokenUsedQuota),
    ] {
        let fixture = fixture(FixtureConfig::default()).await?;
        let id = reservation_id(marker);
        fixture
            .repository
            .precharge(id, fixture.principal, quota(10))
            .await?;
        match damage {
            CounterDamage::UserUsedQuota => {
                update_user(&fixture, |user| user.used_quota = Set(i64::MAX)).await?;
            }
            CounterDamage::RequestCount => {
                update_user(&fixture, |user| user.request_count = Set(i64::MAX)).await?;
            }
            CounterDamage::FrozenQuota => {
                update_user(&fixture, |user| user.frozen_quota = Set(5)).await?;
            }
            CounterDamage::TokenUsedQuota => {
                update_token(&fixture, |token| token.used_quota = Set(i64::MAX)).await?;
            }
        }
        let before = account_snapshot(&fixture.pool, fixture.principal).await?;

        assert_eq!(
            fixture.repository.settle(id, quota(5)).await,
            Err(QuotaRepositoryError::Invariant),
            "损坏计数器必须闭合为持久化不变量错误"
        );
        assert_eq!(
            account_snapshot(&fixture.pool, fixture.principal).await?,
            before
        );
        assert_eq!(
            reservation_snapshot(&fixture.pool, id).await?,
            Some(ReservationSnapshot {
                status: status_code(QuotaReservationStatus::Reserved),
                reserved_quota: 10,
                token_reserved_quota: 10,
                actual_quota: None,
                finalized: false,
            })
        );
        fixture.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn token_remain_overflow_during_refund_rolls_back_user_credit() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(FixtureConfig::default()).await?;
    let id = reservation_id(45);
    fixture
        .repository
        .precharge(id, fixture.principal, quota(10))
        .await?;
    update_token(&fixture, |token| token.remain_quota = Set(i64::MAX)).await?;
    let before = account_snapshot(&fixture.pool, fixture.principal).await?;

    assert_eq!(
        fixture.repository.refund(id).await,
        Err(QuotaRepositoryError::Invariant)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        before
    );
    assert_eq!(
        reservation_snapshot(&fixture.pool, id).await?,
        Some(ReservationSnapshot {
            status: status_code(QuotaReservationStatus::Reserved),
            reserved_quota: 10,
            token_reserved_quota: 10,
            actual_quota: None,
            finalized: false,
        })
    );

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn insufficient_initial_balances_leave_no_reservation() -> Result<(), Box<dyn Error>> {
    for (marker, config, expected) in [
        (
            46,
            FixtureConfig {
                user_quota: 9,
                token_remain_quota: 100,
                ..FixtureConfig::default()
            },
            QuotaRepositoryError::UserQuotaInsufficient,
        ),
        (
            47,
            FixtureConfig {
                user_quota: 100,
                token_remain_quota: 9,
                ..FixtureConfig::default()
            },
            QuotaRepositoryError::TokenQuotaInsufficient,
        ),
    ] {
        let fixture = fixture(config).await?;
        let id = reservation_id(marker);
        let before = account_snapshot(&fixture.pool, fixture.principal).await?;
        assert_eq!(
            fixture
                .repository
                .precharge(id, fixture.principal, quota(10))
                .await,
            Err(expected)
        );
        assert_eq!(
            account_snapshot(&fixture.pool, fixture.principal).await?,
            before
        );
        assert_eq!(reservation_snapshot(&fixture.pool, id).await?, None);
        fixture.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn not_found_zero_amount_and_configuration_boundaries_are_closed()
-> Result<(), Box<dyn Error>> {
    let config = FixtureConfig::default();
    let fixture = fixture(config).await?;
    let missing = reservation_id(48);
    let before = account_snapshot(&fixture.pool, fixture.principal).await?;

    assert_eq!(
        fixture.repository.settle(missing, quota(1)).await,
        Err(QuotaRepositoryError::NotFound)
    );
    assert_eq!(
        fixture.repository.refund(missing).await,
        Err(QuotaRepositoryError::NotFound)
    );
    assert_eq!(
        fixture
            .repository
            .precharge(missing, fixture.principal, Quota::ZERO)
            .await,
        Err(QuotaRepositoryError::ZeroAmount)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        before
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 0);

    for result in [
        QuotaRepository::with_config(fixture.pool.clone(), Duration::ZERO, RESERVATION_TTL),
        QuotaRepository::with_config(
            fixture.pool.clone(),
            VALID_OPERATION_TIMEOUT,
            Duration::from_millis(999),
        ),
        QuotaRepository::with_config(
            fixture.pool.clone(),
            VALID_OPERATION_TIMEOUT,
            Duration::from_secs(i64::MAX as u64 + 1),
        ),
    ] {
        assert!(matches!(
            result,
            Err(QuotaRepositoryError::InvalidConfiguration)
        ));
    }
    assert!(
        QuotaRepository::with_config(
            fixture.pool.clone(),
            VALID_OPERATION_TIMEOUT,
            Duration::from_secs(1),
        )
        .is_ok()
    );

    fixture.close().await?;
    Ok(())
}

#[derive(Clone, Copy)]
enum CounterDamage {
    UserUsedQuota,
    RequestCount,
    FrozenQuota,
    TokenUsedQuota,
}

async fn assert_pending(
    pool: &DatabasePool,
    id: BillingReservationId,
    reserved: i64,
    token_reserved: i64,
    actual: i64,
) -> Result<(), sea_orm::DbErr> {
    assert_eq!(
        reservation_snapshot(pool, id).await?,
        Some(ReservationSnapshot {
            status: status_code(QuotaReservationStatus::SettlementPending),
            reserved_quota: reserved,
            token_reserved_quota: token_reserved,
            actual_quota: Some(actual),
            finalized: false,
        })
    );
    Ok(())
}

fn assert_competition_results(
    results: Vec<Result<QuotaMutationOutcome, QuotaRepositoryError>>,
    rejection: QuotaRepositoryError,
) {
    let mut applied = 0;
    let mut rejected = 0;
    for result in results {
        match result {
            Ok(QuotaMutationOutcome::Applied) => applied += 1,
            Err(error) if error == rejection => rejected += 1,
            outcome => panic!("并发额度竞争返回意外结果：{outcome:?}"),
        }
    }
    assert_eq!(applied, 5);
    assert_eq!(rejected, 5);
}

async fn concurrent_precharges(
    fixture: &Fixture,
    ids: Vec<BillingReservationId>,
    amount: Quota,
) -> Result<Vec<Result<QuotaMutationOutcome, QuotaRepositoryError>>, tokio::task::JoinError> {
    let barrier = Arc::new(Barrier::new(ids.len() + 1));
    let mut tasks = JoinSet::new();
    for id in ids {
        let repository = fixture.repository.clone();
        let principal = fixture.principal;
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository.precharge(id, principal, amount).await
        });
    }
    barrier.wait().await;

    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result?);
    }
    Ok(results)
}

async fn concurrent_settlements(
    fixture: &Fixture,
    id: BillingReservationId,
    actual: Quota,
    attempts: usize,
) -> Result<Vec<Result<QuotaMutationOutcome, QuotaRepositoryError>>, tokio::task::JoinError> {
    let barrier = Arc::new(Barrier::new(attempts + 1));
    let mut tasks = JoinSet::new();
    for _ in 0..attempts {
        let repository = fixture.repository.clone();
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository.settle(id, actual).await
        });
    }
    barrier.wait().await;

    let mut results = Vec::with_capacity(attempts);
    while let Some(result) = tasks.join_next().await {
        results.push(result?);
    }
    Ok(results)
}

async fn concurrent_refunds(
    fixture: &Fixture,
    id: BillingReservationId,
    attempts: usize,
) -> Result<Vec<Result<QuotaMutationOutcome, QuotaRepositoryError>>, tokio::task::JoinError> {
    let barrier = Arc::new(Barrier::new(attempts + 1));
    let mut tasks = JoinSet::new();
    for _ in 0..attempts {
        let repository = fixture.repository.clone();
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository.refund(id).await
        });
    }
    barrier.wait().await;

    let mut results = Vec::with_capacity(attempts);
    while let Some(result) = tasks.join_next().await {
        results.push(result?);
    }
    Ok(results)
}

fn assert_terminal_replay_results(
    results: Vec<Result<QuotaMutationOutcome, QuotaRepositoryError>>,
    status: QuotaReservationStatus,
    attempts: usize,
) {
    let mut applied = 0;
    let mut replayed = 0;
    for result in results {
        match result {
            Ok(QuotaMutationOutcome::Applied) => applied += 1,
            Ok(QuotaMutationOutcome::Existing(existing)) if existing == status => replayed += 1,
            outcome => panic!("并发终态重放返回意外结果：{outcome:?}"),
        }
    }
    assert_eq!(applied, 1);
    assert_eq!(replayed, attempts - 1);
}

fn quota(units: i64) -> Quota {
    Quota::new(units).expect("测试额度必须有效")
}

fn reservation_id(marker: u8) -> BillingReservationId {
    BillingReservationId::new([marker; 16]).expect("测试幂等标识必须非零")
}

fn status_code(status: QuotaReservationStatus) -> i16 {
    status as i16
}

fn initial_snapshot(config: FixtureConfig) -> AccountSnapshot {
    AccountSnapshot {
        user_quota: config.user_quota,
        user_used_quota: config.user_used_quota,
        user_frozen_quota: config.user_frozen_quota,
        user_request_count: config.user_request_count,
        token_remain_quota: config.token_remain_quota,
        token_unlimited_quota: config.token_unlimited_quota,
        token_used_quota: config.token_used_quota,
    }
}

fn after_precharge(config: FixtureConfig, reserved: i64) -> AccountSnapshot {
    AccountSnapshot {
        user_quota: config.user_quota - reserved,
        user_frozen_quota: config.user_frozen_quota + reserved,
        token_remain_quota: if config.token_unlimited_quota {
            config.token_remain_quota
        } else {
            config.token_remain_quota - reserved
        },
        ..initial_snapshot(config)
    }
}

fn after_settlement(config: FixtureConfig, actual: i64) -> AccountSnapshot {
    AccountSnapshot {
        user_quota: config.user_quota - actual,
        user_used_quota: config.user_used_quota + actual,
        user_frozen_quota: config.user_frozen_quota,
        user_request_count: config.user_request_count + 1,
        token_remain_quota: if config.token_unlimited_quota {
            config.token_remain_quota
        } else {
            config.token_remain_quota - actual
        },
        token_unlimited_quota: config.token_unlimited_quota,
        token_used_quota: config.token_used_quota + actual,
    }
}

async fn fixture(config: FixtureConfig) -> Result<Fixture, Box<dyn Error>> {
    fixture_with_options(DatabaseOptions::new("sqlite::memory:")?, config).await
}

async fn concurrent_fixture(
    database: &TestDatabase,
    config: FixtureConfig,
) -> Result<Fixture, Box<dyn Error>> {
    let options = DatabaseOptions::new(database.url.clone())?.with_pool_options(PoolOptions {
        max_connections: 8,
        min_connections: 1,
        acquire_timeout: Duration::from_secs(15),
        ..PoolOptions::default()
    });
    fixture_with_options(options, config).await
}

async fn fixture_with_options(
    options: DatabaseOptions,
    config: FixtureConfig,
) -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let principal = create_account(&pool, 1, config).await?;
    Ok(Fixture {
        repository: QuotaRepository::new(pool.clone()),
        pool,
        principal,
    })
}

async fn add_subscription(
    fixture: &Fixture,
    marker: u8,
    quota_amount: i64,
    cycle: SubscriptionCycle,
) -> Result<TestSubscription, Box<dyn Error>> {
    let repository = SubscriptionRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let now = u64::try_from(TimeDateTimeWithTimeZone::now_utc().unix_timestamp())?;
    let plan_id = SubscriptionPlanId::new([marker; 16])?;
    let stable_id = UserSubscriptionId::new([marker; 16])?;
    let window = SubscriptionWindow::initial(cycle, now)?;
    let plan = SubscriptionPlanWrite::new(
        plan_id,
        format!("额度订阅测试计划 {marker}"),
        fixture.principal.user_id(),
        quota(quota_amount),
        cycle,
        now,
    )?;
    repository.create_plan(&plan).await?;
    let bind = UserSubscriptionBind::new(
        stable_id,
        fixture.principal.user_id(),
        plan_id,
        window.started_at(),
        window.ends_at(),
        now,
    )?;
    repository.bind_user(&bind).await?;
    let row = user_subscriptions::Entity::find()
        .filter(
            user_subscriptions::Column::SubscriptionKey
                .eq(SensitiveString::from(stable_id.persistence_key())),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户订阅必须存在");
    Ok(TestSubscription {
        id: row.id,
        stable_id,
        window_ends_at: window.ends_at(),
        repository,
    })
}

async fn current_window_advance(
    fixture: &Fixture,
    subscription_id: UserSubscriptionId,
) -> Result<UserSubscriptionWindowAdvance, Box<dyn Error>> {
    let repository = SubscriptionRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let page = repository
        .list_user_subscriptions(fixture.principal.user_id(), None, 100)
        .await?;
    let record = page
        .subscriptions()
        .iter()
        .find(|record| record.subscription_id() == subscription_id)
        .expect("测试用户订阅必须可读");
    Ok(UserSubscriptionWindowAdvance::from_record(
        record,
        record.window_ends_at(),
    )?)
}

async fn create_account(
    pool: &DatabasePool,
    marker: u8,
    config: FixtureConfig,
) -> Result<GatewayPrincipal, Box<dyn Error>> {
    let group = groups::ActiveModel {
        name: Set(format!("quota-group-{marker}")),
        display_name: Set(format!("额度测试分组 {marker}")),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set(format!("quota-user-{marker}")),
        status: Set(1),
        default_group_id: Set(group.id),
        quota: Set(config.user_quota),
        used_quota: Set(config.user_used_quota),
        frozen_quota: Set(config.user_frozen_quota),
        request_count: Set(config.user_request_count),
        aff_code: Set(format!("quota-aff-{marker}")),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let hash = format!("{marker:064x}");
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(&hash).expect("测试令牌摘要必须有效")),
        key_prefix: Set(format!("sk-af-{marker}")),
        name: Set(format!("quota-token-{marker}")),
        status: Set(1),
        group_id: Set(Some(group.id)),
        remain_quota: Set(config.token_remain_quota),
        unlimited_quota: Set(config.token_unlimited_quota),
        used_quota: Set(config.token_used_quota),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(GatewayPrincipal::new(
        TokenId::new(token.id)?,
        UserId::new(user.id)?,
        GroupId::new(group.id)?,
    ))
}

async fn account_snapshot(
    pool: &DatabasePool,
    principal: GatewayPrincipal,
) -> Result<AccountSnapshot, sea_orm::DbErr> {
    let user = users::Entity::find_by_id(principal.user_id().get())
        .one(pool.connection())
        .await?
        .expect("测试用户必须存在");
    let token = tokens::Entity::find_by_id(principal.token_id().get())
        .one(pool.connection())
        .await?
        .expect("测试令牌必须存在");
    Ok(AccountSnapshot {
        user_quota: user.quota,
        user_used_quota: user.used_quota,
        user_frozen_quota: user.frozen_quota,
        user_request_count: user.request_count,
        token_remain_quota: token.remain_quota,
        token_unlimited_quota: token.unlimited_quota,
        token_used_quota: token.used_quota,
    })
}

async fn reservation_snapshot(
    pool: &DatabasePool,
    id: BillingReservationId,
) -> Result<Option<ReservationSnapshot>, sea_orm::DbErr> {
    let key = BillingReservationKey::parse(&id.persistence_key()).expect("测试幂等键必须有效");
    Ok(billing_reservations::Entity::find_by_id(key)
        .one(pool.connection())
        .await?
        .map(|reservation| ReservationSnapshot {
            status: reservation.status,
            reserved_quota: reservation.reserved_quota,
            token_reserved_quota: reservation.token_reserved_quota,
            actual_quota: reservation.actual_quota,
            finalized: reservation.finalized_at.is_some(),
        }))
}

async fn subscription_billing_snapshot(
    pool: &DatabasePool,
    id: BillingReservationId,
) -> Result<Option<SubscriptionBillingSnapshot>, sea_orm::DbErr> {
    let key = BillingReservationKey::parse(&id.persistence_key()).expect("测试幂等键必须有效");
    let Some(parent) = billing_reservations::Entity::find_by_id(key.clone())
        .one(pool.connection())
        .await?
    else {
        return Ok(None);
    };
    let allocation = billing_subscription_reservations::Entity::find_by_id(key)
        .one(pool.connection())
        .await?;
    Ok(Some(SubscriptionBillingSnapshot {
        funding_source: parent.funding_source,
        subscription_database_id: allocation.as_ref().map(|row| row.user_subscription_id),
        reserved_quota: allocation.as_ref().map(|row| row.reserved_quota),
        subscription_actual_quota: allocation.and_then(|row| row.subscription_actual_quota),
    }))
}

async fn subscription_quota_used(
    fixture: &Fixture,
    subscription_database_id: i64,
) -> Result<i64, sea_orm::DbErr> {
    Ok(
        user_subscriptions::Entity::find_by_id(subscription_database_id)
            .one(fixture.pool.connection())
            .await?
            .expect("测试用户订阅必须存在")
            .quota_used,
    )
}

async fn reservation_count(pool: &DatabasePool) -> Result<u64, sea_orm::DbErr> {
    billing_reservations::Entity::find()
        .count(pool.connection())
        .await
}

async fn update_user<F>(fixture: &Fixture, update: F) -> Result<(), sea_orm::DbErr>
where
    F: FnOnce(&mut users::ActiveModel),
{
    let mut user = users::Entity::find_by_id(fixture.principal.user_id().get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试用户必须存在")
        .into_active_model();
    update(&mut user);
    user.update(fixture.pool.connection()).await?;
    Ok(())
}

async fn update_token<F>(fixture: &Fixture, update: F) -> Result<(), sea_orm::DbErr>
where
    F: FnOnce(&mut tokens::ActiveModel),
{
    let mut token = tokens::Entity::find_by_id(fixture.principal.token_id().get())
        .one(fixture.pool.connection())
        .await?
        .expect("测试令牌必须存在")
        .into_active_model();
    update(&mut token);
    token.update(fixture.pool.connection()).await?;
    Ok(())
}
