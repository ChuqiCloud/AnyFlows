use std::{error::Error, time::Duration};

use sea_orm::{EntityTrait, PaginatorTrait};

use super::{
    AuthChallengeConsume, AuthChallengeConsumeOutcome, AuthChallengeInputError, AuthChallengeIssue,
    AuthChallengeIssueOutcome, AuthChallengePurpose, AuthChallengeRepository, DatabaseOptions,
    InitialSetupRecord, InitialSetupRepository, MigrationOptions,
};
use crate::entity::{auth_challenges, users};

const SUBJECT: [u8; 32] = [0x11; 32];
const FIRST_SECRET: [u8; 32] = [0x22; 32];
const SECOND_SECRET: [u8; 32] = [0x33; 32];
const WRONG_SECRET: [u8; 32] = [0x44; 32];

async fn installed_repository()
-> Result<(crate::DatabasePool, AuthChallengeRepository), Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = AuthChallengeRepository::new(pool.clone(), Duration::from_secs(5))?;
    Ok((pool, repository))
}

fn registration_issue(
    secret: [u8; 32],
    issued_at: u64,
    max_attempts: u32,
) -> Result<AuthChallengeIssue, AuthChallengeInputError> {
    AuthChallengeIssue::new(
        AuthChallengePurpose::RegistrationEmail,
        SUBJECT,
        secret,
        None,
        issued_at,
        600,
        60,
        max_attempts,
    )
}

fn registration_consume(
    secret: [u8; 32],
    attempted_at: u64,
) -> Result<AuthChallengeConsume, AuthChallengeInputError> {
    AuthChallengeConsume::new(
        AuthChallengePurpose::RegistrationEmail,
        SUBJECT,
        secret,
        attempted_at,
    )
}

