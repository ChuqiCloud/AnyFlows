use std::{error::Error, time::Duration};

use af_domain::{
    ChannelId, ChannelType, CredentialKind, CredentialQuotaDimension, Protocol, Status,
};
use sea_orm::ConnectionTrait;

use crate::{
    AdminChannelRepository, AdminChannelWriteRecord, AdminChannelWriteRepositoryError,
    AdminCredentialCreateOutcome, AdminCredentialDeleteOutcome, AdminCredentialWriteRecord,
    CredentialProxyCreateRecord, CredentialProxyDeleteOutcome, CredentialProxyPasswordUpdate,
    CredentialProxyRepository, CredentialProxyRepositoryError, CredentialProxyScheme,
    CredentialProxyUpdateRecord, DatabaseOptions, EncryptedCredentialEnvelope, MigrationOptions,
};

#[tokio::test]
async fn proxy_directory_enforces_real_bindings_and_reference_conflicts()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let proxies = CredentialProxyRepository::new(pool.clone(), Duration::from_secs(2))?;
    let channels = AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?;
    let proxy = proxies
        .create(
            CredentialProxyCreateRecord::new(
                "账号出口".to_owned(),
                CredentialProxyScheme::Socks5h,
                "proxy.example".to_owned(),
                1080,
                Some("proxy-user".to_owned()),
                true,
                true,
            ),
            |_| Ok(Some(envelope(0x31))),
        )
        .await?;
    let channel_id = create_channel(&channels).await?;
    let credential = create_credential(&channels, channel_id, Some(proxy.proxy_id())).await?;

    assert_eq!(
        proxies.delete(proxy.proxy_id()).await?,
        CredentialProxyDeleteOutcome::Referenced
    );
    assert_eq!(
        proxies
            .update(
                proxy.proxy_id(),
                CredentialProxyUpdateRecord::new(
                    "账号出口".to_owned(),
                    CredentialProxyScheme::Https,
                    "new-proxy.example".to_owned(),
                    8443,
                    Some("proxy-user".to_owned()),
                    CredentialProxyPasswordUpdate::Keep,
                    true,
                    false,
                ),
            )
            .await
            .unwrap_err(),
        CredentialProxyRepositoryError::Referenced
    );
    let updated = proxies
        .update(
            proxy.proxy_id(),
            CredentialProxyUpdateRecord::new(
                "账号出口".to_owned(),
                CredentialProxyScheme::Https,
                "new-proxy.example".to_owned(),
                8443,
                Some("proxy-user".to_owned()),
                CredentialProxyPasswordUpdate::Replace(envelope(0x32)),
                true,
                true,
            ),
        )
        .await?;
    assert!(matches!(
        updated,
        crate::CredentialProxyMutationOutcome::Mutated(_)
    ));

    assert_eq!(
        channels.delete_credential(channel_id, credential).await?,
        AdminCredentialDeleteOutcome::Deleted
    );
    assert_eq!(
        proxies.delete(proxy.proxy_id()).await?,
        CredentialProxyDeleteOutcome::Deleted
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn proxy_directory_rejects_active_duplicate_and_releases_deleted_name()
-> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let proxies = CredentialProxyRepository::new(pool.clone(), Duration::from_secs(2))?;
    let first = proxies
        .create(
            CredentialProxyCreateRecord::new(
                "shared-egress".to_owned(),
                CredentialProxyScheme::Http,
                "proxy-one.example".to_owned(),
                8080,
                None,
                false,
                true,
            ),
            |_| Ok(None),
        )
        .await?;

    assert_eq!(
        proxies
            .create(
                CredentialProxyCreateRecord::new(
                    "shared-egress".to_owned(),
                    CredentialProxyScheme::Https,
                    "proxy-two.example".to_owned(),
                    8443,
                    None,
                    false,
                    true,
                ),
                |_| Ok(None),
            )
            .await
            .unwrap_err(),
        CredentialProxyRepositoryError::Conflict
    );
    assert_eq!(
        proxies.delete(first.proxy_id()).await?,
        CredentialProxyDeleteOutcome::Deleted
    );
    proxies
        .create(
            CredentialProxyCreateRecord::new(
                "shared-egress".to_owned(),
                CredentialProxyScheme::Https,
                "proxy-two.example".to_owned(),
                8443,
                None,
                false,
                true,
            ),
            |_| Ok(None),
        )
        .await?;

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn proxy_schema_rejects_active_record_without_unique_key() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let result = pool
        .connection()
        .execute_unprepared(
            "INSERT INTO proxies \
             (name, active_name, scheme, host, port, trust_proxy_dns, enabled, version, created_at, updated_at) \
             VALUES ('missing-active-name', NULL, 'http', 'proxy.example', 8080, 0, 1, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
        )
        .await;

    assert!(result.is_err());
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn credential_write_rejects_missing_or_disabled_proxy() -> Result<(), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let proxies = CredentialProxyRepository::new(pool.clone(), Duration::from_secs(2))?;
    let channels = AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?;
    let channel_id = create_channel(&channels).await?;
    assert_eq!(
        create_credential(&channels, channel_id, af_domain::ProxyId::new(999).ok())
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidReference
    );
    let proxy = proxies
        .create(
            CredentialProxyCreateRecord::new(
                "停用出口".to_owned(),
                CredentialProxyScheme::Http,
                "proxy.example".to_owned(),
                8080,
                None,
                false,
                false,
            ),
            |_| Ok(None),
        )
        .await?;
    assert_eq!(
        create_credential(&channels, channel_id, Some(proxy.proxy_id()))
            .await
            .unwrap_err(),
        AdminChannelWriteRepositoryError::InvalidReference
    );
    pool.close().await?;
    Ok(())
}

async fn create_channel(repository: &AdminChannelRepository) -> Result<ChannelId, Box<dyn Error>> {
    Ok(repository
        .create_channel(AdminChannelWriteRecord::new(
            "proxy-test".to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://api.example.com/v1".to_owned()),
            None,
            Status::Enabled,
            10,
            0,
            true,
            Vec::new(),
            Vec::new(),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            None,
        ))
        .await?
        .channel_id())
}

async fn create_credential(
    repository: &AdminChannelRepository,
    channel_id: ChannelId,
    proxy_id: Option<af_domain::ProxyId>,
) -> Result<af_domain::CredentialId, AdminChannelWriteRepositoryError> {
    let outcome = repository
        .create_credential(
            channel_id,
            AdminCredentialWriteRecord::new(
                CredentialKind::ApiKey,
                Status::Enabled,
                None,
                0,
                10,
                None,
                None,
                None,
                true,
                None,
                CredentialQuotaDimension::Global,
                proxy_id.map(af_domain::ProxyId::get),
                None,
                None,
                None,
            ),
            |_| Ok(envelope(0x41)),
        )
        .await?;
    match outcome {
        AdminCredentialCreateOutcome::Created(record) => Ok(record.credential_id()),
        AdminCredentialCreateOutcome::ChannelNotFound => {
            Err(AdminChannelWriteRepositoryError::Invariant)
        }
    }
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("test-key", [marker; 24], vec![marker; 32]).unwrap()
}
