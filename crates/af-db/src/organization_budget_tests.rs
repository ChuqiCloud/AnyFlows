use std::{error::Error, time::Duration};

use af_domain::{
    BillingReservationId, OrganizationBudgetLimit, OrganizationBudgetPeriod,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipId, OrganizationRole, OrganizationTeamId, Quota, UserId, WalletEventId,
};
use sea_orm::{
    ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use super::{
    DatabaseOptions, MigrationOptions, OrganizationBillingMutationOutcome,
    OrganizationBillingPrincipal, OrganizationBillingReservationStatus,
    OrganizationBudgetPolicyWrite, OrganizationBudgetRepository, OrganizationBudgetRepositoryError,
    OrganizationBudgetScope, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationMembershipCreate, OrganizationRepository, OrganizationTeamCreate,
};
use crate::{
    OrganizationWalletLedgerEntryType,
    entity::{
        billing_organization_reservations, groups, organization_budget_windows,
        organization_wallet_ledger_entries, organization_wallets, users,
    },
    organization_wallet,
};
use sea_orm::{ActiveModelTrait, Set};

struct Fixture {
    pool: crate::DatabasePool,
    organizations: OrganizationRepository,
    budgets: OrganizationBudgetRepository,
    users: [UserId; 3],
}

#[derive(Clone, Copy)]
struct OrganizationScenario {
    organization_id: OrganizationId,
    owner_user_id: UserId,
    member_user_id: UserId,
    membership_id: OrganizationMembershipId,
    team_id: OrganizationTeamId,
}

impl OrganizationScenario {
    fn principal(self) -> OrganizationBillingPrincipal {
        OrganizationBillingPrincipal::new(
            self.organization_id,
            self.membership_id,
            Some(self.team_id),
            self.member_user_id,
        )
    }
}

#[tokio::test]
async fn policies_require_budget_permission_and_reject_cross_organization_scopes()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first = scenario(&fixture, 1, 0, 1).await?;
    let second = scenario(&fixture, 2, 2, 1).await?;

    assert_eq!(
        fixture
            .budgets
            .create_policy(policy(
                first,
                OrganizationBudgetScope::Organization,
                500,
                first.member_user_id,
            )?)
            .await,
        Err(OrganizationBudgetRepositoryError::PermissionDenied)
    );
    assert_eq!(
        fixture
            .budgets
            .list_policies(first.organization_id, first.member_user_id)
            .await,
        Err(OrganizationBudgetRepositoryError::PermissionDenied)
    );
    assert_eq!(
        fixture
            .budgets
            .create_policy(policy(
                first,
                OrganizationBudgetScope::Team(second.team_id),
                500,
                first.owner_user_id,
            )?)
            .await,
        Err(OrganizationBudgetRepositoryError::NotFound)
    );
    assert_eq!(
        fixture
            .budgets
            .list_policies(second.organization_id, first.owner_user_id)
            .await,
        Err(OrganizationBudgetRepositoryError::PermissionDenied)
    );

    let created = fixture
        .budgets
        .create_policy(policy(
            first,
            OrganizationBudgetScope::Organization,
            500,
            first.owner_user_id,
        )?)
        .await?;
    let listed = fixture
        .budgets
        .list_policies(first.organization_id, first.owner_user_id)
        .await?;
    assert_eq!(listed, vec![created]);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn reserve_and_settle_apply_wallet_and_all_budget_layers_atomically()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let scenario = scenario(&fixture, 3, 0, 1).await?;
    credit_wallet(&fixture, scenario, 500, 0x31).await?;
    install_three_layers(&fixture, scenario, [400, 300, 200]).await?;

    let id = reservation_id(0x41)?;
    assert_eq!(
        fixture
            .budgets
            .reserve(id, scenario.principal(), Quota::new(80)?)
            .await?,
        OrganizationBillingMutationOutcome::Applied(OrganizationBillingReservationStatus::Reserved)
    );
    assert_wallet(&fixture, scenario.organization_id, 420, 0, 80, false).await?;
    assert_windows(&fixture, scenario.organization_id, 0, 80).await?;

    assert_eq!(
        fixture.budgets.settle(id, Quota::new(60)?).await?,
        OrganizationBillingMutationOutcome::Applied(OrganizationBillingReservationStatus::Settled)
    );
    assert_wallet(&fixture, scenario.organization_id, 440, 60, 0, false).await?;
    assert_windows(&fixture, scenario.organization_id, 60, 0).await?;
    let record = fixture.budgets.reservation(id).await?;
    assert_eq!(
        record.status(),
        OrganizationBillingReservationStatus::Settled
    );
    assert_eq!(record.actual_quota(), Some(Quota::new(60)?));
    assert!(record.organization.is_some());
    assert!(record.team.is_some());
    assert!(record.member.is_some());
    let settlement = organization_wallet_ledger_entries::Entity::find()
        .filter(
            organization_wallet_ledger_entries::Column::OrganizationId
                .eq(scenario.organization_id.get()),
        )
        .filter(
            organization_wallet_ledger_entries::Column::EntryType
                .eq(OrganizationWalletLedgerEntryType::UsageSettlement as i16),
        )
        .one(fixture.pool.connection())
        .await?
        .expect("企业调用结算必须写入只追加账本");
    assert_eq!(settlement.event_key, id.persistence_key());
    assert_eq!(settlement.quota_delta, -60);
    assert_eq!(settlement.balance_before, 500);
    assert_eq!(settlement.balance_after, 440);

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn insufficient_member_budget_rolls_back_the_whole_reservation() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let scenario = scenario(&fixture, 4, 0, 1).await?;
    credit_wallet(&fixture, scenario, 500, 0x32).await?;
    install_three_layers(&fixture, scenario, [400, 300, 50]).await?;

    assert_eq!(
        fixture
            .budgets
            .reserve(reservation_id(0x42)?, scenario.principal(), Quota::new(60)?,)
            .await,
        Err(OrganizationBudgetRepositoryError::BudgetQuotaInsufficient)
    );
    // 最后一层失败后，前两层窗口、钱包冻结和预留行必须一并回滚。
    assert_wallet(&fixture, scenario.organization_id, 500, 0, 0, false).await?;
    assert_eq!(
        organization_budget_windows::Entity::find()
            .filter(
                organization_budget_windows::Column::OrganizationId
                    .eq(scenario.organization_id.get()),
            )
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        billing_organization_reservations::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        0
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn reservation_settlement_and_release_replays_are_idempotent() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let scenario = scenario(&fixture, 5, 0, 1).await?;
    credit_wallet(&fixture, scenario, 500, 0x33).await?;
    install_three_layers(&fixture, scenario, [400, 300, 200]).await?;

    let settled_id = reservation_id(0x43)?;
    fixture
        .budgets
        .reserve(settled_id, scenario.principal(), Quota::new(80)?)
        .await?;
    fixture.budgets.settle(settled_id, Quota::new(60)?).await?;
    assert_eq!(
        fixture.budgets.settle(settled_id, Quota::new(60)?).await?,
        OrganizationBillingMutationOutcome::Existing(OrganizationBillingReservationStatus::Settled)
    );
    assert_eq!(
        fixture.budgets.settle(settled_id, Quota::new(61)?).await,
        Err(OrganizationBudgetRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .budgets
            .reserve(settled_id, scenario.principal(), Quota::new(81)?)
            .await,
        Err(OrganizationBudgetRepositoryError::Conflict)
    );

    let released_id = reservation_id(0x44)?;
    fixture
        .budgets
        .reserve(released_id, scenario.principal(), Quota::new(40)?)
        .await?;
    assert_eq!(
        fixture.budgets.release(released_id).await?,
        OrganizationBillingMutationOutcome::Applied(OrganizationBillingReservationStatus::Released)
    );
    assert_eq!(
        fixture.budgets.release(released_id).await?,
        OrganizationBillingMutationOutcome::Existing(
            OrganizationBillingReservationStatus::Released
        )
    );
    assert_eq!(
        fixture.budgets.release(settled_id).await,
        Err(OrganizationBudgetRepositoryError::Conflict)
    );
    assert_wallet(&fixture, scenario.organization_id, 440, 60, 0, false).await?;
    assert_windows(&fixture, scenario.organization_id, 60, 0).await?;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn supplemental_failure_protects_billing_until_the_original_fact_recovers()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let scenario = scenario(&fixture, 6, 0, 1).await?;
    credit_wallet(&fixture, scenario, 100, 0x34).await?;
    let id = reservation_id(0x45)?;
    fixture
        .budgets
        .reserve(id, scenario.principal(), Quota::new(80)?)
        .await?;

    assert_eq!(
        fixture.budgets.settle(id, Quota::new(120)?).await,
        Err(OrganizationBudgetRepositoryError::SettlementPending)
    );
    assert_wallet(&fixture, scenario.organization_id, 20, 0, 80, true).await?;
    let pending = fixture.budgets.reservation(id).await?;
    assert_eq!(
        pending.status(),
        OrganizationBillingReservationStatus::SettlementPending
    );
    assert_eq!(pending.actual_quota(), Some(Quota::new(120)?));
    assert_eq!(
        fixture.budgets.settle(id, Quota::new(121)?).await,
        Err(OrganizationBudgetRepositoryError::Conflict)
    );
    assert_eq!(
        fixture.budgets.release(id).await,
        Err(OrganizationBudgetRepositoryError::Conflict)
    );
    assert_eq!(
        fixture
            .budgets
            .reserve(reservation_id(0x46)?, scenario.principal(), Quota::new(1)?,)
            .await,
        Err(OrganizationBudgetRepositoryError::BillingProtected)
    );

    credit_wallet(&fixture, scenario, 100, 0x35).await?;
    assert_eq!(
        fixture.budgets.settle(id, Quota::new(120)?).await?,
        OrganizationBillingMutationOutcome::Applied(OrganizationBillingReservationStatus::Settled)
    );
    assert_wallet(&fixture, scenario.organization_id, 80, 120, 0, false).await?;
    assert_eq!(
        fixture
            .budgets
            .reserve(reservation_id(0x47)?, scenario.principal(), Quota::new(1)?,)
            .await?,
        OrganizationBillingMutationOutcome::Applied(OrganizationBillingReservationStatus::Reserved)
    );

    fixture.pool.close().await?;
    Ok(())
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("organization-budget-test-group".to_owned()),
        display_name: Set("企业预算测试分组".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("organization-budget-user-{index}")),
            email: Set(Some(format!(
                "organization-budget-user-{index}@example.com"
            ))),
            password_hash: Set(None),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("organization-budget-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    Ok(Fixture {
        organizations: OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?,
        budgets: OrganizationBudgetRepository::new(
            pool.clone(),
            Duration::from_secs(5),
            Duration::from_secs(60),
        )?,
        pool,
        users: user_ids.try_into().expect("测试用户数量固定为三"),
    })
}

async fn scenario(
    fixture: &Fixture,
    marker: u8,
    owner_index: usize,
    member_index: usize,
) -> Result<OrganizationScenario, Box<dyn Error>> {
    let owner_user_id = fixture.users[owner_index];
    let member_user_id = fixture.users[member_index];
    let (organization, _) = fixture
        .organizations
        .create(OrganizationCreate::new(
            format!("Budget Organization {marker}"),
            format!("budget-organization-{marker}"),
            owner_user_id,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let team = fixture
        .organizations
        .create_team(
            organization.id(),
            OrganizationTeamCreate::new(format!("Budget Team {marker}"), None),
        )
        .await?;
    let membership = fixture
        .organizations
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                member_user_id,
                OrganizationRole::Member,
                Some(team.id()),
                Some(owner_user_id),
            ),
        )
        .await?;
    Ok(OrganizationScenario {
        organization_id: organization.id(),
        owner_user_id,
        member_user_id,
        membership_id: membership.id(),
        team_id: team.id(),
    })
}

fn active_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 10)?,
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

