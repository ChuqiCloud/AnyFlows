use std::error::Error;

use af_domain::UserId;
use sea_orm::{ActiveModelTrait, Set};

use crate::{
    DatabaseOptions, DatabasePool, EncryptedCredentialEnvelope,
    ExternalAccountBalanceSnapshotRecord, ExternalAccountBalanceSnapshotWrite,
    ExternalAccountConnectionCreate, ExternalAccountConnectionRepository,
    ExternalAccountInitialBalanceSnapshot, ExternalAccountRepositoryError, ExternalBalanceStatus,
    MAX_EXTERNAL_BALANCE_SNAPSHOTS, MigrationOptions,
    entity::{groups, users},
};

struct Fixture {
    pool: DatabasePool,
    repository: ExternalAccountConnectionRepository,
    users: Vec<UserId>,
}

impl Fixture {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let pool = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let group = groups::ActiveModel {
            name: Set("external-accounts-test".to_owned()),
            display_name: Set("External accounts test".to_owned()),
            flags: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(pool.connection())
        .await?;
        let mut user_ids = Vec::new();
        for index in 0..3 {
            let user = users::ActiveModel {
                username: Set(format!("external-account-{index}")),
                role: Set(0),
                status: Set(1),
                default_group_id: Set(group.id),
                aff_code: Set(format!("external-account-aff-{index}")),
                settings: Set(serde_json::json!({})),
                ..Default::default()
            }
            .insert(pool.connection())
            .await?;
            user_ids.push(UserId::new(user.id)?);
        }
        Ok(Self {
            repository: ExternalAccountConnectionRepository::new(
                pool.clone(),
                std::time::Duration::from_secs(2),
            ),
            pool,
            users: user_ids,
        })
    }

    async fn close(self) -> Result<(), Box<dyn Error>> {
        self.pool.close().await?;
        Ok(())
    }
}

fn link(
    user_id: UserId,
    subject: &str,
    base_url: &str,
    marker: u8,
) -> Result<ExternalAccountConnectionCreate, Box<dyn Error>> {
    Ok(ExternalAccountConnectionCreate {
        user_id,
        connector_id: "org.anyflows.test-panel".to_owned(),
        external_subject: subject.to_owned(),
        display_name: Some("Linked panel account".to_owned()),
        base_url: base_url.to_owned(),
        secret: EncryptedCredentialEnvelope::new(
            "test-key",
            [marker; crate::CREDENTIAL_ENVELOPE_NONCE_BYTES],
            vec![marker; 32],
        )?,
        now: 1_800_000_000,
    })
}

fn initial_snapshot(observed_at: i64) -> ExternalAccountInitialBalanceSnapshot {
    ExternalAccountInitialBalanceSnapshot {
        amount: Some("12.3400".to_owned()),
        unit: Some("USD".to_owned()),
        status: ExternalBalanceStatus::Available,
        observed_at,
    }
}

fn failed_snapshot(
    user_id: UserId,
    connection_id: i64,
    observed_at: i64,
) -> ExternalAccountBalanceSnapshotWrite {
    ExternalAccountBalanceSnapshotWrite {
        user_id,
        connection_id,
        amount: None,
        unit: None,
        status: ExternalBalanceStatus::AuthenticationRequired,
        observed_at,
    }
}

fn assert_snapshot(record: &ExternalAccountBalanceSnapshotRecord) {
    assert_eq!(record.amount.as_deref(), Some("12.3400"));
    assert_eq!(record.unit.as_deref(), Some("USD"));
    assert_eq!(record.status, ExternalBalanceStatus::Available);
}

