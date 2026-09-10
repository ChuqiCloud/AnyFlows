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

use af_domain::{
    MAX_MODEL_NAME_BYTES, MAX_TOKEN_MODEL_ALLOWLIST_COUNT, MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES,
    TokenModelPolicy, TrustedClientIp,
};

use crate::{
    DatabaseOptions, DatabasePool, MigrationOptions, TokenAuthLookup, TokenAuthLookupError,
    TokenAuthLookupOutcome, TokenAuthRepository, TokenAuthRepositoryConfigError,
    TokenAuthRepositoryError,
    entity::{TokenHash, groups, tokens, users},
};

const VALID_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const UNKNOWN_HASH: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

fn test_client_ip() -> TrustedClientIp {
    client_ip("192.0.2.10")
}

fn client_ip(value: &str) -> TrustedClientIp {
    TrustedClientIp::new(value.parse().unwrap())
}

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
    let model_canary = "private-model-policy-canary";
    set_token_column(
        &fixture,
        tokens::Column::ModelLimits,
        Expr::value(JsonValue::Array(vec![JsonValue::String(
            model_canary.to_owned(),
        )])),
    )
    .await?;
    set_token_column(
        &fixture,
        tokens::Column::AllowIps,
        Expr::value(JsonValue::Array(vec![JsonValue::String(
            "192.0.2.0/24".to_owned(),
        )])),
    )
    .await?;
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
            .lookup(&fixture.lookup, test_client_ip())
            .with_subscriber(subscriber)
            .await?,
        TokenAuthLookupOutcome::Authenticated { .. }
    ));
    let trace_output =
        String::from_utf8(captured.lock().expect("日志捕获缓冲区锁不应中毒").clone())?;
    assert!(!trace_output.contains(VALID_HASH));
    assert!(!trace_output.contains("192.0.2.10"));
    assert!(!trace_output.contains("192.0.2.0/24"));
    assert!(!trace_output.contains(model_canary));
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
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
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
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
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
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
        fixture.token_id,
        fixture.user_id,
        fixture.default_group_id,
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn reserved_playground_key_cannot_authenticate_externally() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_token_column(
        &fixture,
        tokens::Column::Name,
        Expr::value(af_domain::PLAYGROUND_TOKEN_NAME),
    )
    .await?;

    assert!(matches!(
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
        TokenAuthLookupOutcome::Rejected
    ));

    set_token_column(&fixture, tokens::Column::Name, Expr::value("regular-token")).await?;
    let TokenAuthLookupOutcome::Authenticated { principal, .. } = fixture
        .repository
        .lookup(&fixture.lookup, test_client_ip())
        .await?
    else {
        return Err("普通 Token 必须返回已认证主体".into());
    };
    assert!(!principal.is_playground());
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn playground_session_reuses_one_internal_principal_and_rotates_legacy_hash()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_token_column(
        &fixture,
        tokens::Column::Name,
        Expr::value(af_domain::PLAYGROUND_TOKEN_NAME),
    )
    .await?;

    let first = fixture
        .repository
        .lookup_playground(af_domain::UserId::new(fixture.user_id)?)
        .await?;
    let second = fixture
        .repository
        .lookup_playground(af_domain::UserId::new(fixture.user_id)?)
        .await?;
    let (
        TokenAuthLookupOutcome::Authenticated {
            principal: first_principal,
            ..
        },
        TokenAuthLookupOutcome::Authenticated {
            principal: second_principal,
            ..
        },
    ) = (first, second)
    else {
        return Err("有效会话必须解析为试炼场主体".into());
    };
    assert!(first_principal.is_playground());
    assert_eq!(first_principal.token_id(), second_principal.token_id());
    assert_eq!(first_principal.token_id().get(), fixture.token_id);

    let stored = tokens::Entity::find_by_id(fixture.token_id)
        .one(fixture.pool.connection())
        .await?
        .expect("内部试炼场主体必须存在");
    assert_eq!(stored.key_prefix, "internal-playground");
    assert_ne!(stored.key_hash, TokenHash::parse(VALID_HASH)?);
    assert!(stored.deleted_at.is_none());
    assert!(matches!(
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
        TokenAuthLookupOutcome::Rejected
    ));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn playground_session_soft_deletes_duplicate_legacy_principals() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_token_column(
        &fixture,
        tokens::Column::Name,
        Expr::value(af_domain::PLAYGROUND_TOKEN_NAME),
    )
    .await?;
    let duplicate = tokens::ActiveModel {
        user_id: Set(fixture.user_id),
        key_hash: Set(TokenHash::parse(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )?),
        key_prefix: Set("sk-af-legacy".to_owned()),
        name: Set(af_domain::PLAYGROUND_TOKEN_NAME.to_owned()),
        status: Set(1),
        ..Default::default()
    }
    .insert(fixture.pool.connection())
    .await?;

    let outcome = fixture
        .repository
        .lookup_playground(af_domain::UserId::new(fixture.user_id)?)
        .await?;
    let TokenAuthLookupOutcome::Authenticated { principal, .. } = outcome else {
        return Err("有效会话必须解析为试炼场主体".into());
    };
    assert_eq!(principal.token_id().get(), fixture.token_id);

    let canonical = tokens::Entity::find_by_id(fixture.token_id)
        .one(fixture.pool.connection())
        .await?
        .expect("规范试炼场主体必须存在");
    let duplicate = tokens::Entity::find_by_id(duplicate.id)
        .one(fixture.pool.connection())
        .await?
        .expect("重复主体必须保留审计记录");
    assert!(canonical.deleted_at.is_none());
    assert!(duplicate.deleted_at.is_some());

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn valid_lookup_carries_the_user_concurrency_snapshot() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_user_column(
        &fixture,
        users::Column::Concurrency,
        Expr::value(Some(3_i32)),
    )
    .await?;

    let outcome = fixture
        .repository
        .lookup(&fixture.lookup, test_client_ip())
        .await?;
    let TokenAuthLookupOutcome::Authenticated {
        user_concurrency, ..
    } = outcome
    else {
        return Err("有效令牌必须返回用户并发快照".into());
    };
    assert_eq!(user_concurrency.map(|value| value.get()), Some(3));

    set_user_column(
        &fixture,
        users::Column::Concurrency,
        Expr::value(Some(0_i32)),
    )
    .await?;
    let outcome = fixture
        .repository
        .lookup(&fixture.lookup, test_client_ip())
        .await?;
    let TokenAuthLookupOutcome::Authenticated {
        user_concurrency, ..
    } = outcome
    else {
        return Err("零并发配置不应拒绝有效令牌".into());
    };
    assert_eq!(user_concurrency, None);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn valid_lookup_carries_user_and_group_rpm_snapshots() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_user_column(&fixture, users::Column::RpmLimit, Expr::value(Some(60_i32))).await?;
    set_group_column(&fixture, fixture.token_group_id, Expr::value(Some(120_i32))).await?;

    let outcome = fixture
        .repository
        .lookup(&fixture.lookup, test_client_ip())
        .await?;
    let TokenAuthLookupOutcome::Authenticated {
        user_rpm_limit,
        group_rpm_limit,
        ..
    } = outcome
    else {
        return Err("有效令牌必须返回 RPM 快照".into());
    };
    assert_eq!(user_rpm_limit.map(|value| value.get()), Some(60));
    assert_eq!(group_rpm_limit.map(|value| value.get()), Some(120));

    set_user_column(&fixture, users::Column::RpmLimit, Expr::value(Some(0_i32))).await?;
    set_group_column(&fixture, fixture.token_group_id, Expr::value(Some(0_i32))).await?;
    let outcome = fixture
        .repository
        .lookup(&fixture.lookup, test_client_ip())
        .await?;
    let TokenAuthLookupOutcome::Authenticated {
        user_rpm_limit,
        group_rpm_limit,
        ..
    } = outcome
    else {
        return Err("零 RPM 配置不应拒绝有效令牌".into());
    };
    assert_eq!(user_rpm_limit, None);
    assert_eq!(group_rpm_limit, None);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn negative_rpm_snapshot_fails_closed() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    // 生产迁移会阻止负值写入；测试显式绕过约束以模拟旧数据或人工损坏。
    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA ignore_check_constraints = ON")
        .await?;
    set_user_column(&fixture, users::Column::RpmLimit, Expr::value(Some(-1_i32))).await?;

    assert_eq!(
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await,
        Err(TokenAuthRepositoryError::Invariant)
    );
    fixture
        .pool
        .connection()
        .execute_unprepared("PRAGMA ignore_check_constraints = OFF")
        .await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn model_policy_null_is_unrestricted_and_allowlist_matches_exactly()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;

    let unrestricted = authenticated_model_policy(
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
    );
    assert!(unrestricted.allows("任意规范模型"));

    set_token_column(
        &fixture,
        tokens::Column::ModelLimits,
        Expr::value(JsonValue::Array(vec![
            JsonValue::String("gpt-test".to_owned()),
            JsonValue::String("gpt-test".to_owned()),
            JsonValue::String("Custom-Model".to_owned()),
        ])),
    )
    .await?;
    let restricted = authenticated_model_policy(
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
    );
    assert!(restricted.allows("gpt-test"));
    assert!(restricted.allows("Custom-Model"));
    assert!(!restricted.allows("GPT-TEST"));
    assert!(!restricted.allows("gpt-test "));

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn malformed_or_unbounded_model_allowlists_are_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    let invalid_values = vec![
        JsonValue::Null,
        JsonValue::Array(Vec::new()),
        JsonValue::Object(Default::default()),
        JsonValue::Array(vec![JsonValue::from(7)]),
        JsonValue::Array(vec![JsonValue::String(String::new())]),
        JsonValue::Array(vec![JsonValue::String(" leading-space".to_owned())]),
        JsonValue::Array(vec![JsonValue::String("line\nbreak".to_owned())]),
        JsonValue::Array(vec![JsonValue::String(
            "x".repeat(MAX_MODEL_NAME_BYTES + 1),
        )]),
        JsonValue::Array(vec![
            JsonValue::String("x".repeat(MAX_MODEL_NAME_BYTES));
            MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES
                / MAX_MODEL_NAME_BYTES
                + 1
        ]),
    ];

    for value in invalid_values {
        set_token_column(&fixture, tokens::Column::ModelLimits, Expr::value(value)).await?;
        assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;
    }

    // 损坏策略优先于令牌禁用态，禁止被普通鉴权拒绝掩盖。
    set_token_column(&fixture, tokens::Column::Status, Expr::value(2_i16)).await?;
    set_token_column(
        &fixture,
        tokens::Column::ModelLimits,
        Expr::value(JsonValue::Null),
    )
    .await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn model_allowlist_accepts_512_entries_and_rejects_513() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    let entries = (0..MAX_TOKEN_MODEL_ALLOWLIST_COUNT)
        .map(|index| JsonValue::String(format!("model-{index}")))
        .collect::<Vec<_>>();

    set_token_column(
        &fixture,
        tokens::Column::ModelLimits,
        Expr::value(JsonValue::Array(entries.clone())),
    )
    .await?;
    let policy = authenticated_model_policy(
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
    );
    assert!(policy.allows("model-511"));

    let mut oversized_count = entries;
    oversized_count.push(JsonValue::String("model-overflow".to_owned()));
    set_token_column(
        &fixture,
        tokens::Column::ModelLimits,
        Expr::value(JsonValue::Array(oversized_count)),
    )
    .await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn oversized_or_malformed_sqlite_model_policy_is_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_token_column(
        &fixture,
        tokens::Column::ModelLimits,
        Expr::value(JsonValue::Array(vec![JsonValue::String(
            "x".repeat(70 * 1024),
        )])),
    )
    .await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture
        .pool
        .connection()
        .execute_unprepared(
            "UPDATE tokens SET model_limits = 'not-json' WHERE key_hash IS NOT NULL",
        )
        .await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn ip_allowlist_supports_exact_cidr_ipv6_and_mapped_addresses() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(Duration::from_secs(1)).await?;

    // SQL NULL 是唯一“不限制 IP”的持久化表示。
    assert!(matches!(
        fixture
            .repository
            .lookup(&fixture.lookup, client_ip("2001:db8:ffff::1"))
            .await?,
        TokenAuthLookupOutcome::Authenticated { .. }
    ));

    set_token_column(
        &fixture,
        tokens::Column::AllowIps,
        Expr::value(JsonValue::Array(vec![
            JsonValue::String("192.0.2.10".to_owned()),
            JsonValue::String("2001:db8:1::1/48".to_owned()),
            JsonValue::String("::ffff:198.51.100.0/120".to_owned()),
        ])),
    )
    .await?;

    for allowed in [
        "192.0.2.10",
        "2001:db8:1::ffff",
        "198.51.100.9",
        "::ffff:198.51.100.9",
    ] {
        assert!(matches!(
            fixture
                .repository
                .lookup(&fixture.lookup, client_ip(allowed))
                .await?,
            TokenAuthLookupOutcome::Authenticated { .. }
        ));
    }
    for rejected in ["192.0.2.11", "2001:db8:2::1", "198.51.101.1"] {
        assert_eq!(
            fixture
                .repository
                .lookup(&fixture.lookup, client_ip(rejected))
                .await?,
            TokenAuthLookupOutcome::Rejected
        );
    }

    for (rule, allowed, rejected) in [
        ("0.0.0.0/0", "203.0.113.9", "2001:db8::9"),
        ("::/0", "2001:db8::9", "203.0.113.9"),
    ] {
        set_token_column(
            &fixture,
            tokens::Column::AllowIps,
            Expr::value(JsonValue::Array(vec![JsonValue::String(rule.to_owned())])),
        )
        .await?;
        assert!(matches!(
            fixture
                .repository
                .lookup(&fixture.lookup, client_ip(allowed))
                .await?,
            TokenAuthLookupOutcome::Authenticated { .. }
        ));
        assert_eq!(
            fixture
                .repository
                .lookup(&fixture.lookup, client_ip(rejected))
                .await?,
            TokenAuthLookupOutcome::Rejected
        );
    }

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn malformed_or_unbounded_ip_allowlists_are_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    let invalid_values = vec![
        JsonValue::Null,
        JsonValue::Array(Vec::new()),
        JsonValue::Object(Default::default()),
        JsonValue::Array(vec![JsonValue::from(7)]),
        JsonValue::Array(vec![JsonValue::String(String::new())]),
        JsonValue::Array(vec![JsonValue::String(" 192.0.2.1".to_owned())]),
        JsonValue::Array(vec![JsonValue::String("192.0.2.1/33".to_owned())]),
        JsonValue::Array(vec![JsonValue::String("::ffff:192.0.2.1/95".to_owned())]),
        JsonValue::Array(vec![JsonValue::String("1".repeat(4_097))]),
    ];

    for value in invalid_values {
        set_token_column(&fixture, tokens::Column::AllowIps, Expr::value(value)).await?;
        assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;
    }

    // 数据损坏优先于业务禁用态，禁止把坏白名单伪装成普通 401。
    set_token_column(&fixture, tokens::Column::Status, Expr::value(2_i16)).await?;
    set_token_column(
        &fixture,
        tokens::Column::AllowIps,
        Expr::value(JsonValue::Null),
    )
    .await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn ip_allowlist_accepts_64_entries_and_rejects_65() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    let entries = (0..64)
        .map(|index| JsonValue::String(format!("192.0.2.{index}")))
        .collect::<Vec<_>>();

    set_token_column(
        &fixture,
        tokens::Column::AllowIps,
        Expr::value(JsonValue::Array(entries.clone())),
    )
    .await?;
    assert!(matches!(
        fixture
            .repository
            .lookup(&fixture.lookup, client_ip("192.0.2.63"))
            .await?,
        TokenAuthLookupOutcome::Authenticated { .. }
    ));

    let mut oversized_count = entries;
    oversized_count.push(JsonValue::String("192.0.2.64".to_owned()));
    set_token_column(
        &fixture,
        tokens::Column::AllowIps,
        Expr::value(JsonValue::Array(oversized_count)),
    )
    .await?;
    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn oversized_serialized_ip_allowlist_is_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    set_token_column(
        &fixture,
        tokens::Column::AllowIps,
        Expr::value(JsonValue::Array(vec![JsonValue::String("x".repeat(8_192))])),
    )
    .await?;

    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn malformed_sqlite_json_allowlist_is_internal() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    fixture
        .pool
        .connection()
        .execute_unprepared("UPDATE tokens SET allow_ips = 'not-json' WHERE key_hash IS NOT NULL")
        .await?;

    assert_repository_error(&fixture, TokenAuthRepositoryError::Invariant).await;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn unknown_disabled_deleted_and_expired_tokens_are_rejected() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(Duration::from_secs(1)).await?;
    let unknown = TokenAuthLookup::new(UNKNOWN_HASH)?;
    assert_eq!(
        fixture
            .repository
            .lookup(&unknown, test_client_ip())
            .await?,
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
        recovery_repository
            .lookup(&timeout_fixture.lookup, test_client_ip())
            .await?,
        TokenAuthLookupOutcome::Authenticated { .. }
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

async fn set_group_column(
    fixture: &Fixture,
    group_id: i64,
    value: SimpleExpr,
) -> Result<(), DbErr> {
    let statement = Query::update()
        .table(groups::Entity)
        .value(groups::Column::RpmLimit, value)
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
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await?,
        TokenAuthLookupOutcome::Rejected
    );
    Ok(())
}

async fn assert_repository_error(fixture: &Fixture, expected: TokenAuthRepositoryError) {
    assert_eq!(
        fixture
            .repository
            .lookup(&fixture.lookup, test_client_ip())
            .await,
        Err(expected)
    );
}

fn assert_authenticated_group(
    outcome: TokenAuthLookupOutcome,
    token_id: i64,
    user_id: i64,
    group_id: i64,
) {
    let TokenAuthLookupOutcome::Authenticated {
        principal,
        model_policy,
        ..
    } = outcome
    else {
        panic!("有效令牌必须返回已认证主体");
    };
    assert_eq!(principal.token_id().get(), token_id);
    assert_eq!(principal.user_id().get(), user_id);
    assert_eq!(principal.group_id().get(), group_id);
    assert!(model_policy.allows("任意规范模型"));
}

fn authenticated_model_policy(outcome: TokenAuthLookupOutcome) -> TokenModelPolicy {
    let TokenAuthLookupOutcome::Authenticated { model_policy, .. } = outcome else {
        panic!("有效令牌必须返回模型策略快照");
    };
    model_policy
}
