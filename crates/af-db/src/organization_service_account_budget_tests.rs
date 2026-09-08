use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationBudgetLimit, OrganizationBudgetPeriod, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationServiceAccountBudgetScope,
    OrganizationServiceAccountName, UserId,
};
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
use sea_orm::{ActiveModelTrait, Set};

use crate::entity::{groups, users};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationBudgetPolicyWrite, OrganizationBudgetRepository,
    OrganizationBudgetScope, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, OrganizationServiceAccountBudgetBindingRepository,
    OrganizationServiceAccountBudgetBindingRepositoryError, OrganizationServiceAccountRepository,
};

#[tokio::test]
async fn service_account_budget_binding_is_enterprise_scoped_and_cas_protected()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("service-account-budget-test-group".to_owned()),
        display_name: Set("Service Account Budget Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = users::ActiveModel {
        username: Set("service-account-budget-owner".to_owned()),
        email: Set(Some("service-account-budget-owner@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("service-account-budget-owner-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner_id = UserId::new(owner.id)?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let entitlement = OrganizationEntitlementWrite::new(
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
    )?;
    let organization = organizations
        .create(OrganizationCreate::new(
            "Service Account Budget Organization".to_owned(),
            "service-account-budget-organization".to_owned(),
            owner_id,
            entitlement.clone(),
        ))
        .await?
        .into_parts()
        .0;
    let other_organization = organizations
        .create(OrganizationCreate::new(
            "Other Service Account Budget Organization".to_owned(),
            "other-service-account-budget-organization".to_owned(),
            owner_id,
            entitlement,
        ))
        .await?
        .into_parts()
        .0;
    let team = organizations
        .create_team(
            organization.id(),
            crate::OrganizationTeamCreate::new("runtime".to_owned(), None),
        )
        .await?;
    let accounts = OrganizationServiceAccountRepository::new(pool.clone(), Duration::from_secs(5))?;
    let account = accounts
        .create(
            organization.id(),
            OrganizationServiceAccountName::new("runtime".to_owned())?,
        )
        .await?
        .into_parts()
        .0;
    let budgets = OrganizationBudgetRepository::new(
        pool.clone(),
        Duration::from_secs(5),
        Duration::from_secs(60),
    )?;
    let first_policy = budgets
        .create_policy(policy(
            organization.id(),
            OrganizationBudgetScope::Organization,
            owner_id,
            -20,
        )?)
        .await?;
    let team_policy = budgets
        .create_policy(policy(
            organization.id(),
            OrganizationBudgetScope::Team(team.id()),
            owner_id,
            -10,
        )?)
        .await?;
    let other_policy = budgets
        .create_policy(policy(
            other_organization.id(),
            OrganizationBudgetScope::Organization,
            owner_id,
            -20,
        )?)
        .await?;

    let bindings = OrganizationServiceAccountBudgetBindingRepository::new(
        pool.clone(),
        Duration::from_secs(5),
    )?;
    let created = bindings
        .set_binding(
            organization.id(),
            account.locator().service_account_id(),
            first_policy.id(),
            None,
        )
        .await?;
    assert_eq!(created.version(), 1);
    assert_eq!(
        created.scope(),
        OrganizationServiceAccountBudgetScope::Organization
    );
    assert_eq!(created.policy_version(), first_policy.version());
    assert_eq!(
        bindings
            .resolve_binding(organization.id(), account.locator().service_account_id())
            .await?,
        created
    );

    let replaced = bindings
        .set_binding(
            organization.id(),
            account.locator().service_account_id(),
            team_policy.id(),
            Some(1),
        )
        .await?;
    assert_eq!(replaced.version(), 2);
    assert_eq!(
        replaced.scope(),
        OrganizationServiceAccountBudgetScope::Team(team.id())
    );
    assert_eq!(
        bindings
            .set_binding(
                organization.id(),
                account.locator().service_account_id(),
                first_policy.id(),
                Some(1),
            )
            .await,
        Err(OrganizationServiceAccountBudgetBindingRepositoryError::Conflict)
    );
    assert_eq!(
        bindings
            .set_binding(
                organization.id(),
                account.locator().service_account_id(),
                other_policy.id(),
                Some(2),
            )
            .await,
        Err(OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable)
    );

    assert_eq!(
        organizations
            .disable_team(organization.id(), team.id(), team.version())
            .await?
            .status(),
        af_domain::OrganizationTeamStatus::Disabled
    );
    assert_eq!(
        bindings
            .resolve_binding(organization.id(), account.locator().service_account_id())
            .await,
        Err(OrganizationServiceAccountBudgetBindingRepositoryError::TeamUnavailable)
    );

    let unbound = accounts
        .create(
            organization.id(),
            OrganizationServiceAccountName::new("unbound".to_owned())?,
        )
        .await?
        .into_parts()
        .0;
    assert_eq!(
        bindings
            .resolve_binding(organization.id(), unbound.locator().service_account_id())
            .await,
        Err(OrganizationServiceAccountBudgetBindingRepositoryError::NotFound)
    );

    bindings
        .set_binding(
            organization.id(),
            unbound.locator().service_account_id(),
            first_policy.id(),
            None,
        )
        .await?;
    let replacement_policy = budgets
        .create_policy(policy(
            organization.id(),
            OrganizationBudgetScope::Organization,
            owner_id,
            -1,
        )?)
        .await?;
    assert!(replacement_policy.id() != first_policy.id());
    assert_eq!(
        bindings
            .resolve_binding(organization.id(), unbound.locator().service_account_id())
            .await,
        Err(OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable)
    );

    let member_policy = budgets
        .create_policy(policy(
            organization.id(),
            OrganizationBudgetScope::Member(
                organizations
                    .authorized_access(organization.id(), owner_id)
                    .await?
                    .membership()
                    .id(),
            ),
            owner_id,
            -5,
        )?)
        .await?;
    assert_eq!(
        bindings
            .set_binding(
                organization.id(),
                unbound.locator().service_account_id(),
                member_policy.id(),
                None,
            )
            .await,
        Err(OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable)
    );

    pool.close().await?;
    Ok(())
}

fn policy(
    organization_id: af_domain::OrganizationId,
    scope: OrganizationBudgetScope,
    actor_user_id: UserId,
    offset_seconds: i64,
) -> Result<OrganizationBudgetPolicyWrite, Box<dyn Error>> {
    let timestamp = TimeDateTimeWithTimeZone::now_utc()
        .unix_timestamp()
        .checked_add(offset_seconds)
        .ok_or("时间范围溢出")?;
    Ok(OrganizationBudgetPolicyWrite::new(
        organization_id,
        scope,
        OrganizationBudgetPeriod::UtcDay,
        OrganizationBudgetLimit::new(Some(af_domain::Quota::new(100)?))?,
        TimeDateTimeWithTimeZone::from_unix_timestamp(timestamp)?,
        actor_user_id,
    ))
}
