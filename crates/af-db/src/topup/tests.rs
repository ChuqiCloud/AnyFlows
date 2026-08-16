use std::{error::Error, time::Duration};

use af_domain::{
    GroupId, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, Quota, TopupOrderId,
    TopupOrderStatus, TopupPaymentEventId, TopupPaymentEventType, TopupRequestId, UserId,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};

use super::*;
use crate::{
    AdminUserCreateRecord, AdminUserRepository, DatabaseOptions, DatabasePool, MigrationOptions,
    OrganizationCreate, OrganizationEntitlementWrite, OrganizationRepository,
    OrganizationWalletBalanceLookupOutcome, OrganizationWalletLedgerEntryType,
    OrganizationWalletLedgerListOutcome, OrganizationWalletRepository, WalletLedgerEntryType,
    WalletLedgerListOutcome, WalletLedgerRepository,
    entity::{groups, organization_wallets, topup_payment_events, users, wallet_ledger_entries},
};

const CREATED_AT: u64 = 1_900_000_000;
const SUBMITTED_AT: u64 = CREATED_AT + 10;
const EXPIRES_AT: u64 = CREATED_AT + 3_600;
const RECEIVED_AT: u64 = CREATED_AT + 60;

#[test]
fn payment_method_and_event_payment_facts_are_required() -> Result<(), Box<dyn Error>> {
    let order = TopupOrderCreate::new(
        topup_order_id(0x01),
        topup_request_id(0x02),
        UserId::new(1)?,
        "stripe".to_owned(),
        "Card".to_owned(),
        1_000,
        "USD".to_owned(),
        Quota::new(10_000)?,
        CREATED_AT,
    );
    assert_eq!(order.unwrap_err(), TopupInputError::InvalidPaymentMethod);

    let event = TopupPaymentEventWrite::new(
        topup_event_id(0x03),
        topup_order_id(0x01),
        "stripe".to_owned(),
        "evt-invalid-payment-facts".to_owned(),
        Some("trade-invalid-payment-facts".to_owned()),
        0,
        "USD".to_owned(),
        "card".to_owned(),
        TopupPaymentEventType::Succeeded,
        [0x11; 32],
        [0x12; 32],
        RECEIVED_AT,
    );
    assert_eq!(event.unwrap_err(), TopupInputError::InvalidAmount);
    Ok(())
}

