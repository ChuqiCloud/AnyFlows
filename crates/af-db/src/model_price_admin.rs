use std::{collections::BTreeMap, fmt};

use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    MAX_ADMIN_MODEL_CONTEXT_WINDOW, ModelPriceBillingMode, ModelPriceRecord, ModelPriceRepository,
    ModelPriceRepositoryError,
    entity::{SensitiveDecimal, SensitiveString, model_prices, models},
    model_price::{is_valid_model_name, is_valid_model_price_decimal},
};

/// 单页管理价格目录允许返回的最大记录数。
pub const MAX_MODEL_PRICE_PAGE_SIZE: usize = 100;
/// 一次原子价格写入允许包含的最大模型数。
pub const MAX_MODEL_PRICE_WRITE_BATCH: usize = 100;

/// 按 Canonical 标识稳定分页的模型价格目录。
pub struct ModelPricePageRecord {
    prices: Vec<ModelPriceRecord>,
    next_cursor: Option<String>,
}

impl ModelPricePageRecord {
    /// 消费当前页面并返回价格记录和下一页游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<ModelPriceRecord>, Option<String>) {
        (self.prices, self.next_cursor)
    }
}

impl fmt::Debug for ModelPricePageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPricePageRecord")
            .field("price_count", &self.prices.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 管理员确认后的单个模型价格及其乐观版本前置条件。
pub struct ModelPriceWriteRecord {
    model: String,
    expected_version: Option<u64>,
    context_window: Option<i64>,
    billing_mode: ModelPriceBillingMode,
    prices: [Decimal; 5],
    billing_expression: Option<String>,
}

impl ModelPriceWriteRecord {
    /// 校验 Canonical、版本、计费模式和五类美元/百万 Token 单价。
    pub fn new(
        model: String,
        expected_version: Option<u64>,
        billing_mode: ModelPriceBillingMode,
        prices: [Decimal; 5],
    ) -> Result<Self, ModelPriceWriteError> {
        Self::new_with_expression(model, expected_version, billing_mode, prices, None)
    }

    /// 校验并构造包含可选计费表达式的管理员写入记录。
    pub fn new_with_expression(
        model: String,
        expected_version: Option<u64>,
        billing_mode: ModelPriceBillingMode,
        prices: [Decimal; 5],
        billing_expression: Option<String>,
    ) -> Result<Self, ModelPriceWriteError> {
        Self::new_with_metadata(
            model,
            expected_version,
            None,
            billing_mode,
            prices,
            billing_expression,
        )
    }

    /// 校验并构造可选上下文长度与正式价格的同事务写入记录。
    pub fn new_with_metadata(
        model: String,
        expected_version: Option<u64>,
        context_window: Option<i64>,
        billing_mode: ModelPriceBillingMode,
        prices: [Decimal; 5],
        billing_expression: Option<String>,
    ) -> Result<Self, ModelPriceWriteError> {
        if !is_valid_model_name(&model)
            || expected_version == Some(0)
            || context_window
                .is_some_and(|value| !(1..=MAX_ADMIN_MODEL_CONTEXT_WINDOW).contains(&value))
            || prices
                .iter()
                .any(|price| !is_valid_model_price_decimal(*price))
            || !crate::model_price::is_valid_model_price_shape(
                billing_mode,
                prices,
                billing_expression.as_deref(),
            )
        {
            return Err(ModelPriceWriteError::InvalidInput);
        }
        Ok(Self {
            model,
            expected_version,
            context_window,
            billing_mode,
            prices,
            billing_expression,
        })
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub const fn expected_version(&self) -> Option<u64> {
        self.expected_version
    }

    /// 返回管理员明确采用的上下文长度；空值表示保持模型元数据现状。
    #[must_use]
    pub const fn context_window(&self) -> Option<i64> {
        self.context_window
    }

    #[must_use]
    pub const fn billing_mode(&self) -> ModelPriceBillingMode {
        self.billing_mode
    }

    #[must_use]
    pub const fn prices(&self) -> [Decimal; 5] {
        self.prices
    }

    /// 返回待写入的表达式正文；调用方不得将正文写入日志或错误。
    #[must_use]
    pub fn billing_expression(&self) -> Option<&str> {
        self.billing_expression.as_deref()
    }
}

impl fmt::Debug for ModelPriceWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPriceWriteRecord")
            .field("has_expected_version", &self.expected_version.is_some())
            .field("billing_mode", &self.billing_mode)
            .finish_non_exhaustive()
    }
}

