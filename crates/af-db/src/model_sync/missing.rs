use std::collections::BTreeMap;

use sea_orm::{
    ColumnTrait, EntityTrait, FromQueryResult, JoinType, QueryFilter, QueryOrder, QuerySelect,
    RelationTrait,
    sea_query::{Expr, Query},
};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::entity::{channel_models, channels, models};

use super::{
    MAX_MISSING_MODEL_CHANNELS, MAX_MISSING_MODEL_PAGE_SIZE, MissingModelChannelRecord,
    MissingModelPageRecord, MissingModelRecord, ModelSyncRepository, ModelSyncRepositoryError,
    record_internal_error, valid_model,
};

const MAX_MISSING_REFERENCE_ROWS: usize = 10_000;

#[derive(FromQueryResult)]
struct MissingModelNameRow {
    model: String,
}

impl ModelSyncRepository {
    /// 按 Canonical 标识列出有效渠道已引用但活动商品元数据缺失的模型。
    pub async fn list_missing_models(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<MissingModelPageRecord, ModelSyncRepositoryError> {
        if !(1..=MAX_MISSING_MODEL_PAGE_SIZE).contains(&limit)
            || after.is_some_and(|value| !valid_model(value))
        {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        let operation = self.list_missing_models_inner(after, limit);
        match timeout(
            self.operation_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ModelSyncRepositoryError::Timeout)),
        }
    }

    async fn list_missing_models_inner(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<MissingModelPageRecord, ModelSyncRepositoryError> {
        let mut active_models = Query::select();
        active_models
            .column(models::Column::Model)
            .from(models::Entity)
            .and_where(Expr::col(models::Column::DeletedAt).is_null());
        let mut query = channel_models::Entity::find()
            .select_only()
            .column(channel_models::Column::Model)
            .join(JoinType::InnerJoin, channel_models::Relation::Channel.def())
            .filter(channels::Column::DeletedAt.is_null())
            .filter(
                Expr::col((channel_models::Entity, channel_models::Column::Model))
                    .not_in_subquery(active_models),
            )
            .group_by(channel_models::Column::Model)
            .order_by_asc(channel_models::Column::Model);
        if let Some(after) = after {
            query = query.filter(channel_models::Column::Model.gt(after));
        }
        let query_limit = u64::try_from(limit)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(ModelSyncRepositoryError::Invariant)?;
        let mut names = query
            .limit(query_limit)
            .into_model::<MissingModelNameRow>()
            .all(self.pool.connection())
            .await
            .map_err(|_| ModelSyncRepositoryError::Query)?;
        let has_more = names.len() > limit;
        if has_more {
            names.pop();
        }
        if names.iter().any(|row| !valid_model(&row.model)) {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        let model_names = names
            .iter()
            .map(|row| row.model.clone())
            .collect::<Vec<_>>();
        let references = if model_names.is_empty() {
            Vec::new()
        } else {
            channel_models::Entity::find()
                .find_also_related(channels::Entity)
                .filter(channel_models::Column::Model.is_in(model_names.clone()))
                .filter(channels::Column::DeletedAt.is_null())
                .order_by_asc(channel_models::Column::Model)
                .order_by_asc(channel_models::Column::ChannelId)
                .limit(
                    u64::try_from(MAX_MISSING_REFERENCE_ROWS + 1)
                        .map_err(|_| ModelSyncRepositoryError::Invariant)?,
                )
                .all(self.pool.connection())
                .await
                .map_err(|_| ModelSyncRepositoryError::Query)?
        };
        if references.len() > MAX_MISSING_REFERENCE_ROWS {
            return Err(ModelSyncRepositoryError::Invariant);
        }

        let mut grouped = BTreeMap::<String, Vec<MissingModelChannelRecord>>::new();
        for (reference, channel) in references {
            let channel = channel.ok_or(ModelSyncRepositoryError::Invariant)?;
            let channel_id = af_domain::ChannelId::new(channel.id)
                .map_err(|_| ModelSyncRepositoryError::Invariant)?;
            if channel.name.is_empty() || channel.name.chars().any(char::is_control) {
                return Err(ModelSyncRepositoryError::Invariant);
            }
            grouped
                .entry(reference.model)
                .or_default()
                .push(MissingModelChannelRecord {
                    channel_id,
                    channel_name: channel.name,
                });
        }
        let models = names
            .into_iter()
            .map(|row| {
                let mut channels = grouped
                    .remove(&row.model)
                    .ok_or(ModelSyncRepositoryError::Invariant)?;
                let channel_count = channels.len();
                channels.truncate(MAX_MISSING_MODEL_CHANNELS);
                Ok(MissingModelRecord {
                    model: row.model,
                    channel_count,
                    channels,
                })
            })
            .collect::<Result<Vec<_>, ModelSyncRepositoryError>>()?;
        if !grouped.is_empty() {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        let next_cursor = has_more
            .then(|| models.last().map(|model| model.model.clone()))
            .flatten();
        Ok(MissingModelPageRecord {
            models,
            next_cursor,
        })
    }
}
