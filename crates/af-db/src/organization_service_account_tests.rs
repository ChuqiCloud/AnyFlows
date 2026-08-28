use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationServiceAccountLocator, OrganizationServiceAccountName,
    OrganizationServiceAccountStatus, UserId,
};
use sea_orm::{ActiveModelTrait, Set};

use crate::entity::{groups, users};
use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, OrganizationServiceAccountRepository,
};

#[tokio::test]
async fn service_account_persistence_rotation_and_cas_are_enterprise_scoped()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("service-account-test-group".to_owned()),
        display_name: Set("Service Account Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("service-account-test-user".to_owned()),
        email: Set(Some("service-account-test@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("service-account-test-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = UserId::new(user.id)?;
    let organization = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?
        .create(OrganizationCreate::new(
            "Service Account Test Organization".to_owned(),
            "service-account-test".to_owned(),
            owner,
            OrganizationEntitlementWrite::new(
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
            )?,
        ))
        .await?
        .into_parts()
        .0;
    let repository =
        OrganizationServiceAccountRepository::new(pool.clone(), Duration::from_secs(5))?;
    let issued = repository
        .create(
            organization.id(),
            OrganizationServiceAccountName::new("automation")?,
        )
        .await?;
    let (record, key, secret) = issued.into_parts();
    assert_eq!(record.version(), 1);
    assert_eq!(
        record.account().status(),
        OrganizationServiceAccountStatus::Active
    );
    assert_eq!(key.locator(), record.locator());
    assert_eq!(
        repository
            .authenticate_secret(record.locator(), secret.clone())
            .await?
            .key_key(),
        key.key_key()
    );
    let authenticated = repository
        .authenticate_secret_for_organization(organization.id(), secret.clone())
        .await?;
    assert_eq!(authenticated.account().locator(), record.locator());
    assert_eq!(authenticated.key().key_key(), key.key_key());
    assert_eq!(authenticated.identity().locator(), record.locator());
    assert_eq!(
        authenticated.identity().account_key(),
        record.account().public_key()
    );
    assert!(format!("{authenticated:?}").contains("<redacted>"));
    assert!(matches!(
        repository
            .authenticate_secret_for_organization(
                OrganizationId::new(organization.id().get() + 1)?,
                secret.clone(),
            )
            .await,
        Err(crate::OrganizationServiceAccountRepositoryError::NotFound)
    ));
    assert!(format!("{record:?}{key:?}{secret:?}").contains("<redacted>"));

    let rotated = repository.rotate_key(record.locator(), 1).await?;
    let (rotated_record, rotated_key, rotated_secret) = rotated.into_parts();
    assert_eq!(rotated_record.version(), 2);
    assert_ne!(rotated_key.key_key(), key.key_key());
    assert!(
        repository
            .authenticate_secret(record.locator(), secret)
            .await
            .is_err()
    );
    assert_eq!(
        repository
            .authenticate_secret(record.locator(), rotated_secret)
            .await?
            .key_key(),
        rotated_key.key_key()
    );
    assert!(matches!(
        repository.rotate_key(record.locator(), 1).await,
        Err(crate::OrganizationServiceAccountRepositoryError::Conflict)
    ));

    let suspended = repository
        .set_status(
            record.locator(),
            2,
            OrganizationServiceAccountStatus::Suspended,
        )
        .await?;
    assert_eq!(
        suspended.account().status(),
        OrganizationServiceAccountStatus::Suspended
    );
    assert!(matches!(
        repository
            .authenticate_secret_for_organization(organization.id(), rotated_secret)
            .await,
        Err(crate::OrganizationServiceAccountRepositoryError::Restricted)
    ));
    assert!(matches!(
        repository.rotate_key(record.locator(), 3).await,
        Err(crate::OrganizationServiceAccountRepositoryError::Restricted)
    ));
    let wrong_scope = OrganizationServiceAccountLocator::new(
        OrganizationId::new(organization.id().get() + 1)?,
        record.locator().service_account_id(),
    );
    assert!(matches!(
        repository.get(wrong_scope).await,
        Err(crate::OrganizationServiceAccountRepositoryError::NotFound)
    ));
    pool.close().await?;
    Ok(())
}