/// 管理价格分页和原子写入的稳定失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ModelPriceWriteError {
    #[error("模型价格写入参数无效")]
    InvalidInput,
    #[error("模型价格对应的模型元数据不存在")]
    ModelNotFound,
    #[error("模型价格版本发生并发冲突")]
    Conflict,
    #[error("模型价格数据库操作失败")]
    Query,
    #[error("模型价格数据库操作超时")]
    Timeout,
    #[error("模型价格持久化状态损坏")]
    Invariant,
}

impl ModelPriceRepository {
    /// 按 Canonical 游标读取一页管理价格目录。
    pub async fn list_page(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<ModelPricePageRecord, ModelPriceWriteError> {
        if !(1..=MAX_MODEL_PRICE_PAGE_SIZE).contains(&limit)
            || after.is_some_and(|value| !is_valid_model_name(value))
        {
            return Err(ModelPriceWriteError::InvalidInput);
        }
        let operation = async {
            let mut query = model_prices::Entity::find()
                .order_by_asc(model_prices::Column::Model)
                .limit(
                    u64::try_from(limit)
                        .ok()
                        .and_then(|value| value.checked_add(1))
                        .ok_or(ModelPriceWriteError::Invariant)?,
                );
            if let Some(after) = after {
                query = query.filter(model_prices::Column::Model.gt(after));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(|_| ModelPriceWriteError::Query)
        }
        .with_subscriber(NoSubscriber::default());
        let mut rows = match timeout(self.load_timeout, operation).await {
            Ok(result) => result.map_err(record_write_error)?,
            Err(_) => return Err(record_write_error(ModelPriceWriteError::Timeout)),
        };
        let has_more = rows.len() > limit;
        if has_more {
            rows.truncate(limit);
        }
        let prices = rows
            .into_iter()
            .map(|row| {
                ModelPriceRecord::try_from_model(row).map_err(|_| ModelPriceWriteError::Invariant)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| prices.last().map(|price| price.model().to_owned()))
            .flatten();
        Ok(ModelPricePageRecord {
            prices,
            next_cursor,
        })
    }

    /// 在同一事务中按稳定模型顺序创建或更新一批价格。
    pub async fn apply_batch(
        &self,
        writes: Vec<ModelPriceWriteRecord>,
    ) -> Result<Vec<ModelPriceRecord>, ModelPriceWriteError> {
        if writes.is_empty() || writes.len() > MAX_MODEL_PRICE_WRITE_BATCH {
            return Err(ModelPriceWriteError::InvalidInput);
        }
        let mut indexed = BTreeMap::new();
        for write in writes {
            if indexed.insert(write.model.clone(), write).is_some() {
                return Err(ModelPriceWriteError::InvalidInput);
            }
        }
        let operation = self.apply_batch_inner(indexed);
        match timeout(self.load_timeout, operation).await {
            Ok(result) => result.map_err(record_write_error),
            Err(_) => Err(record_write_error(ModelPriceWriteError::Timeout)),
        }
    }

    async fn apply_batch_inner(
        &self,
        writes: BTreeMap<String, ModelPriceWriteRecord>,
    ) -> Result<Vec<ModelPriceRecord>, ModelPriceWriteError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelPriceWriteError::Query)?;
        let names = writes.keys().cloned().collect::<Vec<_>>();
        lock_sqlite_models(&transaction, &names).await?;
        let mut model_query = models::Entity::find()
            .filter(models::Column::Model.is_in(names.clone()))
            .filter(models::Column::DeletedAt.is_null());
        if transaction.get_database_backend() != DbBackend::Sqlite {
            model_query = model_query.lock(LockType::Update);
        }
        let active_models = model_query
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelPriceWriteError::Query)?;
        if active_models.len() != names.len() {
            return Err(ModelPriceWriteError::ModelNotFound);
        }
        let model_ids = active_models
            .into_iter()
            .map(|model| (model.model.clone(), model.id))
            .collect::<BTreeMap<_, _>>();

        let mut price_query =
            model_prices::Entity::find().filter(model_prices::Column::Model.is_in(names.clone()));
        if transaction.get_database_backend() != DbBackend::Sqlite {
            price_query = price_query.lock(LockType::Update);
        }
        let existing = price_query
            .all(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelPriceWriteError::Query)?
            .into_iter()
            .map(|row| (row.model.as_str().to_owned(), row))
            .collect::<BTreeMap<_, _>>();

        let mut saved = Vec::with_capacity(writes.len());
        for (name, write) in writes {
            let current = existing.get(&name);
            let current_version = current
                .map(|row| {
                    u64::try_from(row.version)
                        .ok()
                        .filter(|version| *version > 0)
                        .ok_or(ModelPriceWriteError::Invariant)
                })
                .transpose()?;
            if current_version != write.expected_version {
                return Err(ModelPriceWriteError::Conflict);
            }
            if let Some(context_window) = write.context_window {
                let model_id = model_ids
                    .get(&name)
                    .copied()
                    .ok_or(ModelPriceWriteError::Invariant)?;
                let updated = models::Entity::update_many()
                    .filter(models::Column::Id.eq(model_id))
                    .filter(models::Column::DeletedAt.is_null())
                    .col_expr(models::Column::ContextWindow, Expr::value(context_window))
                    .exec(&transaction)
                    .with_subscriber(NoSubscriber::default())
                    .await
                    .map_err(|_| ModelPriceWriteError::Query)?;
                if updated.rows_affected != 1 {
                    return Err(ModelPriceWriteError::Invariant);
                }
            }
            let next_version = current_version
                .unwrap_or(0)
                .checked_add(1)
                .and_then(|value| i64::try_from(value).ok())
                .ok_or(ModelPriceWriteError::Invariant)?;
            let row = match current {
                Some(current) => {
                    let mut active = current.clone().into_active_model();
                    apply_fields(&mut active, &write, next_version);
                    active
                        .update(&transaction)
                        .with_subscriber(NoSubscriber::default())
                        .await
                        .map_err(|_| ModelPriceWriteError::Query)?
                }
                None => {
                    let mut active = model_prices::ActiveModel {
                        model: Set(SensitiveString::from(name)),
                        ..Default::default()
                    };
                    apply_fields(&mut active, &write, next_version);
                    active
                        .insert(&transaction)
                        .with_subscriber(NoSubscriber::default())
                        .await
                        .map_err(|_| ModelPriceWriteError::Query)?
                }
            };
            saved.push(
                ModelPriceRecord::try_from_model(row)
                    .map_err(|_| ModelPriceWriteError::Invariant)?,
            );
        }
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| ModelPriceWriteError::Query)?;
        Ok(saved)
    }
}

