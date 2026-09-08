use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminModelLifecycleRecord, AdminModelRecord, AdminModelRepository, AdminModelRepositoryError,
    AdminModelVisibilityRecord, MAX_ADMIN_MODEL_PAGE_SIZE,
    admin_model::{record_internal_error, valid_canonical_model},
    entity::models,
};

/// 一页按 Canonical 标识稳定排序的公开目录元数据候选。
pub struct ModelCatalogMetadataPageRecord {
    models: Vec<AdminModelRecord>,
    next_cursor: Option<String>,
}

impl ModelCatalogMetadataPageRecord {
    /// 消费页面并返回已校验元数据与下一 Canonical 游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<AdminModelRecord>, Option<String>) {
        (self.models, self.next_cursor)
    }
}

impl std::fmt::Debug for ModelCatalogMetadataPageRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModelCatalogMetadataPageRecord")
            .field("model_count", &self.models.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

impl AdminModelRepository {
    /// 按访问主体筛选活动或弃用元数据，供运行时目录继续组合价格与可用性。
    pub async fn list_catalog_metadata(
        &self,
        authenticated: bool,
        after: Option<&str>,
        limit: usize,
    ) -> Result<ModelCatalogMetadataPageRecord, AdminModelRepositoryError> {
        if !(1..=MAX_ADMIN_MODEL_PAGE_SIZE).contains(&limit)
            || after.is_some_and(|value| !valid_canonical_model(value))
        {
            return Err(record_internal_error(AdminModelRepositoryError::Invariant));
        }
        let operation = async {
            let visible = if authenticated {
                vec![
                    AdminModelVisibilityRecord::Public.database_value(),
                    AdminModelVisibilityRecord::Authenticated.database_value(),
                ]
            } else {
                vec![AdminModelVisibilityRecord::Public.database_value()]
            };
            let mut query = models::Entity::find()
                .filter(models::Column::DeletedAt.is_null())
                .filter(models::Column::Visibility.is_in(visible))
                .filter(models::Column::Lifecycle.is_in([
                    AdminModelLifecycleRecord::Active.database_value(),
                    AdminModelLifecycleRecord::Deprecated.database_value(),
                ]))
                .order_by_asc(models::Column::Model)
                .limit(
                    u64::try_from(limit)
                        .ok()
                        .and_then(|value| value.checked_add(1))
                        .ok_or(AdminModelRepositoryError::Invariant)?,
                );
            if let Some(after) = after {
                query = query.filter(models::Column::Model.gt(after));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(|_| AdminModelRepositoryError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let mut rows = match timeout(self.lookup_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error)?,
            Err(_) => {
                return Err(record_internal_error(AdminModelRepositoryError::Timeout));
            }
        };
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let models = rows
            .into_iter()
            .map(AdminModelRecord::try_from_model)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| models.last().map(|model| model.model().to_owned()))
            .flatten();
        Ok(ModelCatalogMetadataPageRecord {
            models,
            next_cursor,
        })
    }
}
