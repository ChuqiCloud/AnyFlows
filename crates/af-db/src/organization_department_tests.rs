use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationDepartmentStatus, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot,
    UserId,
};
use sea_orm::sea_query::Expr;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::entity::{groups, organization_departments, users};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationDepartmentCreate,
    OrganizationDepartmentRepository, OrganizationDepartmentRepositoryError,
    OrganizationEntitlementWrite, OrganizationRepository,
};

#[tokio::test]
async fn department_repository_enforces_scope_cas_depth_and_stable_cursor()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let owner_id = create_owner(&pool, "department-owner").await?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let entitlement = entitlement()?;
    let organization = organizations
        .create(OrganizationCreate::new(
            "部门测试企业".to_owned(),
            "department-test-organization".to_owned(),
            owner_id,
            entitlement.clone(),
        ))
        .await?
        .into_parts()
        .0;
    let other_organization = organizations
        .create(OrganizationCreate::new(
            "其他部门测试企业".to_owned(),
            "other-department-test-organization".to_owned(),
            owner_id,
            entitlement,
        ))
        .await?
        .into_parts()
        .0;
    let departments = OrganizationDepartmentRepository::new(pool.clone(), Duration::from_secs(5))?;

    let root = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("根部门".to_owned(), None, 10),
        )
        .await?;
    let child = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("子部门".to_owned(), Some(root.id()), 20),
        )
        .await?;
    assert_eq!(root.depth(), 1);
    assert_eq!(child.depth(), 2);
    assert_eq!(child.parent_id(), Some(root.id()));
    assert_eq!(child.status(), OrganizationDepartmentStatus::Active);

    assert_eq!(
        departments
            .create(
                other_organization.id(),
                OrganizationDepartmentCreate::new("跨企业父级".to_owned(), Some(root.id()), 0),
            )
            .await,
        Err(OrganizationDepartmentRepositoryError::ParentOrganizationMismatch)
    );

    let page = departments.list(organization.id(), None, 1).await?;
    let (records, cursor) = page.into_parts();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id(), root.id());
    let cursor = cursor.expect("第一页必须有稳定游标");
    let (records, cursor) = departments
        .list(organization.id(), Some(cursor), 1)
        .await?
        .into_parts();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id(), child.id());
    assert!(cursor.is_none());

    assert_eq!(
        departments
            .move_department(
                organization.id(),
                root.id(),
                Some(child.id()),
                root.version(),
            )
            .await,
        Err(OrganizationDepartmentRepositoryError::CycleDetected)
    );
    let moved = departments
        .move_department(organization.id(), child.id(), None, child.version())
        .await?;
    assert_eq!(moved.depth(), 1);
    assert_eq!(moved.version(), 2);
    assert_eq!(
        departments
            .move_department(organization.id(), child.id(), None, child.version())
            .await,
        Err(OrganizationDepartmentRepositoryError::Conflict)
    );

    assert_eq!(
        departments
            .disable(organization.id(), root.id(), root.version())
            .await,
        Err(OrganizationDepartmentRepositoryError::ActiveChildren)
    );
    let disabled = departments
        .disable(organization.id(), child.id(), moved.version())
        .await?;
    assert_eq!(disabled.status(), OrganizationDepartmentStatus::Disabled);
    assert!(disabled.disabled_at().is_some());
    assert_eq!(
        departments
            .disable(organization.id(), child.id(), disabled.version())
            .await,
        Err(OrganizationDepartmentRepositoryError::AlreadyDisabled)
    );

    let mut parent = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("深度 1".to_owned(), None, 30),
        )
        .await?;
    for depth in 2..=8 {
        parent = departments
            .create(
                organization.id(),
                OrganizationDepartmentCreate::new(
                    format!("深度 {depth}"),
                    Some(parent.id()),
                    i64::from(depth),
                ),
            )
            .await?;
    }
    assert_eq!(parent.depth(), 8);
    assert_eq!(
        departments
            .create(
                organization.id(),
                OrganizationDepartmentCreate::new("超深部门".to_owned(), Some(parent.id()), 99),
            )
            .await,
        Err(OrganizationDepartmentRepositoryError::DepthExceeded)
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn department_repository_fails_closed_on_persisted_cycle() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let owner_id = create_owner(&pool, "department-cycle-owner").await?;
    let organization = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?
        .create(OrganizationCreate::new(
            "循环测试企业".to_owned(),
            "department-cycle-organization".to_owned(),
            owner_id,
            entitlement()?,
        ))
        .await?
        .into_parts()
        .0;
    let departments = OrganizationDepartmentRepository::new(pool.clone(), Duration::from_secs(5))?;
    let root = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("循环根".to_owned(), None, 1),
        )
        .await?;
    let child = departments
        .create(
            organization.id(),
            OrganizationDepartmentCreate::new("循环子".to_owned(), Some(root.id()), 2),
        )
        .await?;
    organization_departments::Entity::update_many()
        .filter(organization_departments::Column::Id.eq(root.id().get()))
        .col_expr(
            organization_departments::Column::ParentDepartmentId,
            Expr::value(Some(child.id().get())),
        )
        .exec(pool.connection())
        .await?;
    assert_eq!(
        departments.list(organization.id(), None, 10).await,
        Err(OrganizationDepartmentRepositoryError::CycleDetected)
    );
    pool.close().await?;
    Ok(())
}

async fn create_owner(pool: &crate::DatabasePool, marker: &str) -> Result<UserId, Box<dyn Error>> {
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

fn entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
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
