//! Durable host records with optimistic concurrency. No operation is exposed to a plugin.
use crate::{
    DatabasePool,
    entity::{credentials, plugin_host_records as records},
};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    sea_query::{Expr, OnConflict},
};
use serde_json::Value;
use thiserror::Error;

#[derive(Clone, PartialEq)]
pub struct PluginHostRecord {
    pub kind: String,
    pub id: String,
    pub plugin_id: String,
    pub owner_id: i64,
    pub revision: i64,
    pub state: String,
    pub payload: Value,
    pub due_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Error)]
pub enum PluginHostError {
    #[error("plugin host record is invalid")]
    Invalid,
    #[error("plugin host record changed")]
    Conflict,
    #[error("plugin host persistence unavailable")]
    Unavailable,
}

#[derive(Clone)]
pub struct PluginHostRepository {
    pool: DatabasePool,
}
impl PluginHostRepository {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }

    pub async fn get(
        &self,
        kind: &str,
        id: &str,
    ) -> Result<Option<PluginHostRecord>, PluginHostError> {
        Ok(
            records::Entity::find_by_id((kind.to_owned(), id.to_owned()))
                .one(self.pool.connection())
                .await
                .map_err(|_| PluginHostError::Unavailable)?
                .map(record),
        )
    }

    /// Insert once. An ambiguous retry reads the existing immutable identity.
    pub async fn insert(
        &self,
        value: PluginHostRecord,
    ) -> Result<PluginHostRecord, PluginHostError> {
        validate(&value)?;
        records::Entity::insert(active(&value))
            .on_conflict(
                OnConflict::columns([records::Column::Kind, records::Column::RecordId])
                    .do_nothing()
                    .to_owned(),
            )
            .exec_without_returning(self.pool.connection())
            .await
            .map_err(|_| PluginHostError::Unavailable)?;
        self.get(&value.kind, &value.id)
            .await?
            .ok_or(PluginHostError::Unavailable)
    }

    /// All state transitions, leases and one-use callback claims use revision CAS.
    pub async fn replace(
        &self,
        value: &PluginHostRecord,
    ) -> Result<PluginHostRecord, PluginHostError> {
        validate(value)?;
        let next = value
            .revision
            .checked_add(1)
            .ok_or(PluginHostError::Invalid)?;
        let changed = records::Entity::update_many()
            .filter(records::Column::Kind.eq(&value.kind))
            .filter(records::Column::RecordId.eq(&value.id))
            .filter(records::Column::Revision.eq(value.revision))
            .filter(records::Column::PluginId.eq(&value.plugin_id))
            .filter(records::Column::OwnerId.eq(value.owner_id))
            .col_expr(records::Column::Revision, Expr::value(next))
            .col_expr(records::Column::State, Expr::value(value.state.clone()))
            .col_expr(records::Column::Payload, Expr::value(value.payload.clone()))
            .col_expr(records::Column::DueAt, Expr::value(value.due_at))
            .col_expr(records::Column::UpdatedAt, Expr::value(value.updated_at))
            .exec(self.pool.connection())
            .await
            .map_err(|_| PluginHostError::Unavailable)?;
        if changed.rows_affected != 1 {
            return Err(PluginHostError::Conflict);
        }
        let mut result = value.clone();
        result.revision = next;
        Ok(result)
    }

    pub async fn list(
        &self,
        kind: &str,
        owner: Option<i64>,
        before: Option<&str>,
        due: Option<i64>,
        limit: u64,
    ) -> Result<Vec<PluginHostRecord>, PluginHostError> {
        if limit == 0 || limit > 100 {
            return Err(PluginHostError::Invalid);
        }
        let mut query = records::Entity::find().filter(records::Column::Kind.eq(kind));
        if let Some(owner) = owner {
            query = query.filter(records::Column::OwnerId.eq(owner));
        }
        if let Some(before) = before {
            query = query.filter(records::Column::RecordId.gt(before));
        }
        if let Some(now) = due {
            query = query.filter(records::Column::DueAt.lte(now));
        }
        Ok(query
            .order_by_asc(records::Column::RecordId)
            .limit(limit)
            .all(self.pool.connection())
            .await
            .map_err(|_| PluginHostError::Unavailable)?
            .into_iter()
            .map(record)
            .collect())
    }

    pub async fn credential_belongs_to(
        &self,
        channel: i64,
        credential: i64,
    ) -> Result<bool, PluginHostError> {
        Ok(credentials::Entity::find_by_id(credential)
            .filter(credentials::Column::ChannelId.eq(channel))
            .filter(credentials::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .await
            .map_err(|_| PluginHostError::Unavailable)?
            .is_some())
    }
}

fn validate(value: &PluginHostRecord) -> Result<(), PluginHostError> {
    if value.kind.is_empty()
        || value.kind.len() > 32
        || value.id.is_empty()
        || value.id.len() > 128
        || value.plugin_id.is_empty()
        || value.plugin_id.len() > 128
        || value.owner_id <= 0
        || value.revision <= 0
        || value.state.len() > 32
        || serde_json::to_vec(&value.payload).map_or(true, |v| v.len() > 2 * 1024 * 1024)
    {
        return Err(PluginHostError::Invalid);
    }
    Ok(())
}
fn active(value: &PluginHostRecord) -> records::ActiveModel {
    records::ActiveModel {
        kind: Set(value.kind.clone()),
        record_id: Set(value.id.clone()),
        plugin_id: Set(value.plugin_id.clone()),
        owner_id: Set(value.owner_id),
        revision: Set(value.revision),
        state: Set(value.state.clone()),
        payload: Set(value.payload.clone()),
        due_at: Set(value.due_at),
        updated_at: Set(value.updated_at),
    }
}
fn record(value: records::Model) -> PluginHostRecord {
    PluginHostRecord {
        kind: value.kind,
        id: value.record_id,
        plugin_id: value.plugin_id,
        owner_id: value.owner_id,
        revision: value.revision,
        state: value.state,
        payload: value.payload,
        due_at: value.due_at,
        updated_at: value.updated_at,
    }
}
