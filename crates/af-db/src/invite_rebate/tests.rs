use std::{error::Error, time::Duration};

use af_domain::{GroupId, Quota, UserId, WalletEventId};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
    sea_query::Expr,
};

use super::{
    InviteRebateGrant, InviteRebateGrantOutcome, InviteRebateInputError, InviteRebateRejection,
    InviteRebateRepositoryError,
};
use crate::{
    AdminUserCreateRecord, AdminUserRepository, DatabaseOptions, DatabasePool, MigrationOptions,
    WalletLedgerEntryType,
    entity::{WalletLedgerKey, groups, invite_rebate_events, users, wallet_ledger_entries},
};

const CREDITED_AT: u64 = 1_900_000_000;

#[test]
fn invite_rebate_inputs_reject_invalid_facts() {
    let invitee = UserId::new(1).unwrap();
    assert_eq!(
        InviteRebateGrant::new(event_id(0x21), invitee, Quota::new(0).unwrap(), CREDITED_AT,)
            .unwrap_err(),
        InviteRebateInputError::InvalidQuota
    );
    let opening = WalletEventId::from_persistence_key("00000000000000010000000000000001").unwrap();
    assert_eq!(
        InviteRebateGrant::new(opening, invitee, Quota::new(1).unwrap(), CREDITED_AT).unwrap_err(),
        InviteRebateInputError::ReservedEventId
    );
    assert_eq!(
        InviteRebateGrant::new(
            event_id(0x22),
            invitee,
            Quota::new(1).unwrap(),
            i64::MAX as u64 + 1,
        )
        .unwrap_err(),
        InviteRebateInputError::InvalidTiming
    );
}

#[tokio::test]
async fn rebate_grant_is_atomic_idempotent_and_auditable() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(10, 0, 0).await?;
    let grant = fixture.grant(0x31, fixture.invitee_user_id, 50)?;

    fixture.repository.inject_outcome_unknown_after_commit();
    assert_eq!(
        fixture.repository.grant(&grant).await.unwrap_err(),
        InviteRebateRepositoryError::OutcomeUnknown
    );
    let InviteRebateGrantOutcome::Existing(existing) = fixture.repository.grant(&grant).await?
    else {
        panic!("提交结果未知后必须按同一返利事件恢复")
    };
    assert_eq!(existing.inviter_user_id(), fixture.inviter_user_id);
    assert_eq!(existing.invitee_user_id(), fixture.invitee_user_id);
    assert_eq!(existing.quota_amount().units(), 50);
    assert_eq!(existing.balance_after().units(), 60);
    assert_eq!(existing.credited_at(), CREDITED_AT);

    let inviter = fixture.inviter().await?;
    assert_eq!(inviter.quota, 60);
    assert_eq!(inviter.aff_quota, 50);
    assert_eq!(inviter.aff_history_quota, 50);
    assert_eq!(
        invite_rebate_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    let wallet = wallet_ledger_entries::Entity::find()
        .filter(
            wallet_ledger_entries::Column::EntryType.eq(WalletLedgerEntryType::InviteRebate as i16),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("邀请返利钱包账本必须存在");
    assert_eq!(wallet.user_id, fixture.inviter_user_id.get());
    assert_eq!(wallet.quota_delta, 50);
    assert_eq!(wallet.balance_before, 10);
    assert_eq!(wallet.balance_after, 60);
    assert!(wallet.actor_user_id.is_none());
    assert!(wallet.reason.is_none());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn one_invitee_can_credit_only_one_rebate_event() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0, 0, 0).await?;
    assert!(matches!(
        fixture
            .repository
            .grant(&fixture.grant(0x32, fixture.invitee_user_id, 25)?)
            .await?,
        InviteRebateGrantOutcome::Applied(_)
    ));
    assert!(matches!(
        fixture
            .repository
            .grant(&fixture.grant(0x33, fixture.invitee_user_id, 25)?)
            .await?,
        InviteRebateGrantOutcome::Rejected(InviteRebateRejection::AlreadyCredited)
    ));
    assert_eq!(fixture.inviter().await?.quota, 25);
    assert_eq!(
        invite_rebate_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn same_event_key_with_different_facts_is_a_conflict() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0, 0, 0).await?;
    fixture
        .repository
        .grant(&fixture.grant(0x34, fixture.invitee_user_id, 10)?)
        .await?;
    assert_eq!(
        fixture
            .repository
            .grant(&fixture.grant(0x34, fixture.invitee_user_id, 11)?)
            .await
            .unwrap_err(),
        InviteRebateRepositoryError::Conflict
    );
    assert_eq!(fixture.inviter().await?.quota, 10);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_invitation_relationship_rejects_without_writes() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0, 0, 0).await?;
    assert!(matches!(
        fixture
            .repository
            .grant(&fixture.grant(0x35, fixture.uninvited_user_id, 10)?)
            .await?,
        InviteRebateGrantOutcome::Rejected(InviteRebateRejection::NoInviter)
    ));
    let missing = UserId::new(9_999_999)?;
    assert!(matches!(
        fixture
            .repository
            .grant(&fixture.grant(0x36, missing, 10)?)
            .await?,
        InviteRebateGrantOutcome::Rejected(InviteRebateRejection::InviteeNotFound)
    ));
    assert_eq!(
        invite_rebate_events::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn wallet_or_rebate_counter_overflow_rejects_atomically() -> Result<(), Box<dyn Error>> {
    for (quota, aff_quota, aff_history) in [(i64::MAX, 0, 0), (0, i64::MAX, 0), (0, 0, i64::MAX)] {
        let fixture = setup_fixture(quota, aff_quota, aff_history).await?;
        assert!(matches!(
            fixture
                .repository
                .grant(&fixture.grant(0x37, fixture.invitee_user_id, 1)?)
                .await?,
            InviteRebateGrantOutcome::Rejected(InviteRebateRejection::CreditOverflow)
        ));
        let inviter = fixture.inviter().await?;
        assert_eq!(inviter.quota, quota);
        assert_eq!(inviter.aff_quota, aff_quota);
        assert_eq!(inviter.aff_history_quota, aff_history);
        assert_eq!(
            invite_rebate_events::Entity::find()
                .count(fixture.pool.connection())
                .await?,
            0
        );
        fixture.pool.close().await?;
    }
    Ok(())
}

#[tokio::test]
async fn concurrent_same_event_creates_one_wallet_entry() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0, 0, 0).await?;
    let first = fixture.grant(0x38, fixture.invitee_user_id, 30)?;
    let second = fixture.grant(0x38, fixture.invitee_user_id, 30)?;
    let first_repository = fixture.repository.clone();
    let second_repository = fixture.repository.clone();
    let (first, second) = tokio::join!(
        first_repository.grant(&first),
        second_repository.grant(&second)
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, InviteRebateGrantOutcome::Applied(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, InviteRebateGrantOutcome::Existing(_)))
            .count(),
        1
    );
    assert_eq!(fixture.inviter().await?.quota, 30);
    assert_eq!(
        wallet_ledger_entries::Entity::find()
            .filter(
                wallet_ledger_entries::Column::EntryType
                    .eq(WalletLedgerEntryType::InviteRebate as i16),
            )
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn rebate_event_active_model_is_append_only() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0, 0, 0).await?;
    fixture
        .repository
        .grant(&fixture.grant(0x39, fixture.invitee_user_id, 10)?)
        .await?;
    let event = invite_rebate_events::Entity::find()
        .one(fixture.pool.connection())
        .await?
        .expect("返利事件必须存在");
    let mut update = event.into_active_model();
    update.quota_amount = Set(11);
    assert!(update.update(fixture.pool.connection()).await.is_err());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn existing_event_requires_matching_wallet_snapshot() -> Result<(), Box<dyn Error>> {
    let fixture = setup_fixture(0, 0, 0).await?;
    let grant = fixture.grant(0x3a, fixture.invitee_user_id, 10)?;
    fixture.repository.grant(&grant).await?;

    // 绕过追加式 ORM 入口模拟损坏审计行，重放不得把孤立账本引用当成既有成功。
    invite_rebate_events::Entity::update_many()
        .filter(
            invite_rebate_events::Column::EventKey
                .eq(WalletLedgerKey::parse(&event_id(0x3a).persistence_key())?),
        )
        .col_expr(
            invite_rebate_events::Column::WalletLedgerEntryId,
            Expr::value(i64::MAX),
        )
        .exec(fixture.pool.connection())
        .await?;
    assert_eq!(
        fixture.repository.grant(&grant).await.unwrap_err(),
        InviteRebateRepositoryError::Invariant
    );

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: DatabasePool,
    repository: super::InviteRebateRepository,
    inviter_user_id: UserId,
    invitee_user_id: UserId,
    uninvited_user_id: UserId,
}

