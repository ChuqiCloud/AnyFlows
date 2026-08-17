use std::{
    error::Error,
    io,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    AdminChannelRepository, AdminChannelWriteRecord, AdminCredentialCreateOutcome,
    AdminCredentialLookupOutcome, AdminCredentialRecord, AdminCredentialWriteRecord,
    DatabaseOptions, DatabasePool, MigrationOptions, OAuthCredentialRepository,
};
use af_domain::{
    ChannelId, ChannelTimeout, ChannelType, CredentialId, CredentialKind, CredentialQuotaDimension,
    Protocol, Status, UserId,
};
use af_httpclient::{
    HttpClientConfig, HttpClientProvider, HttpTimeouts, ProxyConfig, RemoteDnsPolicy,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Notify, Semaphore, oneshot},
    task::{JoinHandle, JoinSet},
};
use url::Url;

use super::*;
use crate::{
    CredentialDecryptor, CredentialEncryptor, PlainCredentialSecret,
    oauth::{
        OAuthAuthorizationContext, OAuthEndpointErrorCode, OAuthTokenPersistenceService,
        OAuthTokenSet,
    },
};

const TEST_TIMEOUT: Duration = Duration::from_secs(3);

#[tokio::test]
async fn held_distributed_lease_skips_upstream_and_is_shared_by_late_follower()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let first_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("完整 OAuth 凭据必须成为候选");
    let second_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("相同事实必须可供 singleflight 合并");
    let expected_key = oauth_refresh_lease_key(&first_candidate);
    let server = ControlledTokenServer::start().await?;
    let leases = Arc::new(RecordingHeldLeasePort::default());
    let lease_port: Arc<dyn OAuthRefreshLeasePort> = leases.clone();
    let request_timeout = Duration::from_secs(47);
    let coordinator = OAuthRefreshCoordinator::new_with_lease_port(
        [codex_profile()],
        proxied_clients_with_request_timeout(server.address(), request_timeout),
        fixture.service,
        lease_port,
    )?;

    assert_eq!(
        coordinator.refresh(first_candidate).await?,
        OAuthRefreshCoordinatorOutcome::LeaseHeld
    );
    assert_eq!(
        coordinator.refresh(second_candidate).await?,
        OAuthRefreshCoordinatorOutcome::LeaseHeld
    );
    assert_eq!(server.request_count(), 0);
    assert_eq!(
        leases.acquisitions.lock().unwrap().as_slice(),
        &[(
            expected_key,
            request_timeout + OAUTH_REFRESH_LEASE_WRITE_BUFFER
        )]
    );

    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn distributed_lease_failure_closes_without_falling_back_to_upstream()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("完整 OAuth 凭据必须成为候选");
    let server = ControlledTokenServer::start().await?;
    let coordinator = OAuthRefreshCoordinator::new_with_lease_port(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
        Arc::new(FailingLeasePort),
    )?;

    let error = coordinator.refresh(candidate).await.unwrap_err();
    assert_eq!(
        error,
        OAuthRefreshCoordinatorError::DistributedLease(CacheError::Redis {
            operation: af_cache::CacheOperation::LeaseAcquire,
            kind: af_cache::RedisFailureKind::Unavailable,
        })
    );
    assert_eq!(server.request_count(), 0);
    let rendered = format!("{error:?}{error}");
    for private in [
        "initial-refresh-private",
        "token.example",
        "redis://",
        "channel:",
        "credential:",
    ] {
        assert!(!rendered.contains(private));
    }

    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[derive(Default)]
struct RecordingHeldLeasePort {
    acquisitions: Mutex<Vec<(String, Duration)>>,
}

#[async_trait::async_trait]
impl OAuthRefreshLeasePort for RecordingHeldLeasePort {
    async fn acquire(&self, key: &str, ttl: Duration) -> Result<LeaseAcquireOutcome, CacheError> {
        self.acquisitions
            .lock()
            .unwrap()
            .push((key.to_owned(), ttl));
        Ok(LeaseAcquireOutcome::Held)
    }
}

struct FailingLeasePort;

#[async_trait::async_trait]
impl OAuthRefreshLeasePort for FailingLeasePort {
    async fn acquire(&self, _key: &str, _ttl: Duration) -> Result<LeaseAcquireOutcome, CacheError> {
        Err(CacheError::Redis {
            operation: af_cache::CacheOperation::LeaseAcquire,
            kind: af_cache::RedisFailureKind::Unavailable,
        })
    }
}

