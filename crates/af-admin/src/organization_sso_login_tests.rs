use std::{
    error::Error,
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_account::{
    OrganizationInvitationCipher, OrganizationSsoLoginStart, PlainOrganizationInvitationEmail,
    SystemSecretCipher, organization_sso_login_state_digest, organization_sso_login_ticket_digest,
};
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    AdminGroupRepository, DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS,
    DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS, DatabaseOptions, InitialSetupOutcome,
    InitialSetupRecord, InitialSetupRepository, MigrationOptions, OrganizationCreate,
    OrganizationEntitlementWrite, OrganizationInvitationCreate, OrganizationRepository,
    OrganizationSsoDomainCreate, OrganizationSsoLoginTransactionCreate,
    OrganizationSsoLoginTransactionResult, OrganizationSsoProviderCreate,
    OrganizationSsoProviderRepository, RegistrationPolicyWriteRecord, RegistrationRepository,
};
use af_domain::{
    DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationId, OrganizationInvitationKey, OrganizationRole,
    OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsVerification, OrganizationSsoDomain,
    OrganizationSsoDomainProof, OrganizationSsoDomainProofToken, OrganizationSsoDomainStatus,
    OrganizationSsoProtocol, OrganizationSsoProviderKey, OrganizationSsoProviderStatus,
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
    let service = DatabaseOrganizationSsoLoginTransactionService::new(
        repository.clone(),
        cipher.clone(),
        invitation_cipher(),
    );

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

#[tokio::test]
async fn exchanged_oidc_ticket_binds_invited_identity_once() -> Result<(), Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await
    .map_err(|error| test_context("连接并迁移测试数据库", error))?;
    let InitialSetupOutcome::Initialized { user_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "sso-jit-owner".to_owned(),
                "sso jit owner password".to_owned(),
            ))
            .await
            .map_err(|error| test_context("完成首次安装", error))?
    else {
        panic!("测试数据库必须完成首次安装");
    };
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let organization_id = organizations
        .create(OrganizationCreate::new(
            "SSO JIT Organization".to_owned(),
            "sso-jit-organization".to_owned(),
            user_id,
            sso_entitlement()?,
        ))
        .await
        .map_err(|error| test_context("创建企业组织", error))?
        .into_parts()
        .0
        .id();
    let repository = OrganizationSsoProviderRepository::new(pool.clone(), Duration::from_secs(5))?;
    let provider_key = OrganizationSsoProviderKey::new([0x75; 16])?;
    let provider = repository
        .create_provider(
            organization_id,
            OrganizationSsoProviderCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                OrganizationSsoProviderStatus::Active,
                "JIT OIDC Provider".to_owned(),
                None,
                None,
            ),
        )
        .await
        .map_err(|error| test_context("创建 OIDC Provider", error))?;
    add_verified_domain(
        &repository,
        organization_id,
        provider_key,
        "jit.example.com",
    )
    .await
    .map_err(|error| test_context("验证企业 SSO 域名", error))?;

    let default_group = AdminGroupRepository::new(pool.clone(), Duration::from_secs(5))?
        .list(None, 1)
        .await
        .map_err(|error| test_context("读取默认分组", error))?
        .into_parts()
        .0
        .into_iter()
        .next()
        .expect("首次安装必须创建默认分组");
    RegistrationRepository::new(pool.clone(), Duration::from_secs(5))?
        .update_policy(RegistrationPolicyWriteRecord::new(
            true,
            true,
            default_group.group_id(),
            0,
            0,
            false,
            DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS,
            DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
        ))
        .await
        .map_err(|error| test_context("写入注册策略", error))?;

    let invitation_cipher = invitation_cipher();
    let invitation_email =
        PlainOrganizationInvitationEmail::new("jit-user@example.com".to_owned())?;
    let invitation_fingerprint =
        invitation_cipher.email_fingerprint(organization_id, &invitation_email);
    let invitation_key = OrganizationInvitationKey::new([0x76; 16])?;
    organizations
        .create_invitation(
            organization_id,
            OrganizationInvitationCreate::new(
                invitation_key,
                invitation_email.expose_email().to_owned(),
                invitation_fingerprint,
                invitation_cipher.encrypt_email(
                    organization_id,
                    invitation_key,
                    &invitation_email,
                )?,
                [0x77; 32],
                OrganizationRole::Member,
                None,
                user_id,
                None,
                None,
                None,
                now_timestamp().saturating_add(3_600),
            )?,
        )
        .await
        .map_err(|error| test_context("创建企业邀请", error))?;

    let cipher = cipher();
    let service = DatabaseOrganizationSsoLoginTransactionService::new_with_organization_repository(
        repository.clone(),
        organizations.clone(),
        cipher.clone(),
        invitation_cipher,
    );
    let (state, state_digest, envelope, _, _) = oidc_login_material(
        &cipher,
        organization_id,
        provider.id(),
        provider.version(),
        "j".repeat(64),
    )?;
    let transaction = repository
        .create_login_transaction(
            organization_id,
            OrganizationSsoLoginTransactionCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                state_digest,
                "j".repeat(64),
                now_timestamp().saturating_add(3_600),
            )
            .with_login_material_envelope(envelope),
        )
        .await
        .map_err(|error| test_context("完成登录事务", error))?;
    let claimed = service
        .claim(organization_id, state)
        .await
        .map_err(|error| test_context("领取登录事务", error))?;
    assert_eq!(claimed.transaction_id(), transaction.id());

    let ticket = "sso-jit-ticket-once";
    let now = now_timestamp();
    repository
        .complete_login_transaction(
            organization_id,
            transaction.id(),
            OrganizationSsoLoginTransactionResult::new(
                1,
                Some(organization_sso_login_ticket_digest(ticket)?),
                Some(now + 600),
                Some("k".repeat(64)),
                Some(now + 600),
            )
            .with_invitation_email_fingerprint(invitation_fingerprint),
            now,
        )
        .await
        .map_err(|error| test_context("ticket JIT 绑定", error))?;

    let membership = service
        .exchange_ticket_and_bind_invited_identity(organization_id, ticket.to_owned())
        .await
        .map_err(|error| test_context("ticket JIT 服务绑定", error))?;
    assert_eq!(membership.role(), OrganizationRole::Member);
    assert_eq!(
        membership.status(),
        af_domain::OrganizationMembershipStatus::Active
    );
    let invitations = organizations
        .list_invitations(organization_id)
        .await
        .map_err(|error| test_context("读取邀请列表", error))?;
    assert_eq!(
        invitations[0].status(),
        af_domain::OrganizationInvitationStatus::Accepted
    );
    assert_eq!(
        invitations[0].accepted_by_user_id(),
        Some(membership.user_id())
    );
    assert!(matches!(
        service
            .exchange_ticket_and_bind_invited_identity(organization_id, ticket.to_owned())
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));

    let (missing_state, missing_state_digest, missing_envelope, _, _) = oidc_login_material(
        &cipher,
        organization_id,
        provider.id(),
        provider.version(),
        "m".repeat(64),
    )?;
    let missing_transaction = repository
        .create_login_transaction(
            organization_id,
            OrganizationSsoLoginTransactionCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                missing_state_digest,
                "m".repeat(64),
                now + 600,
            )
            .with_login_material_envelope(missing_envelope),
        )
        .await
        .map_err(|error| test_context("创建缺少邮箱指纹的登录事务", error))?;
    service
        .claim(organization_id, missing_state)
        .await
        .map_err(|error| test_context("领取缺少邮箱指纹的登录事务", error))?;
    let missing_ticket = "sso-jit-ticket-without-email-fingerprint";
    repository
        .complete_login_transaction(
            organization_id,
            missing_transaction.id(),
            OrganizationSsoLoginTransactionResult::new(
                1,
                Some(organization_sso_login_ticket_digest(missing_ticket)?),
                Some(now + 600),
                Some("n".repeat(64)),
                Some(now + 600),
            ),
            now,
        )
        .await
        .map_err(|error| test_context("完成缺少邮箱指纹的登录事务", error))?;
    assert!(matches!(
        service
            .exchange_ticket_and_bind_invited_identity(organization_id, missing_ticket.to_owned(),)
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::Rejected)
    ));
    assert!(matches!(
        service
            .exchange_ticket_and_bind_invited_identity(organization_id, missing_ticket.to_owned(),)
            .await,
        Err(OrganizationSsoLoginTransactionServiceError::AlreadyProcessed)
    ));
    pool.close().await?;
    Ok(())
}

