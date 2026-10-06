use std::{error::Error, time::Duration};

use af_domain::{GroupId, UserId};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::{
    AdminTokenCreateRecord, AdminTokenDeleteOutcome, AdminTokenMutationOutcome,
    AdminTokenRepository, AdminTokenWriteRecord, AdminTokenWriteRepositoryError, DatabaseOptions,
    MAX_USER_TOKENS_PER_USER, MigrationOptions, UserTokenCreateRecord, UserTokenDeleteOutcome,
    UserTokenRepository, UserTokenRepositoryError, UserTokenWriteRecord,
};

#[tokio::test]
async fn create_and_update_preserve_secret_and_usage_state() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let created = fixture
        .repository
        .create_token(create_record(fixture.user_id, fixture.vip_group_id))
        .await?;
    let token_id = created.token_id();
    assert_eq!(created.user_id(), fixture.user_id);
    assert_eq!(created.key_prefix(), "sk-af-public000001");
    assert_eq!(created.name(), "primary");
    assert_eq!(created.group_id(), Some(fixture.vip_group_id));
    assert_eq!(created.remain_quota(), 1_000);
    assert_eq!(created.used_quota(), 0);
    assert_eq!(created.usage_5h(), 0);
    assert_eq!(created.used_requests(), 0);

    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE tokens SET used_quota = ?, usage_5h = ?, usage_1d = ?, usage_7d = ?, used_requests = ? WHERE id = ?",
            [
                25_i64.into(),
                10_i64.into(),
                20_i64.into(),
                30_i64.into(),
                4_i64.into(),
                token_id.get().into(),
            ],
        ))
        .await?;

    let AdminTokenMutationOutcome::Mutated(updated) = fixture
        .repository
        .update_token(
            token_id,
            fixture.user_id,
            write_record("updated", None, false),
        )
        .await?
    else {
        panic!("有效令牌必须完成更新");
    };
    assert_eq!(updated.name(), "updated");
    assert_eq!(updated.key_prefix(), "sk-af-public000001");
    assert_eq!(updated.group_id(), None);
    assert_eq!(updated.used_quota(), 25);
    assert_eq!(updated.usage_5h(), 10);
    assert_eq!(updated.usage_1d(), 20);
    assert_eq!(updated.usage_7d(), 30);
    assert_eq!(updated.used_requests(), 4);

    let row = fixture
        .pool
        .connection()
        .query_one(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT key_hash FROM tokens WHERE id = ?",
            [token_id.get().into()],
        ))
        .await?
        .expect("签发令牌必须存在");
    let key_hash: String = row.try_get("", "key_hash")?;
    assert_eq!(key_hash, "11".repeat(32));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn writes_reject_invalid_references_and_owner_transfer() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let invalid_user = UserId::new(999_999)?;
    assert_eq!(
        fixture
            .repository
            .create_token(create_record(invalid_user, fixture.vip_group_id))
            .await
            .unwrap_err(),
        AdminTokenWriteRepositoryError::InvalidReference
    );

    let created = fixture
        .repository
        .create_token(create_record(fixture.user_id, fixture.vip_group_id))
        .await?;
    assert_eq!(
        fixture
            .repository
            .update_token(
                created.token_id(),
                invalid_user,
                write_record("owner-transfer", Some(fixture.vip_group_id), false),
            )
            .await
            .unwrap_err(),
        AdminTokenWriteRepositoryError::InvalidInput
    );
    assert_eq!(
        fixture
            .repository
            .update_token(
                created.token_id(),
                fixture.user_id,
                write_record("missing-group", Some(GroupId::new(999_999)?), false),
            )
            .await
            .unwrap_err(),
        AdminTokenWriteRepositoryError::InvalidReference
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn admin_and_user_creation_share_atomic_capacity_guard() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let user_repository = UserTokenRepository::new(fixture.pool.clone(), Duration::from_secs(2))?;
    for index in 1..MAX_USER_TOKENS_PER_USER {
        user_repository
            .create(user_create_record(fixture.user_id, index))
            .await?;
    }

    let admin_repository = fixture.repository.clone();
    let concurrent_user_repository = user_repository.clone();
    let (admin_result, user_result) = tokio::join!(
        admin_repository.create_token(create_record(fixture.user_id, fixture.vip_group_id)),
        concurrent_user_repository.create(user_create_record(fixture.user_id, 100)),
    );
    assert!(matches!(
        (&admin_result, &user_result),
        (Ok(_), Err(UserTokenRepositoryError::LimitReached))
            | (Err(AdminTokenWriteRepositoryError::LimitReached), Ok(_))
    ));

    let (tokens, _) = user_repository
        .list(fixture.user_id, None, MAX_USER_TOKENS_PER_USER)
        .await?
        .into_parts();
    assert_eq!(tokens.len(), MAX_USER_TOKENS_PER_USER);
    assert_eq!(
        user_repository
            .delete(fixture.user_id, tokens[0].token_id())
            .await?,
        UserTokenDeleteOutcome::Deleted
    );
    fixture
        .repository
        .create_token(AdminTokenCreateRecord::new(
            fixture.user_id,
            "22".repeat(32),
            "sk-af-public000002".to_owned(),
            write_record("released-slot", Some(fixture.vip_group_id), false),
        ))
        .await?;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn delete_writes_one_tombstone_and_hides_token() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let token_id = fixture
        .repository
        .create_token(create_record(fixture.user_id, fixture.vip_group_id))
        .await?
        .token_id();
    assert_eq!(
        fixture.repository.delete_token(token_id).await?,
        AdminTokenDeleteOutcome::Deleted
    );
    assert_eq!(
        fixture.repository.delete_token(token_id).await?,
        AdminTokenDeleteOutcome::NotFound
    );
    assert!(matches!(
        fixture.repository.get(token_id).await?,
        super::AdminTokenLookupOutcome::NotFound
    ));
    assert!(matches!(
        fixture
            .repository
            .update_token(
                token_id,
                fixture.user_id,
                write_record("deleted", None, false),
            )
            .await?,
        AdminTokenMutationOutcome::NotFound
    ));

    fixture.pool.close().await?;
    Ok(())
}

