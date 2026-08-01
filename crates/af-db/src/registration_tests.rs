use std::{error::Error, time::Duration};

use sea_orm::{EntityTrait, PaginatorTrait};

use super::{
    DatabaseOptions, InitialSetupRecord, InitialSetupRepository, MigrationOptions,
    RegistrationAttemptOutcome, RegistrationPolicyWriteRecord, RegistrationRepository,
    RegistrationRepositoryError,
};
use crate::entity::{groups, registration_rate_limits};

async fn installed_repository() -> Result<
    (
        crate::DatabasePool,
        RegistrationRepository,
        af_domain::GroupId,
    ),
    Box<dyn Error>,
> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
        .initialize(InitialSetupRecord::new(
            "owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await?;
    let group_id = af_domain::GroupId::new(
        groups::Entity::find()
            .one(pool.connection())
            .await?
            .expect("首次安装必须创建默认分组")
            .id,
    )?;
    let repository = RegistrationRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository, group_id))
}

#[tokio::test]
async fn missing_policy_is_closed_but_admin_defaults_to_the_real_group()
-> Result<(), Box<dyn Error>> {
    let (pool, repository, group_id) = installed_repository().await?;

    let status = repository.status().await?;
    assert!(status.password_login_enabled());
    assert!(!status.enabled());
    assert!(!status.email_required());

    let policy = repository.policy().await?;
    assert!(policy.password_login_enabled());
    assert!(!policy.enabled());
    assert_eq!(policy.default_group_id(), group_id);
    assert_eq!(policy.initial_quota(), 0);
    assert_eq!(policy.rate_limit_attempts(), 5);
    assert_eq!(policy.rate_limit_window_seconds(), 3_600);
    assert_eq!(
        repository.claim_attempt([0x11; 32], 120).await?,
        RegistrationAttemptOutcome::Disabled
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn policy_update_validates_group_and_controls_public_status() -> Result<(), Box<dyn Error>> {
    let (pool, repository, group_id) = installed_repository().await?;

    assert_eq!(
        repository
            .update_policy(RegistrationPolicyWriteRecord::new(
                true,
                true,
                af_domain::GroupId::new(i64::MAX)?,
                500,
                0,
                true,
                3,
                900,
            ))
            .await,
        Err(RegistrationRepositoryError::InvalidReference)
    );

    let policy = repository
        .update_policy(RegistrationPolicyWriteRecord::new(
            true, true, group_id, 500, 0, true, 3, 900,
        ))
        .await?;
    assert!(policy.enabled());
    assert_eq!(policy.default_group_id(), group_id);
    assert_eq!(policy.initial_quota(), 500);
    assert_eq!(policy.invitation_rebate_quota(), 0);
    assert!(policy.email_required());
    assert_eq!(policy.version(), 2);

    let status = repository.status().await?;
    assert!(status.password_login_enabled());
    assert!(status.enabled());
    assert!(status.email_required());

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn fixed_window_limit_is_atomic_and_reuses_one_fingerprint_row() -> Result<(), Box<dyn Error>>
{
    let (pool, repository, group_id) = installed_repository().await?;
    repository
        .update_policy(RegistrationPolicyWriteRecord::new(
            true, true, group_id, 0, 0, false, 2, 60,
        ))
        .await?;

    let fingerprint = [0x22; 32];
    assert!(matches!(
        repository.claim_attempt(fingerprint, 120).await?,
        RegistrationAttemptOutcome::Allowed(_)
    ));
    assert!(matches!(
        repository.claim_attempt(fingerprint, 121).await?,
        RegistrationAttemptOutcome::Allowed(_)
    ));
    assert_eq!(
        repository.claim_attempt(fingerprint, 122).await?,
        RegistrationAttemptOutcome::RateLimited {
            retry_after_seconds: 58
        }
    );
    assert!(matches!(
        repository.claim_attempt(fingerprint, 180).await?,
        RegistrationAttemptOutcome::Allowed(_)
    ));

    assert_eq!(
        registration_rate_limits::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );
    let state = registration_rate_limits::Entity::find()
        .one(pool.connection())
        .await?
        .expect("限流状态必须存在");
    assert_eq!(state.window_started_at, 180);
    assert_eq!(state.attempts, 1);
    assert!(!format!("{state:?}").contains(&"22".repeat(32)));

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn concurrent_claims_cannot_exceed_the_database_limit() -> Result<(), Box<dyn Error>> {
    let (pool, repository, group_id) = installed_repository().await?;
    repository
        .update_policy(RegistrationPolicyWriteRecord::new(
            true, true, group_id, 0, 0, false, 1, 60,
        ))
        .await?;

    let first = repository.clone();
    let second = repository.clone();
    let (first, second) = tokio::join!(
        first.claim_attempt([0x33; 32], 240),
        second.claim_attempt([0x33; 32], 240),
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RegistrationAttemptOutcome::Allowed(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, RegistrationAttemptOutcome::RateLimited { .. }))
            .count(),
        1
    );

    pool.close().await?;
    Ok(())
}