#[tokio::test]
async fn create_and_submit_are_idempotent_and_version_guarded() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(0).await?;
    let created = fixture
        .repository
        .create_order(fixture.order(0x11, 0x21, 1_000, 10_000)?)
        .await?;
    let TopupOrderCreateOutcome::Created(created) = created else {
        panic!("首次请求必须创建订单")
    };
    assert_eq!(created.status(), TopupOrderStatus::Created);
    assert_eq!(created.version(), 1);
    assert_eq!(created.payment_method(), Some("card"));
    assert_eq!(created.created_at(), CREATED_AT);

    assert!(matches!(
        fixture
            .repository
            .create_order(fixture.order(0x11, 0x21, 1_000, 10_000)?)
            .await?,
        TopupOrderCreateOutcome::Existing(_)
    ));
    let later_retry = TopupOrderCreate::new(
        topup_order_id(0x11),
        topup_request_id(0x21),
        fixture.user_id,
        "stripe".to_owned(),
        "card".to_owned(),
        1_000,
        "USD".to_owned(),
        Quota::new(10_000)?,
        CREATED_AT + 5,
    )?;
    let TopupOrderCreateOutcome::Existing(later_retry) =
        fixture.repository.create_order(later_retry).await?
    else {
        panic!("跨秒重试必须恢复首次创建事实")
    };
    assert_eq!(later_retry.created_at(), CREATED_AT);
    assert_eq!(
        fixture
            .repository
            .create_order(fixture.order(0x12, 0x21, 2_000, 10_000)?)
            .await
            .unwrap_err(),
        TopupRepositoryError::Conflict
    );

    let submitted = fixture
        .repository
        .submit_order(fixture.submission(0x11, 1, "provider-order-a")?)
        .await?;
    let TopupOrderSubmitOutcome::Applied(submitted) = submitted else {
        panic!("Created 订单必须首次进入 Pending")
    };
    assert_eq!(submitted.status(), TopupOrderStatus::Pending);
    assert_eq!(submitted.payment_method(), Some("card"));
    assert_eq!(submitted.version(), 2);
    assert_eq!(submitted.provider_order_id(), Some("provider-order-a"));
    assert_eq!(submitted.expires_at(), Some(EXPIRES_AT));

    assert!(matches!(
        fixture
            .repository
            .submit_order(fixture.submission(0x11, 1, "provider-order-a")?)
            .await?,
        TopupOrderSubmitOutcome::Existing(_)
    ));
    assert_eq!(
        fixture
            .repository
            .submit_order(fixture.submission(0x11, 2, "provider-order-b")?)
            .await
            .unwrap_err(),
        TopupRepositoryError::Conflict
    );

    let missing = TopupOrderCreate::new(
        topup_order_id(0x13),
        topup_request_id(0x23),
        UserId::new(i64::MAX)?,
        "stripe".to_owned(),
        "card".to_owned(),
        1_000,
        "USD".to_owned(),
        Quota::new(1_000)?,
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create_order(missing).await?,
        TopupOrderCreateOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn succeeded_event_credits_once_and_unknown_outcome_replays() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(10).await?;
    fixture.create_pending(0x31, 0x41, 25).await?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture
            .repository
            .accept_verified_event(fixture.event(
                0x51,
                0x31,
                "evt-paid-a",
                Some("trade-a"),
                TopupPaymentEventType::Succeeded,
                0xa1,
                "stripe",
            )?)
            .await
            .unwrap_err(),
        TopupRepositoryError::OutcomeUnknown
    );

    let replay = fixture
        .repository
        .accept_verified_event(fixture.event(
            0x52,
            0x31,
            "evt-paid-a",
            Some("trade-a"),
            TopupPaymentEventType::Succeeded,
            0xa1,
            "stripe",
        )?)
        .await?;
    let TopupPaymentEventOutcome::Existing(order) = replay else {
        panic!("相同 Provider 事件必须识别为 Existing")
    };
    assert_eq!(order.status(), TopupOrderStatus::Paid);
    assert_eq!(order.trade_no(), Some("trade-a"));
    assert_eq!(fixture.balance().await?, 35);
    assert_eq!(
        topup_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(
                wallet_ledger_entries::Column::EntryType.eq(WalletLedgerEntryType::Topup as i16),
            )
            .count(fixture.pool.connection())
            .await?,
        1
    );

    let WalletLedgerListOutcome::Found(page) =
        WalletLedgerRepository::new(fixture.pool.clone(), Duration::from_secs(5))?
            .list(fixture.user_id, None, 10)
            .await?
    else {
        panic!("有效用户必须返回钱包账本")
    };
    let (entries, _) = page.into_parts();
    let topup = entries
        .iter()
        .find(|entry| entry.entry_type() == WalletLedgerEntryType::Topup)
        .expect("支付成功必须产生 Topup 账本");
    assert_eq!(topup.quota_delta().units(), 25);
    assert_eq!(topup.balance_before().units(), 10);
    assert_eq!(topup.balance_after().units(), 35);

    assert_eq!(
        fixture
            .repository
            .accept_verified_event(fixture.event(
                0x53,
                0x31,
                "evt-paid-a",
                Some("trade-a"),
                TopupPaymentEventType::Succeeded,
                0xa2,
                "stripe",
            )?)
            .await
            .unwrap_err(),
        TopupRepositoryError::Conflict
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn organization_success_credits_only_the_enterprise_wallet() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(77).await?;
    let organizations = OrganizationRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organizations
        .create(OrganizationCreate::new(
            "Topup Organization".to_owned(),
            "topup-organization".to_owned(),
            fixture.user_id,
            test_entitlement()?,
        ))
        .await?
        .into_parts();

    let order = TopupOrderCreate::new_for_organization(
        topup_order_id(0xd1),
        topup_request_id(0xd2),
        fixture.user_id,
        organization.id(),
        "stripe".to_owned(),
        "card".to_owned(),
        1_000,
        "USD".to_owned(),
        Quota::new(25)?,
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create_order(order).await?,
        TopupOrderCreateOutcome::Created(_)
    ));
    assert!(matches!(
        fixture
            .repository
            .submit_order(fixture.submission(0xd1, 1, "provider-order-org")?)
            .await?,
        TopupOrderSubmitOutcome::Applied(_)
    ));

    let paid = fixture
        .repository
        .accept_verified_event(fixture.event(
            0xd3,
            0xd1,
            "evt-paid-org",
            Some("trade-org"),
            TopupPaymentEventType::Succeeded,
            0xd4,
            "stripe",
        )?)
        .await?;
    assert!(matches!(paid, TopupPaymentEventOutcome::Applied(_)));
    assert_eq!(fixture.balance().await?, 77);

    let wallet = OrganizationWalletRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let OrganizationWalletBalanceLookupOutcome::Found(balance) =
        wallet.balance(organization.id()).await?
    else {
        panic!("企业充值订单必须存在企业钱包")
    };
    assert_eq!(balance.quota().units(), 25);
    assert_eq!(balance.version(), 2);
    let OrganizationWalletLedgerListOutcome::Found(page) =
        wallet.list(organization.id(), None, 10).await?
    else {
        panic!("企业充值必须产生企业账本")
    };
    let (entries, _) = page.into_parts();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].entry_type(),
        OrganizationWalletLedgerEntryType::Topup
    );
    assert_eq!(entries[0].quota_delta().units(), 25);
    assert_eq!(entries[0].balance_after().units(), 25);
    assert_eq!(
        wallet_ledger_topup_count(&fixture.pool).await?,
        0,
        "企业充值不能写入个人钱包账本"
    );

    let model = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(organization.id().get()))
        .one(fixture.pool.connection())
        .await?
        .expect("测试企业钱包必须存在");
    let mut saturated: organization_wallets::ActiveModel = model.into();
    saturated.quota = Set(i64::MAX);
    saturated.update(fixture.pool.connection()).await?;
    let overflow_order = TopupOrderCreate::new_for_organization(
        topup_order_id(0xd5),
        topup_request_id(0xd6),
        fixture.user_id,
        organization.id(),
        "stripe".to_owned(),
        "card".to_owned(),
        1_000,
        "USD".to_owned(),
        Quota::new(1)?,
        CREATED_AT,
    )?;
    assert!(matches!(
        fixture.repository.create_order(overflow_order).await?,
        TopupOrderCreateOutcome::Created(_)
    ));
    assert!(matches!(
        fixture
            .repository
            .submit_order(fixture.submission(0xd5, 1, "provider-order-org-overflow")?)
            .await?,
        TopupOrderSubmitOutcome::Applied(_)
    ));
    let overflow = fixture
        .repository
        .accept_verified_event(fixture.event(
            0xd7,
            0xd5,
            "evt-paid-org-overflow",
            Some("trade-org-overflow"),
            TopupPaymentEventType::Succeeded,
            0xd8,
            "stripe",
        )?)
        .await?;
    assert!(matches!(
        overflow,
        TopupPaymentEventOutcome::RecordedUnprocessed {
            reason: TopupPaymentEventRejection::CreditOverflow,
            ..
        }
    ));
    let overflow_event = topup_payment_events::Entity::find()
        .filter(topup_payment_events::Column::EventKey.eq(topup_event_id(0xd7).persistence_key()))
        .one(fixture.pool.connection())
        .await?
        .expect("企业余额溢出事件必须保留审计事实");
    assert!(overflow_event.processed_at.is_none());
    assert_eq!(fixture.balance().await?, 77);

    fixture.pool.close().await?;
    Ok(())
}

