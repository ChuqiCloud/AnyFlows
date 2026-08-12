use af_domain::ModelId;
use sea_orm::{ActiveModelTrait, DatabaseTransaction};
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminModelRecord, AdminModelWriteRecord,
    admin_model_write::{active_model_from_write, map_write_db_error},
};

use super::ModelSyncRepositoryError;

/// 在既有事务中创建一个模型草稿，并统一数据库冲突分类。
pub(super) async fn insert_model_draft(
    transaction: &DatabaseTransaction,
    model: String,
    fields: AdminModelWriteRecord,
) -> Result<(ModelId, AdminModelRecord), ModelSyncRepositoryError> {
    let inserted = active_model_from_write(model, fields)
        .map_err(|_| ModelSyncRepositoryError::Invariant)?
        .insert(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| match map_write_db_error(error) {
            crate::AdminModelRepositoryError::Conflict => ModelSyncRepositoryError::Conflict,
            crate::AdminModelRepositoryError::Query
            | crate::AdminModelRepositoryError::Timeout
            | crate::AdminModelRepositoryError::Invariant => ModelSyncRepositoryError::Query,
        })?;
    let model_id = ModelId::new(inserted.id).map_err(|_| ModelSyncRepositoryError::Invariant)?;
    let snapshot = AdminModelRecord::try_from_model(inserted)
        .map_err(|_| ModelSyncRepositoryError::Invariant)?;
    Ok((model_id, snapshot))
}