struct Fixture {
    pool: super::DatabasePool,
    repository: AdminTokenRepository,
    user_id: UserId,
    vip_group_id: GroupId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    for (id, name) in [(1_i64, "default"), (2_i64, "vip")] {
        pool.connection()
            .execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
                [id.into(), name.into(), name.into(), "{}".into()],
            ))
            .await?;
    }
    pool.connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, role, status, default_group_id, quota, used_quota, frozen_quota, request_count, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                10_i64.into(),
                "token-owner".into(),
                0_i16.into(),
                1_i16.into(),
                1_i64.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                0_i64.into(),
                "token-owner-aff".into(),
                "{}".into(),
            ],
        ))
        .await?;
    Ok(Fixture {
        repository: AdminTokenRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
        user_id: UserId::new(10)?,
        vip_group_id: GroupId::new(2)?,
    })
}

fn create_record(user_id: UserId, group_id: GroupId) -> AdminTokenCreateRecord {
    AdminTokenCreateRecord::new(
        user_id,
        "11".repeat(32),
        "sk-af-public000001".to_owned(),
        write_record("primary", Some(group_id), true),
    )
}

fn user_create_record(user_id: UserId, index: usize) -> UserTokenCreateRecord {
    UserTokenCreateRecord::new(
        user_id,
        format!("{index:064x}"),
        format!("sk-af-{index:012}"),
        UserTokenWriteRecord::new(
            format!("user-key-{index}"),
            1,
            1_000,
            false,
            None,
            None,
            None,
        ),
    )
}

fn write_record(name: &str, group_id: Option<GroupId>, with_limits: bool) -> AdminTokenWriteRecord {
    AdminTokenWriteRecord::new(
        name.to_owned(),
        1,
        group_id,
        1_000,
        false,
        Some(1_800_000_000),
        with_limits.then(|| vec!["gpt-5.5".to_owned(), "gpt-5.5".to_owned()]),
        with_limits.then(|| vec!["192.0.2.0/24".to_owned(), "192.0.2.0/24".to_owned()]),
        with_limits,
        with_limits.then_some(100),
        with_limits.then_some(200),
        with_limits.then_some(300),
        with_limits.then_some(1_000),
    )
}

#[test]
fn write_record_debug_is_redacted() {
    let record = write_record("private-token", None, true);
    assert_eq!(format!("{record:?}"), "AdminTokenWriteRecord(<redacted>)");
    assert!(!format!("{record:?}").contains("gpt-5.5"));
}

#[tokio::test]
async fn invalid_timestamp_is_rejected_before_database_write() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let record = AdminTokenWriteRecord::new(
        "invalid-time".to_owned(),
        1,
        None,
        0,
        false,
        Some(i64::MAX),
        None,
        None,
        false,
        None,
        None,
        None,
        None,
    );
    assert_eq!(
        fixture
            .repository
            .create_token(AdminTokenCreateRecord::new(
                fixture.user_id,
                "22".repeat(32),
                "sk-af-public000002".to_owned(),
                record,
            ))
            .await
            .unwrap_err(),
        AdminTokenWriteRepositoryError::InvalidInput
    );
    fixture.pool.close().await?;
    Ok(())
}