#[tokio::test]
async fn same_credential_shares_one_refresh_and_retains_completed_result()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let mut candidates = Vec::with_capacity(3);
    for _ in 0..3 {
        candidates.push(
            fixture
                .service
                .due_candidates(i64::MAX, 1)
                .await?
                .pop()
                .expect("完整 OAuth 凭据必须成为候选"),
        );
    }
    let late_candidate = candidates.pop().unwrap();
    let follower_candidate = candidates.pop().unwrap();
    let leader_candidate = candidates.pop().unwrap();
    let server = ControlledTokenServer::start().await?;
    let coordinator = Arc::new(OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?);

    let leader_coordinator = Arc::clone(&coordinator);
    let leader = tokio::spawn(async move { leader_coordinator.refresh(leader_candidate).await });
    server.wait_for_requests(1).await?;
    let follower_coordinator = Arc::clone(&coordinator);
    let follower =
        tokio::spawn(async move { follower_coordinator.refresh(follower_candidate).await });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(server.request_count(), 1);

    server.release(1);
    assert_eq!(leader.await??, OAuthRefreshCoordinatorOutcome::Stored);
    assert_eq!(follower.await??, OAuthRefreshCoordinatorOutcome::Stored);
    assert_eq!(
        coordinator.refresh(late_candidate).await?,
        OAuthRefreshCoordinatorOutcome::Stored
    );
    assert_eq!(server.request_count(), 1);

    let current = refresh_persistence(&fixture.pool)
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("刷新后的凭据必须保留新的到期投影");
    assert_eq!(current.expected_revision(), 2);
    let (_, current_request) = current.into_parts();
    assert_eq!(current_request.refresh_token(), "rotated-refresh-private");
    assert_eq!(current_request.scope(), Some("openid email"));

    let rendered = format!("{coordinator:?}");
    for private in [
        "codex-client-private",
        "initial-refresh-private",
        "rotated-refresh-private",
        "token.example",
    ] {
        assert!(!rendered.contains(private));
    }
    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn different_credentials_refresh_in_parallel() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(2).await?;
    let candidates = fixture.service.due_candidates(i64::MAX, 2).await?;
    assert_eq!(candidates.len(), 2);
    let server = ControlledTokenServer::start().await?;
    let coordinator = Arc::new(OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?);
    let mut tasks = JoinSet::new();
    for candidate in candidates {
        let coordinator = Arc::clone(&coordinator);
        tasks.spawn(async move { coordinator.refresh(candidate).await });
    }

    // 两个请求都必须在释放任一响应前到达，否则 registry 把不同凭据错误地串行化了。
    server.wait_for_requests(2).await?;
    server.release(2);
    while let Some(result) = tasks.join_next().await {
        assert_eq!(result??, OAuthRefreshCoordinatorOutcome::Stored);
    }

    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_grant_auto_disables_once_and_shares_the_failure() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let leader_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let follower_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let server = ControlledTokenServer::start_with_response(
        400,
        br#"{"error":"invalid_grant","error_description":"private upstream detail"}"#,
    )
    .await?;
    let coordinator = Arc::new(OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?);

    let leader_coordinator = Arc::clone(&coordinator);
    let leader = tokio::spawn(async move { leader_coordinator.refresh(leader_candidate).await });
    server.wait_for_requests(1).await?;
    let follower_coordinator = Arc::clone(&coordinator);
    let follower =
        tokio::spawn(async move { follower_coordinator.refresh(follower_candidate).await });
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(server.request_count(), 1);
    server.release(1);

    let expected = Err(OAuthRefreshCoordinatorError::TokenExchange(
        OAuthTokenExchangeError::EndpointRejected {
            status: 400,
            code: Some(OAuthEndpointErrorCode::InvalidGrant),
        },
    ));
    assert_eq!(leader.await?, expected);
    assert_eq!(follower.await?, expected);
    let credential = only_credential(
        &fixture.admin,
        fixture.channel_id,
        fixture.credential_ids[0],
    )
    .await?;
    assert_eq!(credential.status(), Status::AutoDisabled);
    assert!(credential.rate_limited_at().is_none());
    assert!(credential.rate_limit_reset_at().is_none());
    assert!(credential.overload_until().is_none());
    assert!(credential.temp_unschedulable_until().is_none());
    assert!(!format!("{expected:?}").contains("private upstream detail"));

    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn ambiguous_endpoint_rejection_enters_transient_cooldown() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let server = ControlledTokenServer::start_with_response(401, b"not-json-private").await?;
    let coordinator = OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?;

    let refresh = coordinator.refresh(candidate);
    tokio::pin!(refresh);
    tokio::select! {
        result = &mut refresh => panic!("token server 未释放前不应完成：{result:?}"),
        wait = server.wait_for_requests(1) => wait?,
    }
    server.release(1);
    let error = refresh.await.unwrap_err();
    assert_eq!(
        error,
        OAuthRefreshCoordinatorError::TokenExchange(OAuthTokenExchangeError::EndpointRejected {
            status: 401,
            code: None,
        })
    );
    let credential = only_credential(
        &fixture.admin,
        fixture.channel_id,
        fixture.credential_ids[0],
    )
    .await?;
    assert_eq!(credential.status(), Status::Enabled);
    let now_epoch_seconds = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    )?;
    assert!(
        credential
            .temp_unschedulable_until()
            .is_some_and(|until| until > now_epoch_seconds)
    );
    assert!(
        refresh_persistence(&fixture.pool)
            .due_candidates(i64::MAX, 1)
            .await?
            .is_empty()
    );
    assert!(!format!("{error:?}{error}").contains("not-json-private"));

    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn reauthorization_wins_over_an_old_invalid_grant() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let channel_id = fixture.channel_id;
    let credential_id = fixture.credential_ids[0];
    let candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let server =
        ControlledTokenServer::start_with_response(400, br#"{"error":"invalid_grant"}"#).await?;
    let coordinator = OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?;

    let refresh = coordinator.refresh(candidate);
    tokio::pin!(refresh);
    tokio::select! {
        result = &mut refresh => panic!("token server 未释放前不应完成：{result:?}"),
        wait = server.wait_for_requests(1) => wait?,
    }
    let reauthorization = OAuthTokenPersistenceService::new(
        CredentialEncryptor::new(&settings())?,
        OAuthCredentialRepository::new(fixture.pool.clone()),
    );
    assert_eq!(
        reauthorization
            .persist(OAuthTokenSet::for_test(
                UpstreamOAuthProvider::Codex,
                OAuthAuthorizationContext::new(
                    UserId::new(11)?,
                    Some(channel_id),
                    Some(credential_id),
                )?,
                "reauthorized-access-private".to_owned(),
                Some("reauthorized-refresh-private".to_owned()),
                Some(Duration::from_secs(180)),
                Some("openid profile".to_owned()),
            ))
            .await?,
        super::super::OAuthTokenPersistenceOutcome::Stored
    );
    server.release(1);
    assert_eq!(refresh.await?, OAuthRefreshCoordinatorOutcome::Stale);

    let current = only_credential(&fixture.admin, channel_id, credential_id).await?;
    assert_eq!(current.status(), Status::Enabled);
    assert_eq!(current.oauth_revision(), 2);
    assert!(current.temp_unschedulable_until().is_none());
    let current = refresh_persistence(&fixture.pool)
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .expect("重新授权后的新 token 必须保留");
    let (_, request) = current.into_parts();
    assert_eq!(request.refresh_token(), "reauthorized-refresh-private");

    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn cancelled_leader_wakes_real_refresh_follower() -> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let leader_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let follower_candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let server = ControlledTokenServer::start().await?;
    let coordinator = Arc::new(OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?);

    let leader_coordinator = Arc::clone(&coordinator);
    let leader = tokio::spawn(async move { leader_coordinator.refresh(leader_candidate).await });
    server.wait_for_requests(1).await?;
    let follower_coordinator = Arc::clone(&coordinator);
    let follower =
        tokio::spawn(async move { follower_coordinator.refresh(follower_candidate).await });
    tokio::task::yield_now().await;
    leader.abort();
    assert!(leader.await.unwrap_err().is_cancelled());
    assert_eq!(
        tokio::time::timeout(TEST_TIMEOUT, follower).await??,
        Err(OAuthRefreshCoordinatorError::LeaderAborted)
    );

    server.release(4);
    server.stop().await?;
    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn configuration_and_runtime_errors_remain_closed_and_redacted() -> Result<(), Box<dyn Error>>
{
    let fixture = fixture(1).await?;
    let duplicate = OAuthRefreshCoordinator::new(
        [codex_profile(), codex_profile()],
        direct_clients(),
        refresh_persistence(&fixture.pool),
    )
    .unwrap_err();
    assert_eq!(
        duplicate,
        OAuthRefreshCoordinatorError::DuplicateProviderProfile {
            provider: UpstreamOAuthProvider::Codex
        }
    );
    let candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let coordinator = OAuthRefreshCoordinator::new(
        Vec::<OAuthProviderProfile>::new(),
        direct_clients(),
        fixture.service,
    )?;
    let missing_profile = coordinator.refresh(candidate).await.unwrap_err();
    assert_eq!(
        missing_profile,
        OAuthRefreshCoordinatorError::ProviderProfileNotConfigured {
            provider: UpstreamOAuthProvider::Codex
        }
    );

    let rendered = format!(
        "{duplicate:?}{duplicate}{missing_profile:?}{missing_profile}{}{}",
        OAuthRefreshCoordinatorError::HttpClientUnavailable,
        OAuthRefreshCoordinatorError::Persistence(
            OAuthRefreshPersistenceError::RepositoryUnavailable
        )
    );
    for private in [
        "codex-client-private",
        "initial-access-private",
        "initial-refresh-private",
        "token.example",
        "test-key",
    ] {
        assert!(!rendered.contains(private));
    }

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn persistence_failure_is_shared_without_exposing_database_details()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let server = ControlledTokenServer::start().await?;
    let coordinator = OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?;
    fixture.pool.close().await?;

    let refresh = coordinator.refresh(candidate);
    tokio::pin!(refresh);
    tokio::select! {
        result = &mut refresh => panic!("token server 未释放前不应完成：{result:?}"),
        wait = server.wait_for_requests(1) => wait?,
    }
    server.release(1);
    let error = refresh.await.unwrap_err();
    assert_eq!(
        error,
        OAuthRefreshCoordinatorError::Persistence(
            OAuthRefreshPersistenceError::RepositoryUnavailable
        )
    );
    assert!(!format!("{error:?}{error}").contains("rotated-refresh-private"));

    server.stop().await?;
    Ok(())
}

#[tokio::test]
async fn failure_state_persistence_error_retains_only_redacted_classification()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture(1).await?;
    let candidate = fixture
        .service
        .due_candidates(i64::MAX, 1)
        .await?
        .pop()
        .unwrap();
    let server = ControlledTokenServer::start_with_response(
        400,
        br#"{"error":"invalid_grant","error_description":"refresh-private-database-private"}"#,
    )
    .await?;
    let coordinator = OAuthRefreshCoordinator::new(
        [codex_profile()],
        proxied_clients(server.address()),
        fixture.service,
    )?;
    fixture.pool.close().await?;

    let refresh = coordinator.refresh(candidate);
    tokio::pin!(refresh);
    tokio::select! {
        result = &mut refresh => panic!("token server 未释放前不应完成：{result:?}"),
        wait = server.wait_for_requests(1) => wait?,
    }
    server.release(1);
    let error = refresh.await.unwrap_err();
    assert_eq!(
        error,
        OAuthRefreshCoordinatorError::FailureStatePersistence {
            exchange: OAuthTokenExchangeError::EndpointRejected {
                status: 400,
                code: Some(OAuthEndpointErrorCode::InvalidGrant),
            },
            persistence: OAuthRefreshPersistenceError::RepositoryUnavailable,
        }
    );
    let rendered = format!("{error:?}{error}");
    for private in [
        "refresh-private-database-private",
        "initial-refresh-private",
        "token.example",
        "test-key",
    ] {
        assert!(!rendered.contains(private));
    }

    server.stop().await?;
    Ok(())
}

