use std::{
    env, process,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_cache::{
    CacheError, CacheMode, DEFAULT_REDIS_CONNECT_TIMEOUT, DistributedLease, DistributedLeaseConfig,
    DistributedLeaseManager, DistributedLeaseMode, HybridCache, HybridCacheConfig,
    LeaseAcquireOutcome, LeaseReleaseOutcome, RedisBroadcastConfig, RedisBroadcastPublisher,
    RedisBroadcastSubscriber, RedisConfig,
};

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
