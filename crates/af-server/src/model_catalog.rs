use std::{collections::BTreeSet, fmt, sync::Arc};

use af_admin::{
    AdminModelModalities, ModelCatalogBillingMode, ModelCatalogCapability, ModelCatalogItem,
    ModelCatalogLifecycle, ModelCatalogListFuture, ModelCatalogMetadata, ModelCatalogModality,
    ModelCatalogPage, ModelCatalogPricingScope, ModelCatalogQuery, ModelCatalogRatios,
    ModelCatalogReadError, ModelCatalogReader, ModelCatalogRuntimeStatus, ModelCatalogTokenPrices,
    SessionAuthentication,
};
use af_billing::{
    BillingMode, ModelPrice, ModelPriceCache, PricingRatio, PricingRatios, RatioPricingResolver,
    RequestPricingSnapshotError, RequestPricingSnapshotSource,
};
use af_db::{
    AdminModelLifecycleRecord, AdminModelModalitiesRecord, AdminModelRecord, AdminModelRepository,
    MAX_ADMIN_MODEL_PAGE_SIZE,
};
use af_domain::{GroupId, Protocol};
use af_scheduler::{ChannelIndexSnapshot, InMemoryChannelIndex};

mod gateway;
mod providers;

type CatalogClock = Arc<dyn Fn() -> Option<u32> + Send + Sync>;
const MAX_METADATA_SCAN_PAGES: usize = 100;

/// 组合模型价格、调度运行时与请求定价快照的模型目录读取服务。
pub(crate) struct RuntimeModelCatalogReader {
    channel_index: InMemoryChannelIndex,
    model_prices: ModelPriceCache,
    pricing_source: Arc<dyn RequestPricingSnapshotSource>,
    model_metadata: AdminModelRepository,
    clock: CatalogClock,
}

impl RuntimeModelCatalogReader {
    /// 使用生产调度索引、定价缓存和 UTC 时钟构造目录服务。
    pub(crate) fn new(
        channel_index: InMemoryChannelIndex,
        model_prices: ModelPriceCache,
        pricing_source: Arc<dyn RequestPricingSnapshotSource>,
        model_metadata: AdminModelRepository,
    ) -> Self {
        Self {
            channel_index,
            model_prices,
            pricing_source,
            model_metadata,
            clock: Arc::new(crate::utc_time::current_utc_day_second),
        }
    }

