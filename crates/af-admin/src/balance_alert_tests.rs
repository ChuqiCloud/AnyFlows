use std::{error::Error, sync::Mutex, time::Duration};

use af_account::SystemSecretCipher;
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    AdminUserLookupOutcome, AdminUserRepository, BalanceAlertRepository,
    BalanceAlertSettingsRepository, DatabaseOptions, EmailPasswordUpdate, EmailSettingsRepository,
    EmailSettingsWriteRecord, EmailTlsMode, InitialSetupOutcome, InitialSetupRecord,
    InitialSetupRepository, MigrationOptions, SiteSettingsRepository,
    SubscriptionBalanceAlertRepository,
};
use af_domain::Quota;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;

use super::{
    AdminBalanceAlertSettingsCommand, AdminBalanceAlertSettingsError,
    AdminBalanceAlertSettingsService, AdminUserCreateCommand, AdminUserStatus, AdminUserWriter,
    BalanceAlertTask, DatabaseAdminBalanceAlertSettingsService, DatabaseAdminUserWriter,
    EmailDelivery, EmailDeliveryFuture, EmailDeliveryRequest, SessionPrincipal, SessionRole,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ObservedBalanceAlert {
    recipient: String,
    subject: String,
    body: String,
}

#[derive(Default)]
struct RecordingDelivery {
    requests: Mutex<Vec<ObservedBalanceAlert>>,
}

impl RecordingDelivery {
    fn requests(&self) -> Vec<ObservedBalanceAlert> {
        self.requests.lock().unwrap().clone()
    }
}

impl EmailDelivery for RecordingDelivery {
    fn send<'a>(&'a self, request: EmailDeliveryRequest) -> EmailDeliveryFuture<'a> {
        self.requests.lock().unwrap().push(ObservedBalanceAlert {
            recipient: request.recipient().to_owned(),
            subject: request.subject().to_owned(),
            body: request.body().to_owned(),
        });
        Box::pin(async { Ok(()) })
    }
}

fn cipher() -> SystemSecretCipher {
    let settings: CredentialEncryptionSettings = serde_json::from_value(json!({
        "key_id": "balance-alert-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x51; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .unwrap();
    SystemSecretCipher::new(&settings).unwrap()
}

async fn installed_pool() -> Result<af_db::DatabasePool, Box<dyn Error>> {
    Ok(af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?)
}

#[tokio::test]
async fn admin_settings_require_admin_and_preserve_integer_boundaries() -> Result<(), Box<dyn Error>>
{
    let pool = installed_pool().await?;
    let repository = BalanceAlertSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let service = DatabaseAdminBalanceAlertSettingsService::new(repository);
    let admin = SessionPrincipal::new(af_domain::UserId::new(1)?, SessionRole::Admin);
    let user = SessionPrincipal::new(af_domain::UserId::new(2)?, SessionRole::User);

    let defaults = service.settings(admin).await?;
    assert!(!defaults.enabled());
    assert_eq!(defaults.default_threshold().units(), 1_000);
    assert_eq!(defaults.reminder_interval_seconds(), 86_400);
    assert!(!defaults.subscription_alert_enabled());
    assert_eq!(defaults.subscription_remaining_percent(), 20);
    assert_eq!(
        service.settings(user).await.unwrap_err(),
        AdminBalanceAlertSettingsError::Forbidden
    );

    let updated = service
        .update(
            admin,
            AdminBalanceAlertSettingsCommand::new(true, Quota::new(2_500)?, 7_200, true, 15)?,
        )
        .await?;
    assert!(updated.enabled());
    assert_eq!(updated.default_threshold().units(), 2_500);
    assert_eq!(updated.reminder_interval_seconds(), 7_200);
    assert!(updated.subscription_alert_enabled());
    assert_eq!(updated.subscription_remaining_percent(), 15);
    assert_eq!(updated.version(), 2);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn task_delivers_once_per_user_and_window() -> Result<(), Box<dyn Error>> {
    let pool = installed_pool().await?;
    let InitialSetupOutcome::Initialized { user_id: admin_id } =
        InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
            .initialize(InitialSetupRecord::new(
                "balance-admin".to_owned(),
                "a secure balance password".to_owned(),
            ))
            .await?
    else {
        panic!("测试数据库必须完成首次安装");
    };
    let admin_repository = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let AdminUserLookupOutcome::Found(admin_record) = admin_repository.get(admin_id).await? else {
        panic!("初始管理员必须存在");
    };
    let admin = SessionPrincipal::new(admin_id, SessionRole::Admin);
    DatabaseAdminUserWriter::new(admin_repository)
        .create(
            admin,
            AdminUserCreateCommand::new(
                "balance-user".to_owned(),
                Some("balance-user@example.com".to_owned()),
                None,
                SessionRole::User,
                AdminUserStatus::Enabled,
                admin_record.default_group_id(),
                500,
                None,
                None,
            )?,
        )
        .await?;

    let settings = BalanceAlertSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    settings
        .update(af_db::BalanceAlertSettingsWriteRecord::new(
            true,
            Quota::new(1_000)?,
            3_600,
        ))
        .await?;
    let email_settings = EmailSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    email_settings
        .update(EmailSettingsWriteRecord::new(
            true,
            "smtp.example.com".to_owned(),
            587,
            EmailTlsMode::StartTls,
            None,
            EmailPasswordUpdate::Clear,
            "sender@example.com".to_owned(),
            Some("AnyFlows".to_owned()),
            None,
            5,
        ))
        .await?;
    let delivery = std::sync::Arc::new(RecordingDelivery::default());
    let task = BalanceAlertTask::new(
        settings,
        BalanceAlertRepository::new(pool.clone(), Duration::from_secs(5))?,
        SubscriptionBalanceAlertRepository::new(pool.clone(), Duration::from_secs(5))?,
        email_settings,
        SiteSettingsRepository::new(pool.clone(), Duration::from_secs(5))?,
        cipher(),
        delivery.clone(),
    );

    let first = task.run_once().await?;
    assert_eq!(first.enqueued(), 1);
    assert_eq!(first.sent(), 1);
    assert_eq!(first.retry_scheduled(), 0);
    let second = task.run_once().await?;
    assert_eq!(second.enqueued(), 0);
    assert_eq!(second.sent(), 0);

    let requests = delivery.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].recipient, "balance-user@example.com");
    assert_eq!(requests[0].subject, "AnyFlows 余额预警");
    assert!(requests[0].body.contains("500"));
    assert!(requests[0].body.contains("1000"));
    pool.close().await?;
    Ok(())
}