struct Fixture {
    pool: DatabasePool,
    admin: AdminChannelRepository,
    channel_id: ChannelId,
    credential_ids: Vec<CredentialId>,
    service: OAuthRefreshPersistenceService,
}

async fn fixture(credential_count: usize) -> Result<Fixture, Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let admin = AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?;
    let channel_id = create_channel(&admin).await?;
    let mut credential_ids = Vec::with_capacity(credential_count);
    for index in 0..credential_count {
        let credential_id = create_credential(&admin, channel_id, index).await?;
        credential_ids.push(credential_id);
        let persistence = OAuthTokenPersistenceService::new(
            CredentialEncryptor::new(&settings())?,
            OAuthCredentialRepository::new(pool.clone()),
        );
        let outcome = persistence
            .persist(OAuthTokenSet::for_test(
                UpstreamOAuthProvider::Codex,
                OAuthAuthorizationContext::new(
                    UserId::new(11)?,
                    Some(channel_id),
                    Some(credential_id),
                )?,
                format!("initial-access-private-{index}"),
                Some(format!("initial-refresh-private-{index}")),
                Some(Duration::from_secs(60)),
                Some("openid profile".to_owned()),
            ))
            .await?;
        assert_eq!(outcome, super::super::OAuthTokenPersistenceOutcome::Stored);
    }
    Ok(Fixture {
        service: refresh_persistence(&pool),
        pool,
        admin,
        channel_id,
        credential_ids,
    })
}

