use std::{fmt, future::Future, pin::Pin};

use af_db::{
    BalanceAlertSettingsRecord, BalanceAlertSettingsRepository,
    BalanceAlertSettingsRepositoryError, BalanceAlertSettingsWriteRecord,
    MAX_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS, MAX_SUBSCRIPTION_REMAINING_PERCENT,
    MIN_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS, MIN_SUBSCRIPTION_REMAINING_PERCENT,
};
use af_domain::Quota;
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理员可读取的余额预警全局设置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminBalanceAlertSettings {
    enabled: bool,
    default_threshold: Quota,
    reminder_interval_seconds: u64,
    subscription_alert_enabled: bool,
    subscription_remaining_percent: i16,
    version: i64,
}

impl AdminBalanceAlertSettings {
    /// 为替代应用服务构造经过边界校验的设置投影。
    pub fn new(
        enabled: bool,
        default_threshold: Quota,
        reminder_interval_seconds: u64,
        subscription_alert_enabled: bool,
        subscription_remaining_percent: i16,
        version: i64,
    ) -> Result<Self, AdminBalanceAlertSettingsError> {
        if default_threshold.is_zero()
            || !(MIN_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS
                ..=MAX_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS)
                .contains(&reminder_interval_seconds)
            || !(MIN_SUBSCRIPTION_REMAINING_PERCENT..=MAX_SUBSCRIPTION_REMAINING_PERCENT)
                .contains(&subscription_remaining_percent)
            || version < 1
        {
            return Err(AdminBalanceAlertSettingsError::InvalidInput);
        }
        Ok(Self {
            enabled,
            default_threshold,
            reminder_interval_seconds,
            subscription_alert_enabled,
            subscription_remaining_percent,
            version,
        })
    }

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

    #[must_use]
    pub const fn subscription_alert_enabled(self) -> bool {
        self.subscription_alert_enabled
    }

    #[must_use]
    pub const fn subscription_remaining_percent(self) -> i16 {
        self.subscription_remaining_percent
    }

    #[must_use]
    pub const fn version(self) -> i64 {
        self.version
    }

    fn from_record(
        record: BalanceAlertSettingsRecord,
    ) -> Result<Self, AdminBalanceAlertSettingsError> {
        Self::new(
            record.enabled(),
            record.default_threshold(),
            record.reminder_interval_seconds(),
            record.subscription_alert_enabled(),
            record.subscription_remaining_percent(),
            record.version(),
        )
        .map_err(|_| AdminBalanceAlertSettingsError::Internal)
    }
}

/// 管理员完整覆盖余额预警设置的命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminBalanceAlertSettingsCommand {
    enabled: bool,
    default_threshold: Quota,
    reminder_interval_seconds: u64,
    subscription_alert_enabled: bool,
    subscription_remaining_percent: i16,
}

impl AdminBalanceAlertSettingsCommand {
    /// 校验正整数阈值和受控提醒窗口。
    pub fn new(
        enabled: bool,
        default_threshold: Quota,
        reminder_interval_seconds: u64,
        subscription_alert_enabled: bool,
        subscription_remaining_percent: i16,
    ) -> Result<Self, AdminBalanceAlertSettingsError> {
        if default_threshold.is_zero()
            || !(MIN_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS
                ..=MAX_BALANCE_ALERT_REMINDER_INTERVAL_SECONDS)
                .contains(&reminder_interval_seconds)
            || !(MIN_SUBSCRIPTION_REMAINING_PERCENT..=MAX_SUBSCRIPTION_REMAINING_PERCENT)
                .contains(&subscription_remaining_percent)
        {
            return Err(AdminBalanceAlertSettingsError::InvalidInput);
        }
        Ok(Self {
            enabled,
            default_threshold,
            reminder_interval_seconds,
            subscription_alert_enabled,
            subscription_remaining_percent,
        })
    }

    fn into_record(self) -> BalanceAlertSettingsWriteRecord {
        BalanceAlertSettingsWriteRecord::new(
            self.enabled,
            self.default_threshold,
            self.reminder_interval_seconds,
        )
        .with_subscription_alerts(
            self.subscription_alert_enabled,
            self.subscription_remaining_percent,
        )
    }
}

/// 管理员余额预警设置应用服务错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminBalanceAlertSettingsError {
    #[error("余额预警设置输入无效")]
    InvalidInput,
    #[error("当前会话无权管理余额预警设置")]
    Forbidden,
    #[error("余额预警设置服务内部错误")]
    Internal,
}

pub type AdminBalanceAlertSettingsReadFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminBalanceAlertSettings, AdminBalanceAlertSettingsError>>
            + Send
            + 'a,
    >,
>;
pub type AdminBalanceAlertSettingsUpdateFuture<'a> = AdminBalanceAlertSettingsReadFuture<'a>;

/// 管理员余额预警设置用例端口。
pub trait AdminBalanceAlertSettingsService: Send + Sync {
    fn settings<'a>(
        &'a self,
        principal: SessionPrincipal,
    ) -> AdminBalanceAlertSettingsReadFuture<'a>;

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminBalanceAlertSettingsCommand,
    ) -> AdminBalanceAlertSettingsUpdateFuture<'a>;
}

/// 使用固定设置仓储实现管理员余额预警配置。
pub struct DatabaseAdminBalanceAlertSettingsService {
    repository: BalanceAlertSettingsRepository,
}

impl DatabaseAdminBalanceAlertSettingsService {
    #[must_use]
    pub const fn new(repository: BalanceAlertSettingsRepository) -> Self {
        Self { repository }
    }
}

impl AdminBalanceAlertSettingsService for DatabaseAdminBalanceAlertSettingsService {
    fn settings<'a>(
        &'a self,
        principal: SessionPrincipal,
    ) -> AdminBalanceAlertSettingsReadFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .settings()
                .await
                .map_err(map_repository_error)
                .and_then(AdminBalanceAlertSettings::from_record)
        })
    }

    fn update<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminBalanceAlertSettingsCommand,
    ) -> AdminBalanceAlertSettingsUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .update(command.into_record())
                .await
                .map_err(map_repository_error)
                .and_then(AdminBalanceAlertSettings::from_record)
        })
    }
}

impl fmt::Debug for DatabaseAdminBalanceAlertSettingsService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminBalanceAlertSettingsService")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminBalanceAlertSettingsError> {
    if principal.role() != SessionRole::Admin {
        return Err(AdminBalanceAlertSettingsError::Forbidden);
    }
    Ok(())
}

fn map_repository_error(
    error: BalanceAlertSettingsRepositoryError,
) -> AdminBalanceAlertSettingsError {
    match error {
        BalanceAlertSettingsRepositoryError::InvalidSettings => {
            AdminBalanceAlertSettingsError::InvalidInput
        }
        BalanceAlertSettingsRepositoryError::Query
        | BalanceAlertSettingsRepositoryError::Timeout
        | BalanceAlertSettingsRepositoryError::Invariant => {
            AdminBalanceAlertSettingsError::Internal
        }
    }
}