    fn catalog_item_from_pricing(
        metadata: ModelCatalogMetadata,
        billing_mode: BillingMode,
        resolver: RatioPricingResolver,
        ratios: PricingRatios,
        model_version: u64,
        runtime_status: ModelCatalogRuntimeStatus,
        available_protocols: Vec<Protocol>,
        supports_responses_compact: bool,
    ) -> Result<ModelCatalogItem, ModelCatalogReadError> {
        let billing_mode = Self::catalog_billing_mode(billing_mode)?;
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
            metadata,
            billing_mode,
            prices,
            ratios,
            model_version,
            runtime_status,
            available_protocols,
            supports_responses_compact,
        )
    }

    fn catalog_billing_mode(
        billing_mode: BillingMode,
    ) -> Result<ModelCatalogBillingMode, ModelCatalogReadError> {
        match billing_mode {
            BillingMode::PerToken => Ok(ModelCatalogBillingMode::PerToken),
            BillingMode::Free => Ok(ModelCatalogBillingMode::Free),
            _ => Err(ModelCatalogReadError::Internal),
        }
    }

    fn group_catalog_item(
        &self,
        metadata: ModelCatalogMetadata,
        authentication: SessionAuthentication,
        available_protocols: Vec<Protocol>,
        supports_responses_compact: bool,
        current_second: u32,
    ) -> Result<Option<ModelCatalogItem>, ModelCatalogReadError> {
        let group_id = authentication.group_id();
        // 当前调度快照按实际候选分组建键，因此目录与请求使用同一来源和目标分组。
        let pricing =
            match self
                .pricing_source
                .capture(metadata.model(), group_id, group_id, current_second)
            {
                Ok(pricing) => pricing,
                Err(RequestPricingSnapshotError::ModelNotFound) => return Ok(None),
                Err(_) => return Err(ModelCatalogReadError::Internal),
            };
        Self::catalog_item_from_pricing(
            metadata,
            pricing.billing_mode(),
            pricing
                .ratio_resolver()
                .ok_or(ModelCatalogReadError::Internal)?,
            pricing.ratios(),
            pricing.model_version(),
            ModelCatalogRuntimeStatus::Available,
            available_protocols,
            supports_responses_compact,
        )
        .map(Some)
    }

    fn public_catalog_item(
        metadata: ModelCatalogMetadata,
        price: ModelPrice,
        supports_responses_compact: bool,
    ) -> Result<ModelCatalogItem, ModelCatalogReadError> {
        let ratios = PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE);
        // 表达式生产快照尚未接通时，公开目录也不得把它渲染成零价固定模式。
        let billing_mode = price
            .billing_mode()
            .ok_or(ModelCatalogReadError::Internal)?;
        let resolver = price
            .resolver(ratios)
            .ok_or(ModelCatalogReadError::Internal)?;
        Self::catalog_item_from_pricing(
            metadata,
            billing_mode,
            resolver,
            ratios,
            price.version(),
            ModelCatalogRuntimeStatus::NotEvaluated,
            Vec::new(),
            supports_responses_compact,
        )
    }

    fn metadata_matches_search(record: &AdminModelRecord, search: Option<&str>) -> bool {
        search.is_none_or(|search| {
            [record.model(), record.display_name(), record.provider()]
                .into_iter()
                .any(|value| contains_search(value, search))
        })
    }

    fn metadata_matches_filters(record: &AdminModelRecord, query: &ModelCatalogQuery) -> bool {
        (query.providers().is_empty()
            || query
                .providers()
                .iter()
                .any(|provider| provider == record.provider()))
            && modalities_match(record.input_modalities(), query.input_modalities())
            && modalities_match(record.output_modalities(), query.output_modalities())
            && query
                .capabilities()
                .iter()
                .all(|capability| match capability {
                    ModelCatalogCapability::Reasoning => record.supports_reasoning(),
                    ModelCatalogCapability::ToolCalls => record.supports_tool_calls(),
                    // Compact 属于运行时事实，需在拿到调度快照后继续判断。
                    ModelCatalogCapability::ResponsesCompact => true,
                })
    }

    fn runtime_matches_filters(
        item: &ModelCatalogItem,
        query: &ModelCatalogQuery,
        protocols_are_known: bool,
    ) -> bool {
        let compact_matches = !query
            .capabilities()
            .contains(&ModelCatalogCapability::ResponsesCompact)
            || item.supports_responses_compact();
        let protocols_match = query.protocols().is_empty()
            || (protocols_are_known
                && query
                    .protocols()
                    .iter()
                    .any(|protocol| item.available_protocols().contains(protocol)));
        compact_matches && protocols_match
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

    async fn list_public(
        &self,
        query: &ModelCatalogQuery,
    ) -> Result<ModelCatalogPage, ModelCatalogReadError> {
        if self.model_prices.is_stale() {
            return Err(ModelCatalogReadError::Internal);
        }
        let prices = self
            .model_prices
            .snapshot()
            .map_err(|_| ModelCatalogReadError::Internal)?;
        let runtime = self
            .channel_index
            .snapshot()
            .map_err(|_| ModelCatalogReadError::Internal)?;
        let mut cursor = query.after().map(str::to_owned);
        let mut items = Vec::with_capacity(query.limit());
        let mut has_more = false;
        let mut scan_complete = false;
        for _ in 0..MAX_METADATA_SCAN_PAGES {
            let page = self
                .model_metadata
                .list_catalog_metadata(false, cursor.as_deref(), MAX_ADMIN_MODEL_PAGE_SIZE)
                .await
                .map_err(|_| ModelCatalogReadError::Internal)?;
            let (records, next_cursor) = page.into_parts();
            for record in records {
                if !Self::metadata_matches_search(&record, query.search())
                    || !Self::metadata_matches_filters(&record, query)
                {
                    continue;
                }
                let Some(price) = prices.get(record.model()) else {
                    continue;
                };
                let supports_responses_compact = runtime
                    .responses_compact_available_any_group(record.model())
                    .map_err(|_| ModelCatalogReadError::Internal)?;
                let metadata = metadata_from_record(record)?;
                let item = Self::public_catalog_item(metadata, price, supports_responses_compact)?;
                if !Self::runtime_matches_filters(&item, query, false) {
                    continue;
                }
                if Self::append_item(&mut items, item, query) {
                    has_more = true;
                    break;
                }
            }
            if has_more {
                break;
            }
            if next_cursor.is_none() {
                scan_complete = true;
                break;
            }
            if next_cursor.as_deref() <= cursor.as_deref() {
                return Err(ModelCatalogReadError::Internal);
            }
            cursor = next_cursor;
        }
        if !has_more && !scan_complete {
            return Err(ModelCatalogReadError::Internal);
        }
        // 捕获快照后再次检查，避免失效通知落在读取窗口内仍返回旧公开价格。
        if self.model_prices.is_stale() {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(catalog_page(
            ModelCatalogPricingScope::PublicBase,
            items,
            has_more,
        ))
    }

    async fn list_group(
        &self,
        authentication: SessionAuthentication,
        query: &ModelCatalogQuery,
    ) -> Result<ModelCatalogPage, ModelCatalogReadError> {
        let current_second = (self.clock)().ok_or(ModelCatalogReadError::Internal)?;
        let runtime = self
            .channel_index
            .snapshot()
            .map_err(|_| ModelCatalogReadError::Internal)?;
        let mut cursor = query.after().map(str::to_owned);
        let mut items = Vec::with_capacity(query.limit());
        let mut has_more = false;
        let mut scan_complete = false;
        for _ in 0..MAX_METADATA_SCAN_PAGES {
            let page = self
                .model_metadata
                .list_catalog_metadata(true, cursor.as_deref(), MAX_ADMIN_MODEL_PAGE_SIZE)
                .await
                .map_err(|_| ModelCatalogReadError::Internal)?;
            let (records, next_cursor) = page.into_parts();
            for record in records {
                if !Self::metadata_matches_search(&record, query.search())
                    || !Self::metadata_matches_filters(&record, query)
                {
                    continue;
                }
                let available_protocols =
                    available_protocols(&runtime, authentication.group_id(), record.model())?;
                if available_protocols.is_empty() {
                    continue;
                }
                let metadata = metadata_from_record(record)?;
                let supports_responses_compact = runtime
                    .responses_compact_available(authentication.group_id(), metadata.model())
                    .map_err(|_| ModelCatalogReadError::Internal)?;
                let Some(item) = self.group_catalog_item(
                    metadata,
                    authentication,
                    available_protocols,
                    supports_responses_compact,
                    current_second,
                )?
                else {
                    continue;
                };
                if !Self::runtime_matches_filters(&item, query, true) {
                    continue;
                }
                if Self::append_item(&mut items, item, query) {
                    has_more = true;
                    break;
                }
            }
            if has_more {
                break;
            }
            if next_cursor.is_none() {
                scan_complete = true;
                break;
            }
            if next_cursor.as_deref() <= cursor.as_deref() {
                return Err(ModelCatalogReadError::Internal);
            }
            cursor = next_cursor;
        }
        if !has_more && !scan_complete {
            return Err(ModelCatalogReadError::Internal);
        }
        Ok(catalog_page(
            ModelCatalogPricingScope::Group,
            items,
            has_more,
        ))
    }
}

impl ModelCatalogReader for RuntimeModelCatalogReader {
    fn list_for_token(
        &self,
        authentication: af_admin::TokenAuthentication,
    ) -> af_admin::GatewayModelListFuture<'_> {
        Box::pin(async move { self.list_gateway_models(authentication).await })
    }

    fn list<'a>(
        &'a self,
        authentication: Option<SessionAuthentication>,
        query: &'a ModelCatalogQuery,
    ) -> ModelCatalogListFuture<'a> {
        Box::pin(async move {
            match authentication {
                Some(authentication) => self.list_group(authentication, query).await,
                None => self.list_public(query).await,
            }
        })
    }

    fn providers(
        &self,
        authentication: Option<SessionAuthentication>,
    ) -> af_admin::ModelCatalogProvidersFuture<'_> {
        Box::pin(async move {
            match authentication {
                Some(authentication) => self.list_group_providers(authentication).await,
                None => self.list_public_providers().await,
            }
        })
    }
}

