use std::{
    error::Error,
    io::{self, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ConnectionTrait, DbErr, EntityTrait, IntoActiveModel, JsonValue, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, Query, SimpleExpr},
};
use tracing::{Level, instrument::WithSubscriber};
use tracing_subscriber::fmt::format::FmtSpan;

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, TokenAuthLookup, TokenAuthLookupError,
    TokenAuthLookupOutcome, TokenAuthRepository, TokenAuthRepositoryConfigError,
    TokenAuthRepositoryError,
    entity::{TokenHash, groups, tokens, users},
};

const VALID_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const UNKNOWN_HASH: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

struct Fixture {
    pool: DatabasePool,
    repository: TokenAuthRepository,
    lookup: TokenAuthLookup,
    token_id: i64,
    user_id: i64,
    default_group_id: i64,
    token_group_id: i64,
}

#[derive(Clone)]
struct SharedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("日志捕获缓冲区锁已中毒"))?
            .write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .map_err(|_| io::Error::other("日志捕获缓冲区锁已中毒"))?
            .flush()
    }
}

#[test]
fn lookup_value_requires_canonical_digest_and_redacts_debug() {
    let lookup = TokenAuthLookup::new(VALID_HASH).unwrap();
    assert_eq!(format!("{lookup:?}"), "TokenAuthLookup(<redacted>)");
    assert!(!format!("{lookup:?}").contains(VALID_HASH));

    for invalid in [
        "short",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
    ] {
        assert_eq!(
            TokenAuthLookup::new(invalid),
            Err(TokenAuthLookupError::InvalidDigest)
        );
    }
}

