use std::{
    error::Error,
    fs,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use af_domain::TokenId;
use sea_orm::{ConnectionTrait, DbBackend, Statement, TransactionTrait, Value as DatabaseValue};
use tokio::{sync::Barrier, task::JoinSet};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, PoolOptions, TokenRequestAdmissionOutcome,
    TokenRequestAdmissionRepository, TokenRequestAdmissionRepositoryConfigError,
    TokenRequestAdmissionRepositoryError, connect_and_migrate,
};

static NEXT_DATABASE: AtomicU64 = AtomicU64::new(0);

struct TestDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl TestDatabase {
    fn new() -> Self {
        let serial = NEXT_DATABASE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "anyflows-token-request-admission-{}-{serial}.db",
            std::process::id()
        ));
        let mut sqlite_path = path.to_string_lossy().replace('\\', "/");
        if !sqlite_path.starts_with('/') {
            sqlite_path.insert(0, '/');
        }
        Self {
            path,
            url: format!("sqlite://{sqlite_path}?mode=rwc"),
        }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = std::path::PathBuf::from(format!("{}{}", self.path.display(), suffix));
            let _ = fs::remove_file(candidate);
        }
    }
}

struct Fixture {
    pool: DatabasePool,
    repository: TokenRequestAdmissionRepository,
    token_id: TokenId,
}

impl Fixture {
    async fn close(self) -> Result<(), crate::DatabaseError> {
        let pool = self.pool.clone();
        drop(self);
        pool.close().await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn finite_limit_releases_exactly_n_requests_under_concurrency() -> Result<(), Box<dyn Error>>
{
    let database = TestDatabase::new();
    let fixture = fixture(&database, Some(7), 0).await?;
    let attempts = 24;
    let barrier = Arc::new(Barrier::new(attempts + 1));
    let mut tasks = JoinSet::new();
    for _ in 0..attempts {
        let repository = fixture.repository.clone();
        let barrier = Arc::clone(&barrier);
        let token_id = fixture.token_id;
        tasks.spawn(async move {
            barrier.wait().await;
            repository.admit(token_id).await
        });
    }
    barrier.wait().await;

    let mut admitted = 0;
    let mut limited = 0;
    while let Some(result) = tasks.join_next().await {
        match result?? {
            TokenRequestAdmissionOutcome::Admitted => admitted += 1,
            TokenRequestAdmissionOutcome::LimitReached => limited += 1,
            TokenRequestAdmissionOutcome::Rejected => {
                panic!("并发准入不应返回不可用令牌")
            }
        }
    }
    assert_eq!(admitted, 7);
    assert_eq!(limited, attempts - 7);
    assert_eq!(used_requests(&fixture.pool).await?, 7);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn zero_limit_blocks_and_unlimited_limit_keeps_accumulating() -> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let fixture = fixture(&database, Some(0), 0).await?;
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await?,
        TokenRequestAdmissionOutcome::LimitReached
    );
    assert_eq!(used_requests(&fixture.pool).await?, 0);

    set_token(&fixture.pool, "max_requests = NULL, used_requests = 0").await?;
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await?,
        TokenRequestAdmissionOutcome::Admitted
    );
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await?,
        TokenRequestAdmissionOutcome::Admitted
    );
    assert_eq!(used_requests(&fixture.pool).await?, 2);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn disabled_deleted_and_missing_tokens_are_rejected_without_increment()
-> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let fixture = fixture(&database, None, 0).await?;

    set_token(&fixture.pool, "status = 2").await?;
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await?,
        TokenRequestAdmissionOutcome::Rejected
    );
    set_token(&fixture.pool, "status = 1, deleted_at = datetime('now')").await?;
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await?,
        TokenRequestAdmissionOutcome::Rejected
    );
    assert_eq!(
        fixture
            .repository
            .admit(TokenId::new(9_999).expect("测试标识必须为正数"))
            .await?,
        TokenRequestAdmissionOutcome::Rejected
    );
    assert_eq!(used_requests(&fixture.pool).await?, 0);

    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn corrupt_counters_and_unrepresentable_unlimited_counter_fail_closed()
-> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    // SQLite 的约束绕过开关属于连接级状态，损坏注入必须固定在单连接池中。
    let fixture = fixture_with_pool_size(&database, None, 0, Duration::from_secs(5), 1).await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA ignore_check_constraints = ON")
        .await?;

    set_token(&fixture.pool, "used_requests = -1").await?;
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await,
        Err(TokenRequestAdmissionRepositoryError::Invariant)
    );
    set_token(&fixture.pool, "used_requests = 0, max_requests = -1").await?;
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await,
        Err(TokenRequestAdmissionRepositoryError::Invariant)
    );
    set_token(
        &fixture.pool,
        "used_requests = 9223372036854775807, max_requests = NULL",
    )
    .await?;
    assert_eq!(
        fixture.repository.admit(fixture.token_id).await,
        Err(TokenRequestAdmissionRepositoryError::Invariant)
    );

    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA ignore_check_constraints = OFF")
        .await?;
    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_operation_timeout_is_fail_closed_while_write_lock_is_held()
