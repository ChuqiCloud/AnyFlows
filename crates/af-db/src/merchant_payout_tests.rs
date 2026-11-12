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

async fn set_payout_actor_role(fixture: &Fixture, role: i16) -> Result<(), Box<dyn Error>> {
    let query = Query::update()
        .table(sea_orm::sea_query::Alias::new("users"))
        .value(sea_orm::sea_query::Alias::new("role"), role)
        .and_where(
            Expr::col(sea_orm::sea_query::Alias::new("id")).eq(fixture.principal.user_id().get()),
        )
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

fn payout_initiator(fixture: &Fixture) -> crate::MerchantPayoutInitiator {
    crate::MerchantPayoutInitiator::new(
        fixture.principal.user_id().get(),
        Some(91),
        "payout-pat-accepted-request".to_owned(),
        Some("admin:submit_merchant_payout:execute".to_owned()),
    )
    .unwrap()
}

#[tokio::test]
async fn merchant_payout_revoked_admin_cancels_only_unsubmitted_and_releases_once()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let withdrawal = finance
        .request_withdrawal(77, "00112233445566778899aabbccddeeff", 200)
        .await?;
    set_payout_actor_role(&fixture, 1).await?;
    payouts
        .enqueue_with_initiator(withdrawal.id, false, Some(&payout_initiator(&fixture)))
        .await?;
    set_payout_actor_role(&fixture, 0).await?;
    assert!(payouts.claim("worker-before-submit").await?.is_none());
    let after = finance.balance(77).await?;
    assert_eq!(after.withdrawing_quota, 0);
    assert_eq!(after.available_quota, 950);
    assert_eq!(after.total_paid_quota, 0);
    let page = finance.withdrawals(Some(77), None, 20, None).await?;
    assert_eq!(page.items[0].status, "rejected");
    assert_eq!(page.items[0].payout.as_ref().unwrap().status, "failed");
    assert_eq!(
        page.items[0].payout.as_ref().unwrap().reason.as_deref(),
        Some("initiator_permission_revoked")
    );
    assert!(payouts.claim("worker-retry").await?.is_none());
    assert_eq!(finance.balance(77).await?.available_quota, 950);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_payout_admin_revoked_after_claim_cancels_before_first_submit_once()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let withdrawal = finance
        .request_withdrawal(77, "102132435465768798a9bacbdcedfe0f", 200)
        .await?;
    set_payout_actor_role(&fixture, 1).await?;
    payouts
        .enqueue_with_initiator(withdrawal.id, false, Some(&payout_initiator(&fixture)))
        .await?;
    let claim = payouts
        .claim("worker-claimed-before-revocation")
        .await?
        .unwrap();
    assert!(claim.submit);
    set_payout_actor_role(&fixture, 0).await?;
    assert!(!payouts.authorize_first_submit(&claim).await?);
    assert!(!payouts.authorize_first_submit(&claim).await?);
    assert!(payouts.claim("worker-retry-after-cancel").await?.is_none());
    assert_eq!(
        payouts
            .complete(&claim, "paid", "must-not-submit", None)
            .await,
        Err(MerchantFinanceError::Conflict)
    );
    let balance = finance.balance(77).await?;
    assert_eq!(balance.available_quota, 950);
    assert_eq!(balance.withdrawing_quota, 0);
    assert_eq!(balance.total_paid_quota, 0);
    let withdrawals = finance.withdrawals(Some(77), None, 20, None).await?;
    let cancelled = &withdrawals.items[0];
    assert_eq!(cancelled.status, "rejected");
    assert_eq!(cancelled.payout.as_ref().unwrap().status, "failed");
    assert_eq!(
        cancelled.reason.as_deref(),
        Some("initiator_permission_revoked")
    );
    let entries = finance.entries(77, 100, None).await?;
    assert_eq!(
        entries
            .items
            .iter()
            .filter(|entry| entry.kind == "withdrawal_rejected")
            .count(),
        1
    );
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_payout_authorized_first_submit_cannot_cancel_or_authorize_twice()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let withdrawal = finance
        .request_withdrawal(77, "2031425364758697a8b9cadbecfd0e1f", 200)
        .await?;
    set_payout_actor_role(&fixture, 1).await?;
    payouts
        .enqueue_with_initiator(withdrawal.id, false, Some(&payout_initiator(&fixture)))
        .await?;
    let claim = payouts
        .claim("first-submit-authorized-worker")
        .await?
        .unwrap();
    assert!(payouts.authorize_first_submit(&claim).await?);
    assert!(!payouts.authorize_first_submit(&claim).await?);
    set_payout_actor_role(&fixture, 0).await?;
    assert!(!payouts.authorize_first_submit(&claim).await?);
    let balance = finance.balance(77).await?;
    assert_eq!(balance.available_quota, 750);
    assert_eq!(balance.withdrawing_quota, 200);
    assert_eq!(balance.total_paid_quota, 0);
    assert!(
        finance
            .entries(77, 100, None)
            .await?
            .items
            .iter()
            .all(|entry| entry.kind != "withdrawal_rejected")
    );
    due(&fixture, withdrawal.id).await?;
    let recovery = payouts
        .claim("authorized-query-only-worker")
        .await?
        .unwrap();
    assert!(!recovery.submit);
    payouts
        .complete(&recovery, "paid", "confirmed-authorized-transfer", None)
        .await?;
    assert_eq!(finance.balance(77).await?.withdrawing_quota, 0);
    assert_eq!(finance.balance(77).await?.total_paid_quota, 200);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn merchant_payout_unknown_keeps_funds_and_reconciles_after_admin_revocation()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(config()).await?;
    let (finance, payouts) = setup(&fixture).await?;
    let withdrawal = finance
        .request_withdrawal(77, "11223344556677889900aabbccddeeff", 200)
        .await?;
    set_payout_actor_role(&fixture, 1).await?;
    payouts
        .enqueue_with_initiator(withdrawal.id, false, Some(&payout_initiator(&fixture)))
        .await?;
    let first = payouts.claim("initial-worker").await?.unwrap();
    assert!(first.submit);
    assert!(payouts.authorize_first_submit(&first).await?);
    assert_eq!(first.initiator_access_token_id, Some(91));
    assert_eq!(
        first.initiator_request_id.as_deref(),
        Some("payout-pat-accepted-request")
    );
    payouts
        .complete(&first, "unknown", "", Some("gateway_timeout"))
        .await?;
    set_payout_actor_role(&fixture, 0).await?;
    assert!(!payouts.authorize_first_submit(&first).await?);
    due(&fixture, withdrawal.id).await?;
    let recovery = payouts.claim("reconciliation-worker").await?.unwrap();
    assert!(!recovery.submit);
    assert_eq!(
        payouts.authorize_first_submit(&recovery).await,
        Err(MerchantFinanceError::InvalidInput)
    );
    assert_eq!(finance.balance(77).await?.withdrawing_quota, 200);
    payouts
        .complete(&recovery, "paid", "remote-confirmed", None)
        .await?;
    assert_eq!(finance.balance(77).await?.withdrawing_quota, 0);
    assert_eq!(finance.balance(77).await?.total_paid_quota, 200);
    fixture.close().await?;
    Ok(())
}