#[tokio::test]
async fn lookup_digest_never_enters_trace_output() -> Result<(), Box<dyn Error>> {
    let fixture = fixture_with_sqlx_logging(Duration::from_secs(1), true).await?;
    let captured = Arc::new(Mutex::new(Vec::new()));
    let writer_buffer = Arc::clone(&captured);
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_max_level(Level::TRACE)
        .with_span_events(FmtSpan::NEW)
        .with_writer(move || SharedWriter(Arc::clone(&writer_buffer)))
        .finish();

    assert!(matches!(
        fixture
            .repository
            .lookup(&fixture.lookup)
            .with_subscriber(subscriber)
            .await?,
        TokenAuthLookupOutcome::Authenticated(_)
    ));
    let trace_output =
        String::from_utf8(captured.lock().expect("日志捕获缓冲区锁不应中毒").clone())?;
    assert!(!trace_output.contains(VALID_HASH));
    for identifier in [fixture.token_id, fixture.user_id, fixture.token_group_id] {
        assert!(!trace_output.contains(&identifier.to_string()));
    }

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn valid_lookup_uses_token_override_and_user_default_group() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;

    assert_authenticated_group(
        fixture.repository.lookup(&fixture.lookup).await?,
        fixture.token_id,
        fixture.user_id,
        fixture.token_group_id,
    );

    let mut token = tokens::Entity::find_by_id(fixture.token_id)
        .one(fixture.pool.connection())
        .await?
        .expect("测试令牌必须存在")
        .into_active_model();
    token.group_id = Set(None);
    token.update(fixture.pool.connection()).await?;

    assert_authenticated_group(
        fixture.repository.lookup(&fixture.lookup).await?,
        fixture.token_id,
        fixture.user_id,
        fixture.default_group_id,
    );

    set_token_column(
        &fixture,
        tokens::Column::ExpiredAt,
        Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
    )
    .await?;
    assert_authenticated_group(
        fixture.repository.lookup(&fixture.lookup).await?,
        fixture.token_id,
        fixture.user_id,
        fixture.default_group_id,
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_disabled_deleted_and_expired_tokens_are_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    let unknown = TokenAuthLookup::new(UNKNOWN_HASH)?;
    assert_eq!(
        fixture.repository.lookup(&unknown).await?,
        TokenAuthLookupOutcome::Rejected
    );

    set_token_column(&fixture, tokens::Column::Status, Expr::value(2_i16)).await?;
    assert_rejected(&fixture).await?;

    set_token_column(&fixture, tokens::Column::Status, Expr::value(1_i16)).await?;
    set_token_column(
        &fixture,
        tokens::Column::DeletedAt,
        Expr::value(Some(TimeDateTimeWithTimeZone::now_utc())),
    )
    .await?;
    assert_rejected(&fixture).await?;

    set_token_column(
        &fixture,
        tokens::Column::DeletedAt,
        Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
    )
    .await?;
    set_token_column(
        &fixture,
        tokens::Column::ExpiredAt,
        Expr::value(Some(TimeDateTimeWithTimeZone::now_utc())),
    )
    .await?;
    assert_rejected(&fixture).await?;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn disabled_or_deleted_user_and_deleted_group_are_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;

    set_user_column(&fixture, users::Column::Status, Expr::value(2_i16)).await?;
    assert_rejected(&fixture).await?;

    set_user_column(&fixture, users::Column::Status, Expr::value(1_i16)).await?;
    set_user_column(
        &fixture,
        users::Column::DeletedAt,
        Expr::value(Some(TimeDateTimeWithTimeZone::now_utc())),
    )
    .await?;
    assert_rejected(&fixture).await?;

    set_user_column(
        &fixture,
        users::Column::DeletedAt,
        Expr::value(Option::<TimeDateTimeWithTimeZone>::None),
    )
    .await?;
    set_group_deleted(&fixture, fixture.token_group_id).await?;
    assert_rejected(&fixture).await?;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn corrupt_token_or_user_status_is_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA ignore_check_constraints = ON")
        .await?;

    set_token_column(&fixture, tokens::Column::Status, Expr::value(9_i16)).await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    set_token_column(&fixture, tokens::Column::Status, Expr::value(1_i16)).await?;
    set_user_column(&fixture, users::Column::Status, Expr::value(9_i16)).await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn duplicate_digest_is_internal_even_if_unique_index_is_damaged() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(Duration::from_secs(1)).await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("DROP INDEX uq_tokens_key_hash")
        .await?;
    tokens::ActiveModel {
        user_id: Set(fixture.user_id),
        key_hash: Set(TokenHash::parse(VALID_HASH).expect("固定摘要必须有效")),
        key_prefix: Set("sk-af-duplicate".to_owned()),
        name: Set("duplicate-auth-token".to_owned()),
        status: Set(1),
        group_id: Set(Some(fixture.token_group_id)),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;

    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn non_positive_persisted_id_is_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_token_column(&fixture, tokens::Column::Id, Expr::value(0_i64)).await?;

    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn missing_user_or_effective_group_is_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA foreign_keys = OFF")
        .await?;

    // 数据损坏优先于业务禁用态，禁止把缺失外键伪装成普通鉴权拒绝。
    set_token_column(&fixture, tokens::Column::Status, Expr::value(2_i16)).await?;
    set_token_column(&fixture, tokens::Column::UserId, Expr::value(i64::MAX)).await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    set_token_column(
        &fixture,
        tokens::Column::UserId,
        Expr::value(fixture.user_id),
    )
    .await?;
    set_token_column(&fixture, tokens::Column::Status, Expr::value(1_i16)).await?;
    set_token_column(
        &fixture,
        tokens::Column::GroupId,
        Expr::value(Some(i64::MAX)),
    )
    .await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn lookup_timeout_and_closed_pool_are_internal() -> Result<(), Box<dyn Error>> {
    let timeout_fixture = fixture(Duration::from_millis(1)).await?;
    let transaction = timeout_fixture.pool.connection().begin().await?;
    assert_repository_error(&timeout_fixture, TokenAuthRepositoryError::Timeout).await;
    transaction.rollback().await?;
    let recovery_repository =
        TokenAuthRepository::new(timeout_fixture.pool.clone(), Duration::from_secs(1))?;
    assert!(matches!(
        recovery_repository.lookup(&timeout_fixture.lookup).await?,
        TokenAuthLookupOutcome::Authenticated(_)
    ));
    timeout_fixture.pool.close().await?;

    let closed_fixture = fixture(Duration::from_secs(1)).await?;
    closed_fixture.pool.clone().close().await?;
    assert_repository_error(&closed_fixture, TokenAuthRepositoryError::Query).await;
    Ok(())
}

#[test]
fn zero_lookup_timeout_is_rejected() -> Result<(), Box<dyn Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let pool = runtime.block_on(crate::connect(&DatabaseOptions::new("sqlite::memory:")?))?;
    assert!(matches!(
        TokenAuthRepository::new(pool.clone(), Duration::ZERO),
        Err(TokenAuthRepositoryConfigError::ZeroLookupTimeout)
    ));
    runtime.block_on(pool.close())?;
    Ok(())
}

async fn fixture(lookup_timeout: Duration) -> Result<Fixture, Box<dyn Error>> {
    fixture_with_sqlx_logging(lookup_timeout, false).await
}

async fn fixture_with_sqlx_logging(
    lookup_timeout: Duration,
    sqlx_logging: bool,
) -> Result<Fixture, Box<dyn Error>> {
    let options = DatabaseOptions::new("sqlite::memory:")?.with_sqlx_logging(sqlx_logging);
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let default_group = insert_group(&pool, "default-group").await?;
    let token_group = insert_group(&pool, "token-group").await?;
    let user = users::ActiveModel {
        username: Set("auth-user".to_owned()),
        status: Set(1),
        default_group_id: Set(default_group.id),
        aff_code: Set("auth-user-aff".to_owned()),
        settings: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let token = tokens::ActiveModel {
        user_id: Set(user.id),
        key_hash: Set(TokenHash::parse(VALID_HASH).expect("固定摘要必须有效")),
        key_prefix: Set("sk-af-test".to_owned()),
        name: Set("auth-token".to_owned()),
        status: Set(1),
        group_id: Set(Some(token_group.id)),
        expired_at: Set(Some(
            TimeDateTimeWithTimeZone::now_utc() + Duration::from_secs(60 * 60),
        )),
        ..Default::default()
    }
    .insert(pool.connection())
    .await?;
    let repository = TokenAuthRepository::new(pool.clone(), lookup_timeout)?;

    Ok(Fixture {
        pool,
        repository,
        lookup: TokenAuthLookup::new(VALID_HASH)?,
        token_id: token.id,
        user_id: user.id,
        default_group_id: default_group.id,
        token_group_id: token_group.id,
    })
}

async fn insert_group(pool: &DatabasePool, name: &str) -> Result<groups::Model, DbErr> {
    groups::ActiveModel {
        name: Set(name.to_owned()),
        display_name: Set(name.to_owned()),
        flags: Set(JsonValue::Object(Default::default())),
        ..Default::default()
    }
    .insert(pool.connection())
    .await
}

async fn set_token_column(
    fixture: &Fixture,
    column: tokens::Column,
    value: SimpleExpr,
) -> Result<(), DbErr> {
    let statement = Query::update()
        .table(tokens::Entity)
        .value(column, value)
        .and_where(Expr::col(tokens::Column::Id).eq(fixture.token_id))
        .to_owned();
    fixture
        .pool
        .connection()
        .execute(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&statement),
        )
        .await?;
    Ok(())
}

async fn set_user_column(
    fixture: &Fixture,
    column: users::Column,
    value: SimpleExpr,
) -> Result<(), DbErr> {
    let statement = Query::update()
        .table(users::Entity)
        .value(column, value)
        .and_where(Expr::col(users::Column::Id).eq(fixture.user_id))
        .to_owned();
    fixture
        .pool
        .connection()
        .execute(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&statement),
        )
        .await?;
    Ok(())
}

async fn set_group_deleted(fixture: &Fixture, group_id: i64) -> Result<(), DbErr> {
    let statement = Query::update()
        .table(groups::Entity)
        .value(
            groups::Column::DeletedAt,
            Expr::value(Some(TimeDateTimeWithTimeZone::now_utc())),
        )
        .and_where(Expr::col(groups::Column::Id).eq(group_id))
        .to_owned();
    fixture
        .pool
        .connection()
        .execute(
            fixture
                .pool
                .connection()
                .get_database_backend()
                .build(&statement),
        )
        .await?;
    Ok(())
}

async fn assert_rejected(fixture: &Fixture) -> Result<(), TokenAuthRepositoryError> {
    assert_eq!(
        fixture.repository.lookup(&fixture.lookup).await?,
        TokenAuthLookupOutcome::Rejected
    );
    Ok(())
}

async fn assert_repository_error(fixture: &Fixture, expected: TokenAuthRepositoryError) {
    assert_eq!(
        fixture.repository.lookup(&fixture.lookup).await,
        Err(expected)
    );
}

fn assert_authenticated_group(
    outcome: TokenAuthLookupOutcome,
    token_id: i64,
    user_id: i64,
    group_id: i64,
) {
    let TokenAuthLookupOutcome::Authenticated(principal) = outcome else {
        panic!("有效令牌必须返回已认证主体");
    };
    assert_eq!(principal.token_id().get(), token_id);
    assert_eq!(principal.user_id().get(), user_id);
    assert_eq!(principal.group_id().get(), group_id);
}
