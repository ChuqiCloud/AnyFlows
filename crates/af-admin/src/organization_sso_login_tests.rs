use std::{error::Error, time::Duration};

use af_db::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite, OrganizationRepository,
    OrganizationSsoProviderCreate, OrganizationSsoProviderRepository,
};
use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationSsoProtocol,
    OrganizationSsoProviderKey, OrganizationSsoProviderStatus,
};

use super::{
    DatabaseOrganizationSsoLoginTransactionService, OrganizationSsoLoginTransactionService,
    OrganizationSsoLoginTransactionServiceError,
};

#[tokio::test]
async fn login_transaction_service_binds_provider_and_consumes_ticket_once()
-> Result<(), Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let InitialSetupOutcome::Initialized { user_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "sso-login-owner".to_owned(),
                "sso login owner password".to_owned(),
            ))
            .await?
    else {
        panic!("空数据库必须完成首次安装");
    };
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let first_organization = organizations
        .create(OrganizationCreate::new(
            "SSO Login First".to_owned(),
            "sso-login-first".to_owned(),
            user_id,
            entitlement()?,
        ))
        .await?
        .into_parts()
        .0
        .id();
    let second_organization = organizations
        .create(OrganizationCreate::new(
            "SSO Login Second".to_owned(),
            "sso-login-second".to_owned(),
            user_id,
            entitlement()?,
        ))
        .await?
        .into_parts()
        .0
        .id();
    let repository = OrganizationSsoProviderRepository::new(pool.clone(), Duration::from_secs(5))?;
    let provider_key = OrganizationSsoProviderKey::new([0x73; 16])?;
    let provider = repository
        .create_provider(
            first_organization,
            OrganizationSsoProviderCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                OrganizationSsoProviderStatus::Active,
                "Login Provider".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    let service = DatabaseOrganizationSsoLoginTransactionService::new(repository.clone());

    let start = service
        .begin(
            first_organization,
            provider_key.persistence_key(),
            "a".repeat(64),
        )
        .await?;
    let transaction = repository
        .login_transaction(first_organization, start.state_digest().to_owned())
        .await?;
    assert_eq!(transaction.provider_version(), provider.version());
    assert_eq!(transaction.protocol(), OrganizationSsoProtocol::Oidc);

    let claim = service
        .claim(first_organization, start.state().to_owned())
        .await?;
    assert_eq!(claim.transaction_id(), transaction.id());
    assert_eq!(claim.provider_version(), provider.version());
    assert!(matches!(
        service
            .claim(first_organization, start.state().to_owned())
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));

    let assertion_digest = "b".repeat(64);
    let ticket = service
        .complete_verified(
            first_organization,
            claim.transaction_id(),
            assertion_digest.clone(),
        )
        .await?;
    assert!(!format!("{ticket:?}").contains(ticket.ticket()));
    assert!(matches!(
        service
            .complete_rejected(first_organization, claim.transaction_id())
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));
    assert!(matches!(
        service
            .exchange_ticket(second_organization, ticket.ticket().to_owned())
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::Rejected)
    ));

    let exchanged = service
        .exchange_ticket(first_organization, ticket.ticket().to_owned())
        .await?;
    assert_eq!(exchanged.transaction_id(), claim.transaction_id());
    assert_eq!(exchanged.assertion_digest(), assertion_digest);
    assert!(matches!(
        service
            .exchange_ticket(first_organization, ticket.ticket().to_owned())
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));
    pool.close().await?;
    Ok(())
}

fn entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
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
        "organization-sso-login-test".to_owned(),
        None,
        None,
    )?)
}
