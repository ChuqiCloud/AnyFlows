use std::{error::Error, time::Duration};

use af_domain::{
    BillingReservationId, GatewayPrincipal, GroupId, OrganizationBudgetLimit,
    OrganizationBudgetPeriod, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    OrganizationGatewayPrincipal, OrganizationId, OrganizationMembershipId, OrganizationRole,
    OrganizationTeamId, Quota, TokenId, UserId, WalletEventId,
};
use rust_decimal::Decimal;
use sea_orm::{
    ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use super::{
    DatabaseOptions, MigrationOptions, OrganizationBillingMutationOutcome,
    OrganizationBillingPrincipal, OrganizationBillingReservationStatus,
    OrganizationBudgetPolicyWrite, OrganizationBudgetRepository, OrganizationBudgetRepositoryError,
    OrganizationBudgetScope, OrganizationContractPriceSnapshot, OrganizationCreate,
    OrganizationDepartmentCreate, OrganizationDepartmentMembershipCreate,
    OrganizationDepartmentMembershipRepository, OrganizationDepartmentRepository,
    OrganizationEntitlementWrite, OrganizationMembershipCreate, OrganizationRepository,
    OrganizationTeamCreate, OrganizationTokenCreateRecord, OrganizationTokenRepository,
    OrganizationTokenWriteRecord, QuotaRepository, QuotaRepositoryError,
};
use crate::{
    OrganizationWalletLedgerEntryType,
    entity::{
        BillingReservationKey, billing_organization_reservation_departments,
        billing_organization_reservations, billing_reservations, groups,
        organization_budget_windows, organization_wallet_ledger_entries, organization_wallets,
        tokens, users,
    },
    organization_wallet,
    quota::{QuotaMutationOutcome, QuotaRepositoryOperation, QuotaReservationStatus},
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
async fn department_budget_reservation_keeps_path_and_layer_snapshots_after_move()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let scenario = scenario(&fixture, 40, 0, 1).await?;
    let departments =
        OrganizationDepartmentRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let department_memberships = OrganizationDepartmentMembershipRepository::new(
        fixture.pool.clone(),
        Duration::from_secs(5),
    )?;
    let root = departments
        .create(
            scenario.organization_id,
            OrganizationDepartmentCreate::new("根部门".to_owned(), None, 0),
        )
        .await?;
    let leaf = departments
        .create(
            scenario.organization_id,
            OrganizationDepartmentCreate::new("叶部门".to_owned(), Some(root.id()), 0),
        )
        .await?;
    let moved_root = departments
        .create(
            scenario.organization_id,
            OrganizationDepartmentCreate::new("新根部门".to_owned(), None, 1),
        )
        .await?;
    department_memberships
        .add(
            scenario.organization_id,
            scenario.membership_id,
            OrganizationDepartmentMembershipCreate::new(leaf.id(), true, Some(1)),
        )
        .await?;
    credit_wallet(&fixture, scenario, 500, 0x36).await?;
    install_three_layers(&fixture, scenario, [400, 300, 200]).await?;
    for (department_id, limit) in [(root.id(), 350_i64), (leaf.id(), 250_i64)] {
        fixture
            .budgets
            .create_policy(policy(
                scenario,
                OrganizationBudgetScope::Department(department_id),
                limit,
                scenario.owner_user_id,
            )?)
            .await?;
    }

    let principal = OrganizationBillingPrincipal::with_department(
        scenario.organization_id,
        scenario.membership_id,
        Some(scenario.team_id),
        leaf.id(),
        scenario.member_user_id,
    );
    assert_eq!(
        fixture
            .budgets
            .reserve(reservation_id(0x49)?, principal, Quota::new(260)?)
            .await,
        Err(OrganizationBudgetRepositoryError::BudgetQuotaInsufficient)
    );
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
    let id = reservation_id(0x48)?;
    fixture
        .budgets
        .reserve(id, principal, Quota::new(80)?)
        .await?;
    let reservation = fixture.budgets.reservation(id).await?;
    assert_eq!(reservation.principal(), principal);
    assert_eq!(reservation.department_path(), &[root.id(), leaf.id()]);
    assert_eq!(reservation.department_allocations().len(), 2);
    assert_eq!(
        billing_organization_reservation_departments::Entity::find()
            .filter(
                billing_organization_reservation_departments::Column::IdempotencyKey
                    .eq(id.persistence_key()),
            )
            .count(fixture.pool.connection())
            .await?,
        2
    );

    departments
        .move_department(
            scenario.organization_id,
            leaf.id(),
            Some(moved_root.id()),
            leaf.version(),
        )
        .await?;
    let after_move = fixture.budgets.reservation(id).await?;
    assert_eq!(after_move.department_path(), &[root.id(), leaf.id()]);
    assert_eq!(
        fixture.budgets.settle(id, Quota::new(60)?).await?,
        OrganizationBillingMutationOutcome::Applied(OrganizationBillingReservationStatus::Settled)
    );
    assert_eq!(
        fixture.budgets.reservation(id).await?.department_path(),
        &[root.id(), leaf.id()]
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

#[tokio::test]
async fn gateway_quota_uses_enterprise_funds_and_replays_all_terminal_operations()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let scenario = scenario(&fixture, 7, 0, 1).await?;
    set_personal_counters(&fixture, scenario.member_user_id, 777, 5, 11, 2).await?;
    credit_wallet(&fixture, scenario, 500, 0x51).await?;
    install_three_layers(&fixture, scenario, [400, 300, 200]).await?;
    let principal = create_gateway_principal(&fixture, scenario, 150, 0x61).await?;
    let repository = QuotaRepository::new(fixture.pool.clone());
    let id = reservation_id(0x71)?;

    repository.inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Precharge);
    assert_eq!(
        repository.precharge(id, principal, Quota::new(80)?).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        repository.precharge(id, principal, Quota::new(80)?).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );
    assert_eq!(
        personal_snapshot(&fixture, scenario.member_user_id).await?,
        PersonalSnapshot::new(777, 5, 11, 2)
    );
    assert_eq!(
        token_snapshot(&fixture, principal.token_id()).await?,
        (70, 0)
    );
    assert_wallet(&fixture, scenario.organization_id, 420, 0, 80, false).await?;

    repository.inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Settle);
    assert_eq!(
        repository.settle(id, Quota::new(60)?).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        repository.settle(id, Quota::new(60)?).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Settled)
    );
    assert_eq!(
        personal_snapshot(&fixture, scenario.member_user_id).await?,
        PersonalSnapshot::new(777, 65, 11, 3)
    );
    assert_eq!(
        token_snapshot(&fixture, principal.token_id()).await?,
        (90, 60)
    );
    assert_wallet(&fixture, scenario.organization_id, 440, 60, 0, false).await?;
    assert_gateway_reservation_pair(
        &fixture,
        id,
        principal,
        80,
        Some(60),
        QuotaReservationStatus::Settled,
    )
    .await?;

    let released_id = reservation_id(0x72)?;
    repository
        .precharge(released_id, principal, Quota::new(40)?)
        .await?;
    repository.inject_outcome_unknown_after_commit(QuotaRepositoryOperation::Refund);
    assert_eq!(
        repository.refund(released_id).await,
        Err(QuotaRepositoryError::OutcomeUnknown)
    );
    assert_eq!(
        repository.refund(released_id).await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Refunded)
    );
    assert_eq!(
        personal_snapshot(&fixture, scenario.member_user_id).await?,
        PersonalSnapshot::new(777, 65, 11, 3)
    );
    assert_eq!(
        token_snapshot(&fixture, principal.token_id()).await?,
        (90, 60)
    );
    assert_gateway_reservation_pair(
        &fixture,
        released_id,
        principal,
        40,
        None,
        QuotaReservationStatus::Refunded,
    )
    .await?;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn gateway_contract_price_snapshot_is_org_scoped_and_idempotent() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let first = scenario(&fixture, 9, 0, 1).await?;
    let second = scenario(&fixture, 10, 2, 1).await?;
    credit_wallet(&fixture, first, 500, 0x91).await?;
    let principal = create_gateway_principal(&fixture, first, 200, 0x92).await?;
    let repository = QuotaRepository::new(fixture.pool.clone());
    let foreign_snapshot = OrganizationContractPriceSnapshot::from_parts(
        second.organization_id.get(),
        901,
        3,
        [Decimal::ONE; 5],
    )
    .expect("测试合同价快照应有效");
    let id = reservation_id(0x93)?;

    assert_eq!(
        repository
            .precharge_with_contract_price(id, principal, Quota::new(80)?, Some(foreign_snapshot),)
            .await,
        Err(QuotaRepositoryError::Invariant)
    );

    let snapshot = OrganizationContractPriceSnapshot::from_parts(
        first.organization_id.get(),
        902,
        4,
        [Decimal::new(2, 0); 5],
    )
    .expect("测试合同价快照应有效");
    assert_eq!(
        repository
            .precharge_with_contract_price(id, principal, Quota::new(80)?, Some(snapshot))
            .await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        repository
            .precharge_with_contract_price(id, principal, Quota::new(80)?, Some(snapshot))
            .await?,
        QuotaMutationOutcome::Existing(QuotaReservationStatus::Reserved)
    );

    let key = BillingReservationKey::parse(&id.persistence_key())?;
    let shared = billing_reservations::Entity::find_by_id(key)
        .one(fixture.pool.connection())
        .await?
        .expect("共享预留应存在");
    assert_eq!(shared.organization_id, Some(first.organization_id.get()));
    assert_eq!(shared.contract_price_id, Some(902));
    assert_eq!(shared.contract_price_version, Some(4));
    assert_eq!(
        shared.contract_input_price.map(|price| price.expose()),
        Some(Decimal::new(2, 0))
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn gateway_supplement_failure_protects_both_reservations_until_recovery()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let scenario = scenario(&fixture, 8, 0, 1).await?;
    set_personal_counters(&fixture, scenario.member_user_id, 333, 9, 7, 4).await?;
    credit_wallet(&fixture, scenario, 100, 0x52).await?;
    install_three_layers(&fixture, scenario, [500, 500, 500]).await?;
    let principal = create_gateway_principal(&fixture, scenario, 100, 0x62).await?;
    let repository = QuotaRepository::new(fixture.pool.clone());
    let id = reservation_id(0x73)?;
    repository.precharge(id, principal, Quota::new(80)?).await?;

    assert_eq!(
        repository.settle(id, Quota::new(120)?).await,
        Err(QuotaRepositoryError::OrganizationQuotaInsufficient)
    );
    assert_eq!(
        personal_snapshot(&fixture, scenario.member_user_id).await?,
        PersonalSnapshot::new(333, 9, 7, 4)
    );
    assert_eq!(
        token_snapshot(&fixture, principal.token_id()).await?,
        (20, 0)
    );
    assert_wallet(&fixture, scenario.organization_id, 20, 0, 80, true).await?;
    assert_gateway_reservation_pair(
        &fixture,
        id,
        principal,
        80,
        Some(120),
        QuotaReservationStatus::SettlementPending,
    )
    .await?;

    credit_wallet(&fixture, scenario, 100, 0x53).await?;
    set_token_remain(&fixture, principal.token_id(), 60).await?;
    assert_eq!(
        repository.settle(id, Quota::new(120)?).await?,
        QuotaMutationOutcome::Applied
    );
    assert_eq!(
        personal_snapshot(&fixture, scenario.member_user_id).await?,
        PersonalSnapshot::new(333, 129, 7, 5)
    );
    assert_eq!(
        token_snapshot(&fixture, principal.token_id()).await?,
        (20, 120)
    );
    assert_wallet(&fixture, scenario.organization_id, 80, 120, 0, false).await?;
    assert_gateway_reservation_pair(
        &fixture,
        id,
        principal,
        80,
        Some(120),
        QuotaReservationStatus::Settled,
    )
    .await?;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PersonalSnapshot {
    quota: i64,
    used_quota: i64,
    frozen_quota: i64,
    request_count: i64,
}

impl PersonalSnapshot {
    const fn new(quota: i64, used_quota: i64, frozen_quota: i64, request_count: i64) -> Self {
        Self {
            quota,
            used_quota,
            frozen_quota,
            request_count,
        }
    }
}

async fn create_gateway_principal(
    fixture: &Fixture,
    scenario: OrganizationScenario,
    remain_quota: i64,
    marker: u8,
) -> Result<GatewayPrincipal, Box<dyn Error>> {
    let repository =
        OrganizationTokenRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    let hash = format!("{marker:064x}");
    let prefix = format!("sk-af-{marker:012x}");
    let record = repository
        .create(OrganizationTokenCreateRecord::new(
            scenario.organization_id,
            scenario.membership_id,
            scenario.member_user_id,
            hash,
            prefix,
            OrganizationTokenWriteRecord::new(
                format!("enterprise-quota-token-{marker}"),
                1,
                remain_quota,
                false,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ),
        ))
        .await?;
    let user = users::Entity::find_by_id(scenario.member_user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("企业计费测试用户必须存在");
    Ok(GatewayPrincipal::organization(
        record.token().token_id(),
        scenario.member_user_id,
        GroupId::new(user.default_group_id)?,
        OrganizationGatewayPrincipal::new(
            scenario.organization_id,
            scenario.membership_id,
            Some(scenario.team_id),
        ),
    ))
}

async fn set_personal_counters(
    fixture: &Fixture,
    user_id: UserId,
    quota: i64,
    used_quota: i64,
    frozen_quota: i64,
    request_count: i64,
) -> Result<(), Box<dyn Error>> {
    let mut user = users::Entity::find_by_id(user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("企业计费测试用户必须存在")
        .into_active_model();
    user.quota = Set(quota);
    user.used_quota = Set(used_quota);
    user.frozen_quota = Set(frozen_quota);
    user.request_count = Set(request_count);
    user.update(fixture.pool.connection()).await?;
    Ok(())
}

async fn personal_snapshot(
    fixture: &Fixture,
    user_id: UserId,
) -> Result<PersonalSnapshot, Box<dyn Error>> {
    let user = users::Entity::find_by_id(user_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("企业计费测试用户必须存在");
    Ok(PersonalSnapshot::new(
        user.quota,
        user.used_quota,
        user.frozen_quota,
        user.request_count,
    ))
}

async fn token_snapshot(
    fixture: &Fixture,
    token_id: TokenId,
) -> Result<(i64, i64), Box<dyn Error>> {
    let token = tokens::Entity::find_by_id(token_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("企业计费测试 Key 必须存在");
    Ok((token.remain_quota, token.used_quota))
}

async fn set_token_remain(
    fixture: &Fixture,
    token_id: TokenId,
    remain_quota: i64,
) -> Result<(), Box<dyn Error>> {
    let mut token = tokens::Entity::find_by_id(token_id.get())
        .one(fixture.pool.connection())
        .await?
        .expect("企业计费测试 Key 必须存在")
        .into_active_model();
    token.remain_quota = Set(remain_quota);
    token.update(fixture.pool.connection()).await?;
    Ok(())
}

async fn assert_gateway_reservation_pair(
    fixture: &Fixture,
    id: BillingReservationId,
    principal: GatewayPrincipal,
    reserved_quota: i64,
    actual_quota: Option<i64>,
    status: QuotaReservationStatus,
) -> Result<(), Box<dyn Error>> {
    let key = BillingReservationKey::parse(&id.persistence_key())?;
    let parent = billing_reservations::Entity::find_by_id(key.clone())
        .one(fixture.pool.connection())
        .await?
        .expect("共享计费预留必须存在");
    let organization = billing_organization_reservations::Entity::find_by_id(key)
        .one(fixture.pool.connection())
        .await?
        .expect("企业计费预留必须存在");
    assert_eq!(
        parent.organization_id,
        principal
            .organization_principal()
            .map(|value| value.organization_id().get())
    );
    let status = status as i16;
    assert_eq!(parent.status, status);
    assert_eq!(parent.reserved_quota, reserved_quota);
    assert_eq!(parent.actual_quota, actual_quota);
    assert_eq!(organization.status, status);
    assert_eq!(organization.reservation_kind, 2);
    assert_eq!(organization.token_id, Some(principal.token_id().get()));
    assert_eq!(organization.group_id, Some(principal.group_id().get()));
    assert_eq!(organization.reserved_quota, reserved_quota);
    assert_eq!(organization.token_reserved_quota, reserved_quota);
    assert_eq!(organization.actual_quota, actual_quota);
    Ok(())
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
