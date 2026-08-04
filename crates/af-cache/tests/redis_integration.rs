use std::{
    env, process,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_cache::{
    CacheError, CacheMode, ConcurrencyAcquireOutcome, ConcurrencyLeaseOutcome,
    DEFAULT_REDIS_CONNECT_TIMEOUT, DistributedLease, DistributedLeaseConfig,
    DistributedLeaseManager, DistributedLeaseMode, HybridCache, HybridCacheConfig,
    LeaseAcquireOutcome, LeaseReleaseOutcome, ProjectionWriteOutcome, RedisBroadcastConfig,
    RedisBroadcastPublisher, RedisBroadcastSubscriber, RedisConcurrencyConfig,
    RedisConcurrencyStore, RedisConfig, RedisHealthConfig, RedisHealthEvent, RedisHealthFailure,
    RedisHealthStore, RedisHealthTarget, RedisProjectionConfig, RedisStickySessionConfig,
    RedisStickySessionStore, RedisVersionedProjectionStore,
};
use af_domain::{ConcurrencyLimit, CredentialId, TokenId, UserId};

const LIVE_REDIS_GATE: &str = "AF_REQUIRE_LIVE_REDIS";
const LIVE_REDIS_URL: &str = "AF_TEST_REDIS_URL";

/// 验证真实 Redis 下的 L1/L2、TTL、删除和跨实例边界。
#[tokio::test]
#[ignore = "需要显式配置隔离的 Redis 测试实例"]
async fn redis_hybrid_cache_smoke() {
    assert_eq!(
        env::var(LIVE_REDIS_GATE).as_deref(),
        Ok("1"),
        "必须显式设置 {LIVE_REDIS_GATE}=1"
    );
    let redis_url = env::var(LIVE_REDIS_URL).expect("必须设置 AF_TEST_REDIS_URL");
    let namespace = unique_namespace();
    let config = || {
        HybridCacheConfig::new(namespace.clone(), 8)
            .unwrap()
            .with_local_ttl_cap(Duration::from_secs(1))
            .with_redis(
                RedisConfig::new(redis_url.clone())
                    .with_timeouts(DEFAULT_REDIS_CONNECT_TIMEOUT, Duration::from_secs(2)),
            )
    };

    let first = HybridCache::new(config()).await.unwrap();
    let second = HybridCache::new(config()).await.unwrap();
    assert_eq!(first.mode(), CacheMode::Hybrid);
    first.health_check().await.unwrap();

    first
        .set("shared", b"value", Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(
        second.get("shared").await.unwrap().unwrap().as_ref(),
        b"value"
    );
    assert!(second.delete("shared").await.unwrap());
    assert_eq!(
        first.get("shared").await.unwrap().unwrap().as_ref(),
        b"value"
    );

    let observer = HybridCache::new(config()).await.unwrap();
    assert!(observer.get("shared").await.unwrap().is_none());
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    assert!(first.get("shared").await.unwrap().is_none());

    first
        .set("expiring", b"short", Duration::from_millis(80))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(160)).await;
    let after_expiry = HybridCache::new(config()).await.unwrap();
    assert!(after_expiry.get("expiring").await.unwrap().is_none());

    first
        .set("oversized", b"four", Duration::from_secs(2))
        .await
        .unwrap();
    let constrained = HybridCache::new(config().with_max_value_bytes(3).unwrap())
        .await
        .unwrap();
    assert_eq!(
        constrained.get("oversized").await.unwrap_err(),
        CacheError::ValueTooLarge
    );
    assert!(first.delete("oversized").await.unwrap());

    verify_distributed_lease_contract(&redis_url, &format!("{namespace}.lease")).await;
    verify_broadcast_contract(&redis_url, &format!("{namespace}.broadcast")).await;
    verify_projection_contract(&redis_url, &format!("{namespace}.projection")).await;
    verify_sticky_session_contract(&redis_url, &format!("{namespace}.sticky")).await;
    verify_concurrency_contract(&redis_url, &format!("{namespace}.concurrency")).await;
    verify_health_contract(&redis_url, &format!("{namespace}.health")).await;
}

async fn verify_health_contract(redis_url: &str, namespace: &str) {
    let config = || {
        RedisHealthConfig::new(
            RedisConfig::new(redis_url)
                .with_timeouts(DEFAULT_REDIS_CONNECT_TIMEOUT, Duration::from_secs(2)),
            namespace,
        )
        .unwrap()
        .with_policy(
            Duration::from_millis(100),
            Duration::from_millis(500),
            3,
            [
                Duration::from_millis(120),
                Duration::from_millis(250),
                Duration::from_millis(450),
            ],
            Duration::from_secs(3),
        )
        .unwrap()
    };
    let first = RedisHealthStore::connect(config()).await.unwrap();
    let second = RedisHealthStore::connect(config()).await.unwrap();
    let channel = RedisHealthTarget::Channel(af_domain::ChannelId::new(81).unwrap());
    let credential = RedisHealthTarget::Credential(CredentialId::new(82).unwrap());
    let targets = [channel, credential];

    let initial = first.states(&targets).await.unwrap();
    assert!(initial.iter().all(|state| state.penalty_micros() == 0));
    for _ in 0..3 {
        first
            .apply(&[
                RedisHealthEvent::failed(channel, RedisHealthFailure::Server),
                RedisHealthEvent::failed(credential, RedisHealthFailure::Server),
            ])
            .await
            .unwrap();
    }
    let opened = second.states(&targets).await.unwrap();
    assert!(opened.iter().all(|state| state.is_cooling()));
    assert!(opened.iter().all(|state| state.breaker_level() == 1));

    tokio::time::sleep(Duration::from_millis(160)).await;
    let expired = second.states(&targets).await.unwrap();
    assert!(expired.iter().all(|state| !state.is_cooling()));
    assert!(
        expired
            .iter()
            .all(|state| state.penalty_micros() < 7_500_000)
    );
    for _ in 0..3 {
        second
            .apply(&[
                RedisHealthEvent::failed(channel, RedisHealthFailure::Network),
                RedisHealthEvent::failed(credential, RedisHealthFailure::Network),
            ])
            .await
            .unwrap();
    }
    let escalated = first.states(&targets).await.unwrap();
    assert!(escalated.iter().all(|state| state.is_cooling()));
    assert!(escalated.iter().all(|state| state.breaker_level() == 2));

    first
        .apply(&[
            RedisHealthEvent::succeeded(channel),
            RedisHealthEvent::succeeded(credential),
        ])
        .await
        .unwrap();
    let recovered = second.states(&targets).await.unwrap();
    assert!(recovered.iter().all(|state| !state.is_cooling()));
    assert!(recovered.iter().all(|state| state.breaker_level() == 0));

    let decaying = RedisHealthTarget::Credential(CredentialId::new(83).unwrap());
    first
        .apply(&[RedisHealthEvent::failed(
            decaying,
            RedisHealthFailure::ModelUnsupported,
        )])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let decayed = second.states(&[decaying]).await.unwrap()[0].penalty_micros();
    assert!((350_000..=550_000).contains(&decayed));
}

async fn verify_sticky_session_contract(redis_url: &str, namespace: &str) {
    let digest = "a".repeat(64);
    let config = || {
        RedisStickySessionConfig::new(RedisConfig::new(redis_url), namespace)
            .unwrap()
            .with_ttl(Duration::from_millis(500))
            .unwrap()
    };
    let first = RedisStickySessionStore::connect(config()).await.unwrap();
    let second = RedisStickySessionStore::connect(config()).await.unwrap();

    assert_eq!(first.get_and_refresh(&digest).await.unwrap(), None);
    first.bind(&digest, 41).await.unwrap();
    assert_eq!(second.get_and_refresh(&digest).await.unwrap(), Some(41));
    assert!(!second.delete_if_channel(&digest, 42).await.unwrap());
    assert_eq!(first.get_and_refresh(&digest).await.unwrap(), Some(41));

    // 先改绑再执行迟到清理时，条件删除不能误删新渠道。
    first.bind(&digest, 43).await.unwrap();
    assert!(!second.delete_if_channel(&digest, 41).await.unwrap());
    assert!(second.refresh_if_channel(&digest, 43).await.unwrap());
    assert!(first.delete_if_channel(&digest, 43).await.unwrap());
    assert_eq!(second.get_and_refresh(&digest).await.unwrap(), None);
}

async fn verify_concurrency_contract(redis_url: &str, namespace: &str) {
    let config = || {
        RedisConcurrencyConfig::new(
            RedisConfig::new(redis_url)
                .with_timeouts(DEFAULT_REDIS_CONNECT_TIMEOUT, Duration::from_secs(2)),
            namespace,
        )
        .unwrap()
        .with_ttls(Duration::from_millis(220), Duration::from_millis(180))
        .unwrap()
    };
    let first = RedisConcurrencyStore::connect(config()).await.unwrap();
    let second = RedisConcurrencyStore::connect(config()).await.unwrap();
    let account = CredentialId::new(71).unwrap();
    let account_limit = ConcurrencyLimit::new(1).unwrap();

    // 同一账号的竞争必须由 Lua 一次判定，第二个实例不能越过上限。
    let held = expect_concurrency_acquired(
        first
            .acquire_account(account, Some(account_limit))
            .await
            .unwrap(),
    );
    assert!(matches!(
        second
            .acquire_account(account, Some(account_limit))
            .await
            .unwrap(),
        ConcurrencyAcquireOutcome::Limited
    ));
    let loads = second
        .account_loads(&[(account, Some(account_limit))])
        .await
        .unwrap();
    assert_eq!(loads[0].active(), 1);

    // 活跃租约和等待登记都能在短 TTL 内续期，续期不会重新创建迟到成员。
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        held.renew().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );
    let waiting = match second
        .enter_account_wait(account, ConcurrencyLimit::new(2).unwrap())
        .await
        .unwrap()
    {
        af_cache::ConcurrencyWaitOutcome::Entered(waiting) => waiting,
        af_cache::ConcurrencyWaitOutcome::Full => panic!("等待队列不应在首个成员时饱和"),
    };
    tokio::time::sleep(Duration::from_millis(90)).await;
    assert_eq!(
        waiting.renew().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );
    assert_eq!(
        waiting.leave().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );

    assert_eq!(
        held.release().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );
    let replacement = expect_concurrency_acquired(
        second
            .acquire_account(account, Some(account_limit))
            .await
            .unwrap(),
    );
    assert_eq!(
        replacement.release().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );

    // 用户槽位与令牌追踪在同一脚本内竞争，令牌层本身不设置独立上限。
    let user = UserId::new(81).unwrap();
    let token_a = TokenId::new(91).unwrap();
    let token_b = TokenId::new(92).unwrap();
    let user_held = expect_concurrency_acquired(
        first
            .acquire_user_token(user, Some(account_limit), token_a)
            .await
            .unwrap(),
    );
    assert!(matches!(
        second
            .acquire_user_token(user, Some(account_limit), token_b)
            .await
            .unwrap(),
        ConcurrencyAcquireOutcome::Limited
    ));
    assert_eq!(
        user_held.release().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );

    // 批量负载同时返回账号活跃数和等待数，且不接受重复标识。
    let batch_a = CredentialId::new(72).unwrap();
    let batch_b = CredentialId::new(73).unwrap();
    let batch_held =
        expect_concurrency_acquired(first.acquire_account(batch_a, None).await.unwrap());
    let batch_loads = second
        .account_loads(&[(batch_a, None), (batch_b, Some(account_limit))])
        .await
        .unwrap();
    assert_eq!(batch_loads.len(), 2);
    assert_eq!(batch_loads[0].active(), 1);
    assert_eq!(batch_loads[1].active(), 0);
    assert_eq!(
        batch_held.release().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );

    // 过期索引清理后，迟到释放只能得到 Lost，不能删除新接管者。
    let stale_account = CredentialId::new(74).unwrap();
    let stale = expect_concurrency_acquired(
        first
            .acquire_account(stale_account, Some(account_limit))
            .await
            .unwrap(),
    );
    tokio::time::sleep(Duration::from_millis(260)).await;
    let report = second
        .cleanup_expired(af_cache::MAX_CONCURRENCY_CLEANUP_BATCH_SIZE)
        .await
        .unwrap();
    assert!(report.inspected_keys() > 0 || report.removed_members() > 0);
    let current = expect_concurrency_acquired(
        second
            .acquire_account(stale_account, Some(account_limit))
            .await
            .unwrap(),
    );
    assert_eq!(
        stale.release().await.unwrap(),
        ConcurrencyLeaseOutcome::Lost
    );
    assert!(matches!(
        first
            .acquire_account(stale_account, Some(account_limit))
            .await
            .unwrap(),
        ConcurrencyAcquireOutcome::Limited
    ));
    assert_eq!(
        current.release().await.unwrap(),
        ConcurrencyLeaseOutcome::Applied
    );
}

