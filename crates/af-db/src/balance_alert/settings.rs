use std::{fmt, time::Duration};

use af_domain::Quota;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::balance_alert_settings};

const BALANCE_ALERT_SETTINGS_ID: i16 = 1;
pub const DEFAULT_BALANCE_ALERT_THRESHOLD_QUOTA: i64 = 1_000;
pub const DEFAULT_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS: u64 = 86_400;
pub const MIN_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS: u64 = 3_600;
pub const MAX_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS: u64 = 604_800;
pub const DEFAULT_SUBSCRIPTION_REMAINING_PERCENT: i16 = 20;
pub const MIN_SUBSCRIPTION_REMAINING_PERCENT: i16 = 1;
pub const MAX_SUBSCRIPTION_REMAINING_PERCENT: i16 = 99;

/// 已完成持久化校验的余额预警全局设置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BalanceAlertSettingsRecord {
    enabled: bool,
    default_threshold: Quota,
    reminder_interval_seconds: u64,
    subscription_alert_enabled: bool,
    subscription_remaining_percent: i16,
    version: i64,
}

impl BalanceAlertSettingsRecord {
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn default_threshold(self) -> Quota {
        self.default_threshold
    }

    #[must_use]
    pub const fn reminder_interval_seconds(self) -> u64 {
        self.reminder_interval_seconds
    }

    /// 返回订阅窗口剩余额度预警是否启用。
    #[must_use]
    pub const fn subscription_alert_enabled(self) -> bool {
        self.subscription_alert_enabled
    }

    /// 返回触发订阅预警的剩余额度百分比。
    #[must_use]
    pub const fn subscription_remaining_percent(self) -> i16 {
        self.subscription_remaining_percent
    }

    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }

    /// 将受信 UTC 秒时间归入稳定提醒窗口，供唯一约束跨实例去重。
    pub fn window_started_at_epoch(
        self,
        now: TimeDateTimeWithTimeZone,
    ) -> Result<i64, BalanceAlertSettingsRepositoryError> {
        let interval = i64::try_from(self.reminder_interval_seconds)
            .map_err(|_| internal_error(BalanceAlertSettingsRepositoryError::Invariant))?;
        let timestamp = now.unix_timestamp();
        if timestamp < 0 || interval <= 0 {
            return Err(internal_error(
                BalanceAlertSettingsRepositoryError::Invariant,
            ));
        }
        Ok(timestamp - timestamp.rem_euclid(interval))
    }
}

/// 管理员完整覆盖余额预警全局设置时使用的记录。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BalanceAlertSettingsWriteRecord {
    enabled: bool,
    default_threshold: Quota,
    reminder_interval_seconds: u64,
    subscription_alert_enabled: bool,
    subscription_remaining_percent: i16,
}

impl BalanceAlertSettingsWriteRecord {
    /// 组合强类型设置；启停不清除阈值草稿。
    #[must_use]
    pub const fn new(
        enabled: bool,
        default_threshold: Quota,
        reminder_interval_seconds: u64,
    ) -> Self {
        Self {
            enabled,
            default_threshold,
            reminder_interval_seconds,
            subscription_alert_enabled: false,
            subscription_remaining_percent: DEFAULT_SUBSCRIPTION_REMAINING_PERCENT,
        }
    }

    /// 覆盖订阅窗口预警草稿；关闭时仍保留百分比供后续启用。
    #[must_use]
    pub const fn with_subscription_alerts(mut self, enabled: bool, remaining_percent: i16) -> Self {
        self.subscription_alert_enabled = enabled;
        self.subscription_remaining_percent = remaining_percent;
        self
    }

    fn validate(self) -> Result<(), BalanceAlertSettingsRepositoryError> {
        if self.default_threshold.is_zero()
            || !(MIN_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS
                ..=MAX_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS)
                .contains(&self.reminder_interval_seconds)
            || !(MIN_SUBSCRIPTION_REMAINING_PERCENT..=MAX_SUBSCRIPTION_REMAINING_PERCENT)
                .contains(&self.subscription_remaining_percent)
        {
            return Err(BalanceAlertSettingsRepositoryError::InvalidSettings);
        }
        Ok(())
    }
}

/// 余额预警设置仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BalanceAlertSettingsRepositoryConfigError {
    #[error("余额预警设置数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 余额预警设置仓储错误；不透传数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BalanceAlertSettingsRepositoryError {
    #[error("余额预警设置数据库操作失败")]
    Query,
    #[error("余额预警设置数据库操作超时")]
    Timeout,
    #[error("余额预警设置持久化状态损坏")]
    Invariant,
    #[error("余额预警设置字段无效")]
    InvalidSettings,
}