async fn wallet_ledger_topup_count(pool: &DatabasePool) -> Result<u64, sea_orm::DbErr> {
    wallet_ledger_entries::Entity::find()
        .filter(wallet_ledger_entries::Column::EntryType.eq(WalletLedgerEntryType::Topup as i16))
        .count(pool.connection())
        .await
}

#[tokio::test]
async fn rejected_and_terminal_events_remain_auditable_without_credit() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(100).await?;
    fixture.create_pending(0x61, 0x71, 50).await?;

    let rejected = fixture
        .repository
        .accept_verified_event(fixture.event(
            0x81,
            0x61,
            "evt-wrong-provider",
            Some("trade-wrong"),
            TopupPaymentEventType::Succeeded,
            0xb1,
            "epay",
        )?)
        .await?;
    assert!(matches!(
        rejected,
        TopupPaymentEventOutcome::RecordedUnprocessed {
            reason: TopupPaymentEventRejection::ProviderMismatch,
            ..
        }
    ));

    let failed = fixture
        .repository
        .accept_verified_event(fixture.event(
            0x82,
            0x61,
            "evt-failed",
            None,
            TopupPaymentEventType::Failed,
            0xb2,
            "stripe",
        )?)
        .await?;
    let TopupPaymentEventOutcome::Applied(failed) = failed else {
        panic!("首个失败事件必须推进订单")
    };
    assert_eq!(failed.status(), TopupOrderStatus::Failed);

    let late_success = fixture
        .repository
        .accept_verified_event(fixture.event(
            0x83,
            0x61,
            "evt-late-success",
            Some("trade-late"),
            TopupPaymentEventType::Succeeded,
            0xb3,
            "stripe",
        )?)
        .await?;
    assert!(matches!(
        late_success,
        TopupPaymentEventOutcome::RecordedUnprocessed {
            reason: TopupPaymentEventRejection::TerminalConflict,
            ..
        }
    ));
    assert_eq!(fixture.balance().await?, 100);
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(
                wallet_ledger_entries::Column::EntryType.eq(WalletLedgerEntryType::Topup as i16),
            )
            .count(fixture.pool.connection())
            .await?,
        0
    );
    let events = topup_payment_events::Entity::find()
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(events.len(), 3);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.processed_at.is_some())
            .count(),
        1
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn mismatched_payment_facts_are_audited_without_credit() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(100).await?;
    fixture.create_pending(0x86, 0x96, 50).await?;

    for (event_marker, event, expected_reason) in [
        (
            0xa6,
            fixture.event_with_payment_facts(0xa6, 0x86, 999, "USD", "card")?,
            TopupPaymentEventRejection::AmountMismatch,
        ),
        (
            0xa7,
            fixture.event_with_payment_facts(0xa7, 0x86, 1_000, "CNY", "card")?,
            TopupPaymentEventRejection::CurrencyMismatch,
        ),
        (
            0xa8,
            fixture.event_with_payment_facts(0xa8, 0x86, 1_000, "USD", "alipay")?,
            TopupPaymentEventRejection::PaymentMethodMismatch,
        ),
    ] {
        let outcome = fixture.repository.accept_verified_event(event).await?;
        assert!(matches!(
            outcome,
            TopupPaymentEventOutcome::RecordedUnprocessed { reason, .. }
                if reason == expected_reason
        ));
        let stored = topup_payment_events::Entity::find()
            .filter(
                topup_payment_events::Column::EventKey
                    .eq(topup_event_id(event_marker).persistence_key()),
            )
            .one(fixture.pool.connection())
            .await?
            .expect("被拒绝的支付事实仍必须可审计");
        assert!(stored.processed_at.is_none());
    }

    assert_eq!(fixture.balance().await?, 100);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_success_events_acknowledge_once_and_credit_once() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(0).await?;
    fixture.create_pending(0x91, 0xa1, 75).await?;
    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let first = fixture.event(
        0xb1,
        0x91,
        "evt-concurrent-a",
        Some("trade-concurrent"),
        TopupPaymentEventType::Succeeded,
        0xc1,
        "stripe",
    )?;
    let second = fixture.event(
        0xb2,
        0x91,
        "evt-concurrent-b",
        Some("trade-concurrent"),
        TopupPaymentEventType::Succeeded,
        0xc2,
        "stripe",
    )?;
    let (first, second) = tokio::join!(
        first_repository.accept_verified_event(first),
        second_repository.accept_verified_event(second)
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TopupPaymentEventOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, TopupPaymentEventOutcome::Acknowledged(_)))
            .count(),
        1
    );
    assert_eq!(fixture.balance().await?, 75);
    assert_eq!(
        topup_payment_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        2
    );
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(
                wallet_ledger_entries::Column::EntryType.eq(WalletLedgerEntryType::Topup as i16),
            )
            .count(fixture.pool.connection())
            .await?,
        1
    );

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: DatabasePool,
    repository: TopupRepository,
    user_id: UserId,
}