fn expect_concurrency_acquired(
    outcome: ConcurrencyAcquireOutcome,
) -> af_cache::RedisConcurrencyLease {
    let ConcurrencyAcquireOutcome::Acquired(lease) = outcome else {
        panic!("预期取得并发槽位")
    };
    lease
}

async fn verify_projection_contract(redis_url: &str, namespace: &str) {
    let config = || {
        RedisProjectionConfig::new(
            RedisConfig::new(redis_url)
                .with_timeouts(DEFAULT_REDIS_CONNECT_TIMEOUT, Duration::from_secs(2)),
            namespace,
        )
        .unwrap()
        .with_max_payload_bytes(128)
        .unwrap()
    };
    let first = RedisVersionedProjectionStore::connect(config())
        .await
        .unwrap();
    let second = RedisVersionedProjectionStore::connect(config())
        .await
        .unwrap();
    let old_version = (1_u64 << 53) + 1;
    let new_version = old_version + 1;

    assert_eq!(
        first
            .put_if_newer("channel:7", new_version, b"new-projection")
            .await
            .unwrap(),
        ProjectionWriteOutcome::Applied
    );
    assert_eq!(
        second
            .put_if_newer("channel:7", old_version, b"stale-projection")
            .await
            .unwrap(),
        ProjectionWriteOutcome::Stale
    );
    let entry = second.get("channel:7").await.unwrap().unwrap();
    assert_eq!(entry.version(), new_version);
    assert_eq!(entry.payload(), b"new-projection");
    assert!(!format!("{entry:?}").contains("new-projection"));
}

