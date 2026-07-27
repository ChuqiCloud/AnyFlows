use std::{fmt, time::Duration};

use af_domain::MAX_MODEL_NAME_BYTES;
use rust_decimal::Decimal;
use sea_orm::{EntityTrait, QueryOrder, QuerySelect};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::model_prices};

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// 单次目录快照允许加载的模型价格上限。
pub const MAX_MODEL_PRICE_ENTRIES: usize = 100_000;

/// 当前持久化目录支持的计费模式。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelPriceBillingMode {
    /// 五类互斥 token 分项按美元/百万 token 计费。
    PerToken,
    /// 显式免费；上层仍记录 usage，但跳过预扣与结算。
    Free,
}

impl ModelPriceBillingMode {
    const fn from_code(code: i16) -> Result<Self, ModelPriceRepositoryError> {
        match code {
            1 => Ok(Self::PerToken),
            2 => Ok(Self::Free),
            _ => Err(ModelPriceRepositoryError::Invariant),
        }
    }
}

/// 已通过持久化边界校验的单个模型价格记录。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelPriceRecord {
    model: String,
    billing_mode: ModelPriceBillingMode,
    prices: [Decimal; 5],
    version: u64,
}

impl ModelPriceRecord {
    /// 返回 Canonical 模型名；调用方不得将其写入日志或错误。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回当前已实现的计费模式。
    #[must_use]
    pub const fn billing_mode(&self) -> ModelPriceBillingMode {
        self.billing_mode
    }

    /// 返回输入、输出、缓存读、5m 与 1h 缓存创建的单价。
    #[must_use]
    pub const fn prices(&self) -> [Decimal; 5] {
        self.prices
    }

    /// 返回管理员每次修改必须递增的正版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    fn try_from_model(model: model_prices::Model) -> Result<Self, ModelPriceRepositoryError> {
        let name = model.model.as_str();
        let billing_mode = ModelPriceBillingMode::from_code(model.billing_mode)?;
        let prices = [
            model.input_price.expose(),
            model.output_price.expose(),
            model.cache_read_price.expose(),
            model.cache_creation_5m_price.expose(),
            model.cache_creation_1h_price.expose(),
        ];
        let version = u64::try_from(model.version)
            .ok()
            .filter(|version| *version > 0)
            .ok_or(ModelPriceRepositoryError::Invariant)?;
        if !is_valid_model_name(name)
            || prices.iter().any(|price| *price < Decimal::ZERO)
            || (billing_mode == ModelPriceBillingMode::Free
                && prices.iter().any(|price| !price.is_zero()))
        {
            return Err(ModelPriceRepositoryError::Invariant);
        }
        Ok(Self {
            model: name.to_owned(),
            billing_mode,
            prices,
            version,
        })
    }
}

impl fmt::Debug for ModelPriceRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPriceRecord")
            .field("billing_mode", &self.billing_mode)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// 模型价格目录读取错误；不携带模型名、价格或数据库诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ModelPriceRepositoryError {
    /// 查询截止时间配置为零。
    #[error("模型价格查询超时必须大于零")]
    InvalidConfiguration,
    /// 获取连接或读取完整目录失败。
    #[error("读取模型价格目录失败")]
    Query,
    /// 完整目录读取超过硬截止时间。
    #[error("读取模型价格目录超时")]
    Timeout,
    /// 行数、模型名、模式、单价或版本违反持久化不变量。
    #[error("模型价格持久化状态损坏")]
    Invariant,
}

/// 有界读取完整模型价格目录的数据库仓储。
#[derive(Clone)]
pub struct ModelPriceRepository {
    pool: DatabasePool,
    load_timeout: Duration,
}

impl ModelPriceRepository {
    /// 使用默认五秒截止时间创建仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            load_timeout: DEFAULT_LOAD_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间创建仓储。
    pub fn with_load_timeout(
        pool: DatabasePool,
        load_timeout: Duration,
    ) -> Result<Self, ModelPriceRepositoryError> {
        if load_timeout.is_zero() {
            return Err(ModelPriceRepositoryError::InvalidConfiguration);
        }
        Ok(Self { pool, load_timeout })
    }

    /// 按模型名稳定排序读取不可变目录输入；超量或任一损坏行会让整次加载失败。
    pub async fn load_all(&self) -> Result<Vec<ModelPriceRecord>, ModelPriceRepositoryError> {
        let operation = async {
            let limit = u64::try_from(MAX_MODEL_PRICE_ENTRIES)
                .ok()
                .and_then(|limit| limit.checked_add(1))
                .ok_or(ModelPriceRepositoryError::Invariant)?;
            let models = model_prices::Entity::find()
                .order_by_asc(model_prices::Column::Model)
                .limit(limit)
                .all(self.pool.connection())
                .await
                .map_err(|_| ModelPriceRepositoryError::Query)?;
            if models.len() > MAX_MODEL_PRICE_ENTRIES {
                return Err(ModelPriceRepositoryError::Invariant);
            }
            models
                .into_iter()
                .map(ModelPriceRecord::try_from_model)
                .collect()
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.load_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(ModelPriceRepositoryError::Timeout)),
        }
    }
}

impl fmt::Debug for ModelPriceRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelPriceRepository")
            .field("load_timeout", &self.load_timeout)
            .finish_non_exhaustive()
    }
}

pub(crate) fn is_valid_model_name(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_MODEL_NAME_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

fn record_internal_error(error: ModelPriceRepositoryError) -> ModelPriceRepositoryError {
    let error_kind = match error {
        ModelPriceRepositoryError::InvalidConfiguration => return error,
        ModelPriceRepositoryError::Query => "model_price_query",
        ModelPriceRepositoryError::Timeout => "model_price_timeout",
        ModelPriceRepositoryError::Invariant => "model_price_invariant",
    };
    tracing::error!(
        target: "af_db::model_price",
        error_kind,
        "模型价格仓储发生内部错误"
    );
    error
}
