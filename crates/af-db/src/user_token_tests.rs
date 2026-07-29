use std::{error::Error, time::Duration};

use af_domain::{TokenId, UserId};
use sea_orm::{ConnectionTrait, DbBackend, QueryResult, Statement};

use super::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MAX_USER_TOKENS_PER_USER, MigrationOptions, UserTokenCreateRecord, UserTokenDeleteOutcome,
    UserTokenLookupOutcome, UserTokenMutationOutcome, UserTokenRepository,
    UserTokenRepositoryError, UserTokenWriteRecord,
};

#[tokio::test]
async fn owner_scoped_crud_preserves_admin_fields_and_secret() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let created = fixture
        .repository
        .create(create_record(fixture.owner, 1, 1))
        .await?;
    let token_id = created.token_id();
    assert_eq!(created.key_prefix(), "sk-af-000000000001");
    assert!(created.created_at() > 0);

    let (tokens, next_cursor) = fixture
        .repository
        .list(fixture.owner, None, 50)
        .await?
        .into_parts();
    assert_eq!(tokens.len(), 1);
    assert_eq!(next_cursor, None);
    assert!(matches!(
        fixture.repository.get(fixture.other, token_id).await?,
        UserTokenLookupOutcome::NotFound
    ));

    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE tokens SET group_id = ?, cross_group_retry = ?, rate_limit_5h = ?, rate_limit_1d = ?, rate_limit_7d = ?, max_requests = ?, used_quota = ?, used_requests = ? WHERE id = ?",
            [
                2_i64.into(),
                true.into(),
                100_i64.into(),
                200_i64.into(),
                300_i64.into(),
                400_i64.into(),
                7_i64.into(),
                8_i64.into(),
                token_id.get().into(),
            ],
        ))
        .await?;

    assert!(matches!(
        fixture
            .repository
            .update(
                fixture.other,
                token_id,
                write_record("cross-owner", 1, None)
            )
            .await?,
        UserTokenMutationOutcome::NotFound
    ));
    let UserTokenMutationOutcome::Mutated(updated) = fixture
        .repository
        .update(
            fixture.owner,
            token_id,
            UserTokenWriteRecord::new(
                "updated".to_owned(),
                2,
                999,
                false,
                Some(1_900_000_000),
                Some(vec!["gpt-5".to_owned()]),
                Some(vec!["192.0.2.0/24".to_owned()]),
            ),
        )
        .await?
    else {
        panic!("所有者更新必须返回最新快照");
    };
    assert_eq!(updated.name(), "updated");
    assert_eq!(updated.status(), 2);
    assert_eq!(updated.used_quota(), 7);

    let row = token_storage_row(&fixture.pool, token_id).await?;
    assert_eq!(row.try_get::<i64>("", "group_id")?, 2);
    assert!(row.try_get::<bool>("", "cross_group_retry")?);
    assert_eq!(row.try_get::<i64>("", "rate_limit_5h")?, 100);
    assert_eq!(row.try_get::<i64>("", "rate_limit_1d")?, 200);
    assert_eq!(row.try_get::<i64>("", "rate_limit_7d")?, 300);
    assert_eq!(row.try_get::<i64>("", "max_requests")?, 400);
    assert_eq!(row.try_get::<i64>("", "used_requests")?, 8);
    assert_eq!(
        row.try_get::<String>("", "key_hash")?,
        format!("{:064x}", 1)
    );

    assert_eq!(
        fixture.repository.delete(fixture.other, token_id).await?,
        UserTokenDeleteOutcome::NotFound
    );
    assert_eq!(
        fixture.repository.delete(fixture.owner, token_id).await?,
        UserTokenDeleteOutcome::Deleted
    );
    assert_eq!(
        fixture.repository.delete(fixture.owner, token_id).await?,
        UserTokenDeleteOutcome::NotFound
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn capacity_check_is_atomic_and_only_soft_delete_releases_slot() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture().await?;
    for index in 1..MAX_USER_TOKENS_PER_USER {
        let status = if index == 1 { 2 } else { 1 };
        let expired_at = (index == 2).then_some(1_700_000_000);
        fixture
            .repository
            .create(create_record_with_expiration(
                fixture.owner,
                index,
                status,
                expired_at,
            ))
            .await?;
    }

    let first = fixture.repository.clone();
    let second = fixture.repository.clone();
    let (first_result, second_result) = tokio::join!(
        first.create(create_record(fixture.owner, 100, 1)),
        second.create(create_record(fixture.owner, 101, 1)),
    );
    assert_eq!(
        usize::from(first_result.is_ok()) + usize::from(second_result.is_ok()),
        1
    );
    assert!(matches!(
        (&first_result, &second_result),
        (Err(UserTokenRepositoryError::LimitReached), Ok(_))
            | (Ok(_), Err(UserTokenRepositoryError::LimitReached))
    ));
    let (tokens, _) = fixture
        .repository
        .list(fixture.owner, None, 100)
        .await?
        .into_parts();
    assert_eq!(tokens.len(), MAX_USER_TOKENS_PER_USER);

    let released = tokens[0].token_id();
    assert_eq!(
        fixture.repository.delete(fixture.owner, released).await?,
        UserTokenDeleteOutcome::Deleted
    );
    fixture
        .repository
        .create(create_record(fixture.owner, 102, 1))
        .await?;
    assert_eq!(
        fixture
            .repository
            .create(create_record(fixture.other, 200, 1))
            .await?
            .key_prefix(),
        "sk-af-000000000200"
    );
    assert_eq!(
        fixture
            .repository
            .create(create_record(UserId::new(999_999)?, 300, 1))
            .await
            .unwrap_err(),
        UserTokenRepositoryError::OwnerUnavailable
    );
    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: UserTokenRepository,
    owner: UserId,
    other: UserId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let setup = InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let InitialSetupOutcome::Initialized { user_id: owner } = setup
        .initialize(InitialSetupRecord::new(
            "key-owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await?
    else {
        panic!("空数据库必须完成首次安装");
    };
    pool.connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
            [2_i64.into(), "vip".into(), "VIP".into(), "{}".into()],
        ))
        .await?;
    let other = UserId::new(owner.get() + 1)?;
    pool.connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                other.get().into(),
                "other-owner".into(),
                0_i16.into(),
                1_i16.into(),
                1_i64.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                "other-owner-aff".into(),
                "{}".into(),
            ],
        ))
        .await?;
    Ok(Fixture {
        repository: UserTokenRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        owner,
        other,
    })
}

