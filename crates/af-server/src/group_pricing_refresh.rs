use std::fmt;

use af_admin::{
    GroupPricingRuntimeRefreshError, GroupPricingRuntimeRefreshFuture, GroupPricingRuntimeRefresher,
};
use af_billing::GroupPricingCache;

/// 将管理写入后的失效通知同步发布到当前进程分组计费缓存。
#[derive(Clone)]
pub(crate) struct RuntimeGroupPricingRefresher {
    cache: GroupPricingCache,
}

impl RuntimeGroupPricingRefresher {
    /// 绑定 Bootstrap 已完成首轮加载的分组计费缓存。
    #[must_use]
    pub(crate) const fn new(cache: GroupPricingCache) -> Self {
        Self { cache }
    }
}

impl GroupPricingRuntimeRefresher for RuntimeGroupPricingRefresher {
    fn refresh<'a>(&'a self) -> GroupPricingRuntimeRefreshFuture<'a> {
        Box::pin(async move {
            // 先标记 stale，刷新失败时请求定价会失败关闭，不能继续消费旧倍率。
            self.cache
                .invalidate()
                .map_err(|_| GroupPricingRuntimeRefreshError)?;
            self.cache
                .refresh_if_stale()
                .await
                .map(|_| ())
                .map_err(|_| GroupPricingRuntimeRefreshError)
        })
    }
}

impl fmt::Debug for RuntimeGroupPricingRefresher {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeGroupPricingRefresher(<受控>)")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use af_billing::{
        GroupPricingSource, GroupPricingSourceCatalog, GroupPricingSourceError,
        GroupPricingSourceFuture, GroupPricingSourceRecord, PricingRatio,
    };
    use af_domain::GroupId;

    use super::*;

    struct ControlledGroupPricingSource {
        fail: AtomicBool,
    }

    impl ControlledGroupPricingSource {
        fn new() -> Self {
            Self {
                fail: AtomicBool::new(false),
            }
        }
    }

    impl GroupPricingSource for ControlledGroupPricingSource {
        fn load<'a>(&'a self) -> GroupPricingSourceFuture<'a> {
            Box::pin(async move {
                if self.fail.load(Ordering::SeqCst) {
                    return Err(GroupPricingSourceError::Unavailable);
                }
                Ok(GroupPricingSourceCatalog::new(
                    vec![GroupPricingSourceRecord::new(
                        GroupId::new(1).unwrap(),
                        PricingRatio::ONE,
                        None,
                    )],
                    Vec::new(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn successful_refresh_publishes_the_next_generation() {
        let source = Arc::new(ControlledGroupPricingSource::new());
        let cache = GroupPricingCache::load(source).await.unwrap();
        let refresher = RuntimeGroupPricingRefresher::new(cache.clone());

        refresher.refresh().await.unwrap();

        assert!(!cache.is_stale());
        assert_eq!(cache.snapshot().unwrap().generation(), 2);
    }

    #[tokio::test]
    async fn failed_refresh_keeps_the_cache_stale() {
        let source = Arc::new(ControlledGroupPricingSource::new());
        let cache = GroupPricingCache::load(source.clone()).await.unwrap();
        let refresher = RuntimeGroupPricingRefresher::new(cache.clone());
        source.fail.store(true, Ordering::SeqCst);

        assert_eq!(
            refresher.refresh().await,
            Err(GroupPricingRuntimeRefreshError)
        );
        assert!(cache.is_stale());
    }
}
