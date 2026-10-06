use std::collections::BTreeMap;

use af_admin::{
    ModelCatalogPricingScope, ModelCatalogProvider, ModelCatalogProviderSummary,
    ModelCatalogReadError, SessionAuthentication,
};
use af_db::MAX_ADMIN_MODEL_PAGE_SIZE;

use super::{
    MAX_METADATA_SCAN_PAGES, RuntimeModelCatalogReader, available_protocols, metadata_from_record,
};

impl RuntimeModelCatalogReader {
    /// 扫描游客真实可见且可定价的完整目录，避免供应商选项被模型分页截断。
    pub(super) async fn list_public_providers(
        &self,
    ) -> Result<ModelCatalogProviderSummary, ModelCatalogReadError> {
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
        let mut counts = BTreeMap::new();
        let mut cursor = None;
        let mut scan_complete = false;
        for _ in 0..MAX_METADATA_SCAN_PAGES {
            let page = self
                .model_metadata
                .list_catalog_metadata(false, cursor.as_deref(), MAX_ADMIN_MODEL_PAGE_SIZE)
                .await
                .map_err(|_| ModelCatalogReadError::Internal)?;
            let (records, next_cursor) = page.into_parts();
            for record in records {
                let Some(price) = prices.get(record.model()) else {
                    continue;
                };
                let supports_responses_compact = runtime
                    .responses_compact_available_any_group(record.model())
                    .map_err(|_| ModelCatalogReadError::Internal)?;
                let item = Self::public_catalog_item(
                    metadata_from_record(record)?,
                    price,
                    supports_responses_compact,
                )?;
                increment_provider(&mut counts, item.metadata().provider())?;
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
        if !scan_complete || self.model_prices.is_stale() {
            return Err(ModelCatalogReadError::Internal);
        }
        provider_summary(ModelCatalogPricingScope::PublicBase, counts)
    }

    /// 扫描登录用户默认分组真实可用且可定价的完整目录。
    pub(super) async fn list_group_providers(
        &self,
        authentication: SessionAuthentication,
    ) -> Result<ModelCatalogProviderSummary, ModelCatalogReadError> {
        let current_second = (self.clock)().ok_or(ModelCatalogReadError::Internal)?;
        let runtime = self
            .channel_index
            .snapshot()
            .map_err(|_| ModelCatalogReadError::Internal)?;
        let mut counts = BTreeMap::new();
        let mut cursor = None;
        let mut scan_complete = false;
        for _ in 0..MAX_METADATA_SCAN_PAGES {
            let page = self
                .model_metadata
                .list_catalog_metadata(true, cursor.as_deref(), MAX_ADMIN_MODEL_PAGE_SIZE)
                .await
                .map_err(|_| ModelCatalogReadError::Internal)?;
            let (records, next_cursor) = page.into_parts();
            for record in records {
                let protocols =
                    available_protocols(&runtime, authentication.group_id(), record.model())?;
                if protocols.is_empty() {
                    continue;
                }
                let metadata = metadata_from_record(record)?;
                let supports_responses_compact = runtime
                    .responses_compact_available(authentication.group_id(), metadata.model())
                    .map_err(|_| ModelCatalogReadError::Internal)?;
                let Some(item) = self.group_catalog_item(
                    metadata,
                    authentication,
                    protocols,
                    supports_responses_compact,
                    current_second,
                )?
                else {
                    continue;
                };
                increment_provider(&mut counts, item.metadata().provider())?;
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
        if !scan_complete {
            return Err(ModelCatalogReadError::Internal);
        }
        provider_summary(ModelCatalogPricingScope::Group, counts)
    }
}

fn increment_provider(
    counts: &mut BTreeMap<String, u64>,
    provider: &str,
) -> Result<(), ModelCatalogReadError> {
    let count = counts.entry(provider.to_owned()).or_default();
    *count = count
        .checked_add(1)
        .ok_or(ModelCatalogReadError::Internal)?;
    Ok(())
}

fn provider_summary(
    pricing_scope: ModelCatalogPricingScope,
    counts: BTreeMap<String, u64>,
) -> Result<ModelCatalogProviderSummary, ModelCatalogReadError> {
    let providers = counts
        .into_iter()
        .map(|(name, count)| ModelCatalogProvider::new(name, count))
        .collect::<Result<Vec<_>, _>>()?;
    ModelCatalogProviderSummary::new(pricing_scope, providers)
}
