use std::{
    error::Error,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use af_account::SystemSecretCipher;
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    AdminUserLookupOutcome, AdminUserRepository, AuthChallengeRateLimitRepository,
    AuthChallengeRepository, DatabaseOptions, EmailPasswordUpdate, EmailSettingsRepository,
    EmailSettingsWriteRecord, EmailTlsMode, InitialSetupOutcome, InitialSetupRecord,
    InitialSetupRepository, MigrationOptions, RegistrationRepository, UserInvitationLookupOutcome,
    UserInvitationRepository, UserSessionLookupOutcome, UserSessionRepository,
};
use af_domain::{GroupId, TrustedClientIp, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use crate::{
    DatabaseRegistrationService, EmailDelivery, EmailDeliveryFuture, EmailDeliveryRequest,
    RegistrationCommand, RegistrationEmailVerificationCommand, RegistrationError,
    RegistrationPolicyCommand, RegistrationService, SessionPrincipal, SessionRole,
};

const PASSWORD: &str = "correct horse battery staple";

#[derive(Default)]
struct RecordingDelivery {
    bodies: Mutex<Vec<String>>,
}

impl RecordingDelivery {
    fn last_code(&self) -> String {
        self.bodies
            .lock()
            .unwrap()
            .last()
            .and_then(|body| body.split('：').nth(1))
            .and_then(|tail| tail.lines().next())
            .map(str::to_owned)
            .expect("测试邮件必须包含验证码")
    }
}

impl EmailDelivery for RecordingDelivery {
    fn send<'a>(&'a self, request: EmailDeliveryRequest) -> EmailDeliveryFuture<'a> {
        self.bodies.lock().unwrap().push(request.body().to_owned());
        Box::pin(async { Ok(()) })
    }
}

fn cipher() -> SystemSecretCipher {
    let settings: CredentialEncryptionSettings = serde_json::from_value(serde_json::json!({
        "key_id": "registration-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x24; CREDENTIAL_ENCRYPTION_KEY_BYTES]),
    }))
    .unwrap();
    SystemSecretCipher::new(&settings).unwrap()
}

async fn registration_service(
    pool: &af_db::DatabasePool,
    users: AdminUserRepository,
    delivery: Arc<dyn EmailDelivery>,
) -> Result<DatabaseRegistrationService, Box<dyn Error>> {
    let timeout = Duration::from_secs(5);
    EmailSettingsRepository::new(pool.clone(), timeout)?
        .update(EmailSettingsWriteRecord::new(
            true,
            "smtp.example.com".to_owned(),
            465,
            EmailTlsMode::Tls,
            None,
            EmailPasswordUpdate::Clear,
            "noreply@example.com".to_owned(),
            Some("AnyFlows".to_owned()),
            None,
            10,
        ))
        .await?;
    Ok(DatabaseRegistrationService::new(
        RegistrationRepository::new(pool.clone(), timeout)?,
        users,
        AuthChallengeRepository::new(pool.clone(), timeout)?,
        AuthChallengeRateLimitRepository::new(pool.clone(), timeout)?,
        EmailSettingsRepository::new(pool.clone(), timeout)?,
        cipher(),
        delivery,
        Some(&URL_SAFE_NO_PAD.encode([0x42; 32])),
    )?)
}