fn metadata_from_record(
    record: AdminModelRecord,
) -> Result<ModelCatalogMetadata, ModelCatalogReadError> {
    let lifecycle = match record.lifecycle() {
        AdminModelLifecycleRecord::Active => ModelCatalogLifecycle::Active,
        AdminModelLifecycleRecord::Deprecated => ModelCatalogLifecycle::Deprecated,
        AdminModelLifecycleRecord::Draft | AdminModelLifecycleRecord::Retired => {
            return Err(ModelCatalogReadError::Internal);
        }
    };
    ModelCatalogMetadata::new(
        record.model().to_owned(),
        record.display_name().to_owned(),
        record.provider().to_owned(),
        record.description().map(str::to_owned),
        record.icon_url().map(str::to_owned),
        record.tags().to_vec(),
        record.context_window(),
        modalities(record.input_modalities()),
        modalities(record.output_modalities()),
        record.supports_reasoning(),
        record.supports_tool_calls(),
        lifecycle,
    )
}

const fn modalities(value: AdminModelModalitiesRecord) -> AdminModelModalities {
    AdminModelModalities::new(value.text(), value.image(), value.audio(), value.video())
}

fn modalities_match(value: AdminModelModalitiesRecord, filters: &[ModelCatalogModality]) -> bool {
    filters.is_empty()
        || filters.iter().any(|filter| match filter {
            ModelCatalogModality::Text => value.text(),
            ModelCatalogModality::Image => value.image(),
            ModelCatalogModality::Audio => value.audio(),
            ModelCatalogModality::Video => value.video(),
        })
}