impl Fixture {
    fn grant(
        &self,
        marker: u8,
        invitee_user_id: UserId,
        quota_amount: i64,
    ) -> Result<InviteRebateGrant, InviteRebateInputError> {
        InviteRebateGrant::new(
            event_id(marker),
            invitee_user_id,
            Quota::new(quota_amount).expect("测试返利额度必须非负"),
            CREDITED_AT,
        )
    }

    async fn inviter(&self) -> Result<users::Model, sea_orm::DbErr> {
        Ok(users::Entity::find_by_id(self.inviter_user_id.get())
            .one(self.pool.connection())
            .await?
            .expect("测试邀请人必须存在"))
    }
}

async fn setup_fixture(
    inviter_quota: i64,
    aff_quota: i64,
    aff_history_quota: i64,
) -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("invite-rebate-test".to_owned()),
        display_name: Set("邀请返利测试".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let inviter = users
        .create(AdminUserCreateRecord::new(
            "invite-rebate-inviter".to_owned(),
            None,
            None,
            0,
            1,
            GroupId::new(group.id)?,
            inviter_quota,
            None,
            None,
        ))
        .await?;
    let invitee = users
        .create(AdminUserCreateRecord::new(
            "invite-rebate-invitee".to_owned(),
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
    let uninvited = users
        .create(AdminUserCreateRecord::new(
            "invite-rebate-uninvited".to_owned(),
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
    let invitee_model = users::Entity::find_by_id(invitee.user_id().get())
        .one(pool.connection())
        .await?
        .expect("测试被邀请用户必须存在");
    let mut invitee_update = invitee_model.into_active_model();
    invitee_update.inviter_id = Set(Some(inviter.user_id().get()));
    invitee_update.update(pool.connection()).await?;

    if aff_quota != 0 || aff_history_quota != 0 {
        let inviter_model = users::Entity::find_by_id(inviter.user_id().get())
            .one(pool.connection())
            .await?
            .expect("测试邀请人必须存在");
        let mut inviter_update = inviter_model.into_active_model();
        inviter_update.aff_quota = Set(aff_quota);
        inviter_update.aff_history_quota = Set(aff_history_quota);
        inviter_update.update(pool.connection()).await?;
    }

    Ok(Fixture {
        repository: super::InviteRebateRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        inviter_user_id: inviter.user_id(),
        invitee_user_id: invitee.user_id(),
        uninvited_user_id: uninvited.user_id(),
    })
}

fn event_id(marker: u8) -> WalletEventId {
    WalletEventId::new([marker; 16]).expect("测试返利事件标识必须非零")
}
