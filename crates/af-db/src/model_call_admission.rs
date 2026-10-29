use af_domain::MAX_MODEL_NAME_BYTES;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminModelLifecycleRecord, AdminModelRepository, AdminModelRepositoryError,
    admin_model::{record_internal_error, valid_canonical_model},
    entity::models,
};

impl AdminModelRepository {
    /// Checks metadata lifecycle for a model when route planning has no candidate.
    pub async fn is_model_in_call_catalog(
        &self,
        model: &str,
    ) -> Result<bool, AdminModelRepositoryError> {
        if !valid_canonical_model(model) || model.len() > MAX_MODEL_NAME_BYTES {
            return Ok(false);
        }
        let operation = models::Entity::find()
            .select_only()
            .column(models::Column::Model)
            .filter(models::Column::Model.eq(model))
            .filter(models::Column::DeletedAt.is_null())
            .filter(models::Column::Lifecycle.is_in([
                AdminModelLifecycleRecord::Active.database_value(),
                AdminModelLifecycleRecord::Deprecated.database_value(),
            ]))
            .limit(2)
            .into_tuple::<String>()
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default());
        let mut rows = match timeout(self.lookup_timeout, operation).await {
            Ok(Ok(rows)) => rows,
            Ok(Err(_)) => return Err(record_internal_error(AdminModelRepositoryError::Query)),
            Err(_) => return Err(record_internal_error(AdminModelRepositoryError::Timeout)),
        };
        if rows.len() > 1 {
            return Err(record_internal_error(AdminModelRepositoryError::Invariant));
        }
        Ok(rows.pop().is_some_and(|row| row == model))
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, time::Duration};

    use super::*;
    use crate::{
        AdminModelCreateRecord, AdminModelModalitiesRecord, AdminModelVisibilityRecord,
        AdminModelWriteRecord, DatabaseOptions, MigrationOptions, connect_and_migrate,
    };

    #[tokio::test]
    async fn call_catalog_admits_only_active_or_deprecated_exact_live_models()
    -> Result<(), Box<dyn Error>> {
        let pool = connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let repository = AdminModelRepository::new(pool.clone(), Duration::from_secs(2))?;
        for (model, lifecycle) in [
            ("active-model", AdminModelLifecycleRecord::Active),
            ("deprecated-model", AdminModelLifecycleRecord::Deprecated),
            ("draft-model", AdminModelLifecycleRecord::Draft),
            ("retired-model", AdminModelLifecycleRecord::Retired),
            ("deleted-model", AdminModelLifecycleRecord::Active),
        ] {
            let record = AdminModelCreateRecord::new(
                model.to_owned(),
                AdminModelWriteRecord::new(
                    model.to_owned(),
                    "provider".to_owned(),
                    None,
                    None,
                    Vec::new(),
                    None,
                    AdminModelModalitiesRecord::new(true, false, false, false),
                    AdminModelModalitiesRecord::new(true, false, false, false),
                    false,
                    false,
                    AdminModelVisibilityRecord::Public,
                    lifecycle,
                ),
            );
            let created = repository.create(record).await?;
            if model == "deleted-model" {
                repository.delete(created.model_id()).await?;
            }
        }

        for (model, expected) in [
            ("active-model", true),
            ("deprecated-model", true),
            ("draft-model", false),
            ("retired-model", false),
            ("deleted-model", false),
            ("unknown-model", false),
            ("ACTIVE-MODEL", false),
        ] {
            assert_eq!(repository.is_model_in_call_catalog(model).await?, expected);
        }
        pool.close().await?;
        Ok(())
    }
}