async fn add_verified_domain(
    repository: &OrganizationSsoProviderRepository,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
    domain_name: &str,
) -> Result<(), Box<dyn Error>> {
    let now = now_timestamp();
    let domain = OrganizationSsoDomain::new(domain_name)?;
    let proof = OrganizationSsoDomainProof::from_token(
        domain.clone(),
        OrganizationSsoDomainProofToken::new("e".repeat(64))?,
        now,
        3_600,
    )?;
    let record = repository
        .create_domain(
            organization_id,
            OrganizationSsoDomainCreate::new(
                provider_key,
                domain,
                proof.digest(),
                proof.expires_at(),
            ),
        )
        .await
        .map_err(|error| test_context("创建企业 SSO 域名", error))?;
    let verification = OrganizationSsoDnsVerification::verify(
        &proof,
        OrganizationSsoDomainStatus::Pending,
        &OrganizationSsoDnsTxtAnswers::new(vec!["e".repeat(64)]),
        now + 30,
        DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
    )?;
    repository
        .verify_domain(organization_id, record.id(), verification, record.version())
        .await
        .map_err(|error| test_context("确认企业 SSO 域名", error))?;
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

fn sso_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
    Ok(OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 10)?,
            true,
            true,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )?,
        "organization-sso-jit-test".to_owned(),
        None,
        None,
    )?)
}

fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间必须不早于 Unix 纪元")
        .as_secs() as i64
}

fn test_context(context: &str, error: impl fmt::Debug) -> std::io::Error {
    std::io::Error::other(format!("{context}失败: {error:?}"))
}

fn cipher() -> SystemSecretCipher {
    SystemSecretCipher::new(&cipher_settings()).expect("测试系统密钥必须有效")
}

fn invitation_cipher() -> OrganizationInvitationCipher {
    OrganizationInvitationCipher::new(&cipher_settings()).expect("测试邀请密钥必须有效")
}

fn cipher_settings() -> CredentialEncryptionSettings {
    serde_json::from_value(json!({
        "key_id": "organization-sso-login-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x31; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .expect("测试密钥配置必须有效")
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
