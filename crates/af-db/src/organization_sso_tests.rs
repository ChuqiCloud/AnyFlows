use std::{error::Error, time::Duration};

use af_domain::{
    DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS, OrganizationEntitlementCapacity,
    OrganizationEntitlementSnapshot, OrganizationId, OrganizationInvitationKey, OrganizationRole,
    OrganizationSsoDnsTxtAnswers, OrganizationSsoDnsVerification, OrganizationSsoDomain,
    OrganizationSsoDomainProof, OrganizationSsoDomainProofToken, OrganizationSsoDomainStatus,
    OrganizationSsoIdentityBindingMethod, OrganizationSsoIdentityStatus, OrganizationSsoProtocol,
    OrganizationSsoProviderKey, OrganizationSsoProviderStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};

use crate::{
    DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions, OrganizationCreate,
    OrganizationEntitlementWrite, OrganizationInvitationCreate, OrganizationMembershipCreate,
    OrganizationRepository, OrganizationSsoDomainCreate, OrganizationSsoIdentityCreate,
    OrganizationSsoLoginTransactionCreate, OrganizationSsoLoginTransactionResult,
    OrganizationSsoProviderCreate, OrganizationSsoProviderRepository,
    OrganizationSsoRepositoryError,
    entity::{
        authentication_settings, groups, organization_memberships, organization_sso_identities,
        users,
    },
};

struct Fixture {
    pool: crate::DatabasePool,
    organizations: OrganizationRepository,
    sso: OrganizationSsoProviderRepository,
    organizations_ids: [OrganizationId; 2],
    users: [UserId; 3],
    group_id: i64,
}

