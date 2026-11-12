use std::{error::Error, time::Duration};

use af_domain::{TrustedClientIp, UserId};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

use super::*;
use crate::{
    DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
    MigrationOptions,
};

#[tokio::test]
async fn personal_access_token_proof_binding_replay_and_transactional_audit()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let record = create(fixture.user, 1, vec!["account:read"]);
    let claim = proof(&fixture.repository, &record, 1).await?;
    let mut wrong_session = claim.clone();
    wrong_session.session_binding = "b".repeat(64);
    assert!(matches!(
        fixture
            .repository
            .create(record.clone(), wrong_session, context())
            .await,
        Err(PersonalAccessTokenRepositoryError::InvalidProof)
    ));
    let mut stale_session = claim.clone();
    stale_session.session_version = 2;
    assert!(matches!(
        fixture
            .repository
            .create(record.clone(), stale_session, context())
            .await,
        Err(PersonalAccessTokenRepositoryError::OwnerUnavailable)
    ));
    let mut wrong_owner = record.clone();
    wrong_owner.user_id = fixture.admin;
    assert!(matches!(
        fixture
            .repository
            .create(wrong_owner, claim.clone(), context())
            .await,
        Err(PersonalAccessTokenRepositoryError::InvalidProof)
    ));
    let mut wrong = record.clone();
    wrong.name = "tampered".to_owned();
    assert!(matches!(
        fixture
            .repository
            .create(wrong, claim.clone(), context())
            .await,
        Err(PersonalAccessTokenRepositoryError::InvalidProof)
    ));
    let mut invalid_context = context();
    invalid_context.request_id.clear();
    assert!(matches!(
        fixture
            .repository
            .create(record.clone(), claim.clone(), invalid_context)
            .await,
        Err(PersonalAccessTokenRepositoryError::InvalidInput)
    ));
    assert!(
        fixture
            .repository
            .list(fixture.user, None, 50)
            .await?
            .tokens
            .is_empty()
    );
    let created = fixture
        .repository
        .create(record, claim.clone(), context())
        .await?;
    assert_eq!(created.scopes, vec!["account:read"]);
    assert!(matches!(
        fixture
            .repository
            .create(
                create(fixture.user, 2, vec!["account:read"]),
                claim,
                context()
            )
            .await,
        Err(PersonalAccessTokenRepositoryError::InvalidProof)
    ));
    assert!(matches!(
        fixture
            .repository
            .revoke(fixture.admin, created.id, context())
            .await,
        Err(PersonalAccessTokenRepositoryError::NotFound)
    ));
    fixture
        .repository
        .revoke(fixture.user, created.id, context())
        .await?;
    fixture
        .repository
        .revoke(fixture.user, created.id, context())
        .await?;
    let audit = fixture
        .pool
        .connection()
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM access_token_audit_logs WHERE operation = 'create'"
                .to_owned(),
        ))
        .await?
        .unwrap();
    assert_eq!(audit.try_get::<i64>("", "count")?, 1);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn personal_access_token_scope_growth_ip_growth_and_version_require_proof()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let mut record = create(fixture.user, 3, vec!["account:read"]);
    record.allow_ips = Some(vec!["10.0.0.1/32".to_owned()]);
    let claim = proof(&fixture.repository, &record, 3).await?;
    let token = fixture.repository.create(record, claim, context()).await?;
    let update = PersonalAccessTokenUpdateRecord {
        name: "renamed".to_owned(),
        scopes: token.scopes.clone(),
        expires_at: token.expires_at,
        allow_ips: None,
        enabled: true,
        expected_version: 1,
    };
    assert!(matches!(
        fixture
            .repository
            .update(fixture.user, token.id, update.clone(), None, context())
            .await,
        Err(PersonalAccessTokenRepositoryError::InvalidProof)
    ));
    let mut safe = update;
    safe.allow_ips = token.allow_ips;
    let changed = fixture
        .repository
        .update(fixture.user, token.id, safe.clone(), None, context())
        .await?;
    assert_eq!(changed.version, 2);
    assert!(matches!(
        fixture
            .repository
            .update(fixture.user, token.id, safe, None, context())
            .await,
        Err(PersonalAccessTokenRepositoryError::Conflict)
    ));
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn personal_access_token_current_role_policy_and_model_key_lifecycle_are_independent()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let admin_record = create(
        fixture.admin,
        4,
        vec!["admin:list_admin_users:execute", "account:read"],
    );
    let claim = proof(&fixture.repository, &admin_record, 4).await?;
    let admin_token = fixture
        .repository
        .create(admin_record.clone(), claim, context())
        .await?;
    let ordinary = create(fixture.user, 5, vec!["admin:list_admin_users:execute"]);
    let claim = proof(&fixture.repository, &ordinary, 5).await?;
    assert!(matches!(
        fixture.repository.create(ordinary, claim, context()).await,
        Err(PersonalAccessTokenRepositoryError::Forbidden)
    ));
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE users SET role=0 WHERE id=?",
            [fixture.admin.get().into()],
        ))
        .await?;
    let PersonalAccessTokenLookup::Authenticated { role, token, .. } = fixture
        .repository
        .authenticate(&admin_record.secret_hash, ip())
        .await?
    else {
        panic!("valid token");
    };
    assert_eq!(role, 0);
    assert_eq!(token.scopes, vec!["account:read"]);
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE users SET role=1 WHERE id=?",
            [fixture.admin.get().into()],
        ))
        .await?;
    fixture.pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
        "INSERT INTO tokens (user_id,key_hash,key_prefix,name,status,remain_quota,unlimited_quota,used_quota,cross_group_retry,usage_5h,usage_1d,usage_7d) VALUES (?,?,?,?,1,1000,0,0,0,0,0,0)",
        [fixture.admin.get().into(), format!("{:064x}", 700).into(), "sk-af-test000000001".into(), "independent-model-key".into()])).await?;
    let policy = fixture.repository.lookup_policy().await?;
    let disabled = fixture
        .repository
        .update_policy(
            fixture.admin,
            PersonalAccessTokenPolicyUpdateRecord {
                enabled: false,
                issuance_enabled: true,
                max_tokens_per_user: 20,
                expected_version: policy.version,
            },
            context(),
        )
        .await?;
    assert!(!disabled.enabled);
    assert!(matches!(
        fixture
            .repository
            .authenticate(&admin_record.secret_hash, ip())
            .await?,
        PersonalAccessTokenLookup::Rejected(Some(PersonalAccessTokenAuditIdentity {
            reason: PersonalAccessTokenRejectionReason::PolicyDisabled,
            ..
        }))
    ));
    fixture
        .repository
        .revoke(fixture.admin, admin_token.id, context())
        .await?;
    let row = fixture
        .pool
        .connection()
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT status FROM tokens WHERE name='independent-model-key'".to_owned(),
        ))
        .await?
        .unwrap();
    assert_eq!(row.try_get::<i16>("", "status")?, 1);
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn personal_access_token_concurrent_capacity_and_disabled_owner_emergency_revoke()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    fixture
        .repository
        .update_policy(
            fixture.admin,
            PersonalAccessTokenPolicyUpdateRecord {
                enabled: true,
                issuance_enabled: true,
                max_tokens_per_user: 1,
                expected_version: 1,
            },
            context(),
        )
        .await?;
    let first = create(fixture.user, 6, vec!["account:read"]);
    let second = create(fixture.user, 7, vec!["account:read"]);
    let first_proof = proof(&fixture.repository, &first, 6).await?;
    let second_proof = proof(&fixture.repository, &second, 7).await?;
    let (left, right) = tokio::join!(
        fixture.repository.create(first, first_proof, context()),
        fixture.repository.create(second, second_proof, context())
    );
    assert!(matches!(
        (left, right),
        (Ok(_), Err(PersonalAccessTokenRepositoryError::LimitReached))
            | (Err(PersonalAccessTokenRepositoryError::LimitReached), Ok(_))
    ));
    fixture
        .pool
        .connection()
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "UPDATE users SET status=2 WHERE id=?",
            [fixture.user.get().into()],
        ))
        .await?;
    assert_eq!(
        fixture
            .repository
            .revoke_all(fixture.admin, fixture.user, context())
            .await?,
        1
    );
    assert_eq!(
        fixture
            .repository
            .revoke_all(fixture.admin, fixture.user, context())
            .await?,
        0
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn personal_access_token_shared_request_windows_count_atomically()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let other_repository =
        PersonalAccessTokenRepository::new(fixture.pool.clone(), Duration::from_secs(5))?;
    for _ in 0..119 {
        fixture
            .repository
            .admit_authenticated_request(fixture.user, 100)
            .await?;
    }
    let (left, right) = tokio::join!(
        fixture
            .repository
            .admit_authenticated_request(fixture.user, 100),
        other_repository.admit_authenticated_request(fixture.user, 100)
    );
    assert!(matches!(
        (left, right),
        (
            Ok(()),
            Err(PersonalAccessTokenRepositoryError::RateLimited { .. })
        ) | (
            Err(PersonalAccessTokenRepositoryError::RateLimited { .. }),
            Ok(())
        )
    ));
    // A different token shares the user cap but has its own independent token counter.
    fixture
        .repository
        .admit_authenticated_request(fixture.user, 101)
        .await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn personal_access_token_proof_issuance_budget_is_shared_across_sessions()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    for seed in 50..70 {
        proof(
            &fixture.repository,
            &create(fixture.user, seed, vec!["account:read"]),
            seed,
        )
        .await?;
    }
    assert!(matches!(
        proof(
            &fixture.repository,
            &create(fixture.user, 70, vec!["account:read"]),
            70
        )
        .await,
        Err(PersonalAccessTokenRepositoryError::RateLimited {
            retry_after: 1..=300
        })
    ));
    assert!(
        proof(
            &fixture.repository,
            &create(fixture.admin, 71, vec!["account:read"]),
            71
        )
        .await
        .is_ok()
    );
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn personal_access_token_file_sqlite_admission_waits_for_writer_without_read_upgrade()
-> Result<(), Box<dyn Error>> {
    use sea_orm::TransactionTrait;
    let database = FileDatabase::new()?;
    let options =
        DatabaseOptions::new(database.url.clone())?.with_pool_options(crate::PoolOptions {
            max_connections: 2,
            min_connections: 2,
            ..crate::PoolOptions::default()
        });
    let pool = crate::connect_and_migrate(&options, MigrationOptions::default()).await?;
    let other_pool = crate::connection::connect(&options).await?;
    let first = PersonalAccessTokenRepository::new(pool.clone(), Duration::from_secs(5))?;
    let second = PersonalAccessTokenRepository::new(other_pool.clone(), Duration::from_secs(5))?;
    let writer = pool.connection().begin().await?;
    writer.execute(Statement::from_string(DbBackend::Sqlite,
        "INSERT INTO access_token_rate_limits (subject_key,window_started_at,used_requests,expires_at) VALUES ('held-writer',0,0,0)".to_owned())).await?;
    let user = UserId::new(1)?;
    let started_at = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut first_request =
        tokio::spawn(async move { first.admit_authenticated_request(user, 100).await });
    let mut second_request =
        tokio::spawn(async move { second.admit_authenticated_request(user, 100).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut first_request)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut second_request)
            .await
            .is_err()
    );
    writer.commit().await?;
    first_request.await??;
    second_request.await??;
    let finished_at = time::OffsetDateTime::now_utc().unix_timestamp();
    let counts = pool
        .connection()
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT used_requests FROM access_token_rate_limits WHERE subject_key != 'held-writer'"
                .to_owned(),
        ))
        .await?;
    assert_eq!(counts.len(), 2);
    for count in counts {
        let used = count.try_get::<i32>("", "used_requests")?;
        if started_at.div_euclid(60) == finished_at.div_euclid(60) {
            assert_eq!(used, 2);
        } else {
            assert!((1..=2).contains(&used));
        }
    }
    pool.close().await?;
    other_pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn personal_access_token_expired_counter_cleanup_preserves_refreshed_candidates()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    fixture.pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
        "INSERT INTO access_token_rate_limits (subject_key,window_started_at,used_requests,expires_at) VALUES ('refreshed',0,119,?),('expired',0,1,?)",
        [(now - 1).into(), (now - 1).into()])).await?;
    // These IDs represent a cleanup scan completed before a concurrent request refreshed one row.
    let candidates = vec!["refreshed".to_owned(), "expired".to_owned()];
    fixture.pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
        "UPDATE access_token_rate_limits SET used_requests=120,expires_at=? WHERE subject_key='refreshed'",
        [(now + 120).into()])).await?;
    super::rate_limit::delete_expired(&fixture.repository, candidates, now).await?;
    let rows = fixture
        .pool
        .connection()
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT subject_key,used_requests FROM access_token_rate_limits".to_owned(),
        ))
        .await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].try_get::<String>("", "subject_key")?, "refreshed");
    assert_eq!(rows[0].try_get::<i32>("", "used_requests")?, 120);
    fixture.pool.close().await?;
    Ok(())
}

