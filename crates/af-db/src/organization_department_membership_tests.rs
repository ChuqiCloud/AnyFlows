use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationMembershipStatus,
    OrganizationRole, UserId,
};
use sea_orm::{ActiveModelTrait, Set};

use crate::entity::{groups, users};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationDepartmentCreate,
    OrganizationDepartmentMembershipCreate, OrganizationDepartmentMembershipRepository,
    OrganizationDepartmentMembershipRepositoryError, OrganizationDepartmentRepository,
    OrganizationMembershipCreate, OrganizationRepository,
};

#[tokio::test]
async fn department_memberships_enforce_multi_assignment_primary_cas_and_removal()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let owner_id = create_user(&pool, "department-membership-owner").await?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let created = organizations
        .create(OrganizationCreate::new(
            "部门成员企业".to_owned(),
            "department-membership-organization".to_owned(),
            owner_id,
            entitlement()?,
        ))
        .await?;
    let (organization, owner_membership) = created.into_parts();
    let departments = OrganizationDepartmentRepository::new(pool.clone(), Duration::from_secs(5))?;
    let root = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("成员根部门".to_owned(), None, 1),
        )
        .await?;
    let child = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("成员子部门".to_owned(), Some(root.id()), 2),
        )
        .await?;
    let other = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("成员其他部门".to_owned(), None, 3),
        )
        .await?;
    let memberships =
        OrganizationDepartmentMembershipRepository::new(pool.clone(), Duration::from_secs(5))?;

    let primary = memberships
        .add(
            organization.id(),
            owner_membership.id(),
            OrganizationDepartmentMembershipCreate::new(
                root.id(),
                true,
                Some(owner_membership.version()),
            ),
        )
        .await?;
    assert!(primary.is_primary());
    assert_eq!(primary.version(), 1);

    let secondary = memberships
        .add(
            organization.id(),
            owner_membership.id(),
            OrganizationDepartmentMembershipCreate::new(child.id(), false, None),
        )
        .await?;
    assert!(!secondary.is_primary());
    let third = memberships
        .add(
            organization.id(),
            owner_membership.id(),
            OrganizationDepartmentMembershipCreate::new(other.id(), false, None),
        )
        .await?;
    assert!(!third.is_primary());
    assert_eq!(
        memberships
            .list(organization.id(), owner_membership.id())
            .await?
            .len(),
        3
    );

    let promoted = memberships
        .set_primary(
            organization.id(),
            owner_membership.id(),
            child.id(),
            owner_membership.version() + 1,
        )
        .await?;
    assert!(promoted.is_primary());
    assert_eq!(promoted.version(), 2);
    assert_eq!(
        memberships
            .set_primary(
                organization.id(),
                owner_membership.id(),
                other.id(),
                owner_membership.version() + 1,
            )
            .await,
        Err(OrganizationDepartmentMembershipRepositoryError::Conflict)
    );

    let removed = memberships
        .remove(
            organization.id(),
            owner_membership.id(),
            root.id(),
            primary.version() + 1,
        )
        .await?;
    assert_eq!(removed.status().code(), 2);
    assert!(removed.removed_at().is_some());
    assert_eq!(
        memberships
            .remove(
                organization.id(),
                owner_membership.id(),
                child.id(),
                promoted.version(),
            )
            .await,
        Err(OrganizationDepartmentMembershipRepositoryError::PrimaryRemoval)
    );
    assert_eq!(
        memberships
            .add(
                organization.id(),
                owner_membership.id(),
                OrganizationDepartmentMembershipCreate::new(root.id(), false, None),
            )
            .await,
        Err(OrganizationDepartmentMembershipRepositoryError::AlreadyAssigned)
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn department_memberships_enforce_enterprise_and_lifecycle_boundaries()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let owner_id = create_user(&pool, "department-membership-boundary-owner").await?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let first = organizations
        .create(OrganizationCreate::new(
            "成员边界企业一".to_owned(),
            "department-membership-boundary-one".to_owned(),
            owner_id,
            entitlement()?,
        ))
        .await?
        .into_parts();
    let second = organizations
        .create(OrganizationCreate::new(
            "成员边界企业二".to_owned(),
            "department-membership-boundary-two".to_owned(),
            owner_id,
            entitlement()?,
        ))
        .await?
        .into_parts();
    let departments = OrganizationDepartmentRepository::new(pool.clone(), Duration::from_secs(5))?;
    let first_department = departments
        .create(
            first.0.id(),
            OrganizationDepartmentCreate::new("边界部门一".to_owned(), None, 1),
        )
        .await?;
    let second_department = departments
        .create(
            second.0.id(),
            OrganizationDepartmentCreate::new("边界部门二".to_owned(), None, 1),
        )
        .await?;
    let memberships =
        OrganizationDepartmentMembershipRepository::new(pool.clone(), Duration::from_secs(5))?;
    let active_binding = memberships
        .add(
            first.0.id(),
            first.1.id(),
            OrganizationDepartmentMembershipCreate::new(first_department.id(), false, None),
        )
        .await?;
    assert_eq!(
        departments
            .disable(
                first.0.id(),
                first_department.id(),
                first_department.version()
            )
            .await,
        Err(crate::OrganizationDepartmentRepositoryError::ActiveMembers)
    );
    memberships
        .remove(
            first.0.id(),
            first.1.id(),
            first_department.id(),
            active_binding.version(),
        )
        .await?;
    assert_eq!(
        memberships
            .add(
                second.0.id(),
                first.1.id(),
                OrganizationDepartmentMembershipCreate::new(second_department.id(), false, None),
            )
            .await,
        Err(OrganizationDepartmentMembershipRepositoryError::MembershipOrganizationMismatch)
    );
    assert_eq!(
        memberships
            .add(
                first.0.id(),
                first.1.id(),
                OrganizationDepartmentMembershipCreate::new(second_department.id(), false, None),
            )
            .await,
        Err(OrganizationDepartmentMembershipRepositoryError::DepartmentOrganizationMismatch)
    );
    let inactive_member = create_user(&pool, "department-membership-suspended").await?;
    let inactive_membership = organizations
        .add_membership(
            first.0.id(),
            OrganizationMembershipCreate::new(
                inactive_member,
                OrganizationRole::Member,
                None,
                Some(owner_id),
            ),
        )
        .await?;
    organizations
        .change_membership(
            first.0.id(),
            inactive_membership.id(),
            crate::OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Suspended,
                None,
                inactive_membership.version(),
            ),
        )
        .await?;
    assert_eq!(
        memberships
            .add(
                first.0.id(),
                inactive_membership.id(),
                OrganizationDepartmentMembershipCreate::new(first_department.id(), false, None),
            )
            .await,
        Err(OrganizationDepartmentMembershipRepositoryError::MembershipUnavailable)
    );
    assert!(
        departments
            .disable(
                first.0.id(),
                first_department.id(),
                first_department.version()
            )
            .await
            .is_ok()
    );
    assert_eq!(
        memberships
            .add(
                first.0.id(),
                first.1.id(),
                OrganizationDepartmentMembershipCreate::new(first_department.id(), false, None),
            )
            .await,
        Err(OrganizationDepartmentMembershipRepositoryError::DepartmentUnavailable)
    );
    pool.close().await?;
    Ok(())
}

async fn create_user(pool: &crate::DatabasePool, marker: &str) -> Result<UserId, Box<dyn Error>> {
    let group = groups::ActiveModel {
        name: Set(format!("{marker}-group")),
        display_name: Set(format!("{marker} group")),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set(marker.to_owned()),
        email: Set(Some(format!("{marker}@example.com"))),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set(format!("{marker}-aff")),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    Ok(UserId::new(user.id)?)
}

fn entitlement() -> Result<crate::OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(crate::OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(50, 50, 50)?,
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
