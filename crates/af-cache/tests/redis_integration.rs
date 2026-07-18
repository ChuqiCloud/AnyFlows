use std::{
    env, process,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_cache::{
    CacheError, CacheMode, DEFAULT_REDIS_CONNECT_TIMEOUT, HybridCache, HybridCacheConfig,
    RedisConfig,
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
}

fn unique_namespace() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间必须晚于 Unix 纪元")
        .as_nanos();
    format!("ci.v1.{}.{}", process::id(), timestamp)
}
