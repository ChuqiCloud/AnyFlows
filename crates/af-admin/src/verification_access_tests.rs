use std::{error::Error, sync::Arc, time::Duration};

use af_db::{
    AccountVerificationSubmit, AdminTokenCreateRecord, AdminTokenRepository, AdminTokenWriteRecord,
    AdminUserCreateRecord, AdminUserLookupOutcome, AdminUserRepository, DatabaseOptions,
    DatabasePool, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MigrationOptions, RedemptionRepository, TokenAuthRepository, VerificationMaterialWrite,
};
use af_domain::{TrustedClientIp, UserId};

use super::*;
use crate::{
    AdminRedemptionBatchCreateCommand, DatabaseRedemptionService, DatabaseTokenAuthenticator,
    PresentedApiKey, RedemptionService, RedemptionServiceError, SessionPrincipal, SessionRole,
    TokenAuthenticationError, TokenAuthenticator, UserRedemptionCommand,
};

const TIMEOUT: Duration = Duration::from_secs(5);
const TEST_KEY: &str = "sk-af-AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";

struct Fixture {
    pool: DatabasePool,
    settings: AccountVerificationSettingsRepository,
    verifications: AccountVerificationRepository,
    access: Arc<VerificationAccessService>,
    user: UserId,
    reviewer: UserId,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let InitialSetupOutcome::Initialized { user_id: reviewer } =
            InitialSetupRepository::new(pool.clone(), TIMEOUT)?
                .initialize(InitialSetupRecord::new(
                    "verification-reviewer".to_owned(),
                    "verification secure password".to_owned(),
                ))
                .await?
        else {
            panic!("empty database must initialize");
        };
        let users = AdminUserRepository::new(pool.clone(), TIMEOUT)?;
        let AdminUserLookupOutcome::Found(admin) = users.get(reviewer).await? else {
            panic!("reviewer must exist");
        };
        let user = users
            .create(AdminUserCreateRecord::new(
                "verification-applicant".to_owned(),
                None,
                None,
                0,
                1,
                admin.default_group_id(),
                0,
                None,
                None,
            ))
            .await?
            .user_id();
        let settings = AccountVerificationSettingsRepository::new(pool.clone(), TIMEOUT);
        let verifications = AccountVerificationRepository::new(pool.clone());
        let access = Arc::new(VerificationAccessService::new(
            settings.clone(),
            verifications.clone(),
        ));
        Ok(Self {
            pool,
            settings,
            verifications,
            access,
            user,
            reviewer,
        })
    }

    async fn configure(&self, topup: bool, api: bool) -> Result<(), Box<dyn Error>> {
        let mut settings = self.settings.settings().await?;
        settings.require_verification_for_topup = topup;
        settings.require_verification_for_api = api;
        let version = settings.version;
        self.settings.update(settings, version).await?;
        Ok(())
    }

    async fn approve(&self, kind: &str) -> Result<(), Box<dyn Error>> {
        let case = self
            .verifications
            .submit(self.user, submission(kind))
            .await?;
        self.verifications
            .decide(
                case.id,
                case.version,
                self.reviewer,
                4,
                None,
                "approve".to_owned(),
            )
            .await?;
        Ok(())
    }
}

fn submission(kind: &str) -> AccountVerificationSubmit {
    let bytes = b"%PDF-1.7\nidentity fixture".to_vec();
    AccountVerificationSubmit {
        kind: kind.to_owned(),
        provider: "manual".to_owned(),
        provider_reference: None,
        provider_action_url: None,
        provider_status: None,
        document_country: "US".to_owned(),
        document_type: if kind == "enterprise" {
            "business_registration".to_owned()
        } else {
            "passport".to_owned()
        },
        document_number: Some("A12345678".to_owned()),
        subject_name: "Verification applicant".to_owned(),
        summary: "Identity supporting document".to_owned(),
        materials: vec![VerificationMaterialWrite {
            kind: "identity".to_owned(),
            object_reference: "unused".to_owned(),
            file_name: "identity.pdf".to_owned(),
            content_type: "application/pdf".to_owned(),
            size_bytes: bytes.len() as i64,
            content_bytes: bytes,
        }],
    }
}

#[tokio::test]
async fn independent_gates_use_current_settings_for_every_request() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let defaults = fixture.access.status(fixture.user).await?;
    assert!(!defaults.verified);
    assert!(!defaults.require_verification_for_topup);
    assert!(!defaults.require_verification_for_api);
    for (topup, api) in [(false, false), (true, false), (false, true), (true, true)] {
        fixture.configure(topup, api).await?;
        assert_eq!(
            fixture.access.ensure_topup(fixture.user).await,
            if topup {
                Err(VerificationAccessError::Required)
            } else {
                Ok(())
            }
        );
        assert_eq!(
            fixture.access.ensure_api(fixture.user).await,
            if api {
                Err(VerificationAccessError::Required)
            } else {
                Ok(())
            }
        );
    }
    Ok(())
}

