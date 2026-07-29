use std::{fmt, sync::Arc};

use af_admin::{
    ModelCatalogBillingMode, ModelCatalogItem, ModelCatalogPage, ModelCatalogPricingScope,
    ModelCatalogQuery, ModelCatalogRatios, ModelCatalogReadError, ModelCatalogReader,
    ModelCatalogTokenPrices, SessionAuthentication,
};
use af_billing::{
    BillingMode, ModelPrice, ModelPriceCache, PricingRatio, PricingRatios, RatioPricingResolver,
    RequestPricingSnapshotSource,
};
use af_scheduler::InMemoryChannelIndex;

type CatalogClock = Arc<dyn Fn() -> Option<u32> + Send + Sync>;

/// 组合模型价格、调度运行时与请求定价快照的模型目录读取服务。
pub(crate) struct RuntimeModelCatalogReader {
    channel_index: InMemoryChannelIndex,
    model_prices: ModelPriceCache,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    clock: CatalogClock,
}

impl RuntimeModelCatalogReader {
    /// 使用生产调度索引、定价缓存和 UTC 时钟构造目录服务。
    pub(crate) fn new(
        channel_index: InMemoryChannelIndex,
        model_prices: ModelPriceCache,
        pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    ) -> Self {
        Self {
            channel_index,
            model_prices,
            pricing_source,
            clock: Arc::new(crate::utc_time::current_utc_day_second),
        }
    }

    fn catalog_item_from_pricing(
        model: &str,
        billing_mode: BillingMode,
        resolver: RatioPricingResolver,
        ratios: PricingRatios,
        model_version: u64,
    ) -> Result<ModelCatalogItem, ModelCatalogReadError> {
        let billing_mode = Self::catalog_billing_mode(billing_mode);
        let prices = resolver
            .effective_token_prices()
            .map_err(|_| ModelCatalogReadError::Internal)?
            .map(|prices| ModelCatalogTokenPrices::from_decimals(prices.into_values()))
            .transpose()?;
        let ratios = ModelCatalogRatios::new(
            ratios.group().micros(),
            ratios.group_model().micros(),
            ratios.applied_peak().micros(),
        )?;
        ModelCatalogItem::new(
            model.to_owned(),
            billing_mode,
            prices,
            ratios,
            model_version,
        )
    }

    const fn catalog_billing_mode(billing_mode: BillingMode) -> ModelCatalogBillingMode {
        match billing_mode {
            BillingMode::PerToken => ModelCatalogBillingMode::PerToken,
            BillingMode::Free => ModelCatalogBillingMode::Free,
        }
    }

    fn group_catalog_item(
        &self,
        model: &str,
        authentication: SessionAuthentication,
        current_second: u32,
    ) -> Result<ModelCatalogItem, ModelCatalogReadError> {
        let group_id = authentication.group_id();
        // 当前调度快照按实际候选分组建键，因此目录与请求使用同一来源和目标分组。
        let pricing = self
            .pricing_source
            .capture(model, group_id, group_id, current_second)
            .map_err(|_| ModelCatalogReadError::Internal)?;
        Self::catalog_item_from_pricing(
            model,
            pricing.billing_mode(),
            pricing.resolver(),
            pricing.ratios(),
            pricing.model_version(),
        )
    }

    fn public_catalog_item(
        model: &str,
        price: ModelPrice,
    ) -> Result<ModelCatalogItem, ModelCatalogReadError> {
        let ratios = PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE);
        Self::catalog_item_from_pricing(
            model,
            price.billing_mode(),
            price.resolver(ratios),
            ratios,
            price.version(),
        )
    }

    fn model_matches_query(model: &str, search: Option<&str>, query: &ModelCatalogQuery) -> bool {
        !query.after().is_some_and(|after| model <= after)
            && !search.is_some_and(|search| !contains_search(model, search))
    }

    fn append_item(
        items: &mut Vec<ModelCatalogItem>,
        item: ModelCatalogItem,
        query: &ModelCatalogQuery,
    ) -> bool {
        if query
            .billing_mode()
            .is_some_and(|mode| mode != item.billing_mode())
        {
            return false;
        }
        if items.len() == query.limit() {
            return true;
        }
        items.push(item);
        false
    }
}

impl ModelCatalogReader for RuntimeModelCatalogReader {
    fn list(
        &self,
        authentication: Option<SessionAuthentication>,
        query: &ModelCatalogQuery,
    ) -> Result<ModelCatalogPage, ModelCatalogReadError> {
        let search = query.search();
        let mut items = Vec::with_capacity(query.limit());
        let mut has_more = false;
        let pricing_scope = if let Some(authentication) = authentication {
            let current_second = (self.clock)().ok_or(ModelCatalogReadError::Internal)?;
            let snapshot = self
                .channel_index
                .snapshot()
                .map_err(|_| ModelCatalogReadError::Internal)?;
            for model in snapshot.model_names(authentication.group_id()) {
                if !Self::model_matches_query(model, search, query) {
                    continue;
                }
                let item = self.group_catalog_item(model, authentication, current_second)?;
                if Self::append_item(&mut items, item, query) {
                    has_more = true;
                    break;
                }
            }
            ModelCatalogPricingScope::Group
        } else {
            if self.model_prices.is_stale() {
                return Err(ModelCatalogReadError::Internal);
            }
            let snapshot = self
                .model_prices
                .snapshot()
                .map_err(|_| ModelCatalogReadError::Internal)?;
            // 捕获快照后再次检查，避免失效通知落在读取窗口内仍返回旧公开价格。
            if self.model_prices.is_stale() {
                return Err(ModelCatalogReadError::Internal);
            }
            for (model, price) in snapshot.entries() {
                if !Self::model_matches_query(model, search, query)
                    || query.billing_mode().is_some_and(|mode| {
                        mode != Self::catalog_billing_mode(price.billing_mode())
                    })
                {
                    continue;
                }
                let item = Self::public_catalog_item(model, price)?;
                if Self::append_item(&mut items, item, query) {
                    has_more = true;
                    break;
                }
            }
            ModelCatalogPricingScope::PublicBase
        };

        let next_cursor = has_more
            .then(|| items.last().map(|item| item.model().to_owned()))
            .flatten();
        Ok(ModelCatalogPage::from_parts(
            pricing_scope,
            items,
            next_cursor,
        ))
    }
}

