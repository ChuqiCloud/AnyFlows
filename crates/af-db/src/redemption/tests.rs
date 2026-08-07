use std::{error::Error, time::Duration};

use af_domain::{
    GroupId, Quota, RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use sea_orm_migration::prelude::SchemaManager;

use super::{
    RedemptionAttempt, RedemptionAuditQuery, RedemptionAuditStatus, RedemptionBatchCreateOutcome,
    RedemptionBatchDisable, RedemptionBatchDisableOutcome, RedemptionBatchWrite,
    RedemptionInputError, RedemptionMaterialError, RedemptionOutcome, RedemptionRejection,
    RedemptionRepositoryError, material::issued_code_from_test_entropy,
};
use crate::{
    AdminUserCreateRecord, AdminUserRepository, DatabaseOptions, DatabasePool, MigrationOptions,
    WalletLedgerEntryType,
    entity::{groups, redemption_batches, redemption_codes, users, wallet_ledger_entries},
};

const CREATED_AT: u64 = 1_900_000_000;
const REDEEMED_AT: u64 = CREATED_AT + 60;
const EXPIRES_AT: u64 = CREATED_AT + 3_600;

#[test]
fn code_material_is_canonical_and_never_renders_secrets() {
    let issued = issued_code(0x31);
    let secret = issued.code().expose_secret().to_owned();
    let digest = issued.definition().digest().persistence_key();
    let reparsed = super::PresentedRedemptionCode::parse(&secret).unwrap();
    assert_eq!(reparsed.digest().persistence_key(), digest);

    for rendered in [
        format!("{issued:?}"),
        format!("{:?}", issued.code()),
        format!("{:?}", issued.definition()),
        format!("{:?}", issued.definition().digest()),
    ] {
        assert!(!rendered.contains(&secret));
        assert!(!rendered.contains(&digest));
    }
    for invalid in [
        "",
        "RC-AF-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "rc-af-short",
        "rc-af-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA+",
        "rc-af-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    ] {
        assert_eq!(
            super::PresentedRedemptionCode::parse(invalid).unwrap_err(),
            RedemptionMaterialError::InvalidFormat
        );
    }
}

#[test]
fn redemption_inputs_reject_ambiguous_or_unbounded_facts() {
    let code = issued_code(0x32);
    let definition = code.definition();
    let user_id = UserId::new(1).unwrap();
    let valid_quota = Quota::new(1).unwrap();

    for name in ["", " 前导空格", "尾随空格 ", "控制\n字符"] {
        assert_eq!(
            RedemptionBatchWrite::new(
                batch_id(0x11),
                name.to_owned(),
                user_id,
                valid_quota,
                vec![definition],
                Some(EXPIRES_AT),
                CREATED_AT,
            )
            .unwrap_err(),
            RedemptionInputError::InvalidName
        );
    }
    assert_eq!(
        RedemptionBatchWrite::new(
            batch_id(0x11),
            "零额度".to_owned(),
            user_id,
            Quota::new(0).unwrap(),
            vec![definition],
            Some(EXPIRES_AT),
            CREATED_AT,
        )
        .unwrap_err(),
        RedemptionInputError::InvalidQuota
    );
    for codes in [
        Vec::new(),
        vec![definition; super::MAX_REDEMPTION_BATCH_CODES + 1],
    ] {
        assert_eq!(
            RedemptionBatchWrite::new(
                batch_id(0x11),
                "数量边界".to_owned(),
                user_id,
                valid_quota,
                codes,
                Some(EXPIRES_AT),
                CREATED_AT,
            )
            .unwrap_err(),
            RedemptionInputError::InvalidCodeCount
        );
    }
    assert_eq!(
        RedemptionBatchWrite::new(
            batch_id(0x11),
            "重复定义".to_owned(),
            user_id,
            valid_quota,
            vec![definition, definition],
            Some(EXPIRES_AT),
            CREATED_AT,
        )
        .unwrap_err(),
        RedemptionInputError::InvalidCodeDefinition
    );
    assert_eq!(
        RedemptionBatchWrite::new(
            batch_id(0x11),
            "时间边界".to_owned(),
            user_id,
            valid_quota,
            vec![definition],
            Some(CREATED_AT),
            CREATED_AT,
        )
        .unwrap_err(),
        RedemptionInputError::InvalidTiming
    );
    assert_eq!(
        RedemptionBatchDisable::new(batch_id(0x11), 0, CREATED_AT).unwrap_err(),
        RedemptionInputError::InvalidVersion
    );
    assert_eq!(
        RedemptionAttempt::new(
            user_id,
            super::PresentedRedemptionCode::parse(code.code().expose_secret()).unwrap(),
            i64::MAX as u64 + 1,
        )
        .unwrap_err(),
        RedemptionInputError::InvalidTiming
    );
}

#[tokio::test]
async fn batch_creation_is_idempotent_and_never_persists_plaintext() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0).await?;
    let first = issued_code(0x41);
    let second = issued_code(0x42);
    let first_secret = first.code().expose_secret().to_owned();
    let second_secret = second.code().expose_secret().to_owned();
    let write = fixture.batch(0x21, "首发兑换码", 250, [&first, &second], Some(EXPIRES_AT))?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture.repository.create_batch(&write).await.unwrap_err(),
        RedemptionRepositoryError::OutcomeUnknown
    );
    let RedemptionBatchCreateOutcome::Existing(existing) =
        fixture.repository.create_batch(&write).await?
    else {
        panic!("提交结果未知后必须按同一批次事实恢复")
    };
    assert_eq!(existing.status(), RedemptionBatchStatus::Active);
    assert_eq!(existing.code_count(), 2);
    assert_eq!(existing.quota_amount().units(), 250);

    assert_eq!(
        redemption_batches::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    let stored_codes = redemption_codes::Entity::find()
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(stored_codes.len(), 2);
    for stored in &stored_codes {
        let rendered = format!("{stored:?}");
        assert!(!rendered.contains(&first_secret));
        assert!(!rendered.contains(&second_secret));
        assert!(!rendered.contains(stored.code_sha256.as_str()));
    }
    let manager = SchemaManager::new(fixture.pool.connection());
    assert!(!manager.has_column("redemption_codes", "code").await?);
    assert!(
        manager
            .has_column("redemption_codes", "code_sha256")
            .await?
    );

    let conflict = fixture.batch(
        0x21,
        "不同批次事实",
        250,
        [&first, &second],
        Some(EXPIRES_AT),
    )?;
    assert_eq!(
        fixture
            .repository
            .create_batch(&conflict)
            .await
            .unwrap_err(),
        RedemptionRepositoryError::Conflict
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn batch_list_is_newest_first_and_aggregates_redeemed_counts() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0).await?;
    let first = issued_code(0x45);
    let second = issued_code(0x46);
    let third = issued_code(0x47);
    fixture
        .repository
        .create_batch(&fixture.batch(0x25, "第一批", 10, [&first], Some(EXPIRES_AT))?)
        .await?;
    fixture
        .repository
        .create_batch(&fixture.batch(0x26, "第二批", 20, [&second], Some(EXPIRES_AT))?)
        .await?;
    fixture
        .repository
        .create_batch(&fixture.batch(0x27, "第三批", 30, [&third], Some(EXPIRES_AT))?)
        .await?;
    assert!(matches!(
        fixture
            .repository
            .redeem(&fixture.attempt(
                fixture.first_user_id,
                second.code().expose_secret(),
                REDEEMED_AT,
            )?)
            .await?,
        RedemptionOutcome::Applied(_)
    ));

    let (first_page, next_cursor) = fixture.repository.list_batches(None, 2).await?.into_parts();
    assert_eq!(
        first_page
            .iter()
            .map(|record| record.batch().name())
            .collect::<Vec<_>>(),
        ["第三批", "第二批"]
    );
    assert_eq!(
        first_page
            .iter()
            .map(|record| record.redeemed_count())
            .collect::<Vec<_>>(),
        [0, 1]
    );
    let next_cursor = next_cursor.expect("仍有更早批次时必须返回游标");
    assert_eq!(
        next_cursor,
        first_page
            .last()
            .expect("第一页必须非空")
            .batch()
            .database_id()
    );

    let (second_page, final_cursor) = fixture
        .repository
        .list_batches(Some(next_cursor), 2)
        .await?
        .into_parts();
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page[0].batch().name(), "第一批");
    assert_eq!(second_page[0].redeemed_count(), 0);
    assert_eq!(final_cursor, None);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn audit_report_classifies_batches_and_filters_redemption_windows()
-> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0).await?;
    let active_redeemed = issued_code(0x71);
    let active_remaining = issued_code(0x72);
    let expired = issued_code(0x73);
    let disabled = issued_code(0x74);
    fixture
        .repository
        .create_batch(&fixture.batch(
            0x41,
            "活动批次",
            10,
            [&active_redeemed, &active_remaining],
            Some(EXPIRES_AT),
        )?)
        .await?;
    fixture
        .repository
        .create_batch(&fixture.batch(0x42, "过期批次", 20, [&expired], Some(CREATED_AT + 30))?)
        .await?;
    fixture
        .repository
        .create_batch(&fixture.batch(0x43, "停用批次", 30, [&disabled], Some(EXPIRES_AT))?)
        .await?;
    fixture
        .repository
        .disable_batch(&RedemptionBatchDisable::new(
            batch_id(0x43),
            1,
            CREATED_AT + 10,
        )?)
        .await?;
    fixture
        .repository
        .redeem(&fixture.attempt(
            fixture.first_user_id,
            active_redeemed.code().expose_secret(),
            REDEEMED_AT,
        )?)
        .await?;

    let page = fixture
        .repository
        .audit_batches(&RedemptionAuditQuery::new(
            None,
            10,
            None,
            None,
            None,
            None,
            CREATED_AT + 120,
        )?)
        .await?;
    let (batches, summary, next_cursor) = page.into_parts();
    assert_eq!(batches.len(), 3);
    assert_eq!(next_cursor, None);
    assert_eq!(summary.issued_count(), 4);
    assert_eq!(summary.redeemed_count(), 1);
    assert_eq!(summary.remaining_count(), 1);
    assert_eq!(summary.expired_count(), 1);
    assert_eq!(summary.disabled_count(), 1);

    let active = batches
        .iter()
        .find(|batch| batch.batch().name() == "活动批次")
        .expect("活动批次必须进入报表");
    assert_eq!(active.redeemed_count(), 1);
    assert_eq!(active.remaining_count(), 1);
    assert_eq!(active.last_redeemed_at(), Some(REDEEMED_AT));

    let expired_page = fixture
        .repository
        .audit_batches(&RedemptionAuditQuery::new(
            None,
            10,
            None,
            Some(RedemptionAuditStatus::Expired),
            None,
            None,
            CREATED_AT + 120,
        )?)
        .await?;
    let (expired_batches, _, _) = expired_page.into_parts();
    assert_eq!(expired_batches.len(), 1);
    assert_eq!(expired_batches[0].expired_count(), 1);

    let window_page = fixture
        .repository
        .audit_batches(&RedemptionAuditQuery::new(
            None,
            10,
            None,
            Some(RedemptionAuditStatus::Redeemed),
            Some(REDEEMED_AT),
            Some(REDEEMED_AT + 1),
            CREATED_AT + 120,
        )?)
        .await?;
    let (window_batches, _, _) = window_page.into_parts();
    assert_eq!(window_batches.len(), 1);
    assert_eq!(window_batches[0].batch().name(), "活动批次");

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn redemption_credits_once_and_unknown_outcome_replays() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(10).await?;
    let issued = issued_code(0x51);
    let secret = issued.code().expose_secret().to_owned();
    let preserved_updated_at =
        TimeDateTimeWithTimeZone::from_unix_timestamp((REDEEMED_AT + 120) as i64)?;
    users::Entity::update_many()
        .filter(users::Column::Id.eq(fixture.first_user_id.get()))
        .col_expr(users::Column::UpdatedAt, Expr::value(preserved_updated_at))
        .exec(fixture.pool.connection())
        .await?;
    fixture
        .repository
        .create_batch(&fixture.batch(0x31, "幂等到账", 25, [&issued], Some(EXPIRES_AT))?)
        .await?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture
            .repository
            .redeem(&fixture.attempt(fixture.first_user_id, &secret, REDEEMED_AT)?)
            .await
            .unwrap_err(),
        RedemptionRepositoryError::OutcomeUnknown
    );
    let RedemptionOutcome::Existing(existing) = fixture
        .repository
        .redeem(&fixture.attempt(fixture.first_user_id, &secret, REDEEMED_AT + 10)?)
        .await?
    else {
        panic!("同一用户重试必须恢复为 Existing")
    };
    assert_eq!(existing.quota_amount().units(), 25);
    assert_eq!(existing.balance_after().units(), 35);
    assert_eq!(existing.redeemed_at(), REDEEMED_AT);
    assert_eq!(fixture.balance(fixture.first_user_id).await?, 35);
    assert_eq!(
        users::Entity::find_by_id(fixture.first_user_id.get())
            .one(fixture.pool.connection())
            .await?
            .expect("测试用户必须存在")
            .updated_at,
        preserved_updated_at
    );

    assert!(matches!(
        fixture
            .repository
            .redeem(&fixture.attempt(fixture.second_user_id, &secret, REDEEMED_AT + 20)?)
            .await?,
        RedemptionOutcome::Rejected(RedemptionRejection::AlreadyUsed)
    ));
    assert_eq!(fixture.balance(fixture.second_user_id).await?, 0);
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(
                wallet_ledger_entries::Column::EntryType
                    .eq(WalletLedgerEntryType::Redemption as i16),
            )
            .count(fixture.pool.connection())
            .await?,
        1
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn disabled_expired_invalid_and_overflow_codes_never_credit() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0).await?;
    let disabled = issued_code(0x61);
    let disabled_secret = disabled.code().expose_secret().to_owned();
    fixture
        .repository
        .create_batch(&fixture.batch(0x41, "禁用批次", 10, [&disabled], Some(EXPIRES_AT))?)
        .await?;
    let batch = redemption_batches::Entity::find()
        .one(fixture.pool.connection())
        .await?
        .expect("测试兑换码批次必须存在");
    let preserved_updated_at =
        TimeDateTimeWithTimeZone::from_unix_timestamp((REDEEMED_AT + 120) as i64)?;
    redemption_batches::Entity::update_many()
        .filter(redemption_batches::Column::Id.eq(batch.id))
        .col_expr(
            redemption_batches::Column::UpdatedAt,
            Expr::value(preserved_updated_at),
        )
        .exec(fixture.pool.connection())
        .await?;
    let disable = RedemptionBatchDisable::new(batch_id(0x41), 1, CREATED_AT + 20)?;
    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture
            .repository
            .disable_batch(&disable)
            .await
            .unwrap_err(),
        RedemptionRepositoryError::OutcomeUnknown
    );
    assert!(matches!(
        fixture.repository.disable_batch(&disable).await?,
        RedemptionBatchDisableOutcome::Existing(_)
    ));
    assert_eq!(
        redemption_batches::Entity::find_by_id(batch.id)
            .one(fixture.pool.connection())
            .await?
            .expect("测试兑换码批次必须存在")
            .updated_at,
        preserved_updated_at
    );
    assert!(matches!(
        fixture
            .repository
            .redeem(&fixture.attempt(fixture.first_user_id, &disabled_secret, REDEEMED_AT)?)
            .await?,
        RedemptionOutcome::Rejected(RedemptionRejection::BatchDisabled)
    ));

    let expired = issued_code(0x62);
    let expired_secret = expired.code().expose_secret().to_owned();
    fixture
        .repository
        .create_batch(&fixture.batch(0x42, "过期批次", 10, [&expired], Some(CREATED_AT + 30))?)
        .await?;
    assert!(matches!(
        fixture
            .repository
            .redeem(&fixture.attempt(fixture.first_user_id, &expired_secret, CREATED_AT + 30)?)
            .await?,
        RedemptionOutcome::Rejected(RedemptionRejection::Expired)
    ));

    let unknown = issued_code(0x63);
    assert!(matches!(
        fixture
            .repository
            .redeem(&fixture.attempt(
                fixture.first_user_id,
                unknown.code().expose_secret(),
                REDEEMED_AT,
            )?)
            .await?,
        RedemptionOutcome::Rejected(RedemptionRejection::InvalidCode)
    ));
    assert_eq!(fixture.balance(fixture.first_user_id).await?, 0);
    fixture.pool.close().await?;

    let maximum = setup_fixture(i64::MAX).await?;
    let overflow = issued_code(0x64);
    let overflow_secret = overflow.code().expose_secret().to_owned();
    maximum
        .repository
        .create_batch(&maximum.batch(0x43, "溢出批次", 1, [&overflow], Some(EXPIRES_AT))?)
        .await?;
    assert!(matches!(
        maximum
            .repository
            .redeem(&maximum.attempt(maximum.first_user_id, &overflow_secret, REDEEMED_AT,)?)
            .await?,
        RedemptionOutcome::Rejected(RedemptionRejection::CreditOverflow)
    ));
    assert_eq!(maximum.balance(maximum.first_user_id).await?, i64::MAX);
    maximum.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_same_user_redemption_applies_one_ledger_entry() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0).await?;
    let issued = issued_code(0x71);
    let secret = issued.code().expose_secret().to_owned();
    fixture
        .repository
        .create_batch(&fixture.batch(0x51, "并发批次", 75, [&issued], Some(EXPIRES_AT))?)
        .await?;

    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let first = fixture.attempt(fixture.first_user_id, &secret, REDEEMED_AT)?;
    let second = fixture.attempt(fixture.first_user_id, &secret, REDEEMED_AT + 1)?;
    let (first, second) = tokio::join!(
        first_repository.redeem(&first),
        second_repository.redeem(&second)
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RedemptionOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RedemptionOutcome::Existing(_)))
            .count(),
        1
    );
    assert_eq!(fixture.balance(fixture.first_user_id).await?, 75);
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(
                wallet_ledger_entries::Column::EntryType
                    .eq(WalletLedgerEntryType::Redemption as i16),
            )
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_different_users_credit_only_one_wallet() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0).await?;
    let issued = issued_code(0x72);
    let secret = issued.code().expose_secret().to_owned();
    fixture
        .repository
        .create_batch(&fixture.batch(0x52, "跨用户并发", 75, [&issued], Some(EXPIRES_AT))?)
        .await?;

    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let first = fixture.attempt(fixture.first_user_id, &secret, REDEEMED_AT)?;
    let second = fixture.attempt(fixture.second_user_id, &secret, REDEEMED_AT + 1)?;
    let (first, second) = tokio::join!(
        first_repository.redeem(&first),
        second_repository.redeem(&second)
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RedemptionOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(
                outcome,
                RedemptionOutcome::Rejected(RedemptionRejection::AlreadyUsed)
            ))
            .count(),
        1
    );
    assert_eq!(
        fixture.balance(fixture.first_user_id).await?
            + fixture.balance(fixture.second_user_id).await?,
        75
    );
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(
                wallet_ledger_entries::Column::EntryType
                    .eq(WalletLedgerEntryType::Redemption as i16),
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
    repository: super::RedemptionRepository,
    admin_user_id: UserId,
    first_user_id: UserId,
    second_user_id: UserId,
}

