use std::fmt;

use af_domain::GroupId;
use thiserror::Error;

use crate::{
    BillingMode, GroupPricingCache, GroupPricingCacheError, GroupPricingLookupError,
    ModelPriceCache, ModelPriceCacheError, PricingRatios, RatioPricingResolver,
};

/// 从进程内不可变目录同步固定一次请求的完整定价事实。
pub trait RequestPricingSnapshotSource: Send + Sync + 'static {
    /// 固定模型价格、来源/目标分组倍率及当前高峰状态；本方法不执行 IO。
    fn capture(
        &self,
        model: &str,
        source_group_id: GroupId,
        target_group_id: GroupId,
        current_second: u32,
    ) -> Result<RequestPricingSnapshot, RequestPricingSnapshotError>;
}

/// 一次请求从预扣到最终结算始终复用的不可变定价快照。
#[must_use = "请求定价快照必须用于构造同一生命周期的解析器"]
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RequestPricingSnapshot {
    model_price: crate::ModelPrice,
    ratios: PricingRatios,
    model_generation: u64,
    group_generation: u64,
}

impl RequestPricingSnapshot {
    /// 返回快照模型的显式计费模式。
    #[must_use]
    pub const fn billing_mode(self) -> BillingMode {
        self.model_price.billing_mode()
    }

    /// 返回模型价格的持久化正版本。
    #[must_use]
    pub const fn model_version(self) -> u64 {
        self.model_price.version()
    }

    /// 返回捕获时模型价格目录的本地代数。
    #[must_use]
    pub const fn model_generation(self) -> u64 {
        self.model_generation
    }

    /// 返回捕获时分组倍率目录的本地代数。
    #[must_use]
    pub const fn group_generation(self) -> u64 {
        self.group_generation
    }

    /// 返回请求固定的三层倍率，供审计与回归验证使用。
    #[must_use]
    pub const fn ratios(self) -> PricingRatios {
        self.ratios
    }

    /// 构造同时用于预估和实际 usage 的纯计算解析器。
    #[must_use]
    pub const fn resolver(self) -> RatioPricingResolver {
        self.model_price.resolver(self.ratios)
    }
}

impl fmt::Debug for RequestPricingSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestPricingSnapshot")
            .field("billing_mode", &self.billing_mode())
            .field("model_version", &self.model_version())
            .field("model_generation", &self.model_generation)
            .field("group_generation", &self.group_generation)
            .finish_non_exhaustive()
    }
}

/// 请求级定价快照捕获错误；不回显模型名、分组或倍率。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RequestPricingSnapshotError {
    /// 模型价格缓存状态不可用。
    #[error(transparent)]
    ModelCache(#[from] ModelPriceCacheError),
    /// 分组倍率缓存状态不可用。
    #[error(transparent)]
    GroupCache(#[from] GroupPricingCacheError),
    /// 已收到模型价格失效通知，刷新成功前不得继续按旧价格创建请求。
    #[error("模型价格目录等待刷新")]
    ModelPricingStale,
    /// 已收到分组倍率失效通知，刷新成功前不得继续按旧倍率创建请求。
    #[error("分组倍率目录等待刷新")]
    GroupPricingStale,
    /// 请求模型没有显式定价；禁止静默当作免费。
    #[error("请求模型没有可用定价")]
    ModelNotFound,
    /// 请求来源分组没有可用计费配置。
    #[error("请求来源分组没有可用计费配置")]
    SourceGroupNotFound,
    /// 请求目标分组没有可用计费配置。
    #[error("请求目标分组没有可用计费配置")]
    TargetGroupNotFound,
    /// 当前计费时间无效。
    #[error("请求计费时间无效")]
    InvalidCurrentTime,
}

/// 组合模型价格与分组倍率缓存的同步请求快照来源。
#[derive(Clone)]
pub struct CachedRequestPricingSnapshotSource {
    model_prices: ModelPriceCache,
    group_pricing: GroupPricingCache,
}

impl CachedRequestPricingSnapshotSource {
    /// 使用两个已经完成首轮加载的缓存构造请求级来源。
    #[must_use]
    pub const fn new(model_prices: ModelPriceCache, group_pricing: GroupPricingCache) -> Self {
        Self {
            model_prices,
            group_pricing,
        }
    }

    /// 返回模型价格缓存，供监听器标记失效与触发刷新。
    #[must_use]
    pub const fn model_prices(&self) -> &ModelPriceCache {
        &self.model_prices
    }

    /// 返回分组倍率缓存，供监听器标记失效与触发刷新。
    #[must_use]
    pub const fn group_pricing(&self) -> &GroupPricingCache {
        &self.group_pricing
    }
}

impl RequestPricingSnapshotSource for CachedRequestPricingSnapshotSource {
    fn capture(
        &self,
        model: &str,
        source_group_id: GroupId,
        target_group_id: GroupId,
        current_second: u32,
    ) -> Result<RequestPricingSnapshot, RequestPricingSnapshotError> {
        ensure_fresh(&self.model_prices, &self.group_pricing)?;
        let model_snapshot = self.model_prices.snapshot()?;
        let group_snapshot = self.group_pricing.snapshot()?;
        // 捕获两个 Arc 后再次检查，避免在读取窗口内收到失效通知仍创建新请求。
        ensure_fresh(&self.model_prices, &self.group_pricing)?;
        let model_price = model_snapshot
            .get(model)
            .ok_or(RequestPricingSnapshotError::ModelNotFound)?;
        let ratios = group_snapshot
            .ratios_for_request(source_group_id, target_group_id, current_second)
            .map_err(map_lookup_error)?;
        Ok(RequestPricingSnapshot {
            model_price,
            ratios,
            model_generation: model_snapshot.generation(),
            group_generation: group_snapshot.generation(),
        })
    }
}

impl fmt::Debug for CachedRequestPricingSnapshotSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CachedRequestPricingSnapshotSource")
            .field("model_stale", &self.model_prices.is_stale())
            .field("group_stale", &self.group_pricing.is_stale())
            .finish_non_exhaustive()
    }
}

fn ensure_fresh(
    model_prices: &ModelPriceCache,
    group_pricing: &GroupPricingCache,
) -> Result<(), RequestPricingSnapshotError> {
    if model_prices.is_stale() {
        return Err(RequestPricingSnapshotError::ModelPricingStale);
    }
    if group_pricing.is_stale() {
        return Err(RequestPricingSnapshotError::GroupPricingStale);
    }
    Ok(())
}

const fn map_lookup_error(error: GroupPricingLookupError) -> RequestPricingSnapshotError {
    match error {
        GroupPricingLookupError::SourceGroupNotFound => {
            RequestPricingSnapshotError::SourceGroupNotFound
        }
        GroupPricingLookupError::TargetGroupNotFound => {
            RequestPricingSnapshotError::TargetGroupNotFound
        }
        GroupPricingLookupError::InvalidCurrentTime => {
            RequestPricingSnapshotError::InvalidCurrentTime
        }
    }
}
