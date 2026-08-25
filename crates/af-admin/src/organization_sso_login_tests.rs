use std::{error::Error, time::Duration};

use af_account::{
    OrganizationSsoLoginStart, SystemSecretCipher, organization_sso_login_state_digest,
};
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MigrationOptions, OrganizationCreate, OrganizationEntitlementWrite, OrganizationRepository,
    OrganizationSsoLoginTransactionCreate, OrganizationSsoProviderCreate,
    OrganizationSsoProviderRepository,
};
use af_domain::{
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationSsoProtocol,
    OrganizationSsoProviderKey, OrganizationSsoProviderStatus,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};

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
    let cipher = cipher();
    let service =
        DatabaseOrganizationSsoLoginTransactionService::new(repository.clone(), cipher.clone());

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
    assert!(transaction.login_material_envelope().is_some());

    let saml_provider_key = OrganizationSsoProviderKey::new([0x74; 16])?;
    let saml_provider = repository
        .create_provider(
            second_organization,
            OrganizationSsoProviderCreate::new(
                saml_provider_key,
                OrganizationSsoProtocol::Saml,
                OrganizationSsoProviderStatus::Active,
                "SAML Login Provider".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    let saml_start = service
        .begin(
            second_organization,
            saml_provider_key.persistence_key(),
            "b".repeat(64),
        )
        .await?;
    let saml_transaction = repository
        .login_transaction(second_organization, saml_start.state_digest().to_owned())
        .await?;
    assert_eq!(saml_transaction.provider_id(), saml_provider.id());
    assert_eq!(saml_transaction.protocol(), OrganizationSsoProtocol::Saml);
    assert!(saml_transaction.login_material_envelope().is_none());

    // 启动响应只返回 state 摘要；票据交换回归需要独立保留原始 state 才能模拟受控回调。
    let saml_claim_start = OrganizationSsoLoginStart::new("d".repeat(64))?;
    let saml_claim_state = saml_claim_start.state().to_owned();
    let saml_claim_transaction = repository
        .create_login_transaction(
            second_organization,
            OrganizationSsoLoginTransactionCreate::new(
                saml_provider_key,
                OrganizationSsoProtocol::Saml,
                saml_provider.version(),
                saml_claim_start.state_digest().to_owned(),
                "d".repeat(64),
                4_102_444_800,
            ),
        )
        .await?;
    let saml_claim = service.claim(second_organization, saml_claim_state).await?;
    assert_eq!(saml_claim.transaction_id(), saml_claim_transaction.id());
    let saml_ticket = service
        .complete_verified(
            second_organization,
            saml_claim.transaction_id(),
            "e".repeat(64),
        )
        .await?;
    let saml_exchanged = service
        .exchange_ticket(second_organization, saml_ticket.ticket().to_owned())
        .await?;
    assert_eq!(saml_exchanged.subject_digest(), None);

    let (claim_state, claim_digest, claim_envelope, claim_verifier, claim_nonce) =
        oidc_login_material(
            &cipher,
            first_organization,
            provider.id(),
            provider.version(),
            "c".repeat(64),
        )?;
    let claim_transaction = repository
        .create_login_transaction(
            first_organization,
            OrganizationSsoLoginTransactionCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                claim_digest,
                "c".repeat(64),
                4_102_444_800,
            )
            .with_login_material_envelope(claim_envelope),
        )
        .await?;
    let claim = service
        .claim(first_organization, claim_state.clone())
        .await?;
    assert_eq!(claim.transaction_id(), claim_transaction.id());
    assert_eq!(claim.provider_version(), provider.version());
    let claim_material = claim
        .oidc_login_material()
        .expect("OIDC 领取必须恢复短期材料");
    assert_eq!(claim_material.pkce_verifier(), claim_verifier);
    assert_eq!(claim_material.nonce(), claim_nonce);
    assert!(!format!("{claim:?}").contains(claim_material.pkce_verifier()));
    assert!(!format!("{claim:?}").contains(claim_material.nonce()));
    assert!(matches!(
        service.claim(first_organization, claim_state).await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));

    let (callback_state, callback_digest, callback_envelope, _, _) = oidc_login_material(
        &cipher,
        first_organization,
        provider.id(),
        provider.version(),
        "d".repeat(64),
    )?;
    let callback_transaction = repository
        .create_login_transaction(
            first_organization,
            OrganizationSsoLoginTransactionCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                callback_digest,
                "d".repeat(64),
                4_102_444_800,
            )
            .with_login_material_envelope(callback_envelope),
        )
        .await?;
    let callback_claim = service.claim_callback(callback_state.clone()).await?;
    assert_eq!(callback_claim.transaction_id(), callback_transaction.id());
    assert_eq!(callback_claim.organization_id(), first_organization);
    assert_eq!(callback_claim.protocol(), OrganizationSsoProtocol::Oidc);
    assert!(matches!(
        service.claim_callback(callback_state.clone()).await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));
    assert!(matches!(
        service.claim(second_organization, callback_state).await,
        Err(OrganizationSsoLoginTransactionServiceError::Rejected)
    ));

    let missing_state = "missing-sso-material".to_owned();
    let missing_digest = organization_sso_login_state_digest(&missing_state)?;
    let missing = repository
        .create_login_transaction(
            first_organization,
            OrganizationSsoLoginTransactionCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                missing_digest.clone(),
                "e".repeat(64),
                4_102_444_800,
            ),
        )
        .await?;
    assert!(matches!(
        service
            .claim(first_organization, missing_state.clone())
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::Rejected)
    ));
    assert_eq!(
        repository
            .login_transaction(first_organization, missing_digest)
            .await?
            .result_code(),
        Some(2)
    );
    assert!(matches!(
        service.claim(first_organization, missing_state).await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));
    assert!(missing.id() > 0);

    let (mismatched_state, mismatched_digest, mismatched_envelope, _, _) = oidc_login_material(
        &cipher,
        first_organization,
        provider.id() + 1,
        provider.version(),
        "f".repeat(64),
    )?;
    let mismatched = repository
        .create_login_transaction(
            first_organization,
            OrganizationSsoLoginTransactionCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                mismatched_digest.clone(),
                "f".repeat(64),
                4_102_444_800,
            )
            .with_login_material_envelope(mismatched_envelope),
        )
        .await?;
    assert!(matches!(
        service.claim(first_organization, mismatched_state).await,
        Err(OrganizationSsoLoginTransactionServiceError::Rejected)
    ));
    assert_eq!(
        repository
            .login_transaction(first_organization, mismatched_digest)
            .await?
            .result_code(),
        Some(2)
    );
    assert!(mismatched.id() > 0);

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
    assert_eq!(exchanged.subject_digest(), Some(assertion_digest.as_str()));
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