fn create_record(owner: UserId, index: usize, status: i16) -> UserTokenCreateRecord {
    create_record_with_expiration(owner, index, status, None)
}

fn create_record_with_expiration(
    owner: UserId,
    index: usize,
    status: i16,
    expired_at: Option<i64>,
) -> UserTokenCreateRecord {
    UserTokenCreateRecord::new(
        owner,
        format!("{:064x}", index),
        format!("sk-af-{:012}", index),
        write_record(&format!("key-{index}"), status, expired_at),
    )
}

fn write_record(name: &str, status: i16, expired_at: Option<i64>) -> UserTokenWriteRecord {
    UserTokenWriteRecord::new(
        name.to_owned(),
        status,
        1_000,
        false,
        expired_at,
        None,
        None,
    )
}

async fn token_storage_row(
    pool: &super::DatabasePool,
    token_id: TokenId,
) -> Result<QueryResult, Box<dyn Error>> {
    Ok(pool
        .connection()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT key_hash, group_id, cross_group_retry, rate_limit_5h, rate_limit_1d, rate_limit_7d, max_requests, used_requests FROM tokens WHERE id = ?",
            [token_id.get().into()],
        ))
        .await?
        .expect("已创建 Key 必须存在"))
}

#[test]
fn user_write_records_redact_configuration() {
    let record = UserTokenWriteRecord::new(
        "private".to_owned(),
        1,
        100,
        false,
        None,
        Some(vec!["private-model".to_owned()]),
        Some(vec!["192.0.2.1".to_owned()]),
    );
    assert_eq!(format!("{record:?}"), "UserTokenWriteRecord(<redacted>)");
    assert!(!format!("{record:?}").contains("private-model"));
}
