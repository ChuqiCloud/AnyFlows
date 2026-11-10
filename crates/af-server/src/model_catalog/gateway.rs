use af_admin::{GatewayModel, GatewayModelAuthentication, ModelCatalogReadError};
use af_db::{MAX_ADMIN_MODEL_PAGE_SIZE, ModelCatalogAudience};

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
        let group_ids = match &authentication {
            GatewayModelAuthentication::User(authentication)
                if authentication.principal().user_route_id().is_some() =>
            {
                let principal = authentication.principal();
                let route = self
                    .user_routes
                    .as_ref()
                    .ok_or(ModelCatalogReadError::Internal)?
                    .lookup_for_token(principal.user_id(), principal.token_id())
                    .await
                    .map_err(|_| ModelCatalogReadError::Internal)?
                    .ok_or(ModelCatalogReadError::Internal)?;
                if Some(route.id) != principal.user_route_id() {
                    return Err(ModelCatalogReadError::Internal);
                }
                let repository = self
                    .user_routes
                    .as_ref()
                    .ok_or(ModelCatalogReadError::Internal)?;
                let mut permitted = Vec::new();
                for member in route.members {
                    if repository
                        .check_group_access(principal.user_id(), member.group_id)
                        .await
                        .map_err(|_| ModelCatalogReadError::Internal)?
                    {
                        permitted.push(member.group_id);
                    }
                }
                permitted
            }
            _ => vec![authentication.group_id()],
        };
        let mut groups = Vec::with_capacity(group_ids.len());
        for group_id in group_ids {
            let policy = self
                .group_model_policy
                .policy_for_group(group_id)
                .await
                .map_err(super::map_group_model_policy_error)?;
            groups.push((group_id, policy));
        }
        let mut cursor = None;
        let mut models = Vec::new();
        // OpenAI 客户端期望完整列表；内部逐页扫描仍保留目录容量上限。
        for _ in 0..MAX_METADATA_SCAN_PAGES {
            let page = self
                .model_metadata
                .list_catalog_metadata(
                    ModelCatalogAudience::Authenticated,
                    cursor.as_deref(),
                    MAX_ADMIN_MODEL_PAGE_SIZE,
                )
                .await
                .map_err(|_| ModelCatalogReadError::Internal)?;
            let (records, next_cursor) = page.into_parts();
            for record in records {
                if !authentication.allows_model(record.model())
                    || prices.get(record.model()).is_none()
                {
                    continue;
                }
                let mut available = false;
                for (group_id, policy) in &groups {
                    if policy.allows(record.model())
                        && !super::available_protocols(&runtime, *group_id, record.model())?
                            .is_empty()
                    {
                        available = true;
                        break;
                    }
                }
                if !available {
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