fn catalog_page(
    pricing_scope: ModelCatalogPricingScope,
    items: Vec<ModelCatalogItem>,
    has_more: bool,
) -> ModelCatalogPage {
    let next_cursor = has_more
        .then(|| items.last().map(|item| item.model().to_owned()))
        .flatten();
    ModelCatalogPage::from_parts(pricing_scope, items, next_cursor)
}

fn available_protocols(
    runtime: &ChannelIndexSnapshot,
    group_id: GroupId,
    model: &str,
) -> Result<Vec<Protocol>, ModelCatalogReadError> {
    // 同一个 runtime 快照里可能有多个渠道候选，目录只暴露去重后的协议能力集合。
    let protocols = runtime
        .candidates(group_id, model)
        .map_err(|_| ModelCatalogReadError::Internal)?
        .iter()
        .map(|candidate| {
            runtime
                .runtime_target(candidate.channel_id())
                .map(|target| target.protocol())
                .ok_or(ModelCatalogReadError::Internal)
        })
        .collect::<Result<BTreeSet<_>, _>>()?
        .into_iter()
        .collect();
    Ok(protocols)
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
    use af_db::{
        AdminModelCreateRecord, AdminModelLifecycleRecord, AdminModelModalitiesRecord,
        AdminModelVisibilityRecord, AdminModelWriteRecord, DatabaseOptions, MigrationOptions,
    };
    use af_scheduler::{ChannelIndexSource, ChannelIndexSourceFuture};
    use rust_decimal::Decimal;
    use std::time::Duration;

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
        let pool = af_db::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:").unwrap(),
            MigrationOptions::default(),
        )
        .await
        .unwrap();
        let model_metadata = AdminModelRepository::new(pool, Duration::from_secs(2)).unwrap();
        for (model, display_name, provider) in [
            ("free-test", "Community Free", "community"),
            ("gpt-test", "Flagship Model", "OpenAI"),
            ("no-price", "No Price", "provider"),
        ] {
            model_metadata
                .create(catalog_model(model, display_name, provider))
                .await
                .unwrap();
        }
        let channel_index = InMemoryChannelIndex::load(Arc::new(EmptyChannelSource))
            .await
            .unwrap();
        let reader = RuntimeModelCatalogReader::new(
            channel_index,
            model_prices.clone(),
            Arc::new(RejectPricingSource),
            model_metadata,
        );
        (reader, model_prices)
    }

    #[tokio::test]
    async fn guest_catalog_uses_sorted_base_prices_without_runtime_groups() {
        let (reader, _) = public_reader().await;
        let page = reader
            .list(None, &ModelCatalogQuery::default())
            .await
            .unwrap();

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
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    None,
                    24,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(filtered.items().len(), 1);
        assert_eq!(filtered.items()[0].model(), "gpt-test");

        let provider_filtered = reader
            .list(
                None,
                &ModelCatalogQuery::new(
                    None,
                    None,
                    vec!["community".to_owned()],
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    None,
                    24,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(provider_filtered.items().len(), 1);
        assert_eq!(provider_filtered.items()[0].model(), "free-test");

        let providers = reader.providers(None).await.unwrap();
        assert_eq!(
            providers.pricing_scope(),
            ModelCatalogPricingScope::PublicBase
        );
        assert_eq!(
            providers
                .providers()
                .iter()
                .map(|provider| (provider.name(), provider.model_count()))
                .collect::<Vec<_>>(),
            vec![("OpenAI", 1), ("community", 1)]
        );

        let capable = reader
            .list(
                None,
                &ModelCatalogQuery::new(
                    None,
                    None,
                    Vec::new(),
                    vec![ModelCatalogModality::Text],
                    vec![ModelCatalogModality::Text],
                    vec![ModelCatalogCapability::ToolCalls],
                    Vec::new(),
                    None,
                    24,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(capable.items().len(), 2);

        let unavailable_protocol = reader
            .list(
                None,
                &ModelCatalogQuery::new(
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    vec![Protocol::OpenAiChat],
                    None,
                    24,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(unavailable_protocol.items().is_empty());
    }

    #[test]
    fn runtime_filters_require_authoritative_protocol_and_compact_facts() {
        let item = ModelCatalogItem::new(
            ModelCatalogMetadata::new(
                "gpt-test".to_owned(),
                "Flagship Model".to_owned(),
                "OpenAI".to_owned(),
                None,
                None,
                vec!["chat".to_owned()],
                Some(128_000),
                AdminModelModalities::new(true, false, false, false),
                AdminModelModalities::new(true, false, false, false),
                false,
                true,
                ModelCatalogLifecycle::Active,
            )
            .unwrap(),
            ModelCatalogBillingMode::Free,
            None,
            ModelCatalogRatios::new(1_000_000, 1_000_000, 1_000_000).unwrap(),
            1,
            ModelCatalogRuntimeStatus::Available,
            vec![Protocol::OpenAiResponses],
            true,
        )
        .unwrap();
        let matching = ModelCatalogQuery::new(
            None,
            None,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            vec![ModelCatalogCapability::ResponsesCompact],
            vec![Protocol::OpenAiResponses],
            None,
            24,
        )
        .unwrap();
        assert!(RuntimeModelCatalogReader::runtime_matches_filters(
            &item, &matching, true
        ));
        assert!(!RuntimeModelCatalogReader::runtime_matches_filters(
            &item, &matching, false
        ));
    }

    #[tokio::test]
    async fn guest_catalog_rejects_a_stale_price_snapshot() {
        let (reader, model_prices) = public_reader().await;
        model_prices.invalidate().unwrap();

        assert!(matches!(
            reader.list(None, &ModelCatalogQuery::default()).await,
            Err(ModelCatalogReadError::Internal)
        ));
    }

    fn catalog_model(model: &str, display_name: &str, provider: &str) -> AdminModelCreateRecord {
        AdminModelCreateRecord::new(
            model.to_owned(),
            AdminModelWriteRecord::new(
                display_name.to_owned(),
                provider.to_owned(),
                None,
                None,
                vec!["chat".to_owned()],
                Some(128_000),
                AdminModelModalitiesRecord::new(true, false, false, false),
                AdminModelModalitiesRecord::new(true, false, false, false),
                false,
                true,
                AdminModelVisibilityRecord::Public,
                AdminModelLifecycleRecord::Active,
            ),
        )
    }
}
