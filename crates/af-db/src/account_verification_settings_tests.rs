use std::time::Duration;

use sea_orm::EntityTrait;

use super::{AccountVerificationSettingsError, AccountVerificationSettingsRepository};
use crate::{
    DatabaseOptions, MigrationOptions, connect_and_migrate,
    entity::account_verification_settings as settings,
};

#[tokio::test]
async fn missing_verification_settings_fail_closed() {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:").unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    let repository =
        AccountVerificationSettingsRepository::new(pool.clone(), Duration::from_secs(10));
    assert!(repository.settings().await.is_ok());
    settings::Entity::delete_by_id(1_i16)
        .exec(pool.connection())
        .await
        .unwrap();

    assert!(matches!(
        repository.settings().await,
        Err(AccountVerificationSettingsError::Internal)
    ));
    pool.close().await.unwrap();
}