async fn only_credential(
    admin: &AdminChannelRepository,
    channel_id: ChannelId,
    credential_id: CredentialId,
) -> Result<Box<AdminCredentialRecord>, Box<dyn Error>> {
    let AdminCredentialLookupOutcome::Found(credential) =
        admin.get_credential(channel_id, credential_id).await?
    else {
        panic!("测试凭据必须存在");
    };
    Ok(credential)
}

async fn create_channel(admin: &AdminChannelRepository) -> Result<ChannelId, Box<dyn Error>> {
    Ok(admin
        .create_channel(AdminChannelWriteRecord::new(
            "oauth-refresh-coordinator".to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://api.example.com/v1".to_owned()),
            Some(ChannelTimeout::new(60)?),
            Status::Enabled,
            10,
            20,
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
    admin: &AdminChannelRepository,
    channel_id: ChannelId,
    index: usize,
) -> Result<CredentialId, Box<dyn Error>> {
    let encryptor = CredentialEncryptor::new(&settings())?;
    let secret = PlainCredentialSecret::new(
        CredentialKind::Oauth,
        format!("bootstrap-access-private-{index}"),
    )?;
    let AdminCredentialCreateOutcome::Created(credential) = admin
        .create_credential(
            channel_id,
            oauth_credential_record(index),
            |credential_id| {
                encryptor
                    .encrypt(channel_id, credential_id.get(), &secret)
                    .map_err(|_| ())
            },
        )
        .await?
    else {
        panic!("测试渠道必须存在");
    };
    Ok(credential.credential_id())
}

fn oauth_credential_record(index: usize) -> AdminCredentialWriteRecord {
    AdminCredentialWriteRecord::new(
        CredentialKind::Oauth,
        Status::Enabled,
        Some(2),
        30,
        40,
        Some(5),
        Some(1_250_000),
        Some(875_000),
        true,
        None,
        CredentialQuotaDimension::Global,
        None,
        Some("codex".to_owned()),
        Some(format!("account-key-{index}")),
        Some(format!("project-id-{index}")),
    )
}

fn refresh_persistence(pool: &DatabasePool) -> OAuthRefreshPersistenceService {
    OAuthRefreshPersistenceService::new(
        CredentialEncryptor::new(&settings()).unwrap(),
        CredentialDecryptor::new(&settings()).unwrap(),
        OAuthCredentialRepository::new(pool.clone()),
    )
}

fn codex_profile() -> OAuthProviderProfile {
    let mut profile = OAuthProviderProfile::new(
        UpstreamOAuthProvider::Codex,
        "codex-client-private".to_owned(),
        None,
    )
    .unwrap();
    profile.set_token_endpoint_for_test(Url::parse("http://token.example/oauth/token").unwrap());
    profile
}

fn proxied_clients(address: SocketAddr) -> HttpClientProvider {
    proxied_clients_with_request_timeout(address, HttpTimeouts::default().request())
}

fn proxied_clients_with_request_timeout(
    address: SocketAddr,
    request_timeout: Duration,
) -> HttpClientProvider {
    let defaults = HttpTimeouts::default();
    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{address}")).unwrap(),
        HttpTimeouts::new(defaults.connect(), defaults.read(), request_timeout).unwrap(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    HttpClientProvider::new(config, 8).unwrap()
}

fn direct_clients() -> HttpClientProvider {
    HttpClientProvider::new(
        HttpClientConfig::new(ProxyConfig::direct(), HttpTimeouts::default()),
        8,
    )
    .unwrap()
}

fn settings() -> CredentialEncryptionSettings {
    serde_json::from_value(serde_json::json!({
        "key_id": "test-key",
        "key": URL_SAFE_NO_PAD.encode([0x5a; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .unwrap()
}

struct ControlledTokenServer {
    address: SocketAddr,
    requests: Arc<AtomicUsize>,
    request_notification: Arc<Notify>,
    response_permits: Arc<Semaphore>,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<io::Result<()>>,
}

struct TokenServerResponse {
    status: u16,
    body: Vec<u8>,
}

impl ControlledTokenServer {
    async fn start() -> io::Result<Self> {
        Self::start_with_response(
            200,
            br#"{"access_token":"rotated-access-private","token_type":"Bearer","expires_in":120,"refresh_token":"rotated-refresh-private","scope":"openid email"}"#,
        )
        .await
    }

    async fn start_with_response(status: u16, body: &[u8]) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let requests = Arc::new(AtomicUsize::new(0));
        let request_notification = Arc::new(Notify::new());
        let response_permits = Arc::new(Semaphore::new(0));
        let (shutdown, mut shutdown_receiver) = oneshot::channel();
        let task_requests = Arc::clone(&requests);
        let task_notification = Arc::clone(&request_notification);
        let task_permits = Arc::clone(&response_permits);
        let response = Arc::new(TokenServerResponse {
            status,
            body: body.to_vec(),
        });
        let task = tokio::spawn(async move {
            let mut handlers = JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = &mut shutdown_receiver => break,
                    accepted = listener.accept() => {
                        let (stream, _) = accepted?;
                        let requests = Arc::clone(&task_requests);
                        let notification = Arc::clone(&task_notification);
                        let permits = Arc::clone(&task_permits);
                        let response = Arc::clone(&response);
                        handlers.spawn(async move {
                            handle_token_connection(
                                stream,
                                requests,
                                notification,
                                permits,
                                response,
                            )
                            .await
                        });
                    }
                }
            }
            while let Some(result) = handlers.join_next().await {
                result.map_err(io::Error::other)??;
            }
            Ok(())
        });
        Ok(Self {
            address,
            requests,
            request_notification,
            response_permits,
            shutdown: Some(shutdown),
            task,
        })
    }

    const fn address(&self) -> SocketAddr {
        self.address
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::Acquire)
    }

    async fn wait_for_requests(&self, expected: usize) -> Result<(), tokio::time::error::Elapsed> {
        tokio::time::timeout(TEST_TIMEOUT, async {
            loop {
                let notified = self.request_notification.notified();
                if self.request_count() >= expected {
                    return;
                }
                notified.await;
            }
        })
        .await
    }

    fn release(&self, count: usize) {
        self.response_permits.add_permits(count);
    }

    async fn stop(mut self) -> Result<(), Box<dyn Error>> {
        self.response_permits.add_permits(32);
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        tokio::time::timeout(TEST_TIMEOUT, self.task).await???;
        Ok(())
    }
}

async fn handle_token_connection(
    mut stream: TcpStream,
    requests: Arc<AtomicUsize>,
    notification: Arc<Notify>,
    response_permits: Arc<Semaphore>,
    response: Arc<TokenServerResponse>,
) -> io::Result<()> {
    let request = read_request(&mut stream).await?;
    assert!(request.contains("grant_type=refresh_token"));
    assert!(request.contains("refresh_token=initial-refresh-private-"));
    requests.fetch_add(1, Ordering::AcqRel);
    notification.notify_waiters();
    let permit = response_permits
        .acquire_owned()
        .await
        .map_err(io::Error::other)?;
    permit.forget();
    let headers = format!(
        "HTTP/1.1 {} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.body.len()
    );
    if stream.write_all(headers.as_bytes()).await.is_ok() {
        let _ = stream.write_all(&response.body).await;
    }
    Ok(())
}

async fn read_request(stream: &mut TcpStream) -> io::Result<String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4_096];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "请求在正文完整前结束",
            ));
        }
        bytes.extend_from_slice(&buffer[..count]);
        let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let header_end = header_end + 4;
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
            })
            .unwrap_or(0);
        if bytes.len() >= header_end + content_length {
            return String::from_utf8(bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
        }
    }
}
