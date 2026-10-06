use std::collections::{BTreeMap, BTreeSet};

use af_domain::{ChannelId, UserId};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminModelLifecycleRecord, AdminModelRecord, AdminModelVisibilityRecord,
    ability_write::append_channel_models,
    admin_model_write::valid_write_fields,
    entity::{model_sync_items, model_sync_runs, models},
};

use super::{
    MAX_MODEL_SYNC_APPLY_ITEMS, ModelSyncApplyItemRecord, ModelSyncRelationRecord,
    ModelSyncRepository, ModelSyncRepositoryError, record_internal_error, valid_preview_id,
    write::insert_model_draft,
};

impl ModelSyncRepository {
    /// 原子应用固定预览中的明确选择，并只创建隐藏草稿元数据。
    pub async fn apply_preview(
        &self,
        preview_id: &str,
        actor_user_id: UserId,
        now: i64,
        items: Vec<ModelSyncApplyItemRecord>,
    ) -> Result<Vec<AdminModelRecord>, ModelSyncRepositoryError> {
        if !valid_preview_id(preview_id)
            || now <= 0
            || items.is_empty()
            || items.len() > MAX_MODEL_SYNC_APPLY_ITEMS
            || items.iter().any(|item| {
                !valid_write_fields(&item.fields)
                    || item.fields.visibility != AdminModelVisibilityRecord::Hidden
                    || item.fields.lifecycle != AdminModelLifecycleRecord::Draft
            })
            || items
                .iter()
                .map(|item| item.item_id)
                .collect::<BTreeSet<_>>()
                .len()
                != items.len()
        {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        match timeout(
            self.operation_timeout,
            self.apply_preview_inner(preview_id, actor_user_id, now, items),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ModelSyncRepositoryError::Timeout)),
        }
    }

    async fn apply_preview_inner(
        &self,
        preview_id: &str,
        actor_user_id: UserId,
        now: i64,
        items: Vec<ModelSyncApplyItemRecord>,
    ) -> Result<Vec<AdminModelRecord>, ModelSyncRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        lock_sqlite_run(&transaction, preview_id, actor_user_id).await?;
        let run = load_locked_run(&transaction, preview_id, actor_user_id).await?;
        match run.state {
            1 => {}
            2 => return Err(ModelSyncRepositoryError::AlreadyApplied),
            _ => return Err(ModelSyncRepositoryError::Invariant),
        }
        if run.expires_at.unix_timestamp() <= now {
            return Err(ModelSyncRepositoryError::Expired);
        }
        let item_ids = items.iter().map(|item| item.item_id).collect::<Vec<_>>();
        let selected = model_sync_items::Entity::find()
            .filter(model_sync_items::Column::RunId.eq(run.id))
            .filter(model_sync_items::Column::Id.is_in(item_ids.clone()))
            .order_by_asc(model_sync_items::Column::Id)
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        if selected.len() != items.len()
            || selected.iter().any(|item| {
                item.applied_model_id.is_some()
                    || match ModelSyncRelationRecord::try_from_database(item.relation) {
                        Ok(relation) => !relation.is_applicable(),
                        Err(_) => true,
                    }
            })
        {
            return Err(ModelSyncRepositoryError::Conflict);
        }
        let canonical_models = selected
            .iter()
            .map(|item| item.canonical_model.clone())
            .collect::<Vec<_>>();
        let channel_models_to_add = selected
            .iter()
            .filter(|item| {
                ModelSyncRelationRecord::try_from_database(item.relation).ok()
                    == Some(ModelSyncRelationRecord::DiscoveredUnconfigured)
            })
            .map(|item| item.canonical_model.clone())
            .collect::<Vec<_>>();
        if models::Entity::find()
            .filter(models::Column::Model.is_in(canonical_models.clone()))
            .filter(models::Column::DeletedAt.is_null())
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?
            .is_some()
        {
            return Err(ModelSyncRepositoryError::Conflict);
        }
        let mut commands = items
            .into_iter()
            .map(|item| (item.item_id, item.fields))
            .collect::<BTreeMap<_, _>>();
        let mut created = Vec::with_capacity(selected.len());
        for item in selected {
            let fields = commands
                .remove(&item.id)
                .ok_or(ModelSyncRepositoryError::Invariant)?;
            let (model_id, snapshot) =
                insert_model_draft(&transaction, item.canonical_model, fields).await?;
            let updated = model_sync_items::Entity::update_many()
                .filter(model_sync_items::Column::Id.eq(item.id))
                .filter(model_sync_items::Column::RunId.eq(run.id))
                .filter(model_sync_items::Column::AppliedModelId.is_null())
                .col_expr(
                    model_sync_items::Column::AppliedModelId,
                    Expr::value(model_id.get()),
                )
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| ModelSyncRepositoryError::Query)?;
            if updated.rows_affected != 1 {
                return Err(ModelSyncRepositoryError::Conflict);
            }
            created.push(snapshot);
        }
        if !commands.is_empty() {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        // 应用发现结果时同步加入当前渠道路由；已有分组、权重和权限由能力同步逻辑保留。
        let routing_at = TimeDateTimeWithTimeZone::from_unix_timestamp(now)
            .map_err(|_| ModelSyncRepositoryError::Invariant)?;
        let channel_id =
            ChannelId::new(run.channel_id).map_err(|_| ModelSyncRepositoryError::Invariant)?;
        append_channel_models(&transaction, channel_id, &channel_models_to_add, routing_at)
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        let applied_at = TimeDateTimeWithTimeZone::from_unix_timestamp(now)
            .map_err(|_| ModelSyncRepositoryError::Invariant)?;
        let updated = model_sync_runs::Entity::update_many()
            .filter(model_sync_runs::Column::Id.eq(run.id))
            .filter(model_sync_runs::Column::State.eq(1))
            .col_expr(model_sync_runs::Column::State, Expr::value(2_i16))
            .col_expr(model_sync_runs::Column::AppliedAt, Expr::value(applied_at))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        if updated.rows_affected != 1 {
            return Err(ModelSyncRepositoryError::Conflict);
        }
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        Ok(created)
    }
}

async fn lock_sqlite_run(
    transaction: &DatabaseTransaction,
    preview_id: &str,
    actor_user_id: UserId,
) -> Result<(), ModelSyncRepositoryError> {
    if transaction.get_database_backend() != DbBackend::Sqlite {
        return Ok(());
    }
    // SQLite 没有 FOR UPDATE，先执行不改变业务值的写入取得数据库写锁。
    model_sync_runs::Entity::update_many()
        .filter(model_sync_runs::Column::PreviewId.eq(preview_id))
        .filter(model_sync_runs::Column::ActorUserId.eq(actor_user_id.get()))
        .col_expr(
            model_sync_runs::Column::State,
            Expr::col(model_sync_runs::Column::State).into(),
        )
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?;
    Ok(())
}

async fn load_locked_run(
    transaction: &DatabaseTransaction,
    preview_id: &str,
    actor_user_id: UserId,
) -> Result<model_sync_runs::Model, ModelSyncRepositoryError> {
    let mut query = model_sync_runs::Entity::find()
        .filter(model_sync_runs::Column::PreviewId.eq(preview_id))
        .filter(model_sync_runs::Column::ActorUserId.eq(actor_user_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelSyncRepositoryError::Query)?
        .ok_or(ModelSyncRepositoryError::NotFound)
}