fn apply_fields(
    active: &mut model_prices::ActiveModel,
    write: &ModelPriceWriteRecord,
    version: i64,
) {
    let [
        input,
        output,
        cache_read,
        cache_creation_5m,
        cache_creation_1h,
    ] = write.prices;
    active.billing_mode = Set(write.billing_mode.database_value());
    active.input_price = Set(SensitiveDecimal::from(input));
    active.output_price = Set(SensitiveDecimal::from(output));
    active.cache_read_price = Set(SensitiveDecimal::from(cache_read));
    active.cache_creation_5m_price = Set(SensitiveDecimal::from(cache_creation_5m));
    active.cache_creation_1h_price = Set(SensitiveDecimal::from(cache_creation_1h));
    // 表达式正文与计费模式保持同一事务更新，切回固定单价时显式清除旧正文。
    active.billing_expression = Set(write.billing_expression.clone().map(SensitiveString::from));
    active.version = Set(version);
}

async fn lock_sqlite_models(
    transaction: &DatabaseTransaction,
    names: &[String],
) -> Result<(), ModelPriceWriteError> {
    if transaction.get_database_backend() != DbBackend::Sqlite {
        return Ok(());
    }
    // SQLite 不支持 FOR UPDATE，先对权威模型行执行恒等更新以取得写锁。
    models::Entity::update_many()
        .filter(models::Column::Model.is_in(names.iter().cloned()))
        .filter(models::Column::DeletedAt.is_null())
        .col_expr(
            models::Column::UpdatedAt,
            Expr::col(models::Column::UpdatedAt).into(),
        )
        .exec(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| ModelPriceWriteError::Query)?;
    Ok(())
}

fn record_write_error(error: ModelPriceWriteError) -> ModelPriceWriteError {
    let error_kind = match error {
        ModelPriceWriteError::InvalidInput
        | ModelPriceWriteError::ModelNotFound
        | ModelPriceWriteError::Conflict => return error,
        ModelPriceWriteError::Query => "model_price_write_query",
        ModelPriceWriteError::Timeout => "model_price_write_timeout",
        ModelPriceWriteError::Invariant => "model_price_write_invariant",
    };
    tracing::error!(
        target: "af_db::model_price_admin",
        error_kind,
        "管理模型价格仓储发生内部错误"
    );
    error
}

impl From<ModelPriceRepositoryError> for ModelPriceWriteError {
    fn from(error: ModelPriceRepositoryError) -> Self {
        match error {
            ModelPriceRepositoryError::InvalidConfiguration => Self::InvalidInput,
            ModelPriceRepositoryError::Query => Self::Query,
            ModelPriceRepositoryError::Timeout => Self::Timeout,
            ModelPriceRepositoryError::Invariant => Self::Invariant,
        }
    }
}