fn policy(
    scenario: OrganizationScenario,
    scope: OrganizationBudgetScope,
    limit: i64,
    actor_user_id: UserId,
) -> Result<OrganizationBudgetPolicyWrite, Box<dyn Error>> {
    let effective_from = TimeDateTimeWithTimeZone::from_unix_timestamp(
        TimeDateTimeWithTimeZone::now_utc()
            .unix_timestamp()
            .checked_sub(1)
            .expect("当前测试时间必须可减一秒"),
    )?;
    Ok(OrganizationBudgetPolicyWrite::new(
        scenario.organization_id,
        scope,
        OrganizationBudgetPeriod::UtcDay,
        OrganizationBudgetLimit::new(Some(Quota::new(limit)?))?,
        effective_from,
        actor_user_id,
    ))
}

async fn install_three_layers(
    fixture: &Fixture,
    scenario: OrganizationScenario,
    limits: [i64; 3],
) -> Result<(), Box<dyn Error>> {
    for (scope, limit) in [
        (OrganizationBudgetScope::Organization, limits[0]),
        (OrganizationBudgetScope::Team(scenario.team_id), limits[1]),
        (
            OrganizationBudgetScope::Member(scenario.membership_id),
            limits[2],
        ),
    ] {
        fixture
            .budgets
            .create_policy(policy(scenario, scope, limit, scenario.owner_user_id)?)
            .await?;
    }
    Ok(())
}