#[tokio::test]
async fn same_connector_subject_can_be_bound_to_distinct_canonical_issuers()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let first = fixture
        .repository
        .create(link(
            fixture.users[0],
            "external-subject-1",
            "https://panel-a.example/",
            1,
        )?)
        .await?;
    let second = fixture
        .repository
        .create(link(
            fixture.users[0],
            "external-subject-1",
            "https://panel-b.example",
            2,
        )?)
        .await?;

    assert_ne!(first.id, second.id);
    assert_ne!(first.issuer_hash, second.issuer_hash);
    assert_eq!(first.base_url, "https://panel-a.example");
    assert_eq!(fixture.repository.list(fixture.users[0]).await?.len(), 2);
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn active_subject_claim_is_scoped_to_connector_and_canonical_issuer()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    fixture
        .repository
        .create(link(
            fixture.users[0],
            "shared-subject",
            "https://panel.example/",
            3,
        )?)
        .await?;

    assert_eq!(
        fixture
            .repository
            .create(link(
                fixture.users[1],
                "shared-subject",
                "https://panel.example",
                4,
            )?)
            .await,
        Err(ExternalAccountRepositoryError::Conflict)
    );
    let distinct_issuer = fixture
        .repository
        .create(link(
            fixture.users[1],
            "shared-subject",
            "https://other-panel.example",
            5,
        )?)
        .await?;
    assert_eq!(distinct_issuer.external_subject, "shared-subject");
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn revocation_wipes_credentials_and_keeps_owner_scoped_balance_history()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let linked = fixture
        .repository
        .create_with_snapshot(
            link(
                fixture.users[0],
                "history-subject",
                "https://panel.example",
                6,
            )?,
            initial_snapshot(1_800_000_000_001),
        )
        .await?;
    fixture
        .repository
        .insert_snapshot(failed_snapshot(
            fixture.users[0],
            linked.id,
            1_800_000_000_002,
        ))
        .await?;

    fixture
        .repository
        .delete(fixture.users[0], linked.id)
        .await?;
    assert!(matches!(
        fixture.repository.secret(fixture.users[0], linked.id).await,
        Err(ExternalAccountRepositoryError::NotFound)
    ));
    assert_eq!(fixture.repository.list(fixture.users[0]).await?.len(), 0);
    let owner_history = fixture
        .repository
        .list_snapshots(fixture.users[0], linked.id, 10)
        .await?;
    assert_eq!(owner_history.len(), 2);
    assert_snapshot(&owner_history[1]);
    assert_eq!(
        owner_history[0].status,
        ExternalBalanceStatus::AuthenticationRequired
    );
    assert!(
        fixture
            .repository
            .list_snapshots(fixture.users[1], linked.id, 10)
            .await?
            .is_empty()
    );

    // Releasing the live claim allows the external account to be linked by a
    // different local user without deleting the previous owner's observations.
    fixture
        .repository
        .create(link(
            fixture.users[1],
            "history-subject",
            "https://panel.example",
            7,
        )?)
        .await?;
    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn balance_snapshot_history_retains_only_the_most_recent_records()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let linked = fixture
        .repository
        .create_with_snapshot(
            link(
                fixture.users[0],
                "bounded-history-subject",
                "https://panel.example",
                10,
            )?,
            initial_snapshot(1_800_000_000_001),
        )
        .await?;

    for offset in 2..=i64::from(MAX_EXTERNAL_BALANCE_SNAPSHOTS) + 1 {
        fixture
            .repository
            .insert_snapshot(failed_snapshot(
                fixture.users[0],
                linked.id,
                1_800_000_000_000 + offset,
            ))
            .await?;
    }

    let history = fixture
        .repository
        .list_snapshots(fixture.users[0], linked.id, MAX_EXTERNAL_BALANCE_SNAPSHOTS)
        .await?;
    assert_eq!(history.len(), MAX_EXTERNAL_BALANCE_SNAPSHOTS as usize);
    assert_eq!(history.first().unwrap().observed_at, 1_800_000_000_101);
    assert_eq!(history.last().unwrap().observed_at, 1_800_000_000_002);

    fixture.close().await?;
    Ok(())
}

#[tokio::test]
async fn first_snapshot_is_atomic_with_link_and_profile_refresh_updates_display_name()
-> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new().await?;
    let mut invalid_snapshot = initial_snapshot(1_800_000_000_002);
    invalid_snapshot.amount = Some("-1".to_owned());
    assert_eq!(
        fixture
            .repository
            .create_with_snapshot(
                link(
                    fixture.users[0],
                    "profile-subject",
                    "https://panel.example",
                    8,
                )?,
                invalid_snapshot,
            )
            .await,
        Err(ExternalAccountRepositoryError::InvalidInput)
    );
    assert!(fixture.repository.list(fixture.users[0]).await?.is_empty());

    let linked = fixture
        .repository
        .create_with_snapshot(
            link(
                fixture.users[0],
                "profile-subject",
                "https://panel.example",
                8,
            )?,
            initial_snapshot(1_800_000_000_003),
        )
        .await?;
    assert_snapshot(
        &fixture
            .repository
            .list_snapshots(fixture.users[0], linked.id, 1)
            .await?
            .remove(0),
    );

    let updated = fixture
        .repository
        .update_profile(
            fixture.users[0],
            linked.id,
            Some("Refreshed external name".to_owned()),
            1_800_000_010,
        )
        .await?;
    assert_eq!(
        updated.display_name.as_deref(),
        Some("Refreshed external name")
    );
    assert_eq!(
        fixture
            .repository
            .get(fixture.users[0], linked.id)
            .await?
            .display_name
            .as_deref(),
        Some("Refreshed external name")
    );
    fixture.close().await?;
    Ok(())
}