-> Result<(), Box<dyn Error>> {
    let database = TestDatabase::new();
    let fixture = fixture_with_timeout(&database, Some(1), 0, Duration::from_millis(50)).await?;
    let transaction = fixture.pool.connection().begin().await?;
    transaction
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE tokens SET name = name WHERE id = ?",
            [1_i64.into()],
        ))
        .await?;

    assert_eq!(
        fixture.repository.admit(fixture.token_id).await,
        Err(TokenRequestAdmissionRepositoryError::Timeout)
    );
    transaction.rollback().await?;
    fixture.close().await?;
    drop(database);
    Ok(())
}

#[tokio::test]
async fn zero_operation_timeout_is_rejected_at_construction() {
    let options = DatabaseOptions::new("sqlite::memory:").unwrap();
    let pool = connect_and_migrate(&options, MigrationOptions::default())
        .await
        .unwrap();
    assert!(matches!(
        TokenRequestAdmissionRepository::new(pool.clone(), Duration::ZERO),
        Err(TokenRequestAdmissionRepositoryConfigError::ZeroOperationTimeout)
    ));
    pool.close().await.unwrap();
}

async fn fixture(
    database: &TestDatabase,
    max_requests: Option<i64>,
    used_requests: i64,
) -> Result<Fixture, Box<dyn Error>> {
    fixture_with_timeout(
        database,
        max_requests,
        used_requests,
        Duration::from_secs(5),
    )
    .await
}

async fn fixture_with_timeout(
    database: &TestDatabase,
    max_requests: Option<i64>,
    used_requests: i64,
    operation_timeout: Duration,
) -> Result<Fixture, Box<dyn Error>> {
    fixture_with_pool_size(database, max_requests, used_requests, operation_timeout, 8).await
}

async fn fixture_with_pool_size(
    database: &TestDatabase,
    max_requests: Option<i64>,
    used_requests: i64,
    operation_timeout: Duration,
    max_connections: u32,
) -> Result<Fixture, Box<dyn Error>> {
    let options = DatabaseOptions::new(database.url.clone())?.with_pool_options(PoolOptions {
        max_connections,
        min_connections: 1,
        acquire_timeout: Duration::from_secs(15),
        ..PoolOptions::default()
    });
    let pool = connect_and_migrate(&options, MigrationOptions::default()).await?;
    seed_account(&pool, max_requests, used_requests).await?;
    let repository = TokenRequestAdmissionRepository::new(pool.clone(), operation_timeout)?;
    Ok(Fixture {
        pool,
        repository,
        token_id: TokenId::new(1).expect("测试令牌标识必须为正数"),
    })
}

async fn seed_account(
    pool: &DatabasePool,
    max_requests: Option<i64>,
    used_requests: i64,
) -> Result<(), Box<dyn Error>> {
    let connection = pool.connection();
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO groups (id, name, display_name, flags) VALUES (?, ?, ?, ?)",
            [
                3_i64.into(),
                "admission-group".into(),
                "准入测试分组".into(),
                "{}".into(),
            ],
        ))
        .await?;
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO users (id, username, status, default_group_id, quota, aff_code, settings) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [
                2_i64.into(),
                "admission-user".into(),
                1_i16.into(),
                3_i64.into(),
                1_000_000_i64.into(),
                "admission-aff".into(),
                "{}".into(),
            ],
        ))
        .await?;
    let max_requests: DatabaseValue = match max_requests {
        Some(value) => value.into(),
        None => Option::<i64>::None.into(),
    };
    connection
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO tokens (id, user_id, key_hash, key_prefix, name, status, remain_quota, max_requests, used_requests) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            [
                1_i64.into(),
                2_i64.into(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                "sk-af-test".into(),
                "admission-token".into(),
                1_i16.into(),
                1_000_000_i64.into(),
                max_requests,
                used_requests.into(),
            ],
        ))
        .await?;
    Ok(())
}

async fn set_token(pool: &DatabasePool, assignment: &str) -> Result<(), Box<dyn Error>> {
    pool.connection()
        .execute_unprepared(&format!("UPDATE tokens SET {assignment} WHERE id = 1"))
        .await?;
    Ok(())
}

async fn used_requests(pool: &DatabasePool) -> Result<i64, Box<dyn Error>> {
    let row = pool
        .connection()
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT used_requests FROM tokens WHERE id = 1".to_owned(),
        ))
        .await?
        .expect("测试令牌必须存在");
    Ok(row.try_get("", "used_requests")?)
}
