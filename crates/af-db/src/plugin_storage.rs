//! Host-owned, namespaced JSON storage for plugin state.

use std::time::Duration;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{DatabasePool, entity::plugin_storage};

pub const MAX_PLUGIN_STORAGE_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_PLUGIN_STORAGE_LIST_LIMIT: u64 = 100;

#[derive(Clone, Debug, PartialEq)]
pub struct PluginStorageRecord {
    pub plugin_id: String,
    pub key: String,
    pub value: serde_json::Value,
    pub expires_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PluginStorageRepositoryError {
    #[error("插件存储输入无效")]
    InvalidInput,
    #[error("插件存储查询失败")]
    Query,
    #[error("插件存储写入超时")]
    Timeout,
}

#[derive(Clone)]
pub struct PluginStorageRepository {
    pool: DatabasePool,
    timeout: Duration,
}

impl PluginStorageRepository {
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            timeout: Duration::from_secs(5),
        }
    }

    pub fn with_timeout(
        pool: DatabasePool,
        timeout: Duration,
    ) -> Result<Self, PluginStorageRepositoryError> {
        if timeout.is_zero() {
            return Err(PluginStorageRepositoryError::InvalidInput);
        }
        Ok(Self { pool, timeout })
    }

    pub async fn get(
        &self,
        plugin_id: &str,
        key: &str,
        now: i64,
    ) -> Result<Option<PluginStorageRecord>, PluginStorageRepositoryError> {
        validate_scope(plugin_id, key)?;
        if now <= 0 {
            return Err(PluginStorageRepositoryError::InvalidInput);
        }
        let row = plugin_storage::Entity::find_by_id((plugin_id.to_owned(), key.to_owned()))
            .one(self.pool.connection())
            .await
            .map_err(|_| PluginStorageRepositoryError::Query)?;
        Ok(row.and_then(|row| {
            if row.expires_at.is_some_and(|expires_at| expires_at <= now) {
                None
            } else {
                Some(record(row))
            }
        }))
    }

    pub async fn put(
        &self,
        plugin_id: &str,
        key: &str,
        value: serde_json::Value,
        expires_at: Option<i64>,
        now: i64,
    ) -> Result<(), PluginStorageRepositoryError> {
        validate_scope(plugin_id, key)?;
        validate_value(&value, expires_at, now)?;
        let operation = async {
            let existing =
                plugin_storage::Entity::find_by_id((plugin_id.to_owned(), key.to_owned()))
                    .one(self.pool.connection())
                    .await
                    .map_err(|_| PluginStorageRepositoryError::Query)?;
            if existing.is_some() {
                let model = plugin_storage::ActiveModel {
                    plugin_id: Set(plugin_id.to_owned()),
                    storage_key: Set(key.to_owned()),
                    value_json: Set(value),
                    expires_at: Set(expires_at),
                    updated_at: Set(now),
                };
                model
                    .update(self.pool.connection())
                    .await
                    .map_err(|_| PluginStorageRepositoryError::Query)?;
            } else {
                plugin_storage::ActiveModel {
                    plugin_id: Set(plugin_id.to_owned()),
                    storage_key: Set(key.to_owned()),
                    value_json: Set(value),
                    expires_at: Set(expires_at),
                    updated_at: Set(now),
                }
                .insert(self.pool.connection())
                .await
                .map_err(|_| PluginStorageRepositoryError::Query)?;
            }
            Ok(())
        };
        timeout(self.timeout, operation)
            .await
            .map_err(|_| PluginStorageRepositoryError::Timeout)?
    }

    pub async fn delete(
        &self,
        plugin_id: &str,
        key: &str,
    ) -> Result<bool, PluginStorageRepositoryError> {
        validate_scope(plugin_id, key)?;
        let result = plugin_storage::Entity::delete_by_id((plugin_id.to_owned(), key.to_owned()))
            .exec(self.pool.connection())
            .await
            .map_err(|_| PluginStorageRepositoryError::Query)?;
        Ok(result.rows_affected == 1)
    }

    pub async fn list(
        &self,
        plugin_id: &str,
        prefix: Option<&str>,
        limit: u64,
        now: i64,
    ) -> Result<Vec<PluginStorageRecord>, PluginStorageRepositoryError> {
        if !valid_plugin_id(plugin_id)
            || limit == 0
            || limit > MAX_PLUGIN_STORAGE_LIST_LIMIT
            || now <= 0
        {
            return Err(PluginStorageRepositoryError::InvalidInput);
        }
        if let Some(prefix) = prefix {
            validate_key(prefix)?;
        }
        let mut query = plugin_storage::Entity::find()
            .filter(plugin_storage::Column::PluginId.eq(plugin_id))
            .order_by_asc(plugin_storage::Column::StorageKey)
            .limit(limit);
        if let Some(prefix) = prefix {
            query = query.filter(plugin_storage::Column::StorageKey.like(format!("{prefix}%")));
        }
        let rows = query
            .all(self.pool.connection())
            .await
            .map_err(|_| PluginStorageRepositoryError::Query)?;
        Ok(rows
            .into_iter()
            .filter(|row| row.expires_at.is_none_or(|expires_at| expires_at > now))
            .map(record)
            .collect())
    }
}

fn record(row: plugin_storage::Model) -> PluginStorageRecord {
    PluginStorageRecord {
        plugin_id: row.plugin_id,
        key: row.storage_key,
        value: row.value_json,
        expires_at: row.expires_at,
        updated_at: row.updated_at,
    }
}

fn validate_scope(plugin_id: &str, key: &str) -> Result<(), PluginStorageRepositoryError> {
    if !valid_plugin_id(plugin_id) {
        return Err(PluginStorageRepositoryError::InvalidInput);
    }
    validate_key(key)
}

fn validate_key(key: &str) -> Result<(), PluginStorageRepositoryError> {
    if key.is_empty()
        || key.len() > 128
        || key.trim() != key
        || key.contains("//")
        || key.contains("..")
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
    {
        return Err(PluginStorageRepositoryError::InvalidInput);
    }
    Ok(())
}

fn valid_plugin_id(plugin_id: &str) -> bool {
    !plugin_id.is_empty()
        && plugin_id.len() <= 128
        && plugin_id.split('.').count() >= 2
        && plugin_id.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn validate_value(
    value: &serde_json::Value,
    expires_at: Option<i64>,
    now: i64,
) -> Result<(), PluginStorageRepositoryError> {
    if serde_json::to_vec(value).map_or(true, |encoded| {
        encoded.len() > MAX_PLUGIN_STORAGE_VALUE_BYTES
    }) || expires_at.is_some_and(|expires_at| expires_at <= now)
    {
        return Err(PluginStorageRepositoryError::InvalidInput);
    }
    Ok(())
}