fn cipher() -> SystemSecretCipher {
    let settings: CredentialEncryptionSettings = serde_json::from_value(json!({
        "key_id": "organization-sso-login-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x31; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .expect("测试密钥配置必须有效");
    SystemSecretCipher::new(&settings).expect("测试系统密钥必须有效")
}

fn oidc_login_material(
    cipher: &SystemSecretCipher,
    organization_id: af_domain::OrganizationId,
    provider_id: i64,
    provider_version: i64,
    request_digest: String,
) -> Result<(String, String, Value, String, String), Box<dyn Error>> {
    let start = OrganizationSsoLoginStart::new(request_digest)?;
    let state = start.state().to_owned();
    let state_digest = start.state_digest().to_owned();
    let verifier = start.pkce_verifier().to_owned();
    let nonce = start.nonce().to_owned();
    let secret = start.into_oidc_material().into_plain_system_secret()?;
    let envelope = cipher.encrypt_organization_sso_login_material(
        organization_id,
        provider_id,
        OrganizationSsoProtocol::Oidc,
        provider_version,
        &state_digest,
        &secret,
    )?;
    Ok((
        state,
        state_digest,
        json!({
            "version": 1,
            "algorithm": "xchacha20poly1305",
            "key_id": envelope.key_id(),
            "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
            "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
        }),
        verifier,
        nonce,
    ))
}