async fn verify_broadcast_contract(redis_url: &str, channel: &str) {
    let config = RedisBroadcastConfig::new(
        RedisConfig::new(redis_url)
            .with_timeouts(DEFAULT_REDIS_CONNECT_TIMEOUT, Duration::from_secs(2)),
        channel,
    )
    .unwrap();
    // 先等待订阅确认，再发布消息，验证启动顺序不会主动制造丢信号窗口。
    let mut subscriber = RedisBroadcastSubscriber::connect(config.clone())
        .await
        .unwrap();
    let publisher = RedisBroadcastPublisher::connect(config).await.unwrap();
    assert!(publisher.publish(b"scheduler-event-v1").await.unwrap() >= 1);
    let received = tokio::time::timeout(Duration::from_secs(2), subscriber.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, b"scheduler-event-v1");
}

async fn verify_distributed_lease_contract(redis_url: &str, namespace: &str) {
    let config = || {
        DistributedLeaseConfig::new(namespace).unwrap().with_redis(
            RedisConfig::new(redis_url)
                .with_timeouts(DEFAULT_REDIS_CONNECT_TIMEOUT, Duration::from_secs(2)),
        )
    };
    let first = DistributedLeaseManager::new(config()).await.unwrap();
    let second = DistributedLeaseManager::new(config()).await.unwrap();
    assert_eq!(first.mode(), DistributedLeaseMode::Redis);

    // 两个独立连接同时竞争同一事实，只能产生一个 owner。
    let (first_result, second_result) = tokio::join!(
        first.acquire(
            "oauth:channel:7:credential:9:revision:1",
            Duration::from_secs(2)
        ),
        second.acquire(
            "oauth:channel:7:credential:9:revision:1",
            Duration::from_secs(2)
        ),
    );
    let (winner, held_count) = collect_competition(first_result.unwrap(), second_result.unwrap());
    assert_eq!(held_count, 1);
    let rendered = format!("{winner:?}");
    assert!(!rendered.contains(namespace));
    assert!(!rendered.contains("oauth:channel"));
    assert!(rendered.contains("已脱敏"));
    assert_eq!(
        winner.release().await.unwrap(),
        LeaseReleaseOutcome::Released
    );

    let released_key = expect_acquired(
        second
            .acquire(
                "oauth:channel:7:credential:9:revision:1",
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
    );
    assert_eq!(
        released_key.release().await.unwrap(),
        LeaseReleaseOutcome::Released
    );

    // A 过期后由 B 接管，A 的迟到释放不得删除 B 的新租约。
    let stale = expect_acquired(
        first
            .acquire(
                "oauth:channel:8:credential:10:revision:3",
                Duration::from_millis(80),
            )
            .await
            .unwrap(),
    );
    tokio::time::sleep(Duration::from_millis(160)).await;
    let current = expect_acquired(
        second
            .acquire(
                "oauth:channel:8:credential:10:revision:3",
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
    );
    assert_eq!(stale.release().await.unwrap(), LeaseReleaseOutcome::Lost);
    assert!(matches!(
        first
            .acquire(
                "oauth:channel:8:credential:10:revision:3",
                Duration::from_secs(2)
            )
            .await
            .unwrap(),
        LeaseAcquireOutcome::Held
    ));
    assert_eq!(
        current.release().await.unwrap(),
        LeaseReleaseOutcome::Released
    );

    // 渠道、凭据和 OAuth 版本都进入键空间，彼此不同的刷新事实互不阻塞。
    let base = expect_acquired(
        first
            .acquire(
                "oauth:channel:11:credential:12:revision:4",
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
    );
    let different_channel = expect_acquired(
        second
            .acquire(
                "oauth:channel:12:credential:12:revision:4",
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
    );
    let different_credential = expect_acquired(
        first
            .acquire(
                "oauth:channel:11:credential:13:revision:4",
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
    );
    let new_revision = expect_acquired(
        second
            .acquire(
                "oauth:channel:11:credential:12:revision:5",
                Duration::from_secs(2),
            )
            .await
            .unwrap(),
    );
    assert_eq!(base.release().await.unwrap(), LeaseReleaseOutcome::Released);
    assert_eq!(
        different_channel.release().await.unwrap(),
        LeaseReleaseOutcome::Released
    );
    assert_eq!(
        different_credential.release().await.unwrap(),
        LeaseReleaseOutcome::Released
    );
    assert_eq!(
        new_revision.release().await.unwrap(),
        LeaseReleaseOutcome::Released
    );
}

fn collect_competition(
    first: LeaseAcquireOutcome,
    second: LeaseAcquireOutcome,
) -> (DistributedLease, usize) {
    let mut winner = None;
    let mut held_count = 0;
    for outcome in [first, second] {
        match outcome {
            LeaseAcquireOutcome::Acquired(lease) => {
                assert!(winner.replace(lease).is_none(), "同一租约出现多个 owner");
            }
            LeaseAcquireOutcome::Held => held_count += 1,
        }
    }
    (winner.expect("竞争必须产生一个 owner"), held_count)
}

fn expect_acquired(outcome: LeaseAcquireOutcome) -> DistributedLease {
    let LeaseAcquireOutcome::Acquired(lease) = outcome else {
        panic!("预期取得租约")
    };
    lease
}

fn unique_namespace() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间必须晚于 Unix 纪元")
        .as_nanos();
    format!("ci.v1.{}.{}", process::id(), timestamp)
}