#[tokio::test]
async fn sso_repository_closes_provider_scope_and_cas() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let first_key = provider_key(1);
    let first = fixture
        .sso
        .create_provider(
            fixture.organizations_ids[0],
            OrganizationSsoProviderCreate::new(
                first_key,
                OrganizationSsoProtocol::Oidc,
                OrganizationSsoProviderStatus::Active,
                "First SSO".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    assert_eq!(first.version(), 1);
    assert!(!first.secret_configured());
    assert_eq!(
        fixture
            .sso
            .provider(fixture.organizations_ids[1], first_key)
            .await,
        Err(OrganizationSsoRepositoryError::NotFound)
    );

    let updated = fixture
        .sso
        .update_provider(
            fixture.organizations_ids[0],
            first_key,
            crate::OrganizationSsoProviderUpdate::new(
                OrganizationSsoProtocol::Oidc,
                OrganizationSsoProviderStatus::Disabled,
                "First SSO Disabled".to_owned(),
                None,
                crate::OrganizationSsoProviderSecretChange::Clear,
                first.version(),
            ),
        )
        .await?;
    assert_eq!(updated.version(), 2);
    assert_eq!(updated.status(), OrganizationSsoProviderStatus::Disabled);
    assert_eq!(
        fixture
            .sso
            .update_provider(
                fixture.organizations_ids[0],
                first_key,
                crate::OrganizationSsoProviderUpdate::new(
                    OrganizationSsoProtocol::Oidc,
                    OrganizationSsoProviderStatus::Active,
                    "stale".to_owned(),
                    None,
                    crate::OrganizationSsoProviderSecretChange::Keep,
                    first.version(),
                ),
            )
            .await,
        Err(OrganizationSsoRepositoryError::Conflict)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn sso_repository_rejects_identity_conflicts_and_login_replays() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    let organization_id = fixture.organizations_ids[0];
    let key = provider_key(2);
    let provider = fixture
        .sso
        .create_provider(
            organization_id,
            OrganizationSsoProviderCreate::new(
                key,
                OrganizationSsoProtocol::Oidc,
                OrganizationSsoProviderStatus::Active,
                "Login SSO".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    let membership = fixture
        .organizations
        .add_membership(
            organization_id,
            OrganizationMembershipCreate::new(
                fixture.users[1],
                OrganizationRole::Member,
                None,
                Some(fixture.users[0]),
            ),
        )
        .await?;
    let identity = fixture
        .sso
        .create_identity(
            organization_id,
            OrganizationSsoIdentityCreate::new(
                key,
                digest('a'),
                membership.id(),
                OrganizationSsoIdentityBindingMethod::Explicit,
            ),
        )
        .await?;
    assert_eq!(identity.version(), 1);
    assert_eq!(identity.status(), OrganizationSsoIdentityStatus::Active);
    assert_eq!(
        identity.binding_method(),
        OrganizationSsoIdentityBindingMethod::Explicit
    );
    assert!(!format!("{identity:?}").contains(&digest('a')));
    assert_eq!(
        fixture
            .sso
            .create_identity(
                organization_id,
                OrganizationSsoIdentityCreate::new(
                    key,
                    digest('a'),
                    membership.id(),
                    OrganizationSsoIdentityBindingMethod::Explicit,
                ),
            )
            .await,
        Err(OrganizationSsoRepositoryError::Conflict)
    );
    let disabled = fixture
        .sso
        .disable_identity(organization_id, identity.id(), identity.version())
        .await?;
    assert_eq!(disabled.status(), OrganizationSsoIdentityStatus::Disabled);
    assert_eq!(
        fixture
            .sso
            .disable_identity(organization_id, identity.id(), identity.version())
            .await,
        Err(OrganizationSsoRepositoryError::Conflict)
    );

    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let transaction = fixture
        .sso
        .create_login_transaction(
            organization_id,
            OrganizationSsoLoginTransactionCreate::new(
                key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                digest('b'),
                digest('c'),
                now + 3_600,
            ),
        )
        .await?;
    let claimed = fixture
        .sso
        .claim_login_transaction(organization_id, digest('b'), now)
        .await?;
    assert!(claimed.claimed_at().is_some());
    assert_eq!(
        fixture
            .sso
            .claim_login_transaction(organization_id, digest('b'), now)
            .await,
        Err(OrganizationSsoRepositoryError::AlreadyProcessed)
    );
    let completed = fixture
        .sso
        .complete_login_transaction(
            organization_id,
            transaction.id(),
            OrganizationSsoLoginTransactionResult::new(
                1,
                Some(digest('d')),
                Some(now + 300),
                Some(digest('e')),
                Some(now + 300),
            ),
            now,
        )
        .await?;
    assert_eq!(completed.result_code(), Some(1));
    assert!(!format!("{completed:?}").contains(&digest('b')));
    let exchanged = fixture
        .sso
        .exchange_ticket(organization_id, digest('d'), now + 1)
        .await?;
    assert!(exchanged.exchanged_at().is_some());
    assert_eq!(
        fixture
            .sso
            .exchange_ticket(organization_id, digest('d'), now + 1)
            .await,
        Err(OrganizationSsoRepositoryError::AlreadyProcessed)
    );

    let expired = fixture
        .sso
        .create_login_transaction(
            organization_id,
            OrganizationSsoLoginTransactionCreate::new(
                key,
                OrganizationSsoProtocol::Oidc,
                provider.version(),
                digest('f'),
                digest('0'),
                now + 60,
            ),
        )
        .await?;
    assert_eq!(
        fixture
            .sso
            .claim_login_transaction(organization_id, digest('f'), expired.expires_at())
            .await,
        Err(OrganizationSsoRepositoryError::Expired)
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invited_sso_jit_is_atomic_and_idempotent() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let organization_id = fixture.organizations_ids[0];
    let provider_key = provider_key(4);
    fixture
        .sso
        .create_provider(
            organization_id,
            OrganizationSsoProviderCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                OrganizationSsoProviderStatus::Active,
                "JIT SSO".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    add_verified_domain(&fixture, organization_id, provider_key, "jit.example.com").await?;

    let settings = authentication_settings::Entity::find_by_id(1_i16)
        .one(fixture.pool.connection())
        .await?
        .expect("认证设置固定行必须存在");
    let mut settings = settings.into_active_model();
    settings.registration_default_group_id = Set(Some(fixture.group_id));
    settings.update(fixture.pool.connection()).await?;

    let fingerprint = [9_u8; 32];
    fixture
        .organizations
        .create_invitation(
            organization_id,
            OrganizationInvitationCreate::new(
                OrganizationInvitationKey::new([4_u8; 16])?,
                "jit-user@example.com".to_owned(),
                fingerprint,
                EncryptedCredentialEnvelope::new(
                    "jit-test-key".to_owned(),
                    [4_u8; 24],
                    vec![4_u8; 16],
                )?,
                [8_u8; 32],
                OrganizationRole::Member,
                None,
                fixture.users[0],
                None,
                None,
                None,
                TimeDateTimeWithTimeZone::now_utc()
                    .unix_timestamp()
                    .saturating_add(3_600),
            )?,
        )
        .await?;

    let subject_digest = digest('d');
    let membership = fixture
        .organizations
        .bind_invited_sso_identity(
            organization_id,
            provider_key,
            subject_digest.clone(),
            fingerprint,
        )
        .await?;
    assert_eq!(
        membership.status(),
        af_domain::OrganizationMembershipStatus::Active
    );
    assert_eq!(membership.role(), OrganizationRole::Member);

    let user = users::Entity::find_by_id(membership.user_id().get())
        .one(fixture.pool.connection())
        .await?
        .expect("JIT 用户必须存在");
    assert!(user.email.is_none());
    assert!(user.password_hash.is_none());
    assert_eq!(user.default_group_id, fixture.group_id);
    assert!(user.username.starts_with("sso_jit_"));

    let identity = organization_sso_identities::Entity::find()
        .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_sso_identities::Column::SubjectDigest.eq(subject_digest.clone()))
        .one(fixture.pool.connection())
        .await?
        .expect("JIT 身份必须存在");
    assert_eq!(
        identity.status,
        OrganizationSsoIdentityStatus::Active.code()
    );
    assert_eq!(
        identity.binding_method,
        OrganizationSsoIdentityBindingMethod::Invited.code()
    );
    assert_eq!(identity.organization_membership_id, membership.id().get());

    let invitations = fixture
        .organizations
        .list_invitations(organization_id)
        .await?;
    assert_eq!(invitations.len(), 1);
    assert_eq!(
        invitations[0].status(),
        af_domain::OrganizationInvitationStatus::Accepted
    );
    assert_eq!(
        invitations[0].accepted_by_user_id(),
        Some(membership.user_id())
    );

    let replay = fixture
        .organizations
        .bind_invited_sso_identity(organization_id, provider_key, subject_digest, fingerprint)
        .await?;
    assert_eq!(replay.id(), membership.id());
    assert_eq!(replay.user_id(), membership.user_id());
    assert_eq!(
        users::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        4
    );
    assert_eq!(
        organization_memberships::Entity::find()
            .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
            .count(fixture.pool.connection())
            .await?,
        2
    );
    assert_eq!(
        organization_sso_identities::Entity::find()
            .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
            .count(fixture.pool.connection())
            .await?,
        1
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invited_sso_jit_rejects_mismatch_expiry_and_rolls_back_conflict()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let organization_id = fixture.organizations_ids[0];
    let provider_key = provider_key(5);
    fixture
        .sso
        .create_provider(
            organization_id,
            OrganizationSsoProviderCreate::new(
                provider_key,
                OrganizationSsoProtocol::Oidc,
                OrganizationSsoProviderStatus::Active,
                "JIT failure SSO".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    add_verified_domain(
        &fixture,
        organization_id,
        provider_key,
        "failure.example.com",
    )
    .await?;
    let settings = authentication_settings::Entity::find_by_id(1_i16)
        .one(fixture.pool.connection())
        .await?
        .expect("认证设置固定行必须存在");
    let mut settings = settings.into_active_model();
    settings.registration_default_group_id = Set(Some(fixture.group_id));
    settings.update(fixture.pool.connection()).await?;

    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    fixture
        .organizations
        .create_invitation(
            organization_id,
            OrganizationInvitationCreate::new(
                OrganizationInvitationKey::new([5_u8; 16])?,
                "mismatch@example.com".to_owned(),
                [10_u8; 32],
                EncryptedCredentialEnvelope::new(
                    "jit-failure-key".to_owned(),
                    [5_u8; 24],
                    vec![5_u8; 16],
                )?,
                [9_u8; 32],
                OrganizationRole::Member,
                None,
                fixture.users[0],
                None,
                None,
                None,
                now.saturating_add(3_600),
            )?,
        )
        .await?;
    assert_eq!(
        fixture
            .organizations
            .bind_invited_sso_identity(organization_id, provider_key, digest('a'), [11_u8; 32],)
            .await,
        Err(crate::OrganizationRepositoryError::NotFound)
    );
    assert_eq!(
        organization_memberships::Entity::find()
            .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
            .count(fixture.pool.connection())
            .await?,
        1
    );

    fixture
        .organizations
        .create_invitation(
            organization_id,
            OrganizationInvitationCreate::new(
                OrganizationInvitationKey::new([6_u8; 16])?,
                "expired@example.com".to_owned(),
                [12_u8; 32],
                EncryptedCredentialEnvelope::new(
                    "jit-expired-key".to_owned(),
                    [6_u8; 24],
                    vec![6_u8; 16],
                )?,
                [10_u8; 32],
                OrganizationRole::Member,
                None,
                fixture.users[0],
                None,
                None,
                None,
                now.saturating_sub(1),
            )?,
        )
        .await?;
    assert_eq!(
        fixture
            .organizations
            .bind_invited_sso_identity(organization_id, provider_key, digest('b'), [12_u8; 32],)
            .await,
        Err(crate::OrganizationRepositoryError::InvitationExpired)
    );

    let collision_digest = digest('c');
    users::ActiveModel {
        username: Set(format!("sso_jit_{}", &collision_digest[..56])),
        email: Set(None),
        password_hash: Set(None),
        status: Set(1),
        default_group_id: Set(fixture.group_id),
        aff_code: Set("jit-collision-aff".to_owned()),
        settings: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;
    fixture
        .organizations
        .create_invitation(
            organization_id,
            OrganizationInvitationCreate::new(
                OrganizationInvitationKey::new([7_u8; 16])?,
                "collision@example.com".to_owned(),
                [13_u8; 32],
                EncryptedCredentialEnvelope::new(
                    "jit-collision-key".to_owned(),
                    [7_u8; 24],
                    vec![7_u8; 16],
                )?,
                [11_u8; 32],
                OrganizationRole::Member,
                None,
                fixture.users[0],
                None,
                None,
                None,
                now.saturating_add(3_600),
            )?,
        )
        .await?;
    assert_eq!(
        fixture
            .organizations
            .bind_invited_sso_identity(
                organization_id,
                provider_key,
                collision_digest,
                [13_u8; 32],
            )
            .await,
        Err(crate::OrganizationRepositoryError::Conflict)
    );
    let invitations = fixture
        .organizations
        .list_invitations(organization_id)
        .await?;
    assert_eq!(invitations.len(), 3);
    assert_eq!(
        invitations
            .iter()
            .filter(|invitation| invitation.status()
                == af_domain::OrganizationInvitationStatus::Pending)
            .count(),
        2
    );
    assert_eq!(
        organization_sso_identities::Entity::find()
            .filter(organization_sso_identities::Column::OrganizationId.eq(organization_id.get()))
            .count(fixture.pool.connection())
            .await?,
        0
    );
    assert_eq!(
        users::Entity::find()
            .count(fixture.pool.connection())
            .await?,
        4
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn sso_repository_closes_domain_cas_and_cross_organization_membership()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let organization_id = fixture.organizations_ids[0];
    let key = provider_key(3);
    fixture
        .sso
        .create_provider(
            organization_id,
            OrganizationSsoProviderCreate::new(
                key,
                OrganizationSsoProtocol::Saml,
                OrganizationSsoProviderStatus::Active,
                "Domain SSO".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let domain_value = OrganizationSsoDomain::new("example.com")?;
    let proof = OrganizationSsoDomainProof::from_token(
        domain_value.clone(),
        OrganizationSsoDomainProofToken::new(digest('a'))?,
        now,
        3_600,
    )?;
    let domain = fixture
        .sso
        .create_domain(
            organization_id,
            OrganizationSsoDomainCreate::new(
                key,
                domain_value.clone(),
                proof.digest(),
                proof.expires_at(),
            ),
        )
        .await?;
    let forged_proof = OrganizationSsoDomainProof::from_token(
        domain_value.clone(),
        OrganizationSsoDomainProofToken::new(digest('b'))?,
        now,
        3_600,
    )?;
    let forged_verification = OrganizationSsoDnsVerification::verify(
        &forged_proof,
        OrganizationSsoDomainStatus::Pending,
        &OrganizationSsoDnsTxtAnswers::new(vec![digest('b')]),
        now + 30,
        DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
    )?;
    assert_eq!(
        fixture
            .sso
            .verify_domain(
                organization_id,
                domain.id(),
                forged_verification,
                domain.version()
            )
            .await,
        Err(OrganizationSsoRepositoryError::Conflict)
    );

    let verification = OrganizationSsoDnsVerification::verify(
        &proof,
        OrganizationSsoDomainStatus::Pending,
        &OrganizationSsoDnsTxtAnswers::new(vec![digest('a')]),
        now + 30,
        DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
    )?;
    let verified = fixture
        .sso
        .verify_domain(organization_id, domain.id(), verification, domain.version())
        .await?;
    assert_eq!(verified.status(), OrganizationSsoDomainStatus::Verified);
    assert_eq!(
        fixture
            .sso
            .verify_domain(
                organization_id,
                domain.id(),
                OrganizationSsoDnsVerification::verify(
                    &proof,
                    OrganizationSsoDomainStatus::Verified,
                    &OrganizationSsoDnsTxtAnswers::new(vec![digest('a')]),
                    now + 60,
                    DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
                )?,
                domain.version(),
            )
            .await,
        Err(OrganizationSsoRepositoryError::Conflict)
    );

    fixture
        .sso
        .create_provider(
            fixture.organizations_ids[1],
            OrganizationSsoProviderCreate::new(
                key,
                OrganizationSsoProtocol::Saml,
                OrganizationSsoProviderStatus::Active,
                "Second Domain SSO".to_owned(),
                None,
                None,
            ),
        )
        .await?;
    assert_eq!(
        fixture
            .sso
            .create_domain(
                fixture.organizations_ids[1],
                OrganizationSsoDomainCreate::new(key, domain_value, digest('c'), now + 3_600,),
            )
            .await,
        Err(OrganizationSsoRepositoryError::Conflict)
    );

    let expired_domain_value = OrganizationSsoDomain::new("expired.example.com")?;
    let expired_proof = OrganizationSsoDomainProof::from_token(
        expired_domain_value.clone(),
        OrganizationSsoDomainProofToken::new(digest('d'))?,
        now,
        3_600,
    )?;
    let expired_domain = fixture
        .sso
        .create_domain(
            organization_id,
            OrganizationSsoDomainCreate::new(
                key,
                expired_domain_value,
                expired_proof.digest(),
                now - 1,
            ),
        )
        .await?;
    let expired_verification = OrganizationSsoDnsVerification::verify(
        &expired_proof,
        OrganizationSsoDomainStatus::Pending,
        &OrganizationSsoDnsTxtAnswers::new(vec![digest('d')]),
        now + 30,
        DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
    )?;
    assert_eq!(
        fixture
            .sso
            .verify_domain(
                organization_id,
                expired_domain.id(),
                expired_verification,
                expired_domain.version(),
            )
            .await,
        Err(OrganizationSsoRepositoryError::Expired)
    );
    fixture.pool.close().await?;
    Ok(())
}

fn provider_key(value: u8) -> OrganizationSsoProviderKey {
    OrganizationSsoProviderKey::new([value; 16]).expect("测试 Provider key 非零")
}

fn digest(value: char) -> String {
    std::iter::repeat_n(value, 64).collect()
}

async fn add_verified_domain(
    fixture: &Fixture,
    organization_id: OrganizationId,
    provider_key: OrganizationSsoProviderKey,
    domain_name: &str,
) -> Result<(), Box<dyn Error>> {
    let now = TimeDateTimeWithTimeZone::now_utc().unix_timestamp();
    let domain_value = OrganizationSsoDomain::new(domain_name)?;
    let proof = OrganizationSsoDomainProof::from_token(
        domain_value.clone(),
        OrganizationSsoDomainProofToken::new(digest('e'))?,
        now,
        3_600,
    )?;
    let domain = fixture
        .sso
        .create_domain(
            organization_id,
            OrganizationSsoDomainCreate::new(
                provider_key,
                domain_value,
                proof.digest(),
                proof.expires_at(),
            ),
        )
        .await?;
    let verification = OrganizationSsoDnsVerification::verify(
        &proof,
        OrganizationSsoDomainStatus::Pending,
        &OrganizationSsoDnsTxtAnswers::new(vec![digest('e')]),
        now + 30,
        DEFAULT_ORGANIZATION_SSO_RECHECK_INTERVAL_SECONDS,
    )?;
    fixture
        .sso
        .verify_domain(organization_id, domain.id(), verification, domain.version())
        .await?;
    Ok(())
}

fn active_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn Error>> {
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
        "sso-test".to_owned(),
        None,
        None,
    )?)
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group = groups::ActiveModel {
        name: Set("sso-test-group".to_owned()),
        display_name: Set("SSO 测试分组".to_owned()),
        flags: Set(serde_json::json!({})),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let mut user_ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("sso-test-user-{index}")),
            email: Set(Some(format!("sso-test-user-{index}@example.com"))),
            password_hash: Set(None),
            status: Set(1),
            default_group_id: Set(group.id),
            aff_code: Set(format!("sso-test-aff-{index}")),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        user_ids.push(UserId::new(user.id)?);
    }
    let organizations = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let mut organization_ids = Vec::new();
    for (index, user_id) in user_ids.iter().take(2).enumerate() {
        let (organization, _) = organizations
            .create(OrganizationCreate::new(
                format!("SSO Lab {index}"),
                format!("sso-lab-{index}"),
                *user_id,
                active_entitlement()?,
            ))
            .await?
            .into_parts();
        organization_ids.push(organization.id());
    }
    let sso = OrganizationSsoProviderRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok(Fixture {
        pool,
        organizations,
        sso,
        organizations_ids: organization_ids.try_into().expect("测试企业数量固定为二"),
        users: user_ids.try_into().expect("测试用户数量固定为三"),
        group_id: group.id,
    })
}