#[tokio::test]
async fn registration_reuses_identity_creation_and_counts_conflicts() -> Result<(), Box<dyn Error>>
{
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let InitialSetupOutcome::Initialized { user_id: admin_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "owner".to_owned(),
                "owner secure password".to_owned(),
            ))
            .await?
    else {
        panic!("空数据库必须完成首次安装");
    };
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let delivery = Arc::new(RecordingDelivery::default());
    let registration = registration_service(&pool, users.clone(), delivery.clone()).await?;
    let admin = SessionPrincipal::new(admin_id, SessionRole::Admin);
    let default_group_id = match users.get(admin_id).await? {
        AdminUserLookupOutcome::Found(user) => user.default_group_id(),
        AdminUserLookupOutcome::NotFound => panic!("首次安装管理员必须存在"),
    };
    let invitation_repository =
        UserInvitationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let UserInvitationLookupOutcome::Found(invitation_summary) =
        invitation_repository.get(admin_id).await?
    else {
        panic!("首次安装管理员必须拥有邀请码")
    };
    let invite_code = invitation_summary.invite_code().to_owned();
    registration
        .update_policy(
            admin,
            RegistrationPolicyCommand::new(true, true, default_group_id, 500, 40, true, 2, 86_400)?.with_invitee_quota(75)?,
        )
        .await?;

    let client_ip = TrustedClientIp::new("192.0.2.10".parse::<IpAddr>()?);
    registration
        .send_email_verification(
            client_ip,
            &RegistrationEmailVerificationCommand::new("registered@example.com".to_owned())?,
        )
        .await?;
    let verification_code = delivery.last_code();
    let command = RegistrationCommand::new(
        "registered-user".to_owned(),
        Some("registered@example.com".to_owned()),
        PASSWORD.to_owned(),
        Some(verification_code),
        Some(invite_code),
    )?;
    let registered = registration.register(client_ip, &command).await?;
    let record = match users.get(registered.user_id()).await? {
        AdminUserLookupOutcome::Found(user) => user,
        AdminUserLookupOutcome::NotFound => panic!("注册成功后用户必须存在"),
    };
    assert_eq!(record.role(), 0);
    assert_eq!(record.status(), 1);
    assert_eq!(record.default_group_id(), default_group_id);
    assert_eq!(record.quota(), 575);
    assert_eq!(record.email(), Some("registered@example.com"));
    let inviter = match users.get(admin_id).await? {
        AdminUserLookupOutcome::Found(user) => user,
        AdminUserLookupOutcome::NotFound => panic!("邀请人必须存在"),
    };
    assert_eq!(inviter.quota(), 40);
    let UserInvitationLookupOutcome::Found(summary) = invitation_repository.get(admin_id).await?
    else {
        panic!("邀请汇总必须存在")
    };
    assert_eq!(summary.invited_count(), 1);
    assert_eq!(summary.credited_count(), 1);
    assert_eq!(summary.current_rebate_quota().units(), 40);
    assert_eq!(
        UserSessionRepository::new(pool.clone(), Duration::from_secs(5))?
            .login("registered-user", PASSWORD.as_bytes())
            .await?,
        UserSessionLookupOutcome::Authenticated {
            user_id: registered.user_id(),
            role: 0,
            session_version: 1,
            totp_secret: None,
        }
    );

    assert_eq!(
        registration.register(client_ip, &command).await,
        Err(RegistrationError::VerificationRejected)
    );
    let another = RegistrationCommand::new(
        "another-user".to_owned(),
        Some("another@example.com".to_owned()),
        PASSWORD.to_owned(),
        Some("123456".to_owned()),
        None,
    )?;
    assert!(matches!(
        registration.register(client_ip, &another).await,
        Err(RegistrationError::RateLimited { .. })
    ));

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn policy_access_requires_admin_before_repository_work() -> Result<(), Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let registration =
        registration_service(&pool, users, Arc::new(RecordingDelivery::default())).await?;
    let normal_user = SessionPrincipal::new(UserId::new(9)?, SessionRole::User);

    assert_eq!(
        registration.policy(normal_user).await,
        Err(RegistrationError::Forbidden)
    );
    assert_eq!(
        registration
            .update_policy(
                normal_user,
                RegistrationPolicyCommand::new(
                    true,
                    false,
                    GroupId::new(1)?,
                    0,
                    0,
                    false,
                    5,
                    3_600,
                )?,
            )
            .await,
        Err(RegistrationError::Forbidden)
    );

    pool.close().await?;
    Ok(())
}