impl Fixture {
    fn order(
        &self,
        order_marker: u8,
        request_marker: u8,
        amount_minor: u64,
        quota_amount: i64,
    ) -> Result<TopupOrderCreate, TopupInputError> {
        TopupOrderCreate::new(
            topup_order_id(order_marker),
            topup_request_id(request_marker),
            self.user_id,
            "stripe".to_owned(),
            "card".to_owned(),
            amount_minor,
            "USD".to_owned(),
            Quota::new(quota_amount).expect("测试到账额度必须非负"),
            CREATED_AT,
        )
    }

    fn submission(
        &self,
        order_marker: u8,
        expected_version: u64,
        provider_order_id: &str,
    ) -> Result<TopupOrderSubmission, TopupInputError> {
        TopupOrderSubmission::new(
            topup_order_id(order_marker),
            expected_version,
            provider_order_id.to_owned(),
            SUBMITTED_AT,
            EXPIRES_AT,
        )
    }

    #[allow(clippy::too_many_arguments, reason = "测试参数对应完整支付事件事实")]
    fn event(
        &self,
        event_marker: u8,
        order_marker: u8,
        provider_event_id: &str,
        trade_no: Option<&str>,
        event_type: TopupPaymentEventType,
        payload_marker: u8,
        provider: &str,
    ) -> Result<TopupPaymentEventWrite, TopupInputError> {
        TopupPaymentEventWrite::new(
            topup_event_id(event_marker),
            topup_order_id(order_marker),
            provider.to_owned(),
            provider_event_id.to_owned(),
            trade_no.map(str::to_owned),
            1_000,
            "USD".to_owned(),
            "card".to_owned(),
            event_type,
            [0xd1; 32],
            [payload_marker; 32],
            RECEIVED_AT,
        )
    }

