use std::collections::BTreeSet;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait, JoinType,
    QueryFilter, QuerySelect, RelationTrait, TransactionTrait, sea_query::Expr,
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminModelLifecycleRecord, AdminModelRecord, AdminModelVisibilityRecord,
    admin_model_write::valid_write_fields,
    entity::{channel_models, channels, models},
};

use super::{
    MAX_MODEL_SYNC_APPLY_ITEMS, MissingModelImportItemRecord, ModelSyncRepository,
    ModelSyncRepositoryError, record_internal_error, write::insert_model_draft,
};

impl ModelSyncRepository {
    /// 将缺失清单中的明确选择原子导入为隐藏草稿，不改动渠道映射和价格。
    pub async fn import_missing_models(
        &self,
        items: Vec<MissingModelImportItemRecord>,
    ) -> Result<Vec<AdminModelRecord>, ModelSyncRepositoryError> {
        if items.is_empty()
            || items.len() > MAX_MODEL_SYNC_APPLY_ITEMS
            || items.iter().any(|item| {
                !valid_write_fields(&item.fields)
                    || item.fields.visibility != AdminModelVisibilityRecord::Hidden
                    || item.fields.lifecycle != AdminModelLifecycleRecord::Draft
            })
            || items
                .iter()
                .map(|item| item.model.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                != items.len()
        {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        match timeout(
            self.operation_timeout,
            self.import_missing_models_inner(items),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ModelSyncRepositoryError::Timeout)),
        }
    }

    async fn import_missing_models_inner(
        &self,
        items: Vec<MissingModelImportItemRecord>,
    ) -> Result<Vec<AdminModelRecord>, ModelSyncRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        let selected = items
            .iter()
            .map(|item| item.model.clone())
            .collect::<Vec<_>>();
        lock_sqlite_references(&transaction, &selected).await?;
        ensure_no_active_metadata(&transaction, &selected).await?;
        ensure_all_models_still_referenced(&transaction, &selected).await?;
        let mut created = Vec::with_capacity(items.len());
        for item in items {
            let (_, snapshot) = insert_model_draft(&transaction, item.model, item.fields).await?;
            created.push(snapshot);
        }
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        Ok(created)
    }
}

async fn lock_sqlite_references(
    transaction: &DatabaseTransaction,
    selected: &[String],
) -> Result<(), ModelSyncRepositoryError> {
    if transaction.get_database_backend() != DbBackend::Sqlite {
        return Ok(());
    }
    // SQLite 没有行锁，先对候选关联执行等值写入，确保后续核验和插入处于同一写锁内。
    channel_models::Entity::update_many()
        .filter(channel_models::Column::Model.is_in(selected.to_vec()))
        .col_expr(
            channel_models::Column::Model,
            Expr::col(channel_models::Column::Model).into(),
        )
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?;
    Ok(())
}

async fn ensure_no_active_metadata(
    transaction: &DatabaseTransaction,
    selected: &[String],
) -> Result<(), ModelSyncRepositoryError> {
    let mut query = models::Entity::find()
        .filter(models::Column::Model.is_in(selected.to_vec()))
        .filter(models::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(sea_orm::sea_query::LockType::Update);
    }
    let conflict = query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?;
    if conflict.is_some() {
        return Err(ModelSyncRepositoryError::Conflict);
    }
    Ok(())
}

async fn ensure_all_models_still_referenced(
    transaction: &DatabaseTransaction,
    selected: &[String],
) -> Result<(), ModelSyncRepositoryError> {
    let mut query = channel_models::Entity::find()
        .join(JoinType::InnerJoin, channel_models::Relation::Channel.def())
        .filter(channel_models::Column::Model.is_in(selected.to_vec()))
        .filter(channels::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(sea_orm::sea_query::LockType::Update);
    }
    let referenced = query
        .all(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?;
    let referenced = referenced
        .into_iter()
        .map(|reference| reference.model)
        .collect::<BTreeSet<_>>();
    let selected = selected.iter().cloned().collect::<BTreeSet<_>>();
    if referenced != selected {
        return Err(ModelSyncRepositoryError::Conflict);
    }
    Ok(())
}
