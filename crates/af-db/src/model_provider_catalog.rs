use std::{fmt, time::Duration};

use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set,
    entity::prelude::TimeDateTimeWithTimeZone, sea_query::Expr,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::model_provider_catalog};

pub const MAX_MODEL_PROVIDER_KEY_BYTES: usize = 64;
pub const MAX_MODEL_PROVIDER_DISPLAY_NAME_BYTES: usize = 128;
pub const MAX_MODEL_PROVIDER_LOGO_BYTES: usize = 128;
pub const MAX_MODEL_PROVIDER_ALIASES: usize = 32;
pub const MAX_MODEL_PROVIDER_ALIAS_BYTES: usize = 64;

#[derive(Clone, Eq, PartialEq)]
pub struct ModelProviderCatalogRecord {
    provider_key: String,
    display_name: String,
    logo: Option<String>,
    aliases: Vec<String>,
    enabled: bool,
    sort_order: i32,
    version: i64,
}

impl ModelProviderCatalogRecord {
    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    pub fn logo(&self) -> Option<&str> {
        self.logo.as_deref()
    }
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    pub const fn sort_order(&self) -> i32 {
        self.sort_order
    }
    pub const fn version(&self) -> i64 {
        self.version
    }
}

impl fmt::Debug for ModelProviderCatalogRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelProviderCatalogRecord")
            .field("provider_key", &self.provider_key)
            .field("display_name", &self.display_name)
            .field("logo", &self.logo)
            .field("alias_count", &self.aliases.len())
            .field("enabled", &self.enabled)
            .field("sort_order", &self.sort_order)
            .field("version", &self.version)
            .finish()
    }
}

pub struct ModelProviderCatalogWriteRecord {
    pub expected_version: i64,
    pub display_name: String,
    pub logo: Option<String>,
    pub aliases: Vec<String>,
    pub enabled: bool,
    pub sort_order: i32,
}

impl fmt::Debug for ModelProviderCatalogWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelProviderCatalogWriteRecord(<validated>)")
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ModelProviderCatalogRepositoryError {
    #[error("模型厂商目录输入无效")]
    InvalidInput,
    #[error("模型厂商目录发生并发更新")]
    ConcurrentUpdate,
    #[error("模型厂商目录数据库查询失败")]
    Query,
    #[error("模型厂商目录数据库操作超时")]
    Timeout,
    #[error("模型厂商目录持久化状态无效")]
    Invariant,
}

#[derive(Clone)]
pub struct ModelProviderCatalogRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl ModelProviderCatalogRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, ModelProviderCatalogRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(ModelProviderCatalogRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn providers(
        &self,
    ) -> Result<Vec<ModelProviderCatalogRecord>, ModelProviderCatalogRepositoryError> {
        timeout(self.operation_timeout, self.providers_inner())
            .await
            .map_err(|_| ModelProviderCatalogRepositoryError::Timeout)?
    }

    pub async fn provider(
        &self,
        key: &str,
    ) -> Result<Option<ModelProviderCatalogRecord>, ModelProviderCatalogRepositoryError> {
        timeout(self.operation_timeout, self.provider_inner(key))
            .await
            .map_err(|_| ModelProviderCatalogRepositoryError::Timeout)?
    }

    pub async fn save_provider(
        &self,
        key: &str,
        write: ModelProviderCatalogWriteRecord,
    ) -> Result<ModelProviderCatalogRecord, ModelProviderCatalogRepositoryError> {
        timeout(self.operation_timeout, self.save_provider_inner(key, write))
            .await
            .map_err(|_| ModelProviderCatalogRepositoryError::Timeout)?
    }

    pub async fn delete_provider(
        &self,
        key: &str,
        expected_version: i64,
    ) -> Result<(), ModelProviderCatalogRepositoryError> {
        timeout(
            self.operation_timeout,
            self.delete_provider_inner(key, expected_version),
        )
        .await
        .map_err(|_| ModelProviderCatalogRepositoryError::Timeout)?
    }

    async fn providers_inner(
        &self,
    ) -> Result<Vec<ModelProviderCatalogRecord>, ModelProviderCatalogRepositoryError> {
        model_provider_catalog::Entity::find()
            .order_by_asc(model_provider_catalog::Column::SortOrder)
            .order_by_asc(model_provider_catalog::Column::DisplayName)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelProviderCatalogRepositoryError::Query)?
            .into_iter()
            .map(record_from_model)
            .collect()
    }

    async fn provider_inner(
        &self,
        key: &str,
    ) -> Result<Option<ModelProviderCatalogRecord>, ModelProviderCatalogRepositoryError> {
        validate_key(key)?;
        model_provider_catalog::Entity::find_by_id(key.to_owned())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelProviderCatalogRepositoryError::Query)?
            .map(record_from_model)
            .transpose()
    }

    async fn save_provider_inner(
        &self,
        key: &str,
        write: ModelProviderCatalogWriteRecord,
    ) -> Result<ModelProviderCatalogRecord, ModelProviderCatalogRepositoryError> {
        validate_key(key)?;
        validate_write(&write)?;
        let current = self.provider_inner(key).await?;
        match (current, write.expected_version) {
            (None, 0) => {
                let now = TimeDateTimeWithTimeZone::now_utc();
                model_provider_catalog::ActiveModel {
                    provider_key: Set(key.to_owned()),
                    display_name: Set(write.display_name),
                    logo: Set(write.logo),
                    aliases: Set(serde_json::to_value(write.aliases)
                        .map_err(|_| ModelProviderCatalogRepositoryError::Invariant)?),
                    enabled: Set(write.enabled),
                    sort_order: Set(write.sort_order),
                    version: Set(1),
                    created_at: Set(now),
                    updated_at: Set(now),
                }
                .insert(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| ModelProviderCatalogRepositoryError::ConcurrentUpdate)?;
                self.provider_inner(key)
                    .await?
                    .ok_or(ModelProviderCatalogRepositoryError::Invariant)
            }
            (Some(_), 0) | (None, _) => Err(ModelProviderCatalogRepositoryError::ConcurrentUpdate),
            (Some(_), expected) if expected < 1 => {
                Err(ModelProviderCatalogRepositoryError::InvalidInput)
            }
            (Some(_), expected) => {
                let next_version = expected
                    .checked_add(1)
                    .ok_or(ModelProviderCatalogRepositoryError::Invariant)?;
                let aliases = serde_json::to_value(write.aliases)
                    .map_err(|_| ModelProviderCatalogRepositoryError::Invariant)?;
                let result = model_provider_catalog::Entity::update_many()
                    .filter(model_provider_catalog::Column::ProviderKey.eq(key))
                    .filter(model_provider_catalog::Column::Version.eq(expected))
                    .col_expr(
                        model_provider_catalog::Column::DisplayName,
                        Expr::value(write.display_name),
                    )
                    .col_expr(
                        model_provider_catalog::Column::Logo,
                        Expr::value(write.logo),
                    )
                    .col_expr(
                        model_provider_catalog::Column::Aliases,
                        Expr::value(aliases),
                    )
                    .col_expr(
                        model_provider_catalog::Column::Enabled,
                        Expr::value(write.enabled),
                    )
                    .col_expr(
                        model_provider_catalog::Column::SortOrder,
                        Expr::value(write.sort_order),
                    )
                    .col_expr(
                        model_provider_catalog::Column::Version,
                        Expr::value(next_version),
                    )
                    .col_expr(
                        model_provider_catalog::Column::UpdatedAt,
                        Expr::value(TimeDateTimeWithTimeZone::now_utc()),
                    )
                    .exec(self.pool.connection())
                    .with_subscriber(NoSubscriber::default())
                    .await
                    .map_err(|_| ModelProviderCatalogRepositoryError::Query)?;
                if result.rows_affected != 1 {
                    return Err(ModelProviderCatalogRepositoryError::ConcurrentUpdate);
                }
                self.provider_inner(key)
                    .await?
                    .ok_or(ModelProviderCatalogRepositoryError::Invariant)
            }
        }
    }

    async fn delete_provider_inner(
        &self,
        key: &str,
        expected_version: i64,
    ) -> Result<(), ModelProviderCatalogRepositoryError> {
        validate_key(key)?;
        if expected_version < 1 {
            return Err(ModelProviderCatalogRepositoryError::InvalidInput);
        }
        let result = model_provider_catalog::Entity::delete_many()
            .filter(model_provider_catalog::Column::ProviderKey.eq(key))
            .filter(model_provider_catalog::Column::Version.eq(expected_version))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelProviderCatalogRepositoryError::Query)?;
        match result.rows_affected {
            1 => Ok(()),
            0 => Err(ModelProviderCatalogRepositoryError::ConcurrentUpdate),
            _ => Err(ModelProviderCatalogRepositoryError::Invariant),
        }
    }
}