/// 固定余额预警设置记录的数据库仓储。
#[derive(Clone)]
pub struct BalanceAlertSettingsRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl BalanceAlertSettingsRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, BalanceAlertSettingsRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(BalanceAlertSettingsRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 读取固定余额预警设置；固定行缺失视为持久化损坏。
    pub async fn settings(
        &self,
    ) -> Result<BalanceAlertSettingsRecord, BalanceAlertSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.settings_inner()).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(BalanceAlertSettingsRepositoryError::Timeout)),
        }
    }

    /// 原子覆盖完整设置并单调递增版本。
    pub async fn update(
        &self,
        record: BalanceAlertSettingsWriteRecord,
    ) -> Result<BalanceAlertSettingsRecord, BalanceAlertSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.update_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(BalanceAlertSettingsRepositoryError::Timeout)),
        }
    }

    async fn settings_inner(
        &self,
    ) -> Result<BalanceAlertSettingsRecord, BalanceAlertSettingsRepositoryError> {
        let model = balance_alert_settings::Entity::find_by_id(BALANCE_ALERT_SETTINGS_ID)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("balance_alert_settings_read"))?
            .ok_or_else(|| internal_error(BalanceAlertSettingsRepositoryError::Invariant))?;
        record_from_model(model)
    }

    async fn update_inner(
        &self,
        record: BalanceAlertSettingsWriteRecord,
    ) -> Result<BalanceAlertSettingsRecord, BalanceAlertSettingsRepositoryError> {
        record.validate()?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("balance_alert_settings_begin"))?;
        let existing = lock_settings(&transaction).await?;
        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| internal_error(BalanceAlertSettingsRepositoryError::Invariant))?;
        let reminder_interval_seconds = i64::try_from(record.reminder_interval_seconds)
            .map_err(|_| internal_error(BalanceAlertSettingsRepositoryError::Invariant))?;
        let saved = balance_alert_settings::ActiveModel {
            id: Set(BALANCE_ALERT_SETTINGS_ID),
            enabled: Set(record.enabled),
            default_threshold_quota: Set(record.default_threshold.units()),
            reminder_interval_seconds: Set(reminder_interval_seconds),
            subscription_alert_enabled: Set(record.subscription_alert_enabled),
            subscription_remaining_percent: Set(record.subscription_remaining_percent),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(TimeDateTimeWithTimeZone::now_utc()),
        }
        .update(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("balance_alert_settings_write"))?;
        let saved = record_from_model(saved)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("balance_alert_settings_commit"))?;
        Ok(saved)
    }
}

async fn lock_settings(
    transaction: &DatabaseTransaction,
) -> Result<balance_alert_settings::Model, BalanceAlertSettingsRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，用恒等写入取得固定设置行的数据库写锁。
        let result = balance_alert_settings::Entity::update_many()
            .filter(balance_alert_settings::Column::Id.eq(BALANCE_ALERT_SETTINGS_ID))
            .col_expr(
                balance_alert_settings::Column::Version,
                Expr::col(balance_alert_settings::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query_error("balance_alert_settings_lock"))?;
        if result.rows_affected != 1 {
            return Err(internal_error(
                BalanceAlertSettingsRepositoryError::Invariant,
            ));
        }
    }
    let mut query = balance_alert_settings::Entity::find()
        .filter(balance_alert_settings::Column::Id.eq(BALANCE_ALERT_SETTINGS_ID));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("balance_alert_settings_read_for_update"))?
        .ok_or_else(|| internal_error(BalanceAlertSettingsRepositoryError::Invariant))
}

fn record_from_model(
    model: balance_alert_settings::Model,
) -> Result<BalanceAlertSettingsRecord, BalanceAlertSettingsRepositoryError> {
    let default_threshold = Quota::new(model.default_threshold_quota)
        .map_err(|_| internal_error(BalanceAlertSettingsRepositoryError::Invariant))?;
    let reminder_interval_seconds = u64::try_from(model.reminder_interval_seconds)
        .map_err(|_| internal_error(BalanceAlertSettingsRepositoryError::Invariant))?;
    if model.id != BALANCE_ALERT_SETTINGS_ID
        || model.version < 1
        || default_threshold.is_zero()
        || !(MIN_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS
            ..=MAX_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS)
            .contains(&reminder_interval_seconds)
        || !(MIN_SUBSCRIPTION_REMAINING_PERCENT..=MAX_SUBSCRIPTION_REMAINING_PERCENT)
            .contains(&model.subscription_remaining_percent)
    {
        return Err(internal_error(
            BalanceAlertSettingsRepositoryError::Invariant,
        ));
    }
    Ok(BalanceAlertSettingsRecord {
        enabled: model.enabled,
        default_threshold,
        reminder_interval_seconds,
        subscription_alert_enabled: model.subscription_alert_enabled,
        subscription_remaining_percent: model.subscription_remaining_percent,
        version: model.version,
    })
}

fn query_error(operation: &'static str) -> BalanceAlertSettingsRepositoryError {
    tracing::error!(
        target: "af_db::balance_alert",
        error_kind = operation,
        "余额预警设置数据库操作失败"
    );
    BalanceAlertSettingsRepositoryError::Query
}

fn internal_error(
    error: BalanceAlertSettingsRepositoryError,
) -> BalanceAlertSettingsRepositoryError {
    tracing::error!(
        target: "af_db::balance_alert",
        error_kind = ?error,
        "余额预警设置内部状态无效"
    );
    error
}

impl fmt::Debug for BalanceAlertSettingsRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BalanceAlertSettingsRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}
