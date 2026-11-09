use super::*;
use crate::{MerchantFinanceError, MerchantFinanceRepository};
use af_domain::MerchantBillingSnapshot;
use sea_orm::sea_query::Alias;

pub(super) fn config() -> FixtureConfig {
    FixtureConfig {
        user_quota: 10_000,
        user_used_quota: 0,
        user_frozen_quota: 0,
        token_remain_quota: 10_000,
        ..FixtureConfig::default()
    }
}

pub(super) async fn merchant(
    fixture: &Fixture,
    cash: i64,
    delay: u32,
) -> Result<GatewayPrincipal, Box<dyn Error>> {
    let connection = fixture.pool.connection();
    let backend = connection.get_database_backend();
    let profile = Query::insert()
        .into_table(Alias::new("merchant_profiles"))
        .columns(["id", "owner_user_id", "name", "description", "status"].map(Alias::new))
        .values_panic([
            77_i64.into(),
            fixture.principal.user_id().get().into(),
            "Merchant".into(),
            "".into(),
            "approved".into(),
        ])
        .to_owned();
    connection.execute(backend.build(&profile)).await?;
    let pool = Query::insert()
        .into_table(Alias::new("merchant_pools"))
        .columns(
            [
                "id",
                "group_id",
                "name",
                "description",
                "enabled",
                "commission_bps",
                "min_ratio_micros",
                "max_ratio_micros",
                "settlement_delay_secs",
            ]
            .map(Alias::new),
        )
        .values_panic([
            1_i64.into(),
            fixture.principal.group_id().get().into(),
            "Finance pool".into(),
            "".into(),
            true.into(),
            500_i32.into(),
            1_i64.into(),
            100_000_000_i64.into(),
            i64::from(delay).into(),
        ])
        .to_owned();
    connection.execute(backend.build(&pool)).await?;
    let service = Query::insert()
        .into_table(Alias::new("merchant_services"))
        .columns(
            [
                "id",
                "merchant_id",
                "pool_id",
                "name",
                "description",
                "status",
            ]
            .map(Alias::new),
        )
        .values_panic([
            1_i64.into(),
            77_i64.into(),
            1_i64.into(),
            "Finance service".into(),
            "".into(),
            "published".into(),
        ])
        .to_owned();
    connection.execute(backend.build(&service)).await?;
    if cash > 0 {
        let order = Query::insert()
            .into_table(Alias::new("topup_orders"))
            .columns(
                [
                    "order_key",
                    "user_id",
                    "provider",
                    "status",
                    "amount_minor",
                    "currency",
                    "quota_amount",
                    "idempotency_key",
                    "version",
                    "paid_at",
                ]
                .map(Alias::new),
            )
            .values_panic([
                "11111111111111111111111111111111".into(),
                fixture.principal.user_id().get().into(),
                "stripe".into(),
                3_i16.into(),
                100_i64.into(),
                "USD".into(),
                cash.into(),
                "22222222222222222222222222222222".into(),
                1_i64.into(),
                TimeDateTimeWithTimeZone::now_utc().into(),
            ])
            .to_owned();
        connection.execute(backend.build(&order)).await?;
        let credit = Query::insert()
            .into_table(Alias::new("wallet_ledger_entries"))
            .columns(
                [
                    "event_key",
                    "user_id",
                    "entry_type",
                    "quota_delta",
                    "balance_before",
                    "balance_after",
                ]
                .map(Alias::new),
            )
            .values_panic([
                "33333333333333333333333333333333".into(),
                fixture.principal.user_id().get().into(),
                3_i16.into(),
                cash.into(),
                0_i64.into(),
                cash.into(),
            ])
            .to_owned();
        connection.execute(backend.build(&credit)).await?;
    }
    Ok(fixture.principal.with_merchant_target(
        MerchantBillingSnapshot::new(1, 77, 1, 1, 800_000, 500, delay)?,
        af_domain::ChannelId::new(1)?,
    ))
}

