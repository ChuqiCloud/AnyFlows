use std::{error::Error, net::IpAddr, time::Duration};

use af_db::{
    AdminUserLookupOutcome, AdminUserRepository, DatabaseOptions, InitialSetupOutcome,
    InitialSetupRecord, InitialSetupRepository, MigrationOptions, RegistrationRepository,
    UserSessionLookupOutcome, UserSessionRepository,
};
use af_domain::{GroupId, TrustedClientIp, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use crate::{
    DatabaseRegistrationService, RegistrationCommand, RegistrationError, RegistrationPolicyCommand,
    RegistrationService, SessionPrincipal, SessionRole,
};

const PASSWORD: &str = "correct horse battery staple";

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
    let registration = DatabaseRegistrationService::new(
        RegistrationRepository::new(pool.clone(), Duration::from_secs(5))?,
        users.clone(),
        Some(&URL_SAFE_NO_PAD.encode([0x42; 32])),
    )?;
    let admin = SessionPrincipal::new(admin_id, SessionRole::Admin);
    let default_group_id = match users.get(admin_id).await? {
        AdminUserLookupOutcome::Found(user) => user.default_group_id(),
        AdminUserLookupOutcome::NotFound => panic!("首次安装管理员必须存在"),
    };
    registration
        .update_policy(
            admin,
            RegistrationPolicyCommand::new(true, true, default_group_id, 500, true, 2, 86_400)?,
        )
        .await?;

    let client_ip = TrustedClientIp::new("192.0.2.10".parse::<IpAddr>()?);
    let command = RegistrationCommand::new(
        "registered-user".to_owned(),
        Some("registered@example.com".to_owned()),
        PASSWORD.to_owned(),
    )?;
    let registered = registration.register(client_ip, &command).await?;
    let record = match users.get(registered.user_id()).await? {
        AdminUserLookupOutcome::Found(user) => user,
        AdminUserLookupOutcome::NotFound => panic!("注册成功后用户必须存在"),
    };
    assert_eq!(record.role(), 0);
    assert_eq!(record.status(), 1);
    assert_eq!(record.default_group_id(), default_group_id);
    assert_eq!(record.quota(), 500);
    assert_eq!(record.email(), Some("registered@example.com"));
    assert_eq!(
        UserSessionRepository::new(pool.clone(), Duration::from_secs(5))?
            .login("registered-user", PASSWORD.as_bytes())
            .await?,
        UserSessionLookupOutcome::Authenticated {
            user_id: registered.user_id(),
            role: 0,
        }
    );

    assert_eq!(
        registration.register(client_ip, &command).await,
        Err(RegistrationError::Conflict)
    );
    let another = RegistrationCommand::new(
        "another-user".to_owned(),
        Some("another@example.com".to_owned()),
        PASSWORD.to_owned(),
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
    let registration = DatabaseRegistrationService::new(
        RegistrationRepository::new(pool.clone(), Duration::from_secs(5))?,
        users,
        Some(&URL_SAFE_NO_PAD.encode([0x42; 32])),
    )?;
    let normal_user = SessionPrincipal::new(UserId::new(9)?, SessionRole::User);

    assert_eq!(
        registration.policy(normal_user).await,
        Err(RegistrationError::Forbidden)
    );
    assert_eq!(
        registration
            .update_policy(
                normal_user,
                RegistrationPolicyCommand::new(true, false, GroupId::new(1)?, 0, false, 5, 3_600,)?,
            )
            .await,
        Err(RegistrationError::Forbidden)
    );

    pool.close().await?;
    Ok(())
}
