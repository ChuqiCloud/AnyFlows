use super::merchant_finance_tests::{config, merchant};
use super::*;
use crate::{
    MerchantFinanceError, MerchantFinanceRepository, MerchantPayoutConfig, MerchantPayoutRepository,
};

async fn setup(
    fixture: &Fixture,
) -> Result<(MerchantFinanceRepository, MerchantPayoutRepository), Box<dyn Error>> {
    let principal = merchant(fixture, 1_000, 0).await?;
    fixture
        .repository
        .precharge(reservation_id(0xe0), principal, quota(1_000))
        .await?;
    fixture
        .repository
        .settle(reservation_id(0xe0), quota(1_000))
        .await?;
    let finance = MerchantFinanceRepository::new(fixture.pool.clone(), VALID_OPERATION_TIMEOUT);
    assert_eq!(finance.balance(77).await?.available_quota, 950);
    let payouts = MerchantPayoutRepository::new(fixture.pool.clone(), VALID_OPERATION_TIMEOUT);
    payouts
        .save_settings(
            0,
            MerchantPayoutConfig {
                enabled: true,
                auto_approve_enabled: true,
                gateway_url: "https://gateway.example".to_owned(),
                currency: "USD".to_owned(),
                quota_per_unit: 100,
                min_amount_minor: 1,
                max_amount_minor: 500,
                daily_limit_minor: 500,
            },
            Some("encrypted-key-v1".to_owned()),
            false,
        )
        .await?;
    payouts
        .save_account(
            77,
            0,
            "Bank ending 1234".to_owned(),
            Some("encrypted-account-v1".to_owned()),
        )
        .await?;
    Ok((finance, payouts))
}
async fn due(fixture: &Fixture, id: i64) -> Result<(), Box<dyn Error>> {
    let query = Query::update()
        .table(sea_orm::sea_query::Alias::new("merchant_payouts"))
        .value(sea_orm::sea_query::Alias::new("next_at"), 0_i64)
        .value(sea_orm::sea_query::Alias::new("lease_until"), 0_i64)
        .and_where(Expr::col(sea_orm::sea_query::Alias::new("withdrawal_id")).eq(id))
        .to_owned();
    fixture
        .pool
        .connection()
        .execute(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&query),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn merchant_payout_unknown_never_resubmits_and_blocks_manual_decisions()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let withdrawal = finance
        .request_withdrawal(77, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", 200)
        .await?;
    let queued = payouts.enqueue(withdrawal.id, false).await?;
    assert_eq!(queued.amount_minor, 200);
    assert_eq!(finance.balance(77).await?.withdrawing_quota, 200);
    let claim = payouts.claim("first-worker").await?.unwrap();
    assert!(claim.submit);
    assert!(payouts.claim("second-worker").await?.is_none());
    payouts
        .complete(&claim, "unknown", "", Some("gateway_timeout"))
        .await?;
    assert_eq!(
        finance
            .decide_withdrawal(
                withdrawal.id,
                fixture.principal.user_id(),
                "paid",
                "manual",
                None
            )
            .await,
        Err(MerchantFinanceError::Conflict)
    );
    assert_eq!(
        finance
            .decide_withdrawal(
                withdrawal.id,
                fixture.principal.user_id(),
                "rejected",
                "manual",
                None
            )
            .await,
        Err(MerchantFinanceError::Conflict)
    );
    let previous = payouts.settings().await?;
    let mut config = previous.config;
    config.enabled = false;
    payouts
        .save_settings(
            previous.version,
            config,
            Some("encrypted-key-v2".to_owned()),
            false,
        )
        .await?;
    payouts
        .save_account(
            77,
            1,
            "Updated account".to_owned(),
            Some("encrypted-account-v2".to_owned()),
        )
        .await?;
    due(&fixture, withdrawal.id).await?;
    let recovery = payouts.claim("recovery-worker").await?.unwrap();
    assert!(!recovery.submit);
    assert_eq!(recovery.gateway_secret, "encrypted-key-v1");
    assert_eq!(recovery.account_secret, "encrypted-account-v1");
    payouts
        .complete(&recovery, "paid", "gateway-transfer-1", None)
        .await?;
    assert_eq!(finance.balance(77).await?.total_paid_quota, 200);
    assert_eq!(finance.balance(77).await?.withdrawing_quota, 0);
    assert_eq!(
        payouts
            .complete(&recovery, "paid", "gateway-transfer-1", None)
            .await,
        Err(MerchantFinanceError::Conflict)
    );
    assert_eq!(finance.balance(77).await?.total_paid_quota, 200);
    let page = finance.withdrawals(Some(77), None, 20, None).await?;
    assert_eq!(page.items[0].payout.as_ref().unwrap().status, "paid");
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_payout_expired_submit_lease_recovers_by_query_and_rejection_unlocks()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let withdrawal = finance
        .request_withdrawal(77, "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", 200)
        .await?;
    payouts.enqueue(withdrawal.id, false).await?;
    let lost = payouts.claim("lost-worker").await?.unwrap();
    assert!(lost.submit);
    due(&fixture, withdrawal.id).await?;
    let recovery = payouts.claim("new-worker").await?.unwrap();
    assert!(!recovery.submit);
    assert_eq!(
        payouts.complete(&lost, "paid", "late-response", None).await,
        Err(MerchantFinanceError::Conflict)
    );
    payouts
        .complete(&recovery, "failed", "", Some("gateway_rejected"))
        .await?;
    assert_eq!(finance.balance(77).await?.available_quota, 950);
    assert_eq!(finance.balance(77).await?.withdrawing_quota, 0);
    assert_eq!(finance.balance(77).await?.total_paid_quota, 0);
    assert!(payouts.claim("third-worker").await?.is_none());
    fixture.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merchant_payout_multi_instance_claims_and_daily_limits_are_atomic()
-> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let fixture = concurrent_fixture(&database, config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let first = finance
        .request_withdrawal(77, "cccccccccccccccccccccccccccccccc", 300)
        .await?;
    let second = finance
        .request_withdrawal(77, "dddddddddddddddddddddddddddddddd", 300)
        .await?;
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = JoinSet::new();
    for id in [first.id, second.id] {
        let repository = payouts.clone();
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository.enqueue(id, false).await
        });
    }
    barrier.wait().await;
    let mut applied = 0;
    let mut rejected = 0;
    while let Some(result) = tasks.join_next().await {
        match result? {
            Ok(_) => applied += 1,
            Err(MerchantFinanceError::InsufficientBalance) => rejected += 1,
            outcome => panic!("unexpected payout authorization: {outcome:?}"),
        }
    }
    assert_eq!((applied, rejected), (1, 1));
    let barrier = Arc::new(Barrier::new(3));
    let mut tasks = JoinSet::new();
    for owner in ["worker-a", "worker-b"] {
        let repository = payouts.clone();
        let barrier = Arc::clone(&barrier);
        tasks.spawn(async move {
            barrier.wait().await;
            repository.claim(owner).await.map(|claim| claim.is_some())
        });
    }
    barrier.wait().await;
    let mut claims = 0;
    while let Some(result) = tasks.join_next().await {
        if result?? {
            claims += 1;
        }
    }
    assert_eq!(claims, 1);
    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn merchant_payout_automatic_approval_requires_opt_in_and_freeze_blocks_submit()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let withdrawal = finance
        .request_withdrawal(77, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", 200)
        .await?;
    let previous = payouts.settings().await?;
    let mut config = previous.config;
    config.auto_approve_enabled = false;
    payouts
        .save_settings(previous.version, config, None, false)
        .await?;
    assert_eq!(
        payouts.enqueue(withdrawal.id, true).await.unwrap_err(),
        MerchantFinanceError::Conflict
    );
    payouts.enqueue(withdrawal.id, false).await?;
    finance
        .hold_balance(
            77,
            fixture.principal.user_id(),
            "ffffffffffffffffffffffffffffffff",
            100,
            true,
            "Dispute",
        )
        .await?;
    assert!(payouts.claim("blocked-worker").await?.is_none());
    assert_eq!(finance.balance(77).await?.withdrawing_quota, 200);
    fixture.close().await?;
    Ok(())
}