#[tokio::test]
async fn either_approved_kind_survives_pending_and_rejected_replacements()
-> Result<(), Box<dyn Error>> {
    for kind in ["individual", "enterprise"] {
        let fixture = Fixture::new().await?;
        fixture.configure(true, true).await?;
        let initial = fixture
            .verifications
            .submit(fixture.user, submission(kind))
            .await?;
        assert_eq!(
            fixture.access.ensure_api(fixture.user).await,
            Err(VerificationAccessError::Required)
        );
        fixture
            .verifications
            .decide(
                initial.id,
                initial.version,
                fixture.reviewer,
                5,
                Some("Identity needs correction".to_owned()),
                "reject-initial".to_owned(),
            )
            .await?;
        assert_eq!(
            fixture.access.ensure_topup(fixture.user).await,
            Err(VerificationAccessError::Required)
        );
        fixture.approve(kind).await?;
        assert!(fixture.access.status(fixture.user).await?.verified);
        let pending = fixture
            .verifications
            .submit(fixture.user, submission(kind))
            .await?;
        assert_eq!(fixture.access.ensure_topup(fixture.user).await, Ok(()));
        assert_eq!(fixture.access.ensure_api(fixture.user).await, Ok(()));
        fixture
            .verifications
            .decide(
                pending.id,
                pending.version,
                fixture.reviewer,
                5,
                Some("Replacement needs correction".to_owned()),
                "reject-replacement".to_owned(),
            )
            .await?;
        assert!(fixture.access.status(fixture.user).await?.verified);
        assert_eq!(fixture.access.ensure_api(fixture.user).await, Ok(()));
    }
    Ok(())
}

#[tokio::test]
async fn unavailable_settings_do_not_disable_enforcement() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    fixture.pool.close().await?;
    assert_eq!(
        fixture.access.ensure_topup(fixture.user).await,
        Err(VerificationAccessError::Unavailable)
    );
    assert_eq!(
        fixture.access.ensure_api(fixture.user).await,
        Err(VerificationAccessError::Unavailable)
    );
    assert_eq!(
        fixture.access.status(fixture.user).await,
        Err(VerificationAccessError::Unavailable)
    );
    Ok(())
}

#[tokio::test]
async fn api_keys_and_playground_share_live_verification_enforcement() -> Result<(), Box<dyn Error>>
{
    let fixture = Fixture::new().await?;
    let presented = PresentedApiKey::parse(TEST_KEY)?;
    let digest = presented.digest();
    AdminTokenRepository::new(fixture.pool.clone(), TIMEOUT)?
        .create_token(AdminTokenCreateRecord::new(
            fixture.user,
            digest.as_str().to_owned(),
            presented.display_prefix().as_str().to_owned(),
            AdminTokenWriteRecord::new(
                "verification-key".to_owned(),
                1,
                None,
                0,
                true,
                None,
                None,
                None,
                false,
                None,
                None,
                None,
                None,
            ),
        ))
        .await?;
    let authenticator =
        DatabaseTokenAuthenticator::new(TokenAuthRepository::new(fixture.pool.clone(), TIMEOUT)?)
            .with_verification_access(fixture.access.clone());
    let ip = TrustedClientIp::new("192.0.2.10".parse()?);
    fixture.configure(true, false).await?;
    assert!(authenticator.authenticate(&digest, ip).await.is_ok());
    assert!(
        authenticator
            .authenticate_playground(fixture.user)
            .await
            .is_ok()
    );
    fixture.configure(false, true).await?;
    assert_eq!(
        authenticator.authenticate(&digest, ip).await,
        Err(TokenAuthenticationError::VerificationRequired)
    );
    assert_eq!(
        authenticator.authenticate_playground(fixture.user).await,
        Err(TokenAuthenticationError::VerificationRequired)
    );
    fixture.approve("individual").await?;
    assert!(authenticator.authenticate(&digest, ip).await.is_ok());
    assert!(
        authenticator
            .authenticate_playground(fixture.user)
            .await
            .is_ok()
    );
    Ok(())
}

#[tokio::test]
async fn blocked_redemption_does_not_consume_the_code() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let service =
        DatabaseRedemptionService::new(RedemptionRepository::new(fixture.pool.clone(), TIMEOUT)?)
            .with_verification_access(fixture.access.clone());
    let issued = service
        .create(
            SessionPrincipal::new(fixture.reviewer, SessionRole::Admin),
            AdminRedemptionBatchCreateCommand::new("verification-credit".to_owned(), 100, 1, None)?,
        )
        .await?;
    let code = issued.codes()[0].code().expose_secret();
    let principal = SessionPrincipal::new(fixture.user, SessionRole::User);
    fixture.configure(true, false).await?;
    assert_eq!(
        service
            .redeem(principal, UserRedemptionCommand::new(code.to_owned())?)
            .await,
        Err(RedemptionServiceError::VerificationRequired)
    );
    fixture.configure(false, true).await?;
    let redeemed = service
        .redeem(principal, UserRedemptionCommand::new(code.to_owned())?)
        .await?;
    assert!(!redeemed.replayed());
    assert_eq!(redeemed.quota_amount().units(), 100);
    Ok(())
}