impl Fixture {
    fn batch<const N: usize>(
        &self,
        marker: u8,
        name: &str,
        quota_amount: i64,
        codes: [&super::IssuedRedemptionCode; N],
        expires_at: Option<u64>,
    ) -> Result<RedemptionBatchWrite, super::RedemptionInputError> {
        RedemptionBatchWrite::new(
            batch_id(marker),
            name.to_owned(),
            self.admin_user_id,
            Quota::new(quota_amount).expect("测试兑换额度必须非负"),
            codes.iter().map(|code| code.definition()).collect(),
            expires_at,
            CREATED_AT,
        )
    }

    fn attempt(
        &self,
        user_id: UserId,
        secret: &str,
        redeemed_at: u64,
    ) -> Result<RedemptionAttempt, Box<dyn Error>> {
        Ok(RedemptionAttempt::new(
            user_id,
            super::PresentedRedemptionCode::parse(secret)?,
            redeemed_at,
        )?)
    }

    async fn balance(&self, user_id: UserId) -> Result<i64, sea_orm::DbErr> {
        Ok(users::Entity::find_by_id(user_id.get())
            .one(self.pool.connection())
            .await?
            .expect("测试用户必须存在")
            .quota)
    }
}

async fn setup_fixture(initial_quota: i64) -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("redemption-test".to_owned()),
        display_name: Set("兑换码测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let admin = users
        .create(AdminUserCreateRecord::new(
            "redemption-admin".to_owned(),
            None,
            None,
            1,
            1,
            GroupId::new(group.id)?,
            0,
            None,
            None,
        ))
        .await?;
    let first = users
        .create(AdminUserCreateRecord::new(
            "redemption-user-a".to_owned(),
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
    let second = users
        .create(AdminUserCreateRecord::new(
            "redemption-user-b".to_owned(),
            None,
            None,
            0,
            1,
            GroupId::new(group.id)?,
            0,
            None,
            None,
        ))
        .await?;
    Ok(Fixture {
        repository: super::RedemptionRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        admin_user_id: admin.user_id(),
        first_user_id: first.user_id(),
        second_user_id: second.user_id(),
    })
}

fn issued_code(marker: u8) -> super::IssuedRedemptionCode {
    issued_code_from_test_entropy(code_id(marker), [marker; 32])
}

fn code_id(marker: u8) -> RedemptionCodeId {
    RedemptionCodeId::new([marker; 16]).expect("测试兑换码标识必须非零")
}

fn batch_id(marker: u8) -> RedemptionBatchId {
    RedemptionBatchId::new([marker; 16]).expect("测试批次标识必须非零")
}