#[tokio::test]
async fn invitation_bonus_requires_a_valid_code_and_is_credited_once() -> Result<(), Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?, MigrationOptions::default(),
    ).await?;
    let InitialSetupOutcome::Initialized { user_id: admin_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new("owner".to_owned(), PASSWORD.to_owned())).await?
    else { panic!("setup must initialize the owner"); };
    let users = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let registration = registration_service(&pool, users.clone(), Arc::new(RecordingDelivery::default())).await?;
    let admin = SessionPrincipal::new(admin_id, SessionRole::Admin);
    let AdminUserLookupOutcome::Found(owner) = users.get(admin_id).await?
    else { panic!("owner must exist"); };
    let invitations = UserInvitationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let UserInvitationLookupOutcome::Found(summary) = invitations.get(admin_id).await?
    else { panic!("owner must have an invitation code"); };
    let code = summary.invite_code().to_owned();
    let policy = registration.update_policy(admin,
        RegistrationPolicyCommand::new(true, true, owner.default_group_id(), 100, 20, false, 20, 3600)?
            .with_invitee_quota(75)?).await?;
    assert_eq!(policy.invitation_invitee_quota(), 75);
    let ip = TrustedClientIp::new("192.0.2.101".parse::<IpAddr>()?);
    let ordinary = registration.register(ip, &RegistrationCommand::new(
        "ordinary-user".to_owned(), None, PASSWORD.to_owned(), None, None,
    )?).await?;
    let AdminUserLookupOutcome::Found(ordinary) = users.get(ordinary.user_id()).await?
    else { panic!("registered user must exist"); };
    assert_eq!(ordinary.quota(), 100);
    assert_eq!(registration.register(ip, &RegistrationCommand::new(
        "invalid-invite-user".to_owned(), None, PASSWORD.to_owned(), None,
        Some("af-AAAAAAAAAAAAAAAAAAAAAA".to_owned()),
    )?).await, Err(RegistrationError::InvitationRejected));
    let command = RegistrationCommand::new("referred-user".to_owned(), None, PASSWORD.to_owned(), None, Some(code.clone()))?;
    let referred = registration.register(ip, &command).await?;
    let AdminUserLookupOutcome::Found(referred) = users.get(referred.user_id()).await?
    else { panic!("referred user must exist"); };
    assert_eq!(referred.quota(), 175);
    assert!(registration.register(ip, &command).await.is_err());
    let UserInvitationLookupOutcome::Found(summary) = invitations.get(admin_id).await?
    else { panic!("invitation summary must exist"); };
    assert_eq!(summary.invited_count(), 1);
    assert_eq!(summary.credited_count(), 1);
    assert_eq!(summary.current_rebate_quota().units(), 20);
    assert_eq!(summary.inviter_reward_quota(), 20);
    assert_eq!(summary.new_user_quota(), 100);
    assert_eq!(summary.invited_user_bonus_quota(), 75);
    registration.update_policy(admin,
        RegistrationPolicyCommand::new(true, true, owner.default_group_id(), 100, 0, false, 20, 3600)?).await?;
    let no_bonus = registration.register(ip, &RegistrationCommand::new(
        "zero-bonus-user".to_owned(), None, PASSWORD.to_owned(), None, Some(code),
    )?).await?;
    let AdminUserLookupOutcome::Found(no_bonus) = users.get(no_bonus.user_id()).await?
    else { panic!("new user must exist"); };
    assert_eq!(no_bonus.quota(), 100);
    let UserInvitationLookupOutcome::Found(summary) = invitations.get(admin_id).await?
    else { panic!("invitation summary must exist"); };
    assert_eq!(summary.invited_count(), 2);
    assert_eq!(summary.credited_count(), 1);
    assert_eq!(summary.current_rebate_quota().units(), 20);
    pool.close().await?;
    Ok(())
}

#[test]
fn invitation_bonus_rejects_negative_values_and_balance_overflow() {
    let policy = RegistrationPolicyCommand::new(true, true, GroupId::new(1).unwrap(), i64::MAX, 0, false, 5, 3600).unwrap();
    assert!(policy.with_invitee_quota(-1).is_err());
    assert!(policy.with_invitee_quota(1).is_err());
    assert!(policy.with_invitee_quota(0).is_ok());
}