    fn event_with_payment_facts(
        &self,
        event_marker: u8,
        order_marker: u8,
        amount_minor: u64,
        currency: &str,
        payment_method: &str,
    ) -> Result<TopupPaymentEventWrite, TopupInputError> {
        TopupPaymentEventWrite::new(
            topup_event_id(event_marker),
            topup_order_id(order_marker),
            "stripe".to_owned(),
            format!("evt-payment-fact-{event_marker}"),
            Some(format!("trade-payment-fact-{event_marker}")),
            amount_minor,
            currency.to_owned(),
            payment_method.to_owned(),
            TopupPaymentEventType::Succeeded,
            [0xd1; 32],
            [event_marker; 32],
            RECEIVED_AT,
        )
    }

    async fn create_pending(
        &self,
        order_marker: u8,
        request_marker: u8,
        quota_amount: i64,
    ) -> Result<(), Box<dyn Error>> {
        let created = self
            .repository
            .create_order(self.order(order_marker, request_marker, 1_000, quota_amount)?)
            .await?;
        assert!(matches!(created, TopupOrderCreateOutcome::Created(_)));
        let submitted = self
            .repository
            .submit_order(self.submission(order_marker, 1, "provider-order-test")?)
            .await?;
        assert!(matches!(submitted, TopupOrderSubmitOutcome::Applied(_)));
        Ok(())
    }

    async fn balance(&self) -> Result<i64, sea_orm::DbErr> {
        Ok(users::Entity::find_by_id(self.user_id.get())
            .one(self.pool.connection())
            .await?
            .expect("测试用户必须存在")
            .quota)
    }
}

async fn fixture(initial_quota: i64) -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("topup-test".to_owned()),
        display_name: Set("充值测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let user = users
        .create(AdminUserCreateRecord::new(
            "topup-user".to_owned(),
            None,
            None,
            0,
            1,
            GroupId::new(group.id)?,
            initial_quota,
            None,
            None,
        ))
        .await?;
    Ok(Fixture {
        repository: TopupRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        user_id: user.user_id(),
    })
}

fn test_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 2)?,
            true,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )?,
        "contract".to_owned(),
        None,
        None,
    )?)
}

fn topup_order_id(marker: u8) -> TopupOrderId {
    TopupOrderId::new([marker; 16]).expect("测试订单标识必须非零")
}

fn topup_request_id(marker: u8) -> TopupRequestId {
    TopupRequestId::new([marker; 16]).expect("测试请求标识必须非零")
}

fn topup_event_id(marker: u8) -> TopupPaymentEventId {
    TopupPaymentEventId::new([marker; 16]).expect("测试事件标识必须非零")
}