fn contains_search(model: &str, search: &str) -> bool {
    model
        .as_bytes()
        .windows(search.len())
        .any(|candidate| candidate.eq_ignore_ascii_case(search.as_bytes()))
}

impl fmt::Debug for RuntimeModelCatalogReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeModelCatalogReader(<已脱敏>)")
    }
}

#[cfg(test)]
mod tests {
    use af_billing::{
        ModelPriceSource, ModelPriceSourceFuture, ModelPriceSourceRecord, RequestPricingSnapshot,
        RequestPricingSnapshotError, TokenPrices,
    };
    use af_domain::GroupId;
    use af_scheduler::{ChannelIndexSource, ChannelIndexSourceFuture};
    use rust_decimal::Decimal;

    use super::*;

    #[derive(Clone)]
    struct StaticModelPriceSource {
        records: Vec<ModelPriceSourceRecord>,
    }

    impl ModelPriceSource for StaticModelPriceSource {
        fn load<'a>(&'a self) -> ModelPriceSourceFuture<'a> {
            let records = self.records.clone();
            Box::pin(async move { Ok(records) })
        }
    }

    struct EmptyChannelSource;

    impl ChannelIndexSource for EmptyChannelSource {
        fn load<'a>(&'a self) -> ChannelIndexSourceFuture<'a> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    struct RejectPricingSource;

    impl RequestPricingSnapshotSource for RejectPricingSource {
        fn capture(
            &self,
            _model: &str,
            _source_group_id: GroupId,
            _target_group_id: GroupId,
            _current_second: u32,
        ) -> Result<RequestPricingSnapshot, RequestPricingSnapshotError> {
            Err(RequestPricingSnapshotError::ModelNotFound)
        }
    }

    async fn public_reader() -> (RuntimeModelCatalogReader, ModelPriceCache) {
        let paid_prices = TokenPrices::new(
            Decimal::new(125, 2),
            Decimal::new(10, 0),
            Decimal::new(125, 3),
            Decimal::ZERO,
            Decimal::new(25, 1),
        )
        .unwrap();
        let zero_prices = TokenPrices::new(
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap();
        let model_prices = ModelPriceCache::load(Arc::new(StaticModelPriceSource {
            records: vec![
                ModelPriceSourceRecord::new(
                    "gpt-test".to_owned(),
                    BillingMode::PerToken,
                    paid_prices,
                    2,
                )
                .unwrap(),
                ModelPriceSourceRecord::new(
                    "free-test".to_owned(),
                    BillingMode::Free,
                    zero_prices,
                    1,
                )
                .unwrap(),
            ],
        }))
        .await
        .unwrap();
        let channel_index = InMemoryChannelIndex::load(Arc::new(EmptyChannelSource))
            .await
            .unwrap();
        let reader = RuntimeModelCatalogReader::new(
            channel_index,
            model_prices.clone(),
            Arc::new(RejectPricingSource),
        );
        (reader, model_prices)
    }

    #[tokio::test]
    async fn guest_catalog_uses_sorted_base_prices_without_runtime_groups() {
        let (reader, _) = public_reader().await;
        let page = reader.list(None, &ModelCatalogQuery::default()).unwrap();

        assert_eq!(page.pricing_scope(), ModelCatalogPricingScope::PublicBase);
        assert_eq!(
            page.items()
                .iter()
                .map(ModelCatalogItem::model)
                .collect::<Vec<_>>(),
            vec!["free-test", "gpt-test"]
        );
        let paid = &page.items()[1];
        assert_eq!(paid.prices().unwrap().input(), "1.25");
        assert_eq!(paid.ratios().group_micros(), 1_000_000);
        assert_eq!(paid.ratios().group_model_micros(), 1_000_000);
        assert_eq!(paid.ratios().peak_micros(), 1_000_000);

        let filtered = reader
            .list(
                None,
                &ModelCatalogQuery::new(
                    Some("GPT".to_owned()),
                    Some(ModelCatalogBillingMode::PerToken),
                    None,
                    24,
                )
                .unwrap(),
            )
            .unwrap();
        assert_eq!(filtered.items().len(), 1);
        assert_eq!(filtered.items()[0].model(), "gpt-test");
    }

    #[tokio::test]
    async fn guest_catalog_rejects_a_stale_price_snapshot() {
        let (reader, model_prices) = public_reader().await;
        model_prices.invalidate().unwrap();

        assert!(matches!(
            reader.list(None, &ModelCatalogQuery::default()),
            Err(ModelCatalogReadError::Internal)
        ));
    }
}