#[tokio::test]
async fn reissue_enforces_cooldown_and_invalidates_the_old_digest() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;

    let first = repository
        .issue(registration_issue(FIRST_SECRET, 1_000, 3)?)
        .await?;
    assert!(matches!(
        first,
        AuthChallengeIssueOutcome::Issued(issued)
            if issued.version() == 1
                && issued.expires_at() == 1_600
                && issued.next_send_at() == 1_060
    ));
    assert_eq!(
        repository
            .issue(registration_issue(SECOND_SECRET, 1_030, 3)?)
            .await?,
        AuthChallengeIssueOutcome::Cooldown {
            retry_after_seconds: 30
        }
    );
    let before_reissue = auth_challenges::Entity::find()
        .one(pool.connection())
        .await?
        .expect("首次签发必须写入挑战");
    assert!(before_reissue.secret_digest.matches_bytes(&FIRST_SECRET));

    let reissued = repository
        .issue(registration_issue(SECOND_SECRET, 1_060, 3)?)
        .await?;
    assert!(matches!(
        reissued,
        AuthChallengeIssueOutcome::Issued(issued)
            if issued.version() == 2
                && issued.expires_at() == 1_660
                && issued.next_send_at() == 1_120
    ));
    assert_eq!(
        auth_challenges::Entity::find()
            .count(pool.connection())
            .await?,
        1
    );
    assert_eq!(
        repository
            .consume(registration_consume(FIRST_SECRET, 1_061)?)
            .await?,
        AuthChallengeConsumeOutcome::Rejected
    );
    let consumed = repository
        .consume(registration_consume(SECOND_SECRET, 1_062)?)
        .await?;
    assert!(matches!(
        consumed,
        AuthChallengeConsumeOutcome::Consumed(consumption)
            if consumption.target_user_id().is_none() && consumption.version() == 4
    ));
    assert_eq!(
        repository
            .consume(registration_consume(SECOND_SECRET, 1_063)?)
            .await?,
        AuthChallengeConsumeOutcome::Rejected
    );

    let state = auth_challenges::Entity::find()
        .one(pool.connection())
        .await?
        .expect("消费后必须保留短期状态");
    assert_eq!(state.attempts, 2);
    assert_eq!(state.version, 4);
    assert!(state.consumed_at.is_some());
    assert!(!format!("{state:?}").contains(&"33".repeat(32)));

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn wrong_attempts_exhaust_the_challenge_without_consuming_it() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    repository
        .issue(registration_issue(FIRST_SECRET, 2_000, 2)?)
        .await?;

    for attempted_at in [2_001, 2_002] {
        assert_eq!(
            repository
                .consume(registration_consume(WRONG_SECRET, attempted_at)?)
                .await?,
            AuthChallengeConsumeOutcome::Rejected
        );
    }
    assert_eq!(
        repository
            .consume(registration_consume(FIRST_SECRET, 2_003)?)
            .await?,
        AuthChallengeConsumeOutcome::Rejected
    );

    let state = auth_challenges::Entity::find()
        .one(pool.connection())
        .await?
        .expect("错误次数状态必须存在");
    assert_eq!(state.attempts, 2);
    assert_eq!(state.version, 3);
    assert!(state.consumed_at.is_none());

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn expiry_and_concurrent_replay_are_fail_closed() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    repository
        .issue(registration_issue(FIRST_SECRET, 3_000, 3)?)
        .await?;
    assert_eq!(
        repository
            .consume(registration_consume(FIRST_SECRET, 3_600)?)
            .await?,
        AuthChallengeConsumeOutcome::Rejected
    );

    repository
        .issue(registration_issue(SECOND_SECRET, 3_600, 3)?)
        .await?;
    let first = repository.clone();
    let second = repository.clone();
    let (first, second) = tokio::join!(
        first.consume(registration_consume(SECOND_SECRET, 3_601)?),
        second.consume(registration_consume(SECOND_SECRET, 3_601)?),
    );
    let outcomes = [first?, second?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, AuthChallengeConsumeOutcome::Consumed(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, AuthChallengeConsumeOutcome::Rejected))
            .count(),
        1
    );

    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn password_reset_is_bound_to_an_existing_target_user() -> Result<(), Box<dyn Error>> {
    let (pool, repository) = installed_repository().await?;
    InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
        .initialize(InitialSetupRecord::new(
            "owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await?;
    let user_id = af_domain::UserId::new(
        users::Entity::find()
            .one(pool.connection())
            .await?
            .expect("首次安装必须创建管理员")
            .id,
    )?;

    assert!(matches!(
        AuthChallengeIssue::new(
            AuthChallengePurpose::RegistrationEmail,
            SUBJECT,
            FIRST_SECRET,
            Some(user_id),
            4_000,
            600,
            60,
            3,
        ),
        Err(AuthChallengeInputError::InvalidTarget)
    ));
    assert!(matches!(
        AuthChallengeIssue::new(
            AuthChallengePurpose::PasswordReset,
            SUBJECT,
            FIRST_SECRET,
            None,
            4_000,
            600,
            60,
            3,
        ),
        Err(AuthChallengeInputError::InvalidTarget)
    ));
    assert!(matches!(
        AuthChallengeIssue::new(
            AuthChallengePurpose::PasskeyAuthentication,
            SUBJECT,
            FIRST_SECRET,
            None,
            4_000,
            600,
            60,
            3,
        ),
        Err(AuthChallengeInputError::InvalidTarget)
    ));
    assert!(matches!(
        AuthChallengeConsume::new(
            AuthChallengePurpose::PasskeyAuthentication,
            SUBJECT,
            FIRST_SECRET,
            4_000,
        ),
        Err(AuthChallengeInputError::InvalidTarget)
    ));

    repository
        .issue(AuthChallengeIssue::new(
            AuthChallengePurpose::PasswordReset,
            SUBJECT,
            FIRST_SECRET,
            Some(user_id),
            4_000,
            600,
            60,
            3,
        )?)
        .await?;
    let outcome = repository
        .consume(AuthChallengeConsume::new(
            AuthChallengePurpose::PasswordReset,
            SUBJECT,
            FIRST_SECRET,
            4_001,
        )?)
        .await?;
    assert!(matches!(
        outcome,
        AuthChallengeConsumeOutcome::Consumed(consumption)
            if consumption.target_user_id() == Some(user_id)
    ));

    pool.close().await?;
    Ok(())
}

#[test]
fn constructors_and_debug_output_keep_hard_boundaries() {
    assert!(matches!(
        registration_issue(FIRST_SECRET, 5_000, 0),
        Err(AuthChallengeInputError::InvalidAttempts)
    ));
    assert!(matches!(
        AuthChallengeIssue::new(
            AuthChallengePurpose::RegistrationEmail,
            SUBJECT,
            FIRST_SECRET,
            None,
            5_000,
            59,
            60,
            3,
        ),
        Err(AuthChallengeInputError::InvalidTiming)
    ));
    let issue = registration_issue(FIRST_SECRET, 5_000, 3).unwrap();
    let consume = registration_consume(FIRST_SECRET, 5_001).unwrap();
    assert_eq!(format!("{issue:?}"), "AuthChallengeIssue(<redacted>)");
    assert_eq!(format!("{consume:?}"), "AuthChallengeConsume(<redacted>)");
    assert!(!format!("{issue:?}").contains(&"22".repeat(32)));
}
