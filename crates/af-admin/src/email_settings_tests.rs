use std::{error::Error, sync::Mutex, time::Duration};

use af_account::{SystemSecretCipher, SystemSecretKind};
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{DatabaseOptions, DatabaseTimestamp, EmailSettingsRepository, MigrationOptions};
use af_domain::{Quota, UserId};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;

use super::{
    AdminEmailSettingsCommand, AdminEmailSettingsError, AdminEmailSettingsService,
    AdminEmailTestCommand, AdminEmailTlsMode, DatabaseAdminEmailSettingsService, EmailDelivery,
    EmailDeliveryError, EmailDeliveryFuture, EmailDeliveryRequest, SessionPrincipal, SessionRole,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ObservedDelivery {
    host: String,
    username: Option<String>,
    password: Option<String>,
    from_address: String,
    recipient: String,
    timeout_seconds: u16,
}

struct RecordingDelivery {
    result: Result<(), EmailDeliveryError>,
    requests: Mutex<Vec<ObservedDelivery>>,
}

impl RecordingDelivery {
    fn successful() -> Self {
        Self {
            result: Ok(()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn failing() -> Self {
        Self {
            result: Err(EmailDeliveryError::Failed),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<ObservedDelivery> {
        self.requests.lock().unwrap().clone()
    }
}

impl EmailDelivery for RecordingDelivery {
    fn send<'a>(&'a self, request: EmailDeliveryRequest) -> EmailDeliveryFuture<'a> {
        self.requests.lock().unwrap().push(ObservedDelivery {
            host: request.host().to_owned(),
            username: request.username().map(str::to_owned),
            password: request.password().map(str::to_owned),
            from_address: request.from_address().to_owned(),
            recipient: request.recipient().to_owned(),
            timeout_seconds: request.timeout_seconds(),
        });
        let result = self.result;
        Box::pin(async move { result })
    }
}

async fn installed_repository()
-> Result<(af_db::DatabasePool, EmailSettingsRepository), Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = EmailSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository))
}

fn cipher() -> SystemSecretCipher {
    let settings: CredentialEncryptionSettings = serde_json::from_value(json!({
        "key_id": "email-settings-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .unwrap();
    SystemSecretCipher::new(&settings).unwrap()
}

fn principal(role: SessionRole) -> SessionPrincipal {
    SessionPrincipal::new(UserId::new(7).unwrap(), role)
}

fn settings_command(password: Option<&str>) -> AdminEmailSettingsCommand {
    AdminEmailSettingsCommand::new(
        true,
        "smtp.example.com".to_owned(),
        587,
        AdminEmailTlsMode::StartTls,
        Some("mailer@example.com".to_owned()),
        password.map(str::to_owned),
        "from@example.com".to_owned(),
        Some("AnyFlows".to_owned()),
        Some("reply@example.com".to_owned()),
        15,
    )
    .unwrap()
}

#[tokio::test]
async fn update_and_test_delivery_keep_plaintext_out_of_persistence() -> Result<(), Box<dyn Error>>
{
    let (pool, repository) = installed_repository().await?;
    let cipher = cipher();
    let delivery = std::sync::Arc::new(RecordingDelivery::successful());
    let service = DatabaseAdminEmailSettingsService::new(
        repository.clone(),
        cipher.clone(),
        delivery.clone(),
    );
    let admin = principal(SessionRole::Admin);
    let plaintext = "private smtp password";

    let updated = service
        .update(admin, settings_command(Some(plaintext)))
        .await?;
    assert!(updated.enabled());
    assert!(updated.password_configured());
    assert!(updated.delivery_ready());
    assert_eq!(updated.version(), 2);

    let persisted = repository.settings().await?;
    let envelope = persisted.password_secret().expect("SMTP 密文必须存在");
    assert!(
        !envelope
            .ciphertext()
            .windows(plaintext.len())
            .any(|window| window == plaintext.as_bytes())
    );
    assert_eq!(
        cipher
            .decrypt(SystemSecretKind::SmtpPassword, envelope)?
            .expose_secret(),
        plaintext
    );

    let kept = service.update(admin, settings_command(None)).await?;
    assert_eq!(kept.version(), 3);
    service
        .send_test(
            admin,
            AdminEmailTestCommand::new("recipient@example.com".to_owned())?,
        )
        .await?;
    assert_eq!(
        delivery.requests(),
        vec![ObservedDelivery {
            host: "smtp.example.com".to_owned(),
            username: Some("mailer@example.com".to_owned()),
            password: Some(plaintext.to_owned()),
            from_address: "from@example.com".to_owned(),
            recipient: "recipient@example.com".to_owned(),
            timeout_seconds: 15,
        }]
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn authorization_configuration_and_delivery_failures_are_stable() -> Result<(), Box<dyn Error>>
{
    let (pool, repository) = installed_repository().await?;
    let delivery = std::sync::Arc::new(RecordingDelivery::failing());
    let service = DatabaseAdminEmailSettingsService::new(repository, cipher(), delivery.clone());
    let recipient = || AdminEmailTestCommand::new("recipient@example.com".to_owned()).unwrap();

    assert_eq!(
        service
            .settings(principal(SessionRole::User))
            .await
            .unwrap_err(),
        AdminEmailSettingsError::Forbidden
    );
    assert_eq!(
        service
            .send_test(principal(SessionRole::Admin), recipient())
            .await,
        Err(AdminEmailSettingsError::NotConfigured)
    );
    service
        .update(
            principal(SessionRole::Admin),
            settings_command(Some("private smtp password")),
        )
        .await?;
    assert_eq!(
        service
            .send_test(principal(SessionRole::Admin), recipient())
            .await,
        Err(AdminEmailSettingsError::DeliveryFailed)
    );
    assert_eq!(delivery.requests().len(), 1);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn subscription_alert_body_enforces_floor_boundary() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let cipher = cipher();
    DatabaseAdminEmailSettingsService::new(
        repository.clone(),
        cipher.clone(),
        std::sync::Arc::new(RecordingDelivery::successful()),
    )
    .update(
        principal(SessionRole::Admin),
        settings_command(Some("private smtp password")),
    )
    .await?;
    let record = repository.settings().await?;
    let window_ends_at = DatabaseTimestamp::from_unix_timestamp(1_800_000_000)?;

    let request = EmailDeliveryRequest::subscription_balance_alert(
        &record,
        &cipher,
        "recipient@example.com".to_owned(),
        "AnyFlows",
        "subscription-user",
        "专业订阅",
        Quota::new(999)?,
        Quota::new(800)?,
        window_ends_at,
        20,
    )?;
    assert_eq!(request.subject(), "AnyFlows 订阅额度预警");
    assert!(request.body().contains("199 / 999"));
    assert!(request.body().contains("20%"));
    assert!(request.body().contains("UTC"));

    assert_eq!(
        EmailDeliveryRequest::subscription_balance_alert(
            &record,
            &cipher,
            "recipient@example.com".to_owned(),
            "AnyFlows",
            "subscription-user",
            "专业订阅",
            Quota::new(999)?,
            Quota::new(799)?,
            window_ends_at,
            20,
        )
        .unwrap_err(),
        AdminEmailSettingsError::Internal
    );

    pool.close().await?;
    Ok(())
}