async fn credit_wallet(
    fixture: &Fixture,
    scenario: OrganizationScenario,
    amount: i64,
    marker: u8,
) -> Result<(), Box<dyn Error>> {
    let transaction = fixture.pool.connection().begin().await?;
    organization_wallet::credit_topup_in_transaction(
        &transaction,
        scenario.organization_id,
        WalletEventId::new([marker; 16])?,
        scenario.owner_user_id,
        Quota::new(amount)?,
        TimeDateTimeWithTimeZone::now_utc(),
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn assert_wallet(
    fixture: &Fixture,
    organization_id: OrganizationId,
    quota: i64,
    used: i64,
    frozen: i64,
    protected: bool,
) -> Result<(), Box<dyn Error>> {
    let wallet = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()))
        .one(fixture.pool.connection())
        .await?
        .expect("测试企业钱包必须存在");
    assert_eq!(wallet.quota, quota);
    assert_eq!(wallet.used_quota, used);
    assert_eq!(wallet.frozen_quota, frozen);
    assert_eq!(wallet.billing_protected, protected);
    Ok(())
}

async fn assert_windows(
    fixture: &Fixture,
    organization_id: OrganizationId,
    used: i64,
    frozen: i64,
) -> Result<(), Box<dyn Error>> {
    let windows = organization_budget_windows::Entity::find()
        .filter(organization_budget_windows::Column::OrganizationId.eq(organization_id.get()))
        .all(fixture.pool.connection())
        .await?;
    assert_eq!(windows.len(), 3);
    for window in windows {
        assert_eq!(window.used_quota, used);
        assert_eq!(window.frozen_quota, frozen);
    }
    Ok(())
}

fn reservation_id(marker: u8) -> Result<BillingReservationId, Box<dyn Error>> {
    Ok(BillingReservationId::new([marker; 16])?)
}
