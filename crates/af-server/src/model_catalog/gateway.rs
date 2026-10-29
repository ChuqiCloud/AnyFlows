use af_admin::{GatewayModel, GatewayModelAuthentication, ModelCatalogReadError};
use af_db::MAX_ADMIN_MODEL_PAGE_SIZE;

use super::{MAX_METADATA_SCAN_PAGES, RuntimeModelCatalogReader};

impl RuntimeModelCatalogReader {
    pub(super) async fn list_gateway_models(
        &self,
        authentication: GatewayModelAuthentication,
    ) -> Result<Vec<GatewayModel>, ModelCatalogReadError> {
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
        let group_id = authentication.group_id();
        let group_model_policy = self
            .group_model_policy
            .policy_for_group(group_id)
            .await
            .map_err(super::map_group_model_policy_error)?;
        let mut cursor = None;
        let mut models = Vec::new();
        // OpenAI 客户端期望完整列表；内部逐页扫描仍保留目录容量上限。
        for _ in 0..MAX_METADATA_SCAN_PAGES {
            let page = self
                .model_metadata
                .list_catalog_metadata(true, cursor.as_deref(), MAX_ADMIN_MODEL_PAGE_SIZE)
                .await
                .map_err(|_| ModelCatalogReadError::Internal)?;
            let (records, next_cursor) = page.into_parts();
            for record in records {
                if !authentication.allows_model(record.model())
                    || !group_model_policy.allows(record.model())
                    || prices.get(record.model()).is_none()
                    || runtime
                        .candidates(group_id, record.model())
                        .map_err(|_| ModelCatalogReadError::Internal)?
                        .is_empty()
                {
                    continue;
                }
                models.push(GatewayModel::new(
                    record.model().to_owned(),
                    record.created_at(),
                    record.provider().to_owned(),
                )?);
            }
            if next_cursor.is_none() {
                return if self.model_prices.is_stale() {
                    Err(ModelCatalogReadError::Internal)
                } else {
                    Ok(models)
                };
            }
            if next_cursor.as_deref() <= cursor.as_deref() {
                return Err(ModelCatalogReadError::Internal);
            }
            cursor = next_cursor;
        }
        Err(ModelCatalogReadError::Internal)
    }
}