struct FileDatabase {
    path: std::path::PathBuf,
    url: String,
}

impl FileDatabase {
    fn new() -> Result<Self, std::time::SystemTimeError> {
        let serial = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "anyflows-pat-rate-limit-{}-{serial}.db",
            std::process::id()
        ));
        let mut sqlite_path = path.to_string_lossy().replace('\\', "/");
        if !sqlite_path.starts_with('/') {
            sqlite_path.insert(0, '/');
        }
        Ok(Self {
            path,
            url: format!("sqlite://{sqlite_path}?mode=rwc"),
        })
    }
}

impl Drop for FileDatabase {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let candidate = std::path::PathBuf::from(format!("{}{}", self.path.display(), suffix));
            let _ = std::fs::remove_file(candidate);
        }
    }
}

struct Fixture {
    pool: crate::DatabasePool,
    repository: PersonalAccessTokenRepository,
    admin: UserId,
    user: UserId,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = crate::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let setup = InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let InitialSetupOutcome::Initialized { user_id: admin } = setup
        .initialize(InitialSetupRecord::new(
            "pat-admin".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await?
    else {
        panic!("fresh database");
    };
    let user = UserId::new(admin.get() + 1)?;
    pool.connection().execute(Statement::from_sql_and_values(DbBackend::Sqlite,
        "INSERT INTO users (id,username,role,status,default_group_id,quota,used_quota,frozen_quota,request_count,aff_code,settings) VALUES (?,?,0,1,1,0,0,0,0,?,?)",
        [user.get().into(), "pat-user".into(), "pat-user-aff".into(), "{}".into()])).await?;
    Ok(Fixture {
        repository: PersonalAccessTokenRepository::new(pool.clone(), Duration::from_secs(5))?,
        pool,
        admin,
        user,
    })
}

fn create(owner: UserId, index: i64, scopes: Vec<&str>) -> PersonalAccessTokenCreateRecord {
    PersonalAccessTokenCreateRecord {
        user_id: owner,
        secret_hash: format!("{:064x}", index),
        token_hint: format!("af_pat_{index:012}"),
        name: format!("PAT {index}"),
        scopes: scopes.into_iter().map(str::to_owned).collect(),
        expires_at: Some(time::OffsetDateTime::now_utc().unix_timestamp() + 3600),
        allow_ips: None,
    }
}

async fn proof(
    repository: &PersonalAccessTokenRepository,
    record: &PersonalAccessTokenCreateRecord,
    seed: i64,
) -> Result<PersonalAccessTokenProofClaim, PersonalAccessTokenRepositoryError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let claim = PersonalAccessTokenProofClaim {
        proof_hash: format!("{:064x}", seed + 100),
        session_version: 1,
        session_binding: format!("{seed:064x}"),
        purpose: "create".to_owned(),
        target_id: None,
        payload_digest: personal_access_token_payload_digest(
            &record.name,
            &record.scopes,
            record.expires_at,
            record.allow_ips.as_deref(),
            true,
            None,
        )?,
    };
    repository
        .issue_proof(
            PersonalAccessTokenProofRecord {
                proof_hash: claim.proof_hash.clone(),
                user_id: record.user_id,
                session_version: 1,
                session_binding: claim.session_binding.clone(),
                purpose: claim.purpose.clone(),
                target_id: None,
                payload_digest: claim.payload_digest.clone(),
                issued_at: now,
                expires_at: now + 300,
            },
            context(),
        )
        .await?;
    Ok(claim)
}

fn ip() -> TrustedClientIp {
    TrustedClientIp::new("10.0.0.1".parse().unwrap())
}

fn context() -> PersonalAccessTokenMutationContext {
    PersonalAccessTokenMutationContext {
        request_id: "pat-test-request".to_owned(),
        method: "POST".to_owned(),
        route: "/api/account/access-tokens".to_owned(),
        resource_id: None,
        client_ip: ip(),
    }
}