#[tokio::test]
async fn promotional_wallet_credit_cannot_fund_merchant_proceeds() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let principal = merchant(&fixture, 0, 0).await?;
    let before = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(
        fixture
            .repository
            .precharge(reservation_id(0xd0), principal, quota(100))
            .await,
        Err(QuotaRepositoryError::MerchantFundingInsufficient)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        before
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 0);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn deleted_supply_rejects_a_previously_selected_billing_snapshot()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let principal = merchant(&fixture, 1000, 0).await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE merchant_services SET deleted_at=CURRENT_TIMESTAMP WHERE id=1")
        .await?;
    assert_eq!(
        fixture
            .repository
            .precharge(reservation_id(0xe9), principal, quota(100))
            .await,
        Err(QuotaRepositoryError::Conflict)
    );
    assert_eq!(reservation_count(&fixture.pool).await?, 0);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn cash_backing_is_reserved_once_and_refund_releases_it() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let principal = merchant(&fixture, 1_000, 0).await?;
    let first = reservation_id(0xd1);
    let second = reservation_id(0xd2);
    fixture
        .repository
        .precharge(first, principal, quota(900))
        .await?;
    assert_eq!(
        fixture
            .repository
            .precharge(second, principal, quota(200))
            .await,
        Err(QuotaRepositoryError::MerchantFundingInsufficient)
    );
    fixture.repository.refund(first).await?;
    fixture
        .repository
        .precharge(second, principal, quota(200))
        .await?;
    fixture.repository.settle(second, quota(100)).await?;
    let finance = MerchantFinanceRepository::new(fixture.pool.clone(), VALID_OPERATION_TIMEOUT);
    let balance = finance.balance(77).await?;
    assert_eq!(balance.available_quota, 95);
    assert_eq!(balance.total_gross_quota, 100);
    assert_eq!(balance.total_commission_quota, 5);
    assert_eq!(
        fixture.repository.settle(second, quota(100)).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(finance.balance(77).await?.available_quota, 95);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn payouts_and_sale_reversal_are_idempotent_and_preserve_debt() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(config()).await?;
    let principal = merchant(&fixture, 1_000, 0).await?;
    let id = reservation_id(0xd3);
    fixture
        .repository
        .precharge(id, principal, quota(200))
        .await?;
    fixture.repository.settle(id, quota(100)).await?;
    let finance = MerchantFinanceRepository::new(fixture.pool.clone(), VALID_OPERATION_TIMEOUT);
    let event = "44444444444444444444444444444444";
    let request = finance.request_withdrawal(77, event, 95).await?;
    assert_eq!(
        finance.request_withdrawal(77, event, 95).await?.id,
        request.id
    );
    assert_eq!(
        finance.request_withdrawal(77, event, 94).await,
        Err(MerchantFinanceError::Conflict)
    );
    finance
        .decide_withdrawal(
            request.id,
            fixture.principal.user_id(),
            "paid",
            "bank-payment-1",
            None,
        )
        .await?;
    finance
        .decide_withdrawal(
            request.id,
            fixture.principal.user_id(),
            "paid",
            "bank-payment-1",
            None,
        )
        .await?;
    let before = account_snapshot(&fixture.pool, fixture.principal).await?;
    finance
        .reverse_settlement(
            &id.persistence_key(),
            fixture.principal.user_id(),
            "Approved dispute",
        )
        .await?;
    finance
        .reverse_settlement(
            &id.persistence_key(),
            fixture.principal.user_id(),
            "Approved dispute",
        )
        .await?;
    let after = account_snapshot(&fixture.pool, fixture.principal).await?;
    assert_eq!(after.user_quota, before.user_quota + 100);
    let balance = finance.balance(77).await?;
    assert_eq!(balance.debt_quota, 95);
    assert_eq!(balance.available_quota, 0);
    assert_eq!(balance.total_paid_quota, 95);
    assert_eq!(balance.total_gross_quota, 0);
    assert_eq!(balance.total_commission_quota, 0);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_snapshot_cannot_be_changed_on_precharge_replay() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let principal = merchant(&fixture, 1_000, 86_400).await?;
    let id = reservation_id(0xd4);
    fixture
        .repository
        .precharge(id, principal, quota(100))
        .await?;
    let altered = principal.with_merchant_target(
        MerchantBillingSnapshot::new(1, 77, 1, 1, 800_000, 600, 86_400)?,
        af_domain::ChannelId::new(1)?,
    );
    assert_eq!(
        fixture.repository.precharge(id, altered, quota(100)).await,
        Err(QuotaRepositoryError::Conflict)
    );
    fixture.repository.settle(id, quota(100)).await?;
    let finance = MerchantFinanceRepository::new(fixture.pool.clone(), VALID_OPERATION_TIMEOUT);
    let balance = finance.balance(77).await?;
    assert_eq!(balance.pending_quota, 95);
    assert_eq!(balance.available_quota, 0);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_cannot_turn_oversized_usage_into_additional_wallet_debits()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let principal = merchant(&fixture, 1_000, 0).await?;
    let before = account_snapshot(&fixture.pool, fixture.principal).await?;
    let id = reservation_id(0xd5);
    fixture
        .repository
        .precharge(id, principal, quota(100))
        .await?;
    assert_eq!(
        fixture.repository.settle(id, quota(101)).await,
        Err(QuotaRepositoryError::MerchantUsageExceedsAuthorization)
    );
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal).await?,
        before
    );
    assert_eq!(
        reservation_snapshot(&fixture.pool, id)
            .await?
            .unwrap()
            .status,
        status_code(QuotaReservationStatus::Refunded)
    );
    let query = Query::select()
        .columns([Alias::new("status"), Alias::new("reported_quota")])
        .from(Alias::new("merchant_billing_reservations"))
        .and_where(Expr::col(Alias::new("reservation_key")).eq(id.persistence_key()))
        .to_owned();
    let row = fixture
        .pool
        .connection()
        .query_one(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&query),
        )
        .await?
        .unwrap();
    assert_eq!(row.try_get::<String>("", "status")?, "usage_rejected");
    assert_eq!(row.try_get::<i64>("", "reported_quota")?, 101);
    let finance = MerchantFinanceRepository::new(fixture.pool.clone(), VALID_OPERATION_TIMEOUT);
    assert_eq!(finance.balance(77).await?.total_gross_quota, 0);
    fixture.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_merchant_requests_cannot_reserve_the_same_cash_twice()
-> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let fixture = concurrent_fixture(&database, config()).await?;
    let principal = merchant(&fixture, 1_000, 0).await?;
    let barrier = Arc::new(Barrier::new(3));
    let first_repository = fixture.repository.clone();
    let first_barrier = Arc::clone(&barrier);
    let first = tokio::spawn(async move {
        first_barrier.wait().await;
        first_repository
            .precharge(reservation_id(0xd6), principal, quota(600))
            .await
    });
    let second_repository = fixture.repository.clone();
    let second_barrier = Arc::clone(&barrier);
    let second = tokio::spawn(async move {
        second_barrier.wait().await;
        second_repository
            .precharge(reservation_id(0xd7), principal, quota(600))
            .await
    });
    barrier.wait().await;
    let (left, right) = (first.await?, second.await?);
    assert!(matches!(
        (&left, &right),
        (
            Ok(QuotaMutationOutcome::Applied),
            Err(QuotaRepositoryError::MerchantFundingInsufficient)
        ) | (
            Err(QuotaRepositoryError::MerchantFundingInsufficient),
            Ok(QuotaMutationOutcome::Applied)
        )
    ));
    assert_eq!(reservation_count(&fixture.pool).await?, 1);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal)
            .await?
            .user_frozen_quota,
        600
    );
    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn already_cash_backed_merchant_hold_survives_parallel_gift_spending()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let principal = merchant(&fixture, 1_000, 0).await?;
    let merchant_id = reservation_id(0xd8);
    let ordinary_id = reservation_id(0xd9);
    fixture
        .repository
        .precharge(merchant_id, principal, quota(1_000))
        .await?;
    // The remaining wallet is promotional credit. An ordinary request may spend it
    // while the merchant request is still running, but not the frozen cash backing.
    fixture
        .repository
        .precharge(ordinary_id, fixture.principal, quota(1_000))
        .await?;
    fixture.repository.settle(ordinary_id, quota(1_000)).await?;
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal)
            .await?
            .user_frozen_quota,
        1_000
    );
    assert_eq!(
        fixture.repository.settle(merchant_id, quota(1_000)).await?,
        QuotaMutationOutcome::Applied
    );
    let finance = MerchantFinanceRepository::new(fixture.pool.clone(), VALID_OPERATION_TIMEOUT);
    let balance = finance.balance(77).await?;
    assert_eq!(balance.total_gross_quota, 1_000);
    assert_eq!(balance.total_commission_quota, 50);
    assert_eq!(balance.available_quota, 950);
    assert_eq!(
        account_snapshot(&fixture.pool, fixture.principal)
            .await?
            .user_frozen_quota,
        0
    );
    // A second merchant request cannot spend the original cash hold again.
    assert_eq!(
        fixture
            .repository
            .precharge(reservation_id(0xda), principal, quota(1))
            .await,
        Err(QuotaRepositoryError::MerchantFundingInsufficient)
    );
    fixture.close().await?;
    Ok(())
}