fn record_from_model(
    model: model_provider_catalog::Model,
) -> Result<ModelProviderCatalogRecord, ModelProviderCatalogRepositoryError> {
    let aliases = model
        .aliases
        .as_array()
        .ok_or(ModelProviderCatalogRepositoryError::Invariant)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or(ModelProviderCatalogRepositoryError::Invariant)
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_key(&model.provider_key)?;
    if model.display_name.is_empty() || model.version < 1 || model.sort_order < 0 {
        return Err(ModelProviderCatalogRepositoryError::Invariant);
    }
    if model.logo.as_deref().is_some_and(|logo| logo.is_empty()) {
        return Err(ModelProviderCatalogRepositoryError::Invariant);
    }
    Ok(ModelProviderCatalogRecord {
        provider_key: model.provider_key,
        display_name: model.display_name,
        logo: model.logo,
        aliases,
        enabled: model.enabled,
        sort_order: model.sort_order,
        version: model.version,
    })
}

fn validate_key(value: &str) -> Result<(), ModelProviderCatalogRepositoryError> {
    if value.is_empty()
        || value.len() > MAX_MODEL_PROVIDER_KEY_BYTES
        || !value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'_' | b'-'))
        })
    {
        return Err(ModelProviderCatalogRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_write(
    write: &ModelProviderCatalogWriteRecord,
) -> Result<(), ModelProviderCatalogRepositoryError> {
    if write.expected_version < 0
        || write.display_name.trim().is_empty()
        || write.display_name.len() > MAX_MODEL_PROVIDER_DISPLAY_NAME_BYTES
        || write.display_name.chars().any(char::is_control)
        || write.logo.as_deref().is_some_and(|logo| {
            logo.is_empty()
                || logo.len() > MAX_MODEL_PROVIDER_LOGO_BYTES
                || logo.chars().any(char::is_control)
        })
        || write.aliases.len() > MAX_MODEL_PROVIDER_ALIASES
        || write.aliases.iter().any(|alias| {
            alias.trim().is_empty()
                || alias.len() > MAX_MODEL_PROVIDER_ALIAS_BYTES
                || alias.chars().any(char::is_control)
        })
        || write.sort_order < 0
    {
        return Err(ModelProviderCatalogRepositoryError::InvalidInput);
    }
    let mut aliases = write.aliases.clone();
    aliases.sort_unstable();
    aliases.dedup();
    if aliases.len() != write.aliases.len() {
        return Err(ModelProviderCatalogRepositoryError::InvalidInput);
    }
    Ok(())
}
