use std::{
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};

use af_account::SystemSecretCipher;
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    DatabaseOptions, MigrationOptions, PaymentSecretUpdate, PaymentSettingsRecord,
    PaymentSettingsRepository, PaymentSettingsWriteRecord,
};
use af_domain::UserId;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;

use super::{
    AdminPaymentSettingsCommand, AdminPaymentSettingsError, AdminPaymentSettingsService,
    DatabaseAdminPaymentSettingsService, PaymentSettingsApplyFuture, PaymentSettingsRuntimeApplier,
    PaymentSettingsRuntimeError, SessionPrincipal, SessionRole,
};

struct FailingRuntime {
    versions: Arc<Mutex<Vec<i64>>>,
}

impl PaymentSettingsRuntimeApplier for FailingRuntime {
    fn apply<'a>(
        &'a self,
        record: &'a PaymentSettingsRecord,
        _cipher: &'a SystemSecretCipher,
    ) -> PaymentSettingsApplyFuture<'a> {
        self.versions.lock().unwrap().push(record.version());
        Box::pin(async { Err(PaymentSettingsRuntimeError::Failed) })
    }
}

struct InvalidConfigurationRuntime {
    versions: Arc<Mutex<Vec<i64>>>,
    restore_succeeds: bool,
}

impl PaymentSettingsRuntimeApplier for InvalidConfigurationRuntime {
    fn apply<'a>(
        &'a self,
        record: &'a PaymentSettingsRecord,
        _cipher: &'a SystemSecretCipher,
    ) -> PaymentSettingsApplyFuture<'a> {
        self.versions.lock().unwrap().push(record.version());
        Box::pin(async move {
            if record.version() == 2 {
                Err(PaymentSettingsRuntimeError::InvalidConfiguration)
            } else if self.restore_succeeds {
                Ok(())
            } else {
                Err(PaymentSettingsRuntimeError::Failed)
            }
        })
    }
}

struct ConcurrentRuntime {
    repository: PaymentSettingsRepository,
    versions: Arc<Mutex<Vec<i64>>>,
    concurrent_update_started: Mutex<bool>,
}

impl PaymentSettingsRuntimeApplier for ConcurrentRuntime {
    fn apply<'a>(
        &'a self,
        record: &'a PaymentSettingsRecord,
        _cipher: &'a SystemSecretCipher,
    ) -> PaymentSettingsApplyFuture<'a> {
        self.versions.lock().unwrap().push(record.version());
        let should_compete = if record.version() == 2 {
            let mut started = self.concurrent_update_started.lock().unwrap();
            let should_compete = !*started;
            *started = true;
            should_compete
        } else {
            false
        };
        Box::pin(async move {
            if should_compete {
                self.repository
                    .update(disabled_record(2, 600_000))
                    .await
                    .map_err(|_| PaymentSettingsRuntimeError::Failed)?;
                return Err(PaymentSettingsRuntimeError::Failed);
            }
            Ok(())
        })
    }
}

#[tokio::test]
async fn failed_runtime_switch_restores_previous_database_snapshot() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let versions = Arc::new(Mutex::new(Vec::new()));
    let service = DatabaseAdminPaymentSettingsService::new(
        repository.clone(),
        cipher(),
        Arc::new(FailingRuntime {
            versions: Arc::clone(&versions),
        }),
    );

    assert_eq!(
        service
            .update(admin(), disabled_command(1, 500_000))
            .await
            .unwrap_err(),
        AdminPaymentSettingsError::Internal
    );
    let restored = repository.settings().await?;
    assert!(!restored.initialized());
    assert_eq!(restored.version(), 3);
    assert_eq!(*versions.lock().unwrap(), [2, 1]);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_runtime_configuration_restores_snapshot_and_returns_input_error()
-> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let versions = Arc::new(Mutex::new(Vec::new()));
    let service = DatabaseAdminPaymentSettingsService::new(
        repository.clone(),
        cipher(),
        Arc::new(InvalidConfigurationRuntime {
            versions: Arc::clone(&versions),
            restore_succeeds: true,
        }),
    );

    assert_eq!(
        service
            .update(admin(), disabled_command(1, 500_000))
            .await
            .unwrap_err(),
        AdminPaymentSettingsError::InvalidInput
    );
    let restored = repository.settings().await?;
    assert!(!restored.initialized());
    assert_eq!(restored.version(), 3);
    assert_eq!(*versions.lock().unwrap(), [2, 1]);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_configuration_returns_internal_error_when_snapshot_recovery_fails()
-> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let versions = Arc::new(Mutex::new(Vec::new()));
    let service = DatabaseAdminPaymentSettingsService::new(
        repository,
        cipher(),
        Arc::new(InvalidConfigurationRuntime {
            versions: Arc::clone(&versions),
            restore_succeeds: false,
        }),
    );

    assert_eq!(
        service
            .update(admin(), disabled_command(1, 500_000))
            .await
            .unwrap_err(),
        AdminPaymentSettingsError::Internal
    );
    assert_eq!(*versions.lock().unwrap(), [2, 1]);

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_update_reconciles_runtime_to_latest_database_snapshot()
-> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    let versions = Arc::new(Mutex::new(Vec::new()));
    let service = DatabaseAdminPaymentSettingsService::new(
        repository.clone(),
        cipher(),
        Arc::new(ConcurrentRuntime {
            repository: repository.clone(),
            versions: Arc::clone(&versions),
            concurrent_update_started: Mutex::new(false),
        }),
    );

    assert_eq!(
        service
            .update(admin(), disabled_command(1, 500_000))
            .await
            .unwrap_err(),
        AdminPaymentSettingsError::Internal
    );
    let current = repository.settings().await?;
    assert!(current.initialized());
    assert_eq!(current.epay_quota_per_cny(), 600_000);
    assert_eq!(current.version(), 3);
    assert_eq!(*versions.lock().unwrap(), [2, 3]);

    pool.close().await?;
    Ok(())
}

async fn installed_repository()
-> Result<(af_db::DatabasePool, PaymentSettingsRepository), Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = PaymentSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository))
}

fn cipher() -> SystemSecretCipher {
    let settings: CredentialEncryptionSettings = serde_json::from_value(json!({
        "key_id": "payment-settings-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x62; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .unwrap();
    SystemSecretCipher::new(&settings).unwrap()
}

fn admin() -> SessionPrincipal {
    SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::Admin)
}

fn disabled_command(expected_version: i64, quota_per_cny: i64) -> AdminPaymentSettingsCommand {
    AdminPaymentSettingsCommand::new(
        expected_version,
        false,
        None,
        None,
        false,
        None,
        false,
        300,
        false,
        None,
        None,
        None,
        false,
        true,
        true,
        false,
        false,
        false,
        quota_per_cny,
    )
    .unwrap()
}

fn disabled_record(expected_version: i64, quota_per_cny: i64) -> PaymentSettingsWriteRecord {
    PaymentSettingsWriteRecord::new(
        expected_version,
        false,
        None,
        PaymentSecretUpdate::Keep,
        PaymentSecretUpdate::Keep,
        300,
        false,
        None,
        None,
        PaymentSecretUpdate::Keep,
        true,
        true,
        false,
        false,
        false,
        quota_per_cny,
    )
}
