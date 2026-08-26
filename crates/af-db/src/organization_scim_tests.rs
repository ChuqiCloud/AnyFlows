use std::{error::Error, time::Duration};

use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationScimResourceSet,
    OrganizationScimTokenStatus, UserId,
};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

use crate::{
    DatabaseOptions, MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationRepository, OrganizationScimTokenCreateRecord, OrganizationScimTokenRepository,
    OrganizationScimTokenRepositoryError,
    entity::{groups, organization_scim_tokens, users},
};

#[tokio::test]
async fn scim_token_is_digest_only_and_organization_scoped() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("scim-token-test-group".to_owned()),
        display_name: Set("SCIM Token Test".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let user = users::ActiveModel {
        username: Set("scim-token-test-user".to_owned()),
        email: Set(Some("scim-token-test@example.com".to_owned())),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(group.id),
        aff_code: Set("scim-token-test-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let owner = UserId::new(user.id)?;
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organizations
        .create(OrganizationCreate::new(
            "SCIM Token Test Organization".to_owned(),
            "scim-token-test".to_owned(),
            owner,
            active_entitlement()?,
        ))
        .await?
        .into_parts();
    let scim = OrganizationScimTokenRepository::new(pool.clone(), Duration::from_secs(5))?;
    let issued = scim
        .create(OrganizationScimTokenCreateRecord::new(
            organization.id(),
            "Directory sync".to_owned(),
            OrganizationScimResourceSet::users_and_groups(),
        ))
        .await?;
    let (record, token) = issued.into_parts();
    let token_text = token.as_str().to_owned();
    let stored = organization_scim_tokens::Entity::find_by_id(record.id().get())
        .one(pool.connection())
        .await?
        .expect("令牌实体必须存在");
    assert_eq!(stored.token_digest.as_str(), token.digest().as_str());
    assert!(!format!("{stored:?}").contains(&token_text));
    assert_eq!(record.status(), OrganizationScimTokenStatus::Active);

    let authenticated = scim
        .authenticate_for_organization(organization.id(), token.clone())
        .await?;
    assert_eq!(authenticated.id(), record.id());
    assert!(matches!(
        scim.authenticate_for_organization(
            af_domain::OrganizationId::new(organization.id().get() + 1)?,
            token.clone(),
        )
        .await,
        Err(OrganizationScimTokenRepositoryError::NotFound)
    ));

    let revoked = scim
        .revoke(organization.id(), record.id(), record.version())
        .await?;
    assert_eq!(revoked.status(), OrganizationScimTokenStatus::Revoked);
    assert_eq!(revoked.version(), 2);
    assert!(matches!(
        scim.revoke(organization.id(), record.id(), record.version())
            .await,
        Err(OrganizationScimTokenRepositoryError::Conflict)
    ));
    assert!(matches!(
        scim.authenticate_for_organization(organization.id(), token)
            .await,
        Err(OrganizationScimTokenRepositoryError::NotFound)
    ));
    pool.close().await?;
    Ok(())
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
